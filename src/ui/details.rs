use std::time::SystemTime;

use eframe::egui;
use tokio::sync::mpsc;

use crate::domain::ids::NodeId;
use crate::domain::node::Node;
use crate::domain::snapshot::DeviceSnapshot;
use crate::domain::traceroute::TracerouteResult;
use crate::session::commands::Command;
use crate::ui::{AppState, Tab, TracerouteUi};

pub fn render_overlay(
    ctx: &egui::Context,
    state: &mut AppState,
    cmd: &mpsc::UnboundedSender<Command>,
) {
    let Some(id) = state.detail_node else { return };
    let is_self = id == state.snapshot.my_node;
    let title = state
        .snapshot
        .nodes
        .get(&id)
        .map_or_else(|| format!("!{:08x}", id.0), display_name);

    let mut open = true;
    let mut action: Option<Action> = None;
    let unknown = !state.snapshot.nodes.contains_key(&id);
    egui::Window::new(title).open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
        match state.snapshot.nodes.get(&id) {
            Some(node) => {
                render_body(ui, node);
                ui.separator();
                action = render_actions(ui, node, is_self, &state.traceroutes);
                render_traceroute_section(ui, id, &state.traceroutes, &state.snapshot);
            }
            None => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(format!(
                        "No info yet for !{:08x} — asked the mesh for a NodeInfo reply.",
                        id.0,
                    ));
                });
            }
        }
    });
    if unknown && id.0 != 0 && id != state.snapshot.my_node && state.probed_nodes.insert(id) {
        let _ = cmd.send(Command::RequestNodeInfo { node: id });
    }

    if let Some(action) = action {
        apply_action(state, cmd, id, action);
    }
    if !open {
        state.detail_node = None;
    }
}

#[derive(Copy, Clone)]
enum Action {
    ToggleFavorite,
    ToggleIgnored,
    SendMessage,
    Traceroute,
    RemoteAdmin,
}

fn render_actions(
    ui: &mut egui::Ui,
    node: &Node,
    is_self: bool,
    tracers: &TracerouteUi,
) -> Option<Action> {
    let mut action = None;
    ui.horizontal(|ui| {
        if ui
            .button("Send message")
            .on_hover_text("Open the Chat tab with this node selected as DM target.")
            .clicked()
        {
            action = Some(Action::SendMessage);
        }
        if is_self {
            return;
        }
        let fav_label = if node.is_favorite { "★ Unfavorite" } else { "☆ Favorite" };
        if ui
            .button(fav_label)
            .on_hover_text(
                "Pin this node on the device's NodeDB so it sticks around even when crowded.",
            )
            .clicked()
        {
            action = Some(Action::ToggleFavorite);
        }
        let ign_label = if node.is_ignored { "Unignore" } else { "Ignore" };
        let ign_btn = if node.is_ignored {
            egui::Button::new(ign_label)
        } else {
            egui::Button::new(ign_label).fill(egui::Color32::from_rgb(90, 30, 30))
        };
        if ui
            .add(ign_btn)
            .on_hover_text(
                "Tell the device to drop packets from this node. Reversible. Useful for noisy or \
                 spammy stations.",
            )
            .clicked()
        {
            action = Some(Action::ToggleIgnored);
        }
        let pending = tracers.pending.contains(&node.id);
        let trace_label = if pending { "Tracing…" } else { "Traceroute" };
        let trace_btn = egui::Button::new(trace_label);
        if ui
            .add_enabled(!pending, trace_btn)
            .on_hover_text(
                "Probe the path to this node. Each hop's SNR is reported on the reply. May take up \
                 to a minute.",
            )
            .clicked()
        {
            action = Some(Action::Traceroute);
        }
        if ui
            .button("Remote admin…")
            .on_hover_text(
                "Send admin commands (reboot / shutdown / factory reset / …) to this node over \
                 the mesh. The target accepts them only if YOUR public key is in its \
                 Admin keys list (Settings → Security on the target node).",
            )
            .clicked()
        {
            action = Some(Action::RemoteAdmin);
        }
    });
    action
}

fn apply_action(
    state: &mut AppState,
    cmd: &mpsc::UnboundedSender<Command>,
    id: NodeId,
    action: Action,
) {
    match action {
        Action::ToggleFavorite => {
            if let Some(node) = state.snapshot.nodes.get_mut(&id) {
                node.is_favorite = !node.is_favorite;
                let _ = cmd
                    .send(Command::SetFavorite { node: id, favorite: node.is_favorite });
            }
        }
        Action::ToggleIgnored => {
            if let Some(node) = state.snapshot.nodes.get_mut(&id) {
                node.is_ignored = !node.is_ignored;
                let _ =
                    cmd.send(Command::SetIgnored { node: id, ignored: node.is_ignored });
            }
        }
        Action::SendMessage => {
            state.chat_ui.dm_target = Some(id);
            state.active_tab = Tab::Chat;
            state.detail_node = None;
        }
        Action::Traceroute => {
            let _ = state.traceroutes.pending.insert(id);
            let _ = state.traceroutes.outcomes.remove(&id);
            let _ = cmd.send(Command::Traceroute { node: id });
        }
        Action::RemoteAdmin => {
            state.remote_admin.target = Some(id);
            state.remote_admin.pending = None;
            state.remote_admin.last_dispatched = None;
            state.detail_node = None;
        }
    }
}

fn render_traceroute_section(
    ui: &mut egui::Ui,
    id: NodeId,
    tracers: &TracerouteUi,
    snapshot: &DeviceSnapshot,
) {
    let pending = tracers.pending.contains(&id);
    let outcome = tracers.outcomes.get(&id);
    if !pending && outcome.is_none() {
        return;
    }
    ui.separator();
    ui.label(egui::RichText::new("Traceroute").strong());
    if pending {
        ui.label("Waiting for reply…");
        return;
    }
    match outcome {
        Some(Ok(result)) => render_traceroute_result(ui, result, snapshot),
        Some(Err(reason)) => {
            ui.colored_label(egui::Color32::from_rgb(220, 120, 120), format!("Failed: {reason}"));
        }
        None => {}
    }
}

fn render_traceroute_result(ui: &mut egui::Ui, r: &TracerouteResult, snap: &DeviceSnapshot) {
    ui.label(format!(
        "Completed {}",
        format_last_heard(Some(r.completed_at)).trim_end_matches(" ago")
    ));
    render_hops(ui, "Towards", snap.my_node, r.target, &r.route, &r.snr_towards_db, snap);
    if !r.route_back.is_empty() || !r.snr_back_db.is_empty() {
        render_hops(ui, "Back", r.target, snap.my_node, &r.route_back, &r.snr_back_db, snap);
    }
}

fn render_hops(
    ui: &mut egui::Ui,
    label: &str,
    start: NodeId,
    end: NodeId,
    middle: &[NodeId],
    snrs: &[f32],
    snap: &DeviceSnapshot,
) {
    ui.label(egui::RichText::new(label).weak());
    let mut path = Vec::with_capacity(middle.len().saturating_add(2));
    path.push(start);
    path.extend_from_slice(middle);
    path.push(end);
    for (i, hop) in path.iter().enumerate() {
        let name = snap.nodes.get(hop).map_or_else(|| format!("!{:08x}", hop.0), display_name);
        let snr = i.checked_sub(1).and_then(|idx| snrs.get(idx).copied());
        let line = snr.map_or_else(
            || format!("  {} {name}", hop_arrow(i)),
            |v| format!("  {} {name}  ({v:.2} dB)", hop_arrow(i)),
        );
        ui.monospace(line);
    }
}

const fn hop_arrow(i: usize) -> &'static str {
    if i == 0 { "•" } else { "→" }
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
            let mut flags = Vec::new();
            if node.is_favorite {
                flags.push("favorite");
            }
            if node.is_ignored {
                flags.push("ignored");
            }
            let flags_label = if flags.is_empty() { "—".to_owned() } else { flags.join(", ") };
            row(ui, "Flags", flags_label);
            row(ui, "Battery", node.battery_level.map_or_else(|| "—".into(), |b| format!("{b}%")));
            row(ui, "Voltage", node.voltage_v.map_or_else(|| "—".into(), |v| format!("{v:.2} V")));
            row(ui, "SNR", node.snr_db.map_or_else(|| "—".into(), |s| format!("{s:.1} dB")));
            row(ui, "RSSI", node.rssi_dbm.map_or_else(|| "—".into(), |r| format!("{r} dBm")));
            row(ui, "Hops away", node.hops_away.map_or_else(|| "—".into(), |h| h.to_string()));
            row(ui, "Last heard", format_last_heard(node.last_heard));
            if let Some(pos) = &node.position {
                row(ui, "Latitude", format!("{:.6}°", pos.latitude_deg));
                row(ui, "Longitude", format!("{:.6}°", pos.longitude_deg));
                row(
                    ui,
                    "Altitude",
                    pos.altitude_m.map_or_else(|| "—".to_owned(), |a| format!("{a} m")),
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
    let Some(t) = last_heard else { return "—".into() };
    let Ok(d) = SystemTime::now().duration_since(t) else { return "—".into() };
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
