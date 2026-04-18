use std::time::Instant;

use eframe::egui;

use crate::ui::{AppState, SessionStatus};

pub fn render(ui: &mut egui::Ui, state: &AppState) {
    ui.horizontal(|ui| {
        match &state.status {
            SessionStatus::Disconnected => {
                ui.colored_label(egui::Color32::GRAY, "Disconnected");
            }
            SessionStatus::Connecting => {
                ui.spinner();
                ui.colored_label(egui::Color32::YELLOW, "Connecting...");
            }
            SessionStatus::Connected => {
                ui.colored_label(egui::Color32::LIGHT_GREEN, "Connected");
                ui.separator();
                ui.label(format!("{} [{}]", state.snapshot.long_name, state.snapshot.short_name));
                if !state.snapshot.firmware_version.is_empty() {
                    ui.separator();
                    ui.label(format!("fw {}", state.snapshot.firmware_version));
                }
                ui.separator();
                render_link_health(ui, state.last_activity);
            }
        }
        if let Some(err) = &state.last_error {
            ui.separator();
            ui.colored_label(egui::Color32::LIGHT_RED, err);
        }
    });
}

fn render_link_health(ui: &mut egui::Ui, last_activity: Option<Instant>) {
    let Some(last) = last_activity else {
        ui.colored_label(egui::Color32::GRAY, "link: waiting");
        return;
    };
    let elapsed = Instant::now().duration_since(last);
    let secs = elapsed.as_secs();
    let text = format!("link {}", human_short(secs));
    let color = if secs < 30 {
        egui::Color32::LIGHT_GREEN
    } else if secs < 120 {
        egui::Color32::YELLOW
    } else {
        egui::Color32::LIGHT_RED
    };
    ui.colored_label(color, text)
        .on_hover_text(format!("last frame from device {secs}s ago"));
}

fn human_short(secs: u64) -> String {
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3_600 {
        format!("{}m", secs.div_euclid(60))
    } else if secs < 86_400 {
        format!("{}h", secs.div_euclid(3_600))
    } else {
        format!("{}d", secs.div_euclid(86_400))
    }
}
