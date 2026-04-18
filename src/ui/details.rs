use std::time::SystemTime;

use eframe::egui;

use crate::domain::ids::NodeId;
use crate::domain::node::Node;
use crate::domain::snapshot::DeviceSnapshot;

pub fn render_overlay(
    ctx: &egui::Context,
    snapshot: &DeviceSnapshot,
    detail_node: &mut Option<NodeId>,
) {
    let Some(id) = *detail_node else { return };
    let mut open = true;
    let title = snapshot
        .nodes
        .get(&id)
        .map_or_else(|| format!("!{:08x}", id.0), display_name);
    egui::Window::new(title).open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
        match snapshot.nodes.get(&id) {
            Some(node) => render_body(ui, node),
            None => {
                ui.label(format!("No data yet for !{:08x}", id.0));
            }
        }
    });
    if !open {
        *detail_node = None;
    }
}

fn render_body(ui: &mut egui::Ui, node: &Node) {
    egui::Grid::new("node_detail_grid")
        .num_columns(2)
        .striped(true)
        .spacing([24.0, 4.0])
        .show(ui, |ui| {
            row(ui, "ID", format!("!{:08x}  ({})", node.id.0, node.id.0));
            row(ui, "Long name", non_empty_or(&node.long_name, "—"));
            row(ui, "Short name", non_empty_or(&node.short_name, "—"));
            row(ui, "Role", format!("{:?}", node.role));
            row(ui, "Battery", node.battery_level.map_or_else(|| "—".to_owned(), |b| format!("{b}%")));
            row(ui, "Voltage", node.voltage_v.map_or_else(|| "—".to_owned(), |v| format!("{v:.2} V")));
            row(ui, "SNR", node.snr_db.map_or_else(|| "—".to_owned(), |s| format!("{s:.1} dB")));
            row(ui, "RSSI", node.rssi_dbm.map_or_else(|| "—".to_owned(), |r| format!("{r} dBm")));
            row(ui, "Hops away", node.hops_away.map_or_else(|| "—".to_owned(), |h| h.to_string()));
            row(ui, "Last heard", format_last_heard(node.last_heard));
            if let Some(pos) = &node.position {
                row(
                    ui,
                    "Latitude",
                    format!("{:.6}°", pos.latitude_deg),
                );
                row(
                    ui,
                    "Longitude",
                    format!("{:.6}°", pos.longitude_deg),
                );
                row(
                    ui,
                    "Altitude",
                    pos.altitude_m
                        .map_or_else(|| "—".to_owned(), |a| format!("{a} m")),
                );
            } else {
                row(ui, "Position", "—".to_owned());
            }
        });
}

fn row(ui: &mut egui::Ui, label: &str, value: impl Into<String>) {
    ui.label(label);
    ui.monospace(value.into());
    ui.end_row();
}

fn display_name(node: &Node) -> String {
    if !node.long_name.is_empty() {
        node.long_name.clone()
    } else if !node.short_name.is_empty() {
        node.short_name.clone()
    } else {
        format!("!{:08x}", node.id.0)
    }
}

fn non_empty_or(s: &str, fallback: &str) -> String {
    if s.is_empty() { fallback.into() } else { s.into() }
}

fn format_last_heard(last_heard: Option<SystemTime>) -> String {
    let Some(t) = last_heard else { return "—".to_owned() };
    let Ok(d) = SystemTime::now().duration_since(t) else { return "—".to_owned() };
    let secs = d.as_secs();
    if secs < 60 {
        format!("{secs}s ago")
    } else if secs < 3_600 {
        format!("{}m ago", secs.div_euclid(60))
    } else if secs < 86_400 {
        format!("{}h ago", secs.div_euclid(3_600))
    } else {
        format!("{}d ago", secs.div_euclid(86_400))
    }
}
