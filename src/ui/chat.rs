use std::sync::OnceLock;
use std::time::SystemTime;

use eframe::egui;
use tokio::sync::mpsc;

use crate::domain::channel::ChannelRole;
use crate::domain::ids::{ChannelIndex, NodeId};
use crate::domain::message::{DeliveryState, Direction, Recipient};
use crate::session::commands::Command;
use crate::ui::AppState;

#[derive(Default)]
pub struct ChatUi {
    pub active_channel: u8,
    pub composer_text: String,
    pub dm_target: Option<NodeId>,
    pub last_seen_count: usize,
    pub follow_bottom: bool,
}

pub fn render_messages(ui: &mut egui::Ui, state: &mut AppState) {
    channel_tabs(ui, state);
    ui.separator();
    let active = active_channel(state);
    message_list(ui, state, active);
}


pub fn render_composer(
    ui: &mut egui::Ui,
    state: &mut AppState,
    cmd: &mpsc::UnboundedSender<Command>,
) {
    let active = active_channel(state);
    composer(ui, state, cmd, active);
}

fn active_channel(state: &AppState) -> ChannelIndex {
    ChannelIndex::new(state.chat_ui.active_channel).unwrap_or_else(ChannelIndex::primary)
}

fn channel_tabs(ui: &mut egui::Ui, state: &mut AppState) {
    let usable: Vec<(u8, String)> = state
        .snapshot
        .channels
        .iter()
        .filter(|c| c.role != ChannelRole::Disabled)
        .map(|c| (c.index.get(), channel_label(c)))
        .collect();

    if let Some((first, _)) = usable.first()
        && !usable.iter().any(|(idx, _)| *idx == state.chat_ui.active_channel)
    {
        state.chat_ui.active_channel = *first;
    }

    ui.horizontal(|ui| {
        if usable.is_empty() {
            ui.weak("no channels yet…");
            return;
        }
        for (idx, label) in &usable {
            ui.selectable_value(&mut state.chat_ui.active_channel, *idx, label);
        }
    });
}

fn channel_label(ch: &crate::domain::channel::Channel) -> String {
    if !ch.name.is_empty() {
        return ch.name.clone();
    }
    match ch.role {
        ChannelRole::Primary => "Primary".into(),
        ChannelRole::Secondary => format!("#{}", ch.index.get()),
        ChannelRole::Disabled => format!("#{} (off)", ch.index.get()),
    }
}

fn message_list(ui: &mut egui::Ui, state: &mut AppState, active: ChannelIndex) {
    let messages: Vec<_> =
        state.snapshot.messages.iter().filter(|m| m.channel == active).cloned().collect();

    let mut open_detail: Option<NodeId> = None;
    let new_messages = messages.len() > state.chat_ui.last_seen_count;
    state.chat_ui.last_seen_count = messages.len();
    if new_messages {
        state.chat_ui.follow_bottom = true;
    }
    let should_jump = state.chat_ui.follow_bottom;

    let scroll = egui::ScrollArea::vertical()
        .id_salt(("chat_scroll", active.get()))
        .auto_shrink([false; 2])
        .stick_to_bottom(true);
    let out = scroll.show(ui, |ui| render_message_rows(ui, state, &messages, &mut open_detail));

    if should_jump {
        // Manual forcing: the `stick_to_bottom` heuristic sometimes unsticks
        // when wrapping reflows (e.g. the node-name column grows after a
        // NodeInfo arrives and changes line widths). Explicitly nudging to the
        // bottom avoids the chat from drifting upwards after events.
        ui.scroll_to_rect(
            egui::Rect::from_min_size(
                egui::pos2(0.0, f32::MAX - 1.0),
                egui::vec2(1.0, 1.0),
            ),
            Some(egui::Align::BOTTOM),
        );
        state.chat_ui.follow_bottom = false;
    }

    // If user scrolls up manually, respect that until a new message arrives.
    let at_bottom =
        out.state.offset.y >= out.content_size.y - out.inner_rect.height() - 2.0;
    if !at_bottom {
        state.chat_ui.follow_bottom = false;
    }

    if let Some(id) = open_detail {
        state.detail_node = Some(id);
    }
}

fn render_message_rows(
    ui: &mut egui::Ui,
    state: &AppState,
    messages: &[crate::domain::message::TextMessage],
    open_detail: &mut Option<NodeId>,
) {
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
            let sender_text = match m.direction {
                Direction::Outgoing => {
                    egui::RichText::new(sender_name).color(egui::Color32::LIGHT_BLUE)
                }
                Direction::Incoming => egui::RichText::new(sender_name).strong(),
            };
            if clickable_label(ui, sender_text) {
                *open_detail = Some(m.from);
            }
            if let Recipient::Node(target) = m.to {
                let label = format!("-> {}", node_display_name(state, target));
                if clickable_label(ui, egui::RichText::new(label)) {
                    *open_detail = Some(target);
                }
            }
            ui.label(&m.text);
            if m.direction == Direction::Outgoing {
                render_delivery(ui, &m.state);
            }
        });
    }
}

fn clickable_label(ui: &mut egui::Ui, text: egui::RichText) -> bool {
    ui.add(egui::Label::new(text).sense(egui::Sense::click())).clicked()
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

    ui.add_space(4.0);
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
        let can_send = !state.chat_ui.composer_text.trim().is_empty();
        let response = ui.add(
            egui::TextEdit::singleline(&mut state.chat_ui.composer_text)
                .hint_text("Write a message")
                .desired_width(f32::INFINITY),
        );
        let enter_pressed =
            response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        let clicked = ui.add_enabled(can_send, egui::Button::new("Send")).clicked();
        if (clicked || enter_pressed) && can_send {
            let text = std::mem::take(&mut state.chat_ui.composer_text);
            let to = state.chat_ui.dm_target.map_or(Recipient::Broadcast, Recipient::Node);
            let _ = cmd.send(Command::SendText { channel: active, to, text, want_ack: true });
            response.request_focus();
        }
    });
    ui.add_space(4.0);
}

fn render_delivery(ui: &mut egui::Ui, state: &DeliveryState) {
    match state {
        DeliveryState::Queued => {
            ui.weak("○ queued").on_hover_text("queued on phone, not yet handed to the radio");
        }
        DeliveryState::Sent => {
            ui.weak("◐ sent").on_hover_text("accepted by device, transmitted on the mesh");
        }
        DeliveryState::Acked => {
            ui.colored_label(egui::Color32::LIGHT_GREEN, "✓ acked")
                .on_hover_text("acknowledged by the destination node");
        }
        DeliveryState::Failed(reason) => {
            ui.colored_label(egui::Color32::LIGHT_RED, format!("✗ {reason}"))
                .on_hover_text("delivery failed");
        }
    }
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
    let dt = time::OffsetDateTime::from(t).to_offset(local_offset());
    format!("{:02}:{:02}:{:02}", dt.hour(), dt.minute(), dt.second())
}

/// Cached local-timezone offset.
///
/// `time::UtcOffset::current_local_offset()` can fail on Linux once worker
/// threads exist (it reads `/etc/localtime` via non-reentrant libc). Stashing
/// the result on first call — ideally from `main` before spawning threads —
/// keeps the UI deterministic; we fall back to UTC if the probe returned
/// `Err`.
pub fn local_offset() -> time::UtcOffset {
    static OFFSET: OnceLock<time::UtcOffset> = OnceLock::new();
    *OFFSET.get_or_init(|| {
        time::UtcOffset::current_local_offset().unwrap_or(time::UtcOffset::UTC)
    })
}
