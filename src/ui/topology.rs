use std::cmp::Ordering;
use std::time::SystemTime;

use eframe::egui;

use crate::domain::ids::NodeId;
use crate::domain::node::Node;
use crate::domain::snapshot::DeviceSnapshot;

#[derive(Default, Copy, Clone, Eq, PartialEq)]
pub enum ViewKind {
    #[default]
    Signal,
    Geographic,
}

pub struct TopologyUi {
    pub view: ViewKind,
    pub geo_pan: egui::Vec2,
    pub geo_zoom: f32,
}

impl Default for TopologyUi {
    fn default() -> Self {
        Self { view: ViewKind::Signal, geo_pan: egui::Vec2::ZERO, geo_zoom: 1.0 }
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
) {
    toolbar(ui, state, snapshot);
    ui.separator();
    match state.view {
        ViewKind::Signal => render_signal(ui, snapshot, detail_node),
        ViewKind::Geographic => render_geographic(ui, snapshot, state, detail_node),
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
        if state.view == ViewKind::Geographic {
            ui.separator();
            if ui.small_button("Reset view").clicked() {
                state.geo_pan = egui::Vec2::ZERO;
                state.geo_zoom = 1.0;
            }
            ui.weak(format!("zoom {:.1}×", state.geo_zoom));
        }
    });
}

fn render_signal(
    ui: &mut egui::Ui,
    snapshot: &DeviceSnapshot,
    detail_node: &mut Option<NodeId>,
) {
    let available = ui.available_size();
    let (response, painter) = ui.allocate_painter(available, egui::Sense::click());
    let rect = response.rect;
    painter.rect_filled(rect, 0.0, ui.style().visuals.extreme_bg_color);

    let center = rect.center();
    let max_r = rect.width().min(rect.height()) * 0.45;
    let ring_step = max_r / 4.0;

    let rings = group_by_ring(snapshot);
    let ring_stroke = egui::Stroke::new(1.0, ui.style().visuals.weak_text_color().gamma_multiply(0.5));
    for idx in 1_u32..=4 {
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

    legend_signal(&painter, rect, ui.style());

    let hit_points = collect_hit_points(snapshot.my_node, center, &placements);
    handle_interaction(ui, &response, &hit_points, snapshot, detail_node);
}

fn render_geographic(
    ui: &mut egui::Ui,
    snapshot: &DeviceSnapshot,
    state: &mut TopologyUi,
    detail_node: &mut Option<NodeId>,
) {
    egui::SidePanel::right("topology_no_gps")
        .resizable(true)
        .default_width(220.0)
        .show_inside(ui, |ui| render_no_gps(ui, snapshot, detail_node));
    egui::CentralPanel::default().show_inside(ui, |ui| {
        render_geographic_plane(ui, snapshot, state, detail_node);
    });
}

fn render_geographic_plane(
    ui: &mut egui::Ui,
    snapshot: &DeviceSnapshot,
    state: &mut TopologyUi,
    detail_node: &mut Option<NodeId>,
) {
    let available = ui.available_size();
    let (response, painter) = ui.allocate_painter(available, egui::Sense::click_and_drag());
    let rect = response.rect;
    painter.rect_filled(rect, 0.0, ui.style().visuals.extreme_bg_color);

    let gps_nodes: Vec<(NodeId, &Node)> = snapshot
        .nodes
        .iter()
        .filter(|(_, n)| n.position.is_some())
        .map(|(id, n)| (*id, n))
        .collect();
    if gps_nodes.is_empty() {
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "No nodes with GPS yet.\nA node must broadcast Position to appear here.",
            egui::FontId::proportional(13.0),
            ui.style().visuals.weak_text_color(),
        );
        return;
    }

    if response.dragged() {
        let delta = response.drag_delta();
        state.geo_pan = egui::vec2(state.geo_pan.x + delta.x, state.geo_pan.y + delta.y);
    }
    if response.hovered() {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll.abs() > f32::EPSILON {
            let factor = (scroll * 0.005).exp();
            state.geo_zoom = (state.geo_zoom * factor).clamp(0.25, 20.0);
        }
    }

    let bbox = compute_bbox(&gps_nodes);
    let projector = Projector::new(&bbox, rect.shrink(24.0), state.geo_zoom, state.geo_pan);
    let plane_origin = rect.center();

    let mut placements: Vec<(NodeId, egui::Pos2)> = Vec::with_capacity(gps_nodes.len());
    for (id, n) in &gps_nodes {
        if let Some(p) = &n.position {
            let pos = projector.project(p.latitude_deg, p.longitude_deg, plane_origin);
            placements.push((*id, pos));
        }
    }

    let self_pos = snapshot
        .nodes
        .get(&snapshot.my_node)
        .and_then(|n| n.position.as_ref())
        .map(|p| projector.project(p.latitude_deg, p.longitude_deg, plane_origin));

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

    draw_scale_bar(&painter, rect, projector.scale_m_per_px(), ui.style());
    handle_interaction(ui, &response, &placements, snapshot, detail_node);
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

/// Four buckets: [hops=0, hops=1, hops≥2, unknown]. `my_node` excluded.
fn group_by_ring(snapshot: &DeviceSnapshot) -> [Vec<NodeId>; 4] {
    let mut rings: [Vec<NodeId>; 4] = Default::default();
    for (id, n) in &snapshot.nodes {
        if *id == snapshot.my_node {
            continue;
        }
        let bucket = match n.hops_away {
            Some(0) => 0_usize,
            Some(1) => 1,
            Some(_) => 2,
            None => 3,
        };
        if let Some(ring) = rings.get_mut(bucket) {
            ring.push(*id);
        }
    }
    for ring in &mut rings {
        ring.sort_by_key(|id| id.0);
    }
    rings
}

struct SignalPlacement {
    id: NodeId,
    pos: egui::Pos2,
    direct_neighbor: bool,
}

fn layout_signal(
    rings: &[Vec<NodeId>; 4],
    center: egui::Pos2,
    ring_step: f32,
) -> Vec<SignalPlacement> {
    let mut out = Vec::new();
    for (ring_idx, ids) in rings.iter().enumerate() {
        let count = ids.len().max(1) as f32;
        let radius = ring_step * (ring_idx as f32 + 1.0);
        let phase = if ring_idx % 2 == 0 { 0.0 } else { std::f32::consts::PI / count };
        for (i, id) in ids.iter().enumerate() {
            let theta = (i as f32 / count).mul_add(std::f32::consts::TAU, phase);
            let pos = egui::pos2(
                center.x + radius * theta.cos(),
                center.y + radius * theta.sin(),
            );
            out.push(SignalPlacement { id: *id, pos, direct_neighbor: ring_idx == 0 });
        }
    }
    out
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

struct Bbox {
    lat_min: f64,
    lat_max: f64,
    lon_min: f64,
    lon_max: f64,
}

fn compute_bbox(gps_nodes: &[(NodeId, &Node)]) -> Bbox {
    let mut lat_min = f64::INFINITY;
    let mut lat_max = f64::NEG_INFINITY;
    let mut lon_min = f64::INFINITY;
    let mut lon_max = f64::NEG_INFINITY;
    for (_, n) in gps_nodes {
        if let Some(p) = &n.position {
            lat_min = lat_min.min(p.latitude_deg);
            lat_max = lat_max.max(p.latitude_deg);
            lon_min = lon_min.min(p.longitude_deg);
            lon_max = lon_max.max(p.longitude_deg);
        }
    }
    if (lat_max - lat_min).abs() < 1e-5 {
        lat_min -= 5e-4;
        lat_max += 5e-4;
    }
    if (lon_max - lon_min).abs() < 1e-5 {
        lon_min -= 5e-4;
        lon_max += 5e-4;
    }
    Bbox { lat_min, lat_max, lon_min, lon_max }
}

struct Projector {
    lat_center: f64,
    lon_center: f64,
    meters_per_deg_lat: f64,
    meters_per_deg_lon: f64,
    scale: f64,
    pan: egui::Vec2,
}

impl Projector {
    fn new(bbox: &Bbox, plot: egui::Rect, zoom: f32, pan: egui::Vec2) -> Self {
        let lat_center = (bbox.lat_min + bbox.lat_max) * 0.5;
        let lon_center = (bbox.lon_min + bbox.lon_max) * 0.5;
        let meters_per_deg_lat = 111_320.0_f64;
        let cos_lat = lat_center.to_radians().cos().abs().max(0.01);
        let meters_per_deg_lon = 111_320.0_f64 * cos_lat;
        let world_width_m = (bbox.lon_max - bbox.lon_min) * meters_per_deg_lon;
        let world_height_m = (bbox.lat_max - bbox.lat_min) * meters_per_deg_lat;
        let fit_horizontal = f64::from(plot.width()) / world_width_m.max(1.0);
        let fit_vertical = f64::from(plot.height()) / world_height_m.max(1.0);
        let fit = fit_horizontal.min(fit_vertical).max(1e-9);
        Self {
            lat_center,
            lon_center,
            meters_per_deg_lat,
            meters_per_deg_lon,
            scale: fit * f64::from(zoom),
            pan,
        }
    }

    fn project(&self, lat_deg: f64, lon_deg: f64, origin: egui::Pos2) -> egui::Pos2 {
        let east_m = (lon_deg - self.lon_center) * self.meters_per_deg_lon;
        let south_m = (self.lat_center - lat_deg) * self.meters_per_deg_lat;
        egui::pos2(
            origin.x + (east_m * self.scale) as f32 + self.pan.x,
            origin.y + (south_m * self.scale) as f32 + self.pan.y,
        )
    }

    fn scale_m_per_px(&self) -> f64 {
        if self.scale > 0.0 { 1.0 / self.scale } else { 0.0 }
    }
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

fn legend_signal(painter: &egui::Painter, rect: egui::Rect, style: &egui::Style) {
    let lines = ["ring 1: hops=0", "ring 2: hops=1", "ring 3: hops≥2", "ring 4: unknown"];
    let color = style.visuals.weak_text_color();
    for (i, line) in lines.iter().enumerate() {
        painter.text(
            egui::pos2(rect.left() + 12.0, (i as f32).mul_add(14.0, rect.top() + 10.0)),
            egui::Align2::LEFT_TOP,
            *line,
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
