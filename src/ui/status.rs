use eframe::egui;

use crate::ui::AppState;

pub fn render(ui: &mut egui::Ui, state: &AppState) {
    ui.horizontal(|ui| {
        let label = if state.connected { "● Connected" } else { "○ Disconnected" };
        ui.strong(label);
        if state.connected {
            ui.separator();
            ui.label(format!("{} [{}]", state.snapshot.long_name, state.snapshot.short_name));
            if !state.snapshot.firmware_version.is_empty() {
                ui.separator();
                ui.label(format!("fw {}", state.snapshot.firmware_version));
            }
        }
        if let Some(err) = &state.last_error {
            ui.separator();
            ui.colored_label(egui::Color32::LIGHT_RED, err);
        }
    });
}
