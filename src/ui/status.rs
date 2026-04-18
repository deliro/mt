use std::time::Instant;

use eframe::egui;

use crate::domain::stats::MeshStats;
use crate::ui::{AppState, SessionStatus};

pub fn render(ui: &mut egui::Ui, state: &AppState) {
    ui.horizontal(|ui| {
        match &state.status {
            SessionStatus::Disconnected => {
                ui.colored_label(egui::Color32::GRAY, "● Disconnected");
            }
            SessionStatus::Connecting => {
                ui.spinner();
                ui.colored_label(egui::Color32::YELLOW, "● Connecting…");
            }
            SessionStatus::Connected => {
                ui.colored_label(egui::Color32::LIGHT_GREEN, "● Connected");
                ui.separator();
                ui.label(format!("{} [{}]", state.snapshot.long_name, state.snapshot.short_name));
                if !state.snapshot.firmware_version.is_empty() {
                    ui.separator();
                    ui.label(format!("fw {}", state.snapshot.firmware_version));
                }
                ui.separator();
                render_link_health(ui, state.last_activity);
                render_mesh_stats(ui, &state.snapshot.stats);
                render_mqtt(ui, state);
            }
        }
        if let Some(err) = &state.last_error {
            ui.separator();
            ui.colored_label(egui::Color32::LIGHT_RED, err);
        }
    });
}

fn render_mesh_stats(ui: &mut egui::Ui, stats: &MeshStats) {
    render_battery(ui, stats);
    render_chutil(ui, stats);
    render_airtime(ui, stats);
    render_relay(ui, stats);
}

fn render_mqtt(ui: &mut egui::Ui, state: &AppState) {
    let Some(mqtt) = state.snapshot.mqtt.as_ref() else { return };
    if !mqtt.enabled {
        return;
    }
    ui.separator();
    if mqtt.proxy_to_client_enabled {
        render_mqtt_proxy(ui, state);
    } else {
        ui.colored_label(egui::Color32::GRAY, "mqtt direct")
            .on_hover_text(
                "Device is configured to connect to its MQTT broker directly via Wi-Fi / \
                 Ethernet. Actual broker connectivity is not reported over the phone API, \
                 so this is an intent indicator only.",
            );
    }
}

fn render_mqtt_proxy(ui: &mut egui::Ui, state: &AppState) {
    let recent = state
        .mqtt_last_proxy
        .and_then(|t| Instant::now().checked_duration_since(t))
        .is_some_and(|d| d.as_secs() < 120);
    if recent {
        ui.colored_label(egui::Color32::LIGHT_GREEN, "● mqtt via phone")
            .on_hover_text(
                "Device is forwarding MQTT traffic through this client. A proxy packet \
                 has been seen within the last 2 minutes.",
            );
    } else {
        ui.colored_label(egui::Color32::YELLOW, "○ mqtt via phone")
            .on_hover_text(
                "Device is configured for phone-proxy MQTT but no proxy traffic has been \
                 seen in the last 2 minutes — broker may be unreachable or nothing has \
                 needed forwarding yet.",
            );
    }
}

fn render_battery(ui: &mut egui::Ui, stats: &MeshStats) {
    let Some(level) = stats.battery_level else { return };
    ui.separator();
    let tooltip = stats.voltage_v.map_or_else(
        || "Device battery level. >100 means powered via USB/mains.".to_owned(),
        |v| format!("Battery level ({v:.2} V). >100 means powered via USB/mains."),
    );
    if level > 100 {
        ui.colored_label(egui::Color32::LIGHT_GREEN, "bat AC").on_hover_text(tooltip);
    } else {
        let color = if level < 20 {
            egui::Color32::LIGHT_RED
        } else if level < 40 {
            egui::Color32::YELLOW
        } else {
            egui::Color32::LIGHT_GREEN
        };
        ui.colored_label(color, format!("bat {level}%")).on_hover_text(tooltip);
    }
}

fn render_chutil(ui: &mut egui::Ui, stats: &MeshStats) {
    let Some(chutil) = stats.channel_utilization else { return };
    ui.separator();
    let color = if chutil > 40.0 {
        egui::Color32::LIGHT_RED
    } else if chutil > 25.0 {
        egui::Color32::YELLOW
    } else {
        egui::Color32::LIGHT_GREEN
    };
    ui.colored_label(color, format!("chutil {chutil:.1}%")).on_hover_text(
        "Channel utilization: TX + RX + noise on the current LoRa channel. Over ~40% is a congested mesh.",
    );
}

fn render_airtime(ui: &mut egui::Ui, stats: &MeshStats) {
    let Some(air) = stats.air_util_tx else { return };
    ui.separator();
    let color = if air > 5.0 {
        egui::Color32::LIGHT_RED
    } else if air > 2.0 {
        egui::Color32::YELLOW
    } else {
        egui::Color32::LIGHT_GREEN
    };
    ui.colored_label(color, format!("air {air:.1}%")).on_hover_text(
        "Airtime used for our own transmissions within the last hour. Keep under 5% to respect the duty cycle budget.",
    );
}

fn render_relay(ui: &mut egui::Ui, stats: &MeshStats) {
    let Some(n) = stats.num_tx_relay else { return };
    ui.separator();
    ui.label(format!("relay {n}")).on_hover_text(
        "Packets this node has relayed on behalf of others since boot (from LocalStats telemetry).",
    );
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
