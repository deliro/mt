use std::time::SystemTime;

use eframe::egui;
use tokio::sync::mpsc;

use crate::domain::ids::{ChannelIndex, NodeId};
use crate::domain::message::{DeliveryState, Direction, Recipient};
use crate::session::commands::Command;
use crate::ui::AppState;

#[derive(Default)]
pub struct ChatUi {
    pub active_channel: u8,
    pub composer_text: String,
    pub dm_target: Option<NodeId>,
}

pub fn render(ui: &mut egui::Ui, state: &mut AppState, cmd: &mpsc::UnboundedSender<Command>) {
    let active = ChannelIndex::new(state.chat_ui.active_channel).unwrap_or_else(ChannelIndex::primary);

    channel_tabs(ui, state);
    ui.separator();

    message_list(ui, state, active);

    ui.separator();
    composer(ui, state, cmd, active);
}

fn channel_tabs(ui: &mut egui::Ui, state: &mut AppState) {
    ui.horizontal(|ui| {
        let channels = state.snapshot.channels.clone();
        if channels.is_empty() {
            ui.selectable_value(&mut state.chat_ui.active_channel, 0, "#0");
            return;
        }
        for ch in &channels {
            let idx = ch.index.get();
            let label = if ch.name.is_empty() { format!("#{idx}") } else { ch.name.clone() };
            ui.selectable_value(&mut state.chat_ui.active_channel, idx, label);
        }
    });
}

fn message_list(ui: &mut egui::Ui, state: &AppState, active: ChannelIndex) {
    let messages: Vec<_> =
        state.snapshot.messages.iter().filter(|m| m.channel == active).cloned().collect();

    egui::ScrollArea::vertical().stick_to_bottom(true).auto_shrink([false; 2]).show(ui, |ui| {
        if messages.is_empty() {
            ui.vertical_centered(|ui| {
                ui.add_space(40.0);
                ui.weak("No messages yet on this channel.");
            });
            return;
        }
        for m in messages {
            ui.horizontal_wrapped(|ui| {
                ui.monospace(format_time(m.received_at));
                let sender_name = node_display_name(state, m.from);
                match m.direction {
                    Direction::Outgoing => {
                        ui.colored_label(egui::Color32::LIGHT_BLUE, sender_name);
                    }
                    Direction::Incoming => {
                        ui.strong(sender_name);
                    }
                }
                if let Recipient::Node(target) = m.to {
                    let label = node_display_name(state, target);
                    ui.label(format!("→ {label}"));
                }
                ui.label(&m.text);
                match (&m.direction, &m.state) {
                    (Direction::Outgoing, DeliveryState::Pending) => {
                        ui.weak("…");
                    }
                    (Direction::Outgoing, DeliveryState::Delivered) => {
                        ui.colored_label(egui::Color32::LIGHT_GREEN, "✓");
                    }
                    (Direction::Outgoing, DeliveryState::Failed(reason)) => {
                        ui.colored_label(egui::Color32::LIGHT_RED, format!("! {reason}"));
                    }
                    (Direction::Incoming, _) => {}
                }
            });
        }
    });
}

fn composer(
    ui: &mut egui::Ui,
    state: &mut AppState,
    cmd: &mpsc::UnboundedSender<Command>,
    active: ChannelIndex,
) {
    let dm_options: Vec<(NodeId, String)> = {
        let mut nodes: Vec<_> = state
            .snapshot
            .nodes
            .values()
            .filter(|n| n.id != state.snapshot.my_node)
            .map(|n| (n.id, node_label(n)))
            .collect();
        nodes.sort_by(|a, b| a.1.cmp(&b.1));
        nodes
    };

    ui.horizontal(|ui| {
        ui.label("To:");
        let dm_label = state
            .chat_ui
            .dm_target
            .map_or_else(|| "#channel".into(), |id| node_display_name(state, id));
        egui::ComboBox::from_id_salt("dm_target").selected_text(dm_label).show_ui(ui, |ui| {
            ui.selectable_value(&mut state.chat_ui.dm_target, None, "#channel");
            for (id, label) in &dm_options {
                ui.selectable_value(&mut state.chat_ui.dm_target, Some(*id), label);
            }
        });
    });

    ui.horizontal(|ui| {
        let response = ui.add(
            egui::TextEdit::singleline(&mut state.chat_ui.composer_text)
                .hint_text("Write a message")
                .desired_width(f32::INFINITY),
        );
        let enter_pressed =
            response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        let can_send = !state.chat_ui.composer_text.trim().is_empty();
        let clicked = ui.add_enabled(can_send, egui::Button::new("Send")).clicked();
        if (clicked || enter_pressed) && can_send {
            let text = std::mem::take(&mut state.chat_ui.composer_text);
            let to = state.chat_ui.dm_target.map_or(Recipient::Broadcast, Recipient::Node);
            let _ = cmd.send(Command::SendText { channel: active, to, text, want_ack: true });
            response.request_focus();
        }
    });
}

fn node_display_name(state: &AppState, id: NodeId) -> String {
    state.snapshot.nodes.get(&id).map_or_else(|| format!("!{:08x}", id.0), node_label)
}

fn node_label(node: &crate::domain::node::Node) -> String {
    if !node.long_name.is_empty() {
        node.long_name.clone()
    } else if !node.short_name.is_empty() {
        node.short_name.clone()
    } else {
        format!("!{:08x}", node.id.0)
    }
}

fn format_time(t: SystemTime) -> String {
    let secs = t.duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let h = secs.div_euclid(3_600).rem_euclid(24);
    let m = secs.div_euclid(60).rem_euclid(60);
    let s = secs.rem_euclid(60);
    format!("{h:02}:{m:02}:{s:02}")
}
