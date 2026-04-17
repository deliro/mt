use eframe::egui;
use tokio::sync::mpsc;

use crate::domain::ids::ChannelIndex;
use crate::domain::message::{DeliveryState, Direction, Recipient};
use crate::session::commands::Command;
use crate::ui::AppState;

#[derive(Default)]
pub struct ChatUi {
    pub active_channel: u8,
    pub composer_text: String,
}

pub fn render(ui: &mut egui::Ui, state: &mut AppState, cmd: &mpsc::UnboundedSender<Command>) {
    let active = ChannelIndex::new(state.chat_ui.active_channel).unwrap_or_else(ChannelIndex::primary);

    ui.horizontal(|ui| {
        let channels = state.snapshot.channels.clone();
        for ch in &channels {
            let idx = ch.index.get();
            let label = if ch.name.is_empty() { format!("#{idx}") } else { ch.name.clone() };
            ui.selectable_value(&mut state.chat_ui.active_channel, idx, label);
        }
    });
    ui.separator();

    let messages: Vec<_> =
        state.snapshot.messages.iter().filter(|m| m.channel == active).cloned().collect();
    egui::ScrollArea::vertical().stick_to_bottom(true).show(ui, |ui| {
        for m in messages {
            ui.horizontal(|ui| {
                let sender = state
                    .snapshot
                    .nodes
                    .get(&m.from)
                    .map(|n| n.long_name.clone())
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| format!("{:08x}", m.from.0));
                ui.strong(sender);
                ui.label(&m.text);
                match (&m.direction, &m.state) {
                    (Direction::Outgoing, DeliveryState::Pending) => {
                        ui.label("…");
                    }
                    (Direction::Outgoing, DeliveryState::Delivered) => {
                        ui.label("✓");
                    }
                    (Direction::Outgoing, DeliveryState::Failed(reason)) => {
                        ui.colored_label(egui::Color32::LIGHT_RED, format!("! {reason}"));
                    }
                    (Direction::Incoming, _) => {}
                }
            });
        }
    });

    ui.separator();
    ui.horizontal(|ui| {
        let response = ui.text_edit_singleline(&mut state.chat_ui.composer_text);
        let send_now = ui.button("Send").clicked()
            || (response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
        if send_now && !state.chat_ui.composer_text.is_empty() {
            let text = std::mem::take(&mut state.chat_ui.composer_text);
            let _ = cmd.send(Command::SendText {
                channel: active,
                to: Recipient::Broadcast,
                text,
                want_ack: true,
            });
        }
    });
}
