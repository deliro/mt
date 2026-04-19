use std::cmp::Ordering;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::SystemTime;

use eframe::egui;
use walkers::Tiles as _;

use crate::domain::ids::NodeId;
use crate::domain::node::Node;
use crate::domain::snapshot::DeviceSnapshot;
use crate::ui::map_tiles::SqliteTiles;

#[derive(Default, Copy, Clone, Eq, PartialEq)]
pub enum ViewKind {
    #[default]
    Signal,
    Geographic,
}

pub struct TopologyUi {
    pub view: ViewKind,
    pub signal_pan: egui::Vec2,
    pub signal_zoom: f32,
    pub map_memory: walkers::MapMemory,
    pub map_tiles: Option<SqliteTiles>,
    /// Set to `true` once we've centred the map on `my_node`'s GPS
    /// position at least once — prevents repeatedly forcing the camera
    /// back there as the user pans.
    pub map_centered: bool,
}

impl Default for TopologyUi {
    fn default() -> Self {
        Self {
            view: ViewKind::Signal,
            signal_pan: egui::Vec2::ZERO,
            signal_zoom: 1.0,
            map_memory: walkers::MapMemory::default(),
            map_tiles: None,
            map_centered: false,
        }
    }
}

const NODE_R: f32 = 9.0;
const SELF_R: f32 = 12.0;
const HIT_R: f32 = 16.0;

pub fn render(
    ui: &mut egui::Ui,
    snapshot: &DeviceSnapshot,
    state: &mut TopologyUi,
    detail_node: &mut Option<NodeId>,
    tile_db_path: Option<PathBuf>,
) {
    toolbar(ui, state, snapshot);
    ui.separator();
    match state.view {
        ViewKind::Signal => render_signal(ui, snapshot, state, detail_node),
        ViewKind::Geographic => render_geographic(ui, snapshot, state, detail_node, tile_db_path),
    }
}

fn toolbar(ui: &mut egui::Ui, state: &mut TopologyUi, snapshot: &DeviceSnapshot) {
    ui.horizontal(|ui| {
        ui.selectable_value(&mut state.view, ViewKind::Signal, "Signal");
        ui.selectable_value(&mut state.view, ViewKind::Geographic, "Geographic");
        ui.separator();
        let total = snapshot.nodes.len();
        let with_gps = snapshot.nodes.values().filter(|n| n.position.is_some()).count();
        ui.weak(format!("{total} nodes · {with_gps} with GPS"));
        ui.separator();
        if ui.small_button("Reset view").clicked() {
            match state.view {
                ViewKind::Signal => {
                    state.signal_pan = egui::Vec2::ZERO;
                    state.signal_zoom = 1.0;
                }
                ViewKind::Geographic => {
                    state.map_memory = walkers::MapMemory::default();
                    state.map_centered = false;
                }
            }
        }
        match state.view {
            ViewKind::Signal => {
                ui.weak(format!("zoom {:.1}×", state.signal_zoom));
                ui.weak("· scroll to zoom · drag to pan");
            }
            ViewKind::Geographic => {
                ui.weak(format!("zoom z{:.0}", state.map_memory.zoom()));
                ui.weak("· ctrl+scroll to zoom · drag to pan");
            }
        }
    });
}

fn render_signal(
    ui: &mut egui::Ui,
    snapshot: &DeviceSnapshot,
    state: &mut TopologyUi,
    detail_node: &mut Option<NodeId>,
) {
    let available = ui.available_size();
    let (response, painter) = ui.allocate_painter(available, egui::Sense::click_and_drag());
    let rect = response.rect;
    painter.rect_filled(rect, 0.0, ui.style().visuals.extreme_bg_color);

    if response.dragged() {
        let delta = response.drag_delta();
        state.signal_pan = egui::vec2(
            state.signal_pan.x + delta.x,
            state.signal_pan.y + delta.y,
        );
    }
    if response.hovered() {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll.abs() > f32::EPSILON {
            let factor = (scroll * 0.005).exp();
            state.signal_zoom = (state.signal_zoom * factor).clamp(0.25, 20.0);
        }
    }

    let center = egui::pos2(
        rect.center().x + state.signal_pan.x,
        rect.center().y + state.signal_pan.y,
    );
    let rings = group_by_ring(snapshot);
    let total_rings = ring_count(&rings).max(1);
    let base_max_r = rect.width().min(rect.height()) * 0.45;
    let ring_step = base_max_r * state.signal_zoom / total_rings as f32;

    let ring_stroke =
        egui::Stroke::new(1.0, ui.style().visuals.weak_text_color().gamma_multiply(0.5));
    for idx in 1..=total_rings {
        let r = ring_step * idx as f32;
        painter.circle_stroke(center, r, ring_stroke);
    }

    let placements = layout_signal(&rings, center, ring_step);

    for placed in &placements {
        if !placed.direct_neighbor {
            continue;
        }
        let snr = snapshot.nodes.get(&placed.id).and_then(|n| n.snr_db);
        painter.line_segment([center, placed.pos], edge_stroke(snr));
    }

    draw_node(&painter, center, snapshot.my_node, snapshot, true, ui.style());
    for placed in &placements {
        draw_node(&painter, placed.pos, placed.id, snapshot, false, ui.style());
    }

    legend_signal(&painter, rect, ui.style(), &rings);

    let hit_points = collect_hit_points(snapshot.my_node, center, &placements);
    handle_interaction(ui, &response, &hit_points, snapshot, detail_node);
}

fn render_geographic(
    ui: &mut egui::Ui,
    snapshot: &DeviceSnapshot,
    state: &mut TopologyUi,
    detail_node: &mut Option<NodeId>,
    tile_db_path: Option<PathBuf>,
) {
    egui::SidePanel::right("topology_no_gps")
        .resizable(true)
        .default_width(220.0)
        .show_inside(ui, |ui| render_no_gps(ui, snapshot, detail_node));
    egui::CentralPanel::default().show_inside(ui, |ui| {
        render_geographic_plane(ui, snapshot, state, detail_node, tile_db_path);
    });
}

fn render_geographic_plane(
    ui: &mut egui::Ui,
    snapshot: &DeviceSnapshot,
    state: &mut TopologyUi,
    detail_node: &mut Option<NodeId>,
    tile_db_path: Option<PathBuf>,
) {
    if state.map_tiles.is_none() {
        state.map_tiles = Some(SqliteTiles::open(tile_db_path, ui.ctx().clone()));
    }

    let reference = reference_position(snapshot);
    if !state.map_centered
        && let Some(pos) = my_node_position(snapshot)
    {
        state.map_memory.center_at(pos);
        let _ = state.map_memory.set_zoom(12.0);
        state.map_centered = true;
    }

    let available = ui.available_rect_before_wrap();
    ui.painter()
        .rect_filled(available, 0.0, ui.style().visuals.extreme_bg_color);

    let overlay = NodesOverlay { snapshot, detail_node, reference };
    let tiles = state.map_tiles.as_mut().map(|t| t as &mut dyn walkers::Tiles);
    let map = walkers::Map::new(tiles, &mut state.map_memory, reference).with_plugin(overlay);
    let response = ui.add(map);

    draw_attribution(ui, response.rect, state.map_tiles.as_ref());
}

fn my_node_position(snapshot: &DeviceSnapshot) -> Option<walkers::Position> {
    snapshot
        .nodes
        .get(&snapshot.my_node)
        .and_then(|n| n.position.as_ref())
        .map(|p| walkers::Position::from_lat_lon(p.latitude_deg, p.longitude_deg))
}

/// Pick a sane reference point for scale-bar math and Map's `my_position`:
/// `my_node`'s GPS if we have it, else the centroid of GPS-bearing nodes,
/// else (0, 0).
fn reference_position(snapshot: &DeviceSnapshot) -> walkers::Position {
    if let Some(pos) = my_node_position(snapshot) {
        return pos;
    }
    let mut lat_sum = 0.0_f64;
    let mut lon_sum = 0.0_f64;
    let mut count = 0_u32;
    for n in snapshot.nodes.values() {
        if let Some(p) = &n.position {
            lat_sum += p.latitude_deg;
            lon_sum += p.longitude_deg;
            count = count.saturating_add(1);
        }
    }
    if count == 0 {
        walkers::Position::from_lat_lon(0.0, 0.0)
    } else {
        let denom = f64::from(count);
        walkers::Position::from_lat_lon(lat_sum / denom, lon_sum / denom)
    }
}

fn draw_attribution(
    ui: &egui::Ui,
    rect: egui::Rect,
    tiles: Option<&SqliteTiles>,
) {
    let Some(tiles) = tiles else { return };
    let attribution = tiles.attribution();
    let text = format!("© {}", attribution.text);
    let galley = ui.painter().layout_no_wrap(
        text,
        egui::FontId::proportional(10.0),
        ui.style().visuals.text_color(),
    );
    let pad_x = 6.0_f32;
    let pad_y = 3.0_f32;
    let gs = galley.size();
    let size = egui::vec2(pad_x.mul_add(2.0, gs.x), pad_y.mul_add(2.0, gs.y));
    let top_left = egui::pos2(rect.right() - size.x - 6.0, rect.bottom() - size.y - 6.0);
    let bg_rect = egui::Rect::from_min_size(top_left, size);
    ui.painter().rect_filled(
        bg_rect,
        3.0,
        ui.style().visuals.extreme_bg_color.gamma_multiply(0.85),
    );
    let text_origin = egui::pos2(top_left.x + pad_x, top_left.y + pad_y);
    ui.painter().galley(text_origin, galley, egui::Color32::WHITE);
}

struct NodesOverlay<'a> {
    snapshot: &'a DeviceSnapshot,
    detail_node: &'a mut Option<NodeId>,
    reference: walkers::Position,
}

impl walkers::Plugin for NodesOverlay<'_> {
    fn run(
        self: Box<Self>,
        ui: &mut egui::Ui,
        response: &egui::Response,
        projector: &walkers::Projector,
    ) {
        let Self { snapshot, detail_node, reference } = *self;
        let mut placements: Vec<(NodeId, egui::Pos2)> = Vec::new();
        for (id, n) in &snapshot.nodes {
            let Some(p) = &n.position else { continue };
            let vec = projector
                .project(walkers::Position::from_lat_lon(p.latitude_deg, p.longitude_deg));
            placements.push((*id, vec.to_pos2()));
        }

        let self_pos = placements
            .iter()
            .find(|(id, _)| *id == snapshot.my_node)
            .map(|(_, pos)| *pos);
        let painter = ui.painter().clone();

        if let Some(origin) = self_pos {
            for (id, pos) in &placements {
                if *id == snapshot.my_node {
                    continue;
                }
                let hops = snapshot.nodes.get(id).and_then(|n| n.hops_away);
                if hops != Some(0) {
                    continue;
                }
                let snr = snapshot.nodes.get(id).and_then(|n| n.snr_db);
                painter.line_segment([origin, *pos], edge_stroke(snr));
            }
        }

        for (id, pos) in &placements {
            let is_self = *id == snapshot.my_node;
            draw_node(&painter, *pos, *id, snapshot, is_self, ui.style());
        }

        let scale_px_per_m = projector.scale_pixel_per_meter(reference);
        if scale_px_per_m > 0.0 && scale_px_per_m.is_finite() {
            draw_scale_bar(
                &painter,
                response.rect,
                f64::from(1.0 / scale_px_per_m),
                ui.style(),
            );
        }

        handle_interaction(ui, response, &placements, snapshot, detail_node);
    }
}

fn render_no_gps(
    ui: &mut egui::Ui,
    snapshot: &DeviceSnapshot,
    detail_node: &mut Option<NodeId>,
) {
    ui.heading("No GPS");
    let mut list: Vec<&Node> = snapshot
        .nodes
        .values()
        .filter(|n| n.position.is_none() && n.id != snapshot.my_node)
        .collect();
    list.sort_by_key(|n| display_name(n).to_lowercase());
    ui.weak(format!("{} node(s)", list.len()));
    ui.separator();
    egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
        if list.is_empty() {
            ui.weak("All heard nodes have a GPS fix.");
            return;
        }
        for n in list {
            let label = n.hops_away.map_or_else(
                || format!("{} · hops=?", display_name(n)),
                |h| format!("{} · hops={h}", display_name(n)),
            );
            if ui.link(label).clicked() {
                *detail_node = Some(n.id);
            }
        }
    });
}

// ————— pure helpers —————

/// Per-hop ring buckets: `known[h]` = nodes with `hops_away = h`. `my_node` excluded.
/// `unknown` collects nodes whose `hops_away` is `None`.
struct RingGrouping {
    known: Vec<Vec<NodeId>>,
    unknown: Vec<NodeId>,
}

fn group_by_ring(snapshot: &DeviceSnapshot) -> RingGrouping {
    let mut max_hops: Option<u8> = None;
    let mut unknown: Vec<NodeId> = Vec::new();
    let mut by_hops: HashMap<u8, Vec<NodeId>> = HashMap::new();
    for (id, n) in &snapshot.nodes {
        if *id == snapshot.my_node {
            continue;
        }
        match n.hops_away {
            Some(h) => {
                max_hops = Some(max_hops.map_or(h, |m| m.max(h)));
                by_hops.entry(h).or_default().push(*id);
            }
            None => unknown.push(*id),
        }
    }
    let known = max_hops.map_or_else(Vec::new, |max_h| {
        let len = usize::from(max_h).saturating_add(1);
        let mut rings: Vec<Vec<NodeId>> = vec![Vec::new(); len];
        for (h, ids) in by_hops {
            if let Some(slot) = rings.get_mut(usize::from(h)) {
                *slot = ids;
            }
        }
        for ring in &mut rings {
            ring.sort_by_key(|id| id.0);
        }
        rings
    });
    unknown.sort_by_key(|id| id.0);
    RingGrouping { known, unknown }
}

fn ring_count(rings: &RingGrouping) -> usize {
    let unknown_extra = usize::from(!rings.unknown.is_empty());
    rings.known.len().saturating_add(unknown_extra)
}

struct SignalPlacement {
    id: NodeId,
    pos: egui::Pos2,
    direct_neighbor: bool,
}

fn layout_signal(
    rings: &RingGrouping,
    center: egui::Pos2,
    ring_step: f32,
) -> Vec<SignalPlacement> {
    let mut out = Vec::new();
    for (hops_idx, ids) in rings.known.iter().enumerate() {
        if ids.is_empty() {
            continue;
        }
        place_ring(&mut out, ids, hops_idx, center, ring_step, hops_idx == 0);
    }
    if !rings.unknown.is_empty() {
        let ring_idx = rings.known.len();
        place_ring(&mut out, &rings.unknown, ring_idx, center, ring_step, false);
    }
    out
}

fn place_ring(
    out: &mut Vec<SignalPlacement>,
    ids: &[NodeId],
    ring_idx: usize,
    center: egui::Pos2,
    ring_step: f32,
    direct_neighbor: bool,
) {
    let count = (ids.len().max(1)) as f32;
    let radius = ring_step * (ring_idx as f32 + 1.0);
    let phase = if ring_idx.is_multiple_of(2) { 0.0 } else { std::f32::consts::PI / count };
    for (i, id) in ids.iter().enumerate() {
        let theta = (i as f32 / count).mul_add(std::f32::consts::TAU, phase);
        let pos = egui::pos2(
            center.x + radius * theta.cos(),
            center.y + radius * theta.sin(),
        );
        out.push(SignalPlacement { id: *id, pos, direct_neighbor });
    }
}

fn collect_hit_points(
    my_node: NodeId,
    center: egui::Pos2,
    placements: &[SignalPlacement],
) -> Vec<(NodeId, egui::Pos2)> {
    let mut out = Vec::with_capacity(placements.len().saturating_add(1));
    out.push((my_node, center));
    for p in placements {
        out.push((p.id, p.pos));
    }
    out
}

fn draw_scale_bar(painter: &egui::Painter, rect: egui::Rect, m_per_px: f64, style: &egui::Style) {
    if m_per_px <= 0.0 || !m_per_px.is_finite() {
        return;
    }
    let target_px = 100.0_f64;
    let target_m = target_px * m_per_px;
    let nice_m = nice_scale_meters(target_m);
    let bar_px = (nice_m / m_per_px) as f32;
    let bar_y = rect.bottom() - 18.0;
    let bar_x0 = rect.left() + 16.0;
    let bar_x1 = bar_x0 + bar_px;
    let color = style.visuals.text_color();
    painter.line_segment(
        [egui::pos2(bar_x0, bar_y), egui::pos2(bar_x1, bar_y)],
        egui::Stroke::new(2.0, color),
    );
    painter.line_segment(
        [egui::pos2(bar_x0, bar_y - 4.0), egui::pos2(bar_x0, bar_y + 4.0)],
        egui::Stroke::new(2.0, color),
    );
    painter.line_segment(
        [egui::pos2(bar_x1, bar_y - 4.0), egui::pos2(bar_x1, bar_y + 4.0)],
        egui::Stroke::new(2.0, color),
    );
    painter.text(
        egui::pos2((bar_x0 + bar_x1) * 0.5, bar_y - 6.0),
        egui::Align2::CENTER_BOTTOM,
        format_distance(nice_m),
        egui::FontId::monospace(11.0),
        color,
    );
}

fn nice_scale_meters(target: f64) -> f64 {
    if !target.is_finite() || target <= 0.0 {
        return 1.0;
    }
    let exp = target.log10().floor();
    let base = 10_f64.powf(exp);
    let mantissa = target / base;
    let nice = if mantissa < 1.5 {
        1.0
    } else if mantissa < 3.5 {
        2.0
    } else if mantissa < 7.5 {
        5.0
    } else {
        10.0
    };
    nice * base
}

fn format_distance(m: f64) -> String {
    if m >= 1_000.0 {
        format!("{:.1} km", m / 1_000.0)
    } else {
        format!("{m:.0} m")
    }
}

fn draw_node(
    painter: &egui::Painter,
    pos: egui::Pos2,
    id: NodeId,
    snapshot: &DeviceSnapshot,
    is_self: bool,
    style: &egui::Style,
) {
    let node = snapshot.nodes.get(&id);
    let fill = node_color(is_self, node.and_then(|n| n.hops_away));
    let r = if is_self { SELF_R } else { NODE_R };
    painter.circle_filled(pos, r, fill);
    painter.circle_stroke(pos, r, egui::Stroke::new(1.0, style.visuals.text_color()));
    let label = node.map_or_else(|| format!("!{:08x}", id.0), short_label);
    painter.text(
        egui::pos2(pos.x, pos.y + r + 10.0),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::monospace(11.0),
        style.visuals.text_color(),
    );
}

fn legend_signal(
    painter: &egui::Painter,
    rect: egui::Rect,
    style: &egui::Style,
    rings: &RingGrouping,
) {
    let color = style.visuals.weak_text_color();
    let mut lines: Vec<String> = Vec::new();
    for (hops_idx, ids) in rings.known.iter().enumerate() {
        if ids.is_empty() {
            continue;
        }
        let ring_num = hops_idx.saturating_add(1);
        lines.push(format!("ring {ring_num}: hops={hops_idx} · {} node(s)", ids.len()));
    }
    if !rings.unknown.is_empty() {
        let ring_num = rings.known.len().saturating_add(1);
        lines.push(format!(
            "ring {ring_num}: unknown · {} node(s)",
            rings.unknown.len()
        ));
    }
    if lines.is_empty() {
        return;
    }
    for (i, line) in lines.iter().enumerate() {
        painter.text(
            egui::pos2(rect.left() + 12.0, (i as f32).mul_add(14.0, rect.top() + 10.0)),
            egui::Align2::LEFT_TOP,
            line,
            egui::FontId::monospace(10.0),
            color,
        );
    }
}

fn handle_interaction(
    ui: &egui::Ui,
    response: &egui::Response,
    placements: &[(NodeId, egui::Pos2)],
    snapshot: &DeviceSnapshot,
    detail_node: &mut Option<NodeId>,
) {
    let Some(pointer) = response.hover_pos() else { return };
    let closest = placements
        .iter()
        .map(|(id, pos)| (*id, *pos, pos.distance(pointer)))
        .filter(|(_, _, d)| *d <= HIT_R)
        .min_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(Ordering::Equal));
    let Some((id, _, _)) = closest else { return };
    egui::show_tooltip_at_pointer(
        ui.ctx(),
        response.layer_id,
        egui::Id::new("topology_tooltip"),
        |ui| render_tooltip(ui, id, snapshot),
    );
    if response.clicked() {
        *detail_node = Some(id);
    }
}

fn render_tooltip(ui: &mut egui::Ui, id: NodeId, snapshot: &DeviceSnapshot) {
    let Some(node) = snapshot.nodes.get(&id) else {
        ui.label(format!("!{:08x}", id.0));
        return;
    };
    ui.label(egui::RichText::new(display_name(node)).strong());
    if !node.short_name.is_empty() && !node.long_name.is_empty() {
        ui.weak(&node.short_name);
    }
    if id == snapshot.my_node {
        ui.weak("(this node)");
    }
    ui.separator();
    if let Some(h) = node.hops_away {
        ui.label(format!("Hops: {h}"));
    }
    if let Some(snr) = node.snr_db {
        ui.label(format!("SNR: {snr:.1} dB"));
    }
    if let Some(rssi) = node.rssi_dbm {
        ui.label(format!("RSSI: {rssi} dBm"));
    }
    if let Some(bat) = node.battery_level {
        ui.label(format!("Battery: {bat}%"));
    }
    if let Some(p) = &node.position {
        ui.monospace(format!("{:.5}, {:.5}", p.latitude_deg, p.longitude_deg));
        if let Some(alt) = p.altitude_m {
            ui.weak(format!("alt {alt} m"));
        }
    }
    if let Some(t) = node.last_heard
        && let Ok(d) = SystemTime::now().duration_since(t)
    {
        ui.weak(format!("last heard {}", human_ago(d.as_secs())));
    }
    ui.separator();
    ui.weak("Click for details");
}

fn node_color(is_self: bool, hops: Option<u8>) -> egui::Color32 {
    if is_self {
        return egui::Color32::from_rgb(80, 160, 230);
    }
    match hops {
        Some(0) => egui::Color32::from_rgb(100, 200, 120),
        Some(1) => egui::Color32::from_rgb(200, 200, 100),
        Some(_) => egui::Color32::from_rgb(200, 120, 100),
        None => egui::Color32::from_rgb(130, 130, 130),
    }
}

fn edge_stroke(snr: Option<f32>) -> egui::Stroke {
    let color = match snr {
        Some(s) if s >= 5.0 => egui::Color32::from_rgb(80, 200, 80),
        Some(s) if s >= 0.0 => egui::Color32::from_rgb(210, 210, 80),
        Some(_) => egui::Color32::from_rgb(210, 110, 80),
        None => egui::Color32::from_rgb(120, 120, 120),
    };
    egui::Stroke::new(1.5, color)
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

fn short_label(node: &Node) -> String {
    if !node.short_name.is_empty() {
        node.short_name.clone()
    } else if !node.long_name.is_empty() {
        node.long_name.clone()
    } else {
        format!("!{:08x}", node.id.0)
    }
}

fn human_ago(secs: u64) -> String {
    if secs < 5 {
        "just now".into()
    } else if secs < 60 {
        format!("{secs}s ago")
    } else if secs < 3_600 {
        format!("{}m ago", secs.div_euclid(60))
    } else if secs < 86_400 {
        format!("{}h ago", secs.div_euclid(3_600))
    } else {
        format!("{}d ago", secs.div_euclid(86_400))
    }
}
