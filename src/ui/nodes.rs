use eframe::egui;
use egui_extras::{Column, TableBuilder};

use crate::domain::node::Node;
use crate::ui::AppState;

pub fn render(ui: &mut egui::Ui, state: &AppState) {
    let mut nodes: Vec<&Node> = state.snapshot.nodes.values().collect();
    nodes.sort_by(|a, b| a.long_name.cmp(&b.long_name));

    TableBuilder::new(ui)
        .striped(true)
        .column(Column::auto().resizable(true))
        .column(Column::auto())
        .column(Column::auto())
        .column(Column::auto())
        .column(Column::auto())
        .column(Column::auto())
        .column(Column::remainder())
        .header(20.0, |mut header| {
            for h in ["Long", "Short", "Role", "Bat", "SNR", "Hops", "Position"] {
                header.col(|ui| {
                    ui.strong(h);
                });
            }
        })
        .body(|mut body| {
            for node in nodes {
                body.row(18.0, |mut row| {
                    row.col(|ui| {
                        ui.label(&node.long_name);
                    });
                    row.col(|ui| {
                        ui.label(&node.short_name);
                    });
                    row.col(|ui| {
                        ui.label(format!("{:?}", node.role));
                    });
                    row.col(|ui| {
                        ui.label(
                            node.battery_level.map_or_else(|| "—".into(), |b| format!("{b}%")),
                        );
                    });
                    row.col(|ui| {
                        ui.label(
                            node.snr_db.map_or_else(|| "—".into(), |s| format!("{s:.1}")),
                        );
                    });
                    row.col(|ui| {
                        ui.label(node.hops_away.map_or_else(|| "—".into(), |h| format!("{h}")));
                    });
                    row.col(|ui| {
                        let pos = node.position.as_ref().map_or_else(
                            || "—".into(),
                            |p| format!("{:.4}, {:.4}", p.latitude_deg, p.longitude_deg),
                        );
                        ui.label(pos);
                    });
                });
            }
        });
}
