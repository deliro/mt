use eframe::egui;
use tokio::sync::mpsc;

use crate::domain::config::{
    LoraSettings, MODEM_PRESET_CHOICES, REGION_CHOICES, modem_preset_label, region_label,
};
use crate::domain::snapshot::DeviceSnapshot;
use crate::session::commands::Command;

#[derive(Default)]
pub struct SettingsUi {
    pub draft: Draft,
    pub dirty: Dirty,
    pub last_save: Option<String>,
}

#[derive(Default, Clone)]
pub struct Draft {
    pub long_name: String,
    pub short_name: String,
    pub lora: LoraSettings,
}

#[derive(Default, Clone, Copy)]
pub struct Dirty {
    pub owner: bool,
    pub lora: bool,
}

pub fn render(
    ui: &mut egui::Ui,
    snapshot: &DeviceSnapshot,
    settings_ui: &mut SettingsUi,
    cmd: &mpsc::UnboundedSender<Command>,
) {
    sync_from_snapshot(snapshot, settings_ui);
    owner_section(ui, settings_ui, cmd);
    ui.separator();
    lora_section(ui, settings_ui, cmd);
    if let Some(saved) = &settings_ui.last_save {
        ui.separator();
        ui.colored_label(
            egui::Color32::LIGHT_GREEN,
            format!("{saved} applied (device may reboot)"),
        );
    }
}

fn owner_section(
    ui: &mut egui::Ui,
    settings_ui: &mut SettingsUi,
    cmd: &mpsc::UnboundedSender<Command>,
) {
    ui.heading("Owner");
    ui.horizontal(|ui| {
        ui.label("Long name:");
        if ui.text_edit_singleline(&mut settings_ui.draft.long_name).changed() {
            settings_ui.dirty.owner = true;
        }
    });
    ui.horizontal(|ui| {
        ui.label("Short name:");
        let resp = ui.add(
            egui::TextEdit::singleline(&mut settings_ui.draft.short_name).desired_width(80.0),
        );
        if resp.changed() {
            settings_ui.dirty.owner = true;
        }
        if settings_ui.draft.short_name.chars().count() > 4 {
            ui.colored_label(egui::Color32::LIGHT_RED, "4 chars max");
        }
    });
    ui.horizontal(|ui| {
        let can_save = owner_valid(settings_ui);
        if ui.add_enabled(can_save, egui::Button::new("Save owner")).clicked() {
            let _ = cmd.send(Command::SetOwner {
                long_name: settings_ui.draft.long_name.clone(),
                short_name: settings_ui.draft.short_name.clone(),
            });
            settings_ui.dirty.owner = false;
            settings_ui.last_save = Some("owner".into());
        }
        if settings_ui.dirty.owner {
            ui.weak("unsaved changes");
        }
    });
}

fn owner_valid(settings_ui: &SettingsUi) -> bool {
    settings_ui.dirty.owner
        && !settings_ui.draft.long_name.trim().is_empty()
        && !settings_ui.draft.short_name.trim().is_empty()
        && settings_ui.draft.short_name.chars().count() <= 4
}

fn lora_section(
    ui: &mut egui::Ui,
    settings_ui: &mut SettingsUi,
    cmd: &mpsc::UnboundedSender<Command>,
) {
    ui.heading("LoRa");
    let mut dirty = settings_ui.dirty.lora;
    let lora = &mut settings_ui.draft.lora;
    lora_region_row(ui, lora, &mut dirty);
    lora_preset_rows(ui, lora, &mut dirty);
    lora_numeric_rows(ui, lora, &mut dirty);
    settings_ui.dirty.lora = dirty;
    ui.horizontal(|ui| {
        if ui.add_enabled(settings_ui.dirty.lora, egui::Button::new("Save LoRa")).clicked() {
            let _ = cmd.send(Command::SetLora(settings_ui.draft.lora.clone()));
            settings_ui.dirty.lora = false;
            settings_ui.last_save = Some("LoRa".into());
        }
        if settings_ui.dirty.lora {
            ui.weak("unsaved changes");
        }
    });
}

fn lora_region_row(ui: &mut egui::Ui, lora: &mut LoraSettings, dirty: &mut bool) {
    ui.horizontal(|ui| {
        ui.label("Region:");
        let selected = region_label(lora.region);
        egui::ComboBox::from_id_salt("region").selected_text(selected).show_ui(ui, |ui| {
            for region in REGION_CHOICES {
                if ui
                    .selectable_value(&mut lora.region, *region, region_label(*region))
                    .changed()
                {
                    *dirty = true;
                }
            }
        });
    });
}

fn lora_preset_rows(ui: &mut egui::Ui, lora: &mut LoraSettings, dirty: &mut bool) {
    ui.horizontal(|ui| {
        ui.label("Use preset:");
        if ui.checkbox(&mut lora.use_preset, "").changed() {
            *dirty = true;
        }
    });
    ui.horizontal(|ui| {
        ui.label("Modem preset:");
        let selected = modem_preset_label(lora.modem_preset);
        egui::ComboBox::from_id_salt("modem").selected_text(selected).show_ui(ui, |ui| {
            for preset in MODEM_PRESET_CHOICES {
                if ui
                    .selectable_value(&mut lora.modem_preset, *preset, modem_preset_label(*preset))
                    .changed()
                {
                    *dirty = true;
                }
            }
        });
    });
}

fn lora_numeric_rows(ui: &mut egui::Ui, lora: &mut LoraSettings, dirty: &mut bool) {
    ui.horizontal(|ui| {
        ui.label("Max hops:");
        let mut hop = u32::from(lora.hop_limit);
        if ui.add(egui::Slider::new(&mut hop, 1..=7)).changed() {
            lora.hop_limit = u8::try_from(hop).unwrap_or(lora.hop_limit);
            *dirty = true;
        }
    });
    ui.horizontal(|ui| {
        ui.label("TX enabled:");
        if ui.checkbox(&mut lora.tx_enabled, "").changed() {
            *dirty = true;
        }
    });
    ui.horizontal(|ui| {
        ui.label("TX power (dBm, 0 = default):");
        if ui.add(egui::DragValue::new(&mut lora.tx_power).range(0..=30)).changed() {
            *dirty = true;
        }
    });
}

fn sync_from_snapshot(snapshot: &DeviceSnapshot, ui_state: &mut SettingsUi) {
    if !ui_state.dirty.owner {
        let me = snapshot.nodes.get(&snapshot.my_node);
        ui_state.draft.long_name =
            me.map_or_else(|| snapshot.long_name.clone(), |n| n.long_name.clone());
        ui_state.draft.short_name =
            me.map_or_else(|| snapshot.short_name.clone(), |n| n.short_name.clone());
    }
    if !ui_state.dirty.lora
        && let Some(lora) = &snapshot.lora
    {
        ui_state.draft.lora = lora.clone();
    }
}
