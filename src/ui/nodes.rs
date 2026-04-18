use std::collections::HashMap;
use std::time::{Duration, Instant, SystemTime};

use eframe::egui;
use egui_extras::{Column, TableBuilder};

use crate::domain::ids::NodeId;
use crate::domain::node::Node;
use crate::domain::snapshot::DeviceSnapshot;

const FLASH_DURATION: Duration = Duration::from_millis(1500);
const ONLINE_THRESHOLD: Duration = Duration::from_secs(2 * 60 * 60);

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
    pub search: String,
    pub recently_updated: HashMap<NodeId, Instant>,
    pub seen_live: std::collections::HashSet<NodeId>,
    pub persisted_saved_at: HashMap<NodeId, SystemTime>,
}

impl NodesUi {
    pub fn mark_updated(&mut self, id: NodeId) {
        let _ = self.recently_updated.insert(id, Instant::now());
    }

    fn flash_alpha(&self, id: NodeId, now: Instant) -> f32 {
        self.recently_updated
            .get(&id)
            .and_then(|t| now.checked_duration_since(*t))
            .filter(|d| *d < FLASH_DURATION)
            .map_or(0.0, |d| 1.0 - d.as_secs_f32() / FLASH_DURATION.as_secs_f32())
    }

    fn any_flashing(&self, now: Instant) -> bool {
        self.recently_updated
            .values()
            .any(|t| now.checked_duration_since(*t).is_some_and(|d| d < FLASH_DURATION))
    }
}

pub fn render(
    ui: &mut egui::Ui,
    snapshot: &DeviceSnapshot,
    nodes_ui: &mut NodesUi,
    detail_node: &mut Option<NodeId>,
    focus_search: &mut bool,
) {
    let now_system = SystemTime::now();
    let now_inst = Instant::now();

    let mut nodes: Vec<&Node> = filtered_nodes(snapshot, &nodes_ui.search);
    let counts = NodeCounts::compute(snapshot, nodes_ui, now_system);
    sort_nodes(&mut nodes, nodes_ui.sort, nodes_ui.ascending);
    nodes.sort_by_key(|n| !n.is_favorite);

    if nodes_ui.any_flashing(now_inst) {
        ui.ctx().request_repaint_after(Duration::from_millis(16));
    }

    toolbar(ui, nodes_ui, nodes.len(), counts, focus_search);
    ui.separator();
    table(ui, &nodes, nodes_ui, detail_node, now_inst, now_system);
}

fn filtered_nodes<'a>(snapshot: &'a DeviceSnapshot, query: &str) -> Vec<&'a Node> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return snapshot.nodes.values().collect();
    }
    snapshot
        .nodes
        .values()
        .filter(|n| {
            n.long_name.to_lowercase().contains(&q)
                || n.short_name.to_lowercase().contains(&q)
                || format!("{:08x}", n.id.0).contains(&q)
        })
        .collect()
}

#[derive(Copy, Clone, Debug)]
struct NodeCounts {
    total: usize,
    online: usize,
    idle: usize,
    archived: usize,
}

impl NodeCounts {
    fn compute(snapshot: &DeviceSnapshot, nodes_ui: &NodesUi, now: SystemTime) -> Self {
        let total = snapshot.nodes.len();
        let mut online: usize = 0;
        let mut idle: usize = 0;
        let mut archived: usize = 0;
        for (id, node) in &snapshot.nodes {
            if !nodes_ui.seen_live.contains(id) {
                archived = archived.saturating_add(1);
                continue;
            }
            let fresh = node
                .last_heard
                .and_then(|t| now.duration_since(t).ok())
                .is_some_and(|d| d <= ONLINE_THRESHOLD);
            if fresh {
                online = online.saturating_add(1);
            } else {
                idle = idle.saturating_add(1);
            }
        }
        Self { total, online, idle, archived }
    }
}

fn toolbar(
    ui: &mut egui::Ui,
    nodes_ui: &mut NodesUi,
    shown: usize,
    counts: NodeCounts,
    focus_search: &mut bool,
) {
    ui.horizontal(|ui| {
        if shown == counts.total {
            ui.label(format!("{} nodes", counts.total));
        } else {
            ui.label(format!("{shown}/{}", counts.total));
        }
        ui.colored_label(
            egui::Color32::from_rgb(120, 200, 120),
            format!("● {} online", counts.online),
        )
        .on_hover_text(
            "In the device's NodeDB and heard on the mesh within the last 2 hours.",
        );
        ui.colored_label(
            egui::Color32::from_rgb(170, 170, 120),
            format!("◐ {} idle", counts.idle),
        )
        .on_hover_text(
            "In the device's NodeDB but not heard in the last 2 hours — they're still \
             tracked by the radio, just quiet.",
        );
        ui.colored_label(egui::Color32::GRAY, format!("○ {} archived", counts.archived))
            .on_hover_text(
                "Only in the local database; the device's NodeDB has dropped them. \
                 Kept locally so chat history and display names survive.",
            );
        ui.separator();
        ui.label("Search:");
        let resp = ui.add(
            egui::TextEdit::singleline(&mut nodes_ui.search)
                .hint_text("name or id (⌘K)")
                .desired_width(180.0),
        );
        if *focus_search {
            resp.request_focus();
            *focus_search = false;
        }
        if !nodes_ui.search.is_empty() && resp.has_focus() && ui.small_button("clear").clicked() {
            nodes_ui.search.clear();
        }
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
        ui.label(if nodes_ui.ascending { "asc" } else { "desc" });
    });
}

fn table(
    ui: &mut egui::Ui,
    nodes: &[&Node],
    nodes_ui: &NodesUi,
    detail_node: &mut Option<NodeId>,
    now_inst: Instant,
    now_system: SystemTime,
) {
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
                let alpha = nodes_ui.flash_alpha(node.id, now_inst);
                let flash = (alpha > 0.0).then(|| flash_color(alpha));
                let is_cached = !nodes_ui.seen_live.contains(&node.id);
                let cached_saved_at = nodes_ui.persisted_saved_at.get(&node.id).copied();
                let mut ctx = RowContext {
                    flash,
                    detail_node,
                    now_system,
                    is_cached,
                    cached_saved_at,
                };
                body.row(18.0, |row| row_cells(row, node, &mut ctx));
            }
        });
}

struct RowContext<'a> {
    flash: Option<egui::Color32>,
    detail_node: &'a mut Option<NodeId>,
    now_system: SystemTime,
    is_cached: bool,
    cached_saved_at: Option<SystemTime>,
}

fn row_cells(row: egui_extras::TableRow<'_, '_>, node: &Node, ctx: &mut RowContext<'_>) {
    row_cells_inner(row, node, ctx);
}

fn row_cells_inner(
    mut row: egui_extras::TableRow<'_, '_>,
    node: &Node,
    ctx: &mut RowContext<'_>,
) {
    let flash = ctx.flash;
    row.col(|ui| {
        paint_flash(ui, flash);
        let raw_name = display_name(node);
        let display = if node.is_favorite { format!("★ {raw_name}") } else { raw_name };
        let mut text = egui::RichText::new(display);
        if ctx.is_cached {
            text = text.weak();
        }
        if node.is_ignored {
            text = text.strikethrough();
        }
        let resp = ui.add(
            egui::Label::new(text)
                .truncate()
                .sense(egui::Sense::click()),
        );
        if resp.clicked() {
            *ctx.detail_node = Some(node.id);
        }
        resp.on_hover_text(display_name(node));
    });
    truncated_cell(&mut row, flash, node.short_name.clone());
    truncated_cell(&mut row, flash, format!("{:?}", node.role));
    truncated_cell(
        &mut row,
        flash,
        node.battery_level.map_or_else(|| "—".into(), |b| format!("{b}%")),
    );
    truncated_cell(
        &mut row,
        flash,
        node.snr_db.map_or_else(|| "—".into(), |s| format!("{s:.1}")),
    );
    truncated_cell(
        &mut row,
        flash,
        node.hops_away.map_or_else(|| "—".into(), |h| format!("{h}")),
    );
    row.col(|ui| {
        paint_flash(ui, flash);
        if ctx.is_cached {
            let primary = format_last_heard(node.last_heard, ctx.now_system);
            let cached_age = format_cached_age(ctx.cached_saved_at, ctx.now_system);
            let label = format!("{primary} (cached {cached_age})");
            ui.add(egui::Label::new(egui::RichText::new(label).color(egui::Color32::GRAY)).truncate());
        } else {
            ui.add(
                egui::Label::new(format_last_heard(node.last_heard, ctx.now_system)).truncate(),
            );
        }
    });
    row.col(|ui| {
        paint_flash(ui, flash);
        let pos = node.position.as_ref().map_or_else(
            || "—".into(),
            |p| format!("{:.4}, {:.4}", p.latitude_deg, p.longitude_deg),
        );
        ui.add(egui::Label::new(pos).truncate());
    });
}

fn truncated_cell(row: &mut egui_extras::TableRow<'_, '_>, flash: Option<egui::Color32>, text: String) {
    row.col(|ui| {
        paint_flash(ui, flash);
        ui.add(egui::Label::new(text).truncate());
    });
}

fn format_cached_age(saved_at: Option<SystemTime>, now: SystemTime) -> String {
    let Some(t) = saved_at else { return "—".into() };
    let Ok(d) = now.duration_since(t) else { return "just now".into() };
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

fn flash_color(alpha: f32) -> egui::Color32 {
    let a = (alpha * 140.0).clamp(0.0, 255.0) as u8;
    egui::Color32::from_rgba_unmultiplied(90, 170, 240, a)
}

fn paint_flash(ui: &egui::Ui, color: Option<egui::Color32>) {
    if let Some(color) = color {
        let rect = ui.max_rect();
        ui.painter().rect_filled(rect, 0.0, color);
    }
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
