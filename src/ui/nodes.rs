use std::time::SystemTime;

use eframe::egui;
use egui_extras::{Column, TableBuilder};

use crate::domain::node::Node;
use crate::domain::snapshot::DeviceSnapshot;

#[derive(Copy, Clone, Debug, Eq, PartialEq, Default)]
pub enum NodesSort {
    #[default]
    LastHeard,
    LongName,
    ShortName,
    Battery,
    Snr,
    Hops,
}

impl NodesSort {
    const fn label(self) -> &'static str {
        match self {
            Self::LastHeard => "Heard",
            Self::LongName => "Long",
            Self::ShortName => "Short",
            Self::Battery => "Bat",
            Self::Snr => "SNR",
            Self::Hops => "Hops",
        }
    }
}

#[derive(Default)]
pub struct NodesUi {
    pub sort: NodesSort,
    pub ascending: bool,
}

pub fn render(ui: &mut egui::Ui, snapshot: &DeviceSnapshot, nodes_ui: &mut NodesUi) {
    let now = SystemTime::now();
    let mut nodes: Vec<&Node> = snapshot.nodes.values().collect();
    sort_nodes(&mut nodes, nodes_ui.sort, nodes_ui.ascending);

    ui.horizontal(|ui| {
        ui.label(format!("{} nodes", nodes.len()));
        ui.separator();
        ui.label("Sort:");
        for s in [
            NodesSort::LastHeard,
            NodesSort::LongName,
            NodesSort::ShortName,
            NodesSort::Battery,
            NodesSort::Snr,
            NodesSort::Hops,
        ] {
            if ui.selectable_label(nodes_ui.sort == s, s.label()).clicked() {
                if nodes_ui.sort == s {
                    nodes_ui.ascending = !nodes_ui.ascending;
                } else {
                    nodes_ui.sort = s;
                    nodes_ui.ascending = matches!(s, NodesSort::LongName | NodesSort::ShortName);
                }
            }
        }
        ui.label(if nodes_ui.ascending { "▲" } else { "▼" });
    });
    ui.separator();

    TableBuilder::new(ui)
        .striped(true)
        .column(Column::auto().resizable(true))
        .column(Column::auto())
        .column(Column::auto())
        .column(Column::auto())
        .column(Column::auto())
        .column(Column::auto())
        .column(Column::auto())
        .column(Column::remainder())
        .header(20.0, |mut header| {
            for h in ["Long", "Short", "Role", "Bat", "SNR", "Hops", "Heard", "Position"] {
                header.col(|ui| {
                    ui.strong(h);
                });
            }
        })
        .body(|mut body| {
            for node in nodes {
                body.row(18.0, |mut row| {
                    row.col(|ui| {
                        ui.label(display_name(node));
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
                        ui.label(node.snr_db.map_or_else(|| "—".into(), |s| format!("{s:.1}")));
                    });
                    row.col(|ui| {
                        ui.label(node.hops_away.map_or_else(|| "—".into(), |h| format!("{h}")));
                    });
                    row.col(|ui| {
                        ui.label(format_last_heard(node.last_heard, now));
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

fn display_name(node: &Node) -> String {
    if !node.long_name.is_empty() {
        node.long_name.clone()
    } else if !node.short_name.is_empty() {
        node.short_name.clone()
    } else {
        format!("!{:08x}", node.id.0)
    }
}

fn sort_nodes(nodes: &mut [&Node], by: NodesSort, ascending: bool) {
    nodes.sort_by(|a, b| match by {
        NodesSort::LastHeard => a.last_heard.cmp(&b.last_heard),
        NodesSort::LongName => a.long_name.cmp(&b.long_name),
        NodesSort::ShortName => a.short_name.cmp(&b.short_name),
        NodesSort::Battery => a.battery_level.cmp(&b.battery_level),
        NodesSort::Snr => a.snr_db.partial_cmp(&b.snr_db).unwrap_or(std::cmp::Ordering::Equal),
        NodesSort::Hops => a.hops_away.cmp(&b.hops_away),
    });
    if !ascending {
        nodes.reverse();
    }
}

fn format_last_heard(last_heard: Option<SystemTime>, now: SystemTime) -> String {
    let Some(t) = last_heard else { return "—".into() };
    let Ok(d) = now.duration_since(t) else { return "—".into() };
    let secs = d.as_secs();
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
