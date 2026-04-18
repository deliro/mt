use std::collections::{HashMap, HashSet};

use eframe::egui;
use rand::RngCore;
use tokio::sync::mpsc;

use crate::domain::channel::{Channel, ChannelRole};
use crate::domain::ids::ChannelIndex;
use crate::domain::snapshot::DeviceSnapshot;
use crate::session::commands::Command;

pub const MAX_CHANNELS: u8 = 8;
const PSK_BYTES: usize = 32;
const DEFAULT_PRESET: u8 = 1;

#[derive(Default)]
pub struct ChannelsUi {
    pub drafts: HashMap<u8, Channel>,
    pub dirty: HashSet<u8>,
    pub pending_save: Option<u8>,
    pub last_save: Option<String>,
    pub expanded: HashSet<u8>,
}

pub fn render(
    ui: &mut egui::Ui,
    snapshot: &DeviceSnapshot,
    chs: &mut ChannelsUi,
    cmd: &mpsc::UnboundedSender<Command>,
) {
    sync_from_snapshot(snapshot, chs);
    confirm_save_modal(ui.ctx(), chs, cmd);
    egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
        ui.heading("Channels");
        ui.label(
            "Changes are applied individually. Editing the PSK or role of a shared channel \
             disconnects anyone who does not pick up the new setting.",
        );
        if let Some(msg) = chs.last_save.as_ref() {
            ui.colored_label(egui::Color32::LIGHT_GREEN, format!("Saved: {msg}"));
        }
        ui.separator();
        for index in 0..MAX_CHANNELS {
            channel_row(ui, chs, index);
            ui.separator();
        }
    });
}

fn sync_from_snapshot(snapshot: &DeviceSnapshot, chs: &mut ChannelsUi) {
    for index in 0..MAX_CHANNELS {
        if chs.dirty.contains(&index) {
            continue;
        }
        let live = snapshot.channels.iter().find(|c| c.index.get() == index).cloned();
        let draft = live.unwrap_or_else(|| empty_channel(index));
        let _ = chs.drafts.insert(index, draft);
    }
}

fn empty_channel(index: u8) -> Channel {
    Channel {
        index: ChannelIndex::new(index).unwrap_or_else(ChannelIndex::primary),
        role: if index == 0 { ChannelRole::Primary } else { ChannelRole::Disabled },
        name: String::new(),
        psk: Vec::new(),
        uplink_enabled: false,
        downlink_enabled: false,
        position_precision: 0,
    }
}

fn channel_row(ui: &mut egui::Ui, chs: &mut ChannelsUi, index: u8) {
    let Some(draft) = chs.drafts.get(&index).cloned() else { return };
    let dirty = chs.dirty.contains(&index);
    let title = row_title(&draft, dirty);
    let is_expanded = chs.expanded.contains(&index);

    let header_resp = ui.horizontal(|ui| {
        let btn = egui::Button::new(egui::RichText::new(title).strong())
            .fill(egui::Color32::TRANSPARENT);
        let clicked = ui.add(btn).clicked();
        if dirty {
            ui.colored_label(egui::Color32::YELLOW, "● unsaved");
        }
        clicked
    });
    if header_resp.inner {
        toggle_expanded(chs, index, !is_expanded);
    }
    if !chs.expanded.contains(&index) {
        return;
    }

    let mut new_draft = draft;
    let mut changed = false;
    changed |= role_picker(ui, &mut new_draft, index);
    changed |= name_editor(ui, &mut new_draft);
    changed |= psk_controls(ui, &mut new_draft);
    advanced(ui, &mut new_draft, &mut changed);

    if changed {
        let _ = chs.drafts.insert(index, new_draft);
        let _ = chs.dirty.insert(index);
    }

    action_row(ui, chs, index);
}

fn toggle_expanded(chs: &mut ChannelsUi, index: u8, expanded: bool) {
    if expanded {
        let _ = chs.expanded.insert(index);
    } else {
        let _ = chs.expanded.remove(&index);
    }
}

fn row_title(ch: &Channel, dirty: bool) -> String {
    let role = match ch.role {
        ChannelRole::Primary => "Primary",
        ChannelRole::Secondary => "Secondary",
        ChannelRole::Disabled => "Disabled",
    };
    let name = if ch.name.is_empty() {
        if matches!(ch.role, ChannelRole::Primary) { "(unnamed)" } else { "(empty)" }
    } else {
        ch.name.as_str()
    };
    let suffix = if dirty { " *" } else { "" };
    format!("#{idx}  {role}  ·  {name}{suffix}", idx = ch.index.get())
}

fn role_picker(ui: &mut egui::Ui, draft: &mut Channel, index: u8) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label("Role:");
        if index == 0 {
            ui.add_enabled(false, egui::SelectableLabel::new(true, "Primary"))
                .on_hover_text("Slot 0 is always Primary — the mesh-wide default channel.");
            return;
        }
        let mut role = draft.role;
        for (label, value) in [
            ("Secondary", ChannelRole::Secondary),
            ("Disabled", ChannelRole::Disabled),
        ] {
            if ui.radio_value(&mut role, value, label).changed() {
                changed = true;
            }
        }
        draft.role = role;
    });
    changed
}

fn name_editor(ui: &mut egui::Ui, draft: &mut Channel) -> bool {
    ui.horizontal(|ui| {
        ui.label("Name:");
        ui.text_edit_singleline(&mut draft.name)
            .on_hover_text("Channel name. Empty for the primary is fine. Max 11 chars recommended.")
    })
    .inner
    .changed()
}

fn psk_controls(ui: &mut egui::Ui, draft: &mut Channel) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label("PSK:");
        ui.monospace(draft.psk_summary().label());
    });
    ui.horizontal(|ui| {
        if ui
            .button("Regenerate (AES256)")
            .on_hover_text(
                "Replace the key with 32 fresh random bytes. Everyone still using the old key \
                 will stop seeing messages on this channel.",
            )
            .clicked()
        {
            draft.psk = random_psk();
            changed = true;
        }
        if ui
            .button("Use default preset")
            .on_hover_text("Set PSK to the 1-byte indexed default — equivalent to the stock key.")
            .clicked()
        {
            draft.psk = vec![DEFAULT_PRESET];
            changed = true;
        }
        if ui
            .button("Clear (no encryption)")
            .on_hover_text("Remove the PSK. Traffic on this channel becomes unencrypted.")
            .clicked()
        {
            draft.psk.clear();
            changed = true;
        }
    });
    changed
}

fn advanced(ui: &mut egui::Ui, draft: &mut Channel, changed: &mut bool) {
    ui.collapsing("Advanced", |ui| {
        if ui
            .checkbox(&mut draft.uplink_enabled, "MQTT uplink")
            .on_hover_text("Forward messages on this channel to the device's MQTT server.")
            .changed()
        {
            *changed = true;
        }
        if ui
            .checkbox(&mut draft.downlink_enabled, "MQTT downlink")
            .on_hover_text("Accept incoming messages from MQTT and re-inject them into the mesh.")
            .changed()
        {
            *changed = true;
        }
        ui.horizontal(|ui| {
            ui.label("Position precision:");
            let resp = ui
                .add(egui::DragValue::new(&mut draft.position_precision).range(0..=32))
                .on_hover_text(
                    "0 = no position sharing. 32 = full precision. Lower values round coordinates \
                     down (each step halves the resolution).",
                );
            if resp.changed() {
                *changed = true;
            }
        });
    });
}

fn action_row(ui: &mut egui::Ui, chs: &mut ChannelsUi, index: u8) {
    let dirty = chs.dirty.contains(&index);
    ui.horizontal(|ui| {
        let save = egui::Button::new("Save").fill(if dirty {
            egui::Color32::from_rgb(60, 110, 60)
        } else {
            egui::Color32::TRANSPARENT
        });
        if ui
            .add_enabled(dirty, save)
            .on_hover_text("Send the draft to the device. Asks for confirmation first.")
            .clicked()
        {
            chs.pending_save = Some(index);
        }
        if ui
            .add_enabled(dirty, egui::Button::new("Discard"))
            .on_hover_text("Roll back the draft to what the device currently reports.")
            .clicked()
        {
            let _ = chs.dirty.remove(&index);
            let _ = chs.drafts.remove(&index);
        }
    });
}

fn confirm_save_modal(
    ctx: &egui::Context,
    chs: &mut ChannelsUi,
    cmd: &mpsc::UnboundedSender<Command>,
) {
    let Some(index) = chs.pending_save else { return };
    let Some(draft) = chs.drafts.get(&index).cloned() else {
        chs.pending_save = None;
        return;
    };
    let mut confirm = false;
    let mut cancel = false;
    egui::Window::new(format!("Confirm save: channel #{index}"))
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            ui.set_min_width(380.0);
            ui.colored_label(egui::Color32::LIGHT_RED, "⚠ Mesh-wide change");
            ui.label(save_warning(index, &draft));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Cancel").clicked() {
                    cancel = true;
                }
                if ui
                    .add(egui::Button::new("Yes, apply").fill(egui::Color32::from_rgb(150, 40, 40)))
                    .clicked()
                {
                    confirm = true;
                }
            });
        });
    if confirm {
        let _ = cmd.send(Command::SetChannel(draft));
        let _ = chs.dirty.remove(&index);
        chs.last_save = Some(format!("channel #{index}"));
        chs.pending_save = None;
    } else if cancel {
        chs.pending_save = None;
    }
}

fn save_warning(index: u8, draft: &Channel) -> &'static str {
    if index == 0 {
        "Changing the Primary channel affects every device on your mesh. Nodes that do not get \
         the new name and key will silently drop out of the mesh."
    } else if matches!(draft.role, ChannelRole::Disabled) {
        "Disabling a channel removes it from this device. Peers still using it won't see you."
    } else {
        "Peers currently sharing this channel must pick up the new settings — otherwise they'll \
         stop receiving messages on this slot."
    }
}

fn random_psk() -> Vec<u8> {
    let mut buf = vec![0u8; PSK_BYTES];
    rand::thread_rng().fill_bytes(&mut buf);
    buf
}
