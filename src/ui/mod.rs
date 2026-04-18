pub mod channels;
pub mod chat;
pub mod connect;
pub mod details;
pub mod firmware;
pub mod fonts;
pub mod nodes;
pub mod reconnect;
pub mod remote_admin;
pub mod scan;
pub mod settings;
pub mod status;

pub use fonts::install_fonts;

use std::time::Instant;

use eframe::egui;
use tokio::sync::mpsc;

use tracing::warn;

use crate::domain::ids::NodeId;
use crate::domain::profile::ConnectionProfile;
use crate::domain::snapshot::DeviceSnapshot;
use crate::domain::traceroute::TracerouteResult;
use crate::persist::history::HistoryStore;
use crate::session::Event;
use crate::session::commands::Command;

#[derive(Default, Clone, Debug)]
pub enum SessionStatus {
    #[default]
    Disconnected,
    Connecting,
    Connected,
}

#[derive(Default)]
pub struct AppState {
    pub status: SessionStatus,
    pub snapshot: DeviceSnapshot,
    pub profiles: Vec<ConnectionProfile>,
    pub last_error: Option<String>,
    pub active_tab: Tab,
    pub last_activity: Option<Instant>,
    pub detail_node: Option<NodeId>,
    pub connect_ui: connect::ConnectUi,
    pub scan_ui: scan::ScanUi,
    pub chat_ui: chat::ChatUi,
    pub nodes_ui: nodes::NodesUi,
    pub settings_ui: settings::SettingsUi,
    pub channels_ui: channels::ChannelsUi,
    pub traceroutes: TracerouteUi,
    pub remote_admin: remote_admin::RemoteAdminUi,
    pub probed_nodes: std::collections::HashSet<NodeId>,
    pub reconnect: reconnect::ReconnectUi,
    /// Set to `true` by any mutation of `profiles` — the app loop picks it
    /// up and persists the list through the `HistoryStore`.
    pub profiles_dirty: bool,
    /// One-shot request from a keyboard shortcut to focus the search box
    /// on the currently active tab. Consumed the same frame by the
    /// corresponding tab's renderer.
    pub focus_search: bool,
}

#[derive(Default)]
pub struct TracerouteUi {
    pub pending: std::collections::HashSet<NodeId>,
    pub outcomes: std::collections::HashMap<NodeId, Result<TracerouteResult, String>>,
}

impl AppState {
    pub fn connected(&self) -> bool {
        matches!(self.status, SessionStatus::Connected)
    }
}

#[derive(Default, Copy, Clone, Eq, PartialEq)]
pub enum Tab {
    #[default]
    Chat,
    Nodes,
    Channels,
    Settings,
}

pub struct App {
    state: AppState,
    cmd_tx: mpsc::UnboundedSender<Command>,
    ev_rx: mpsc::Receiver<Event>,
    store: Option<HistoryStore>,
}

impl App {
    pub fn new(
        profiles: Vec<ConnectionProfile>,
        last_active_key: Option<String>,
        cmd_tx: mpsc::UnboundedSender<Command>,
        ev_rx: mpsc::Receiver<Event>,
        store: Option<HistoryStore>,
    ) -> Self {
        let mut reconnect = reconnect::ReconnectUi::default();
        if let Some(key) = last_active_key.as_ref()
            && let Some(profile) = profiles.iter().find(|p| &p.key() == key).cloned()
        {
            reconnect.arm_from_startup(profile);
        }
        reconnect.last_active = last_active_key;
        Self {
            state: AppState { profiles, reconnect, ..AppState::default() },
            cmd_tx,
            ev_rx,
            store,
        }
    }

    fn drain_events(&mut self) {
        while let Ok(ev) = self.ev_rx.try_recv() {
            self.reduce(ev);
        }
    }

    fn reduce(&mut self, ev: Event) {
        if is_activity(&ev) {
            self.state.last_activity = Some(Instant::now());
        }
        match ev {
            Event::Connecting => {
                self.state.status = SessionStatus::Connecting;
                self.state.last_error = None;
            }
            Event::Connected(snap) => self.apply_connected(*snap),
            Event::NodeUpdated(node) => self.apply_node_updated(node),
            Event::ChannelUpdated(ch) => {
                self.state.snapshot.upsert_channel(ch);
            }
            Event::LoraUpdated(s) => self.state.snapshot.lora = Some(s),
            Event::DeviceUpdated(s) => self.state.snapshot.device = Some(s),
            Event::PositionUpdated(s) => self.state.snapshot.position = Some(s),
            Event::PowerUpdated(s) => self.state.snapshot.power = Some(s),
            Event::NetworkUpdated(s) => self.state.snapshot.network = Some(s),
            Event::DisplayUpdated(s) => self.state.snapshot.display = Some(s),
            Event::BluetoothUpdated(s) => self.state.snapshot.bluetooth = Some(s),
            Event::MqttUpdated(s) => self.state.snapshot.mqtt = Some(s),
            Event::TelemetryCfgUpdated(s) => self.state.snapshot.telemetry = Some(s),
            Event::NeighborInfoUpdated(s) => self.state.snapshot.neighbor_info = Some(s),
            Event::StoreForwardUpdated(s) => self.state.snapshot.store_forward = Some(s),
            Event::SecurityUpdated(s) => self.state.snapshot.security = Some(s),
            Event::ExtNotifUpdated(s) => self.state.snapshot.ext_notif = Some(s),
            Event::CannedUpdated(s) => self.state.snapshot.canned = Some(s),
            Event::RangeTestUpdated(s) => self.state.snapshot.range_test = Some(s),
            Event::StatsUpdated(stats) => self.state.snapshot.stats.merge(&stats),
            Event::TracerouteResult(result) => {
                let target = result.target;
                let _ = self.state.traceroutes.pending.remove(&target);
                let _ = self.state.traceroutes.outcomes.insert(target, Ok(result));
            }
            Event::TracerouteFailed { target, reason } => {
                let _ = self.state.traceroutes.pending.remove(&target);
                let _ = self.state.traceroutes.outcomes.insert(target, Err(reason));
            }
            Event::MessageReceived(m) => self.apply_message_received(m),
            Event::MessageStateChanged { id, state } => self.apply_state_changed(id, &state),
            Event::Disconnected => {
                self.state.status = SessionStatus::Disconnected;
                self.state.last_activity = None;
                self.state.reconnect.on_disconnected();
            }
            Event::Error(msg) => {
                self.state.last_error = Some(msg);
            }
        }
    }

    fn apply_connected(&mut self, snap: DeviceSnapshot) {
        self.state.status = SessionStatus::Connected;
        self.state.nodes_ui.seen_live.clear();
        self.state.nodes_ui.persisted_saved_at.clear();
        let mut snapshot = snap;
        if let Some(store) = self.store.as_ref() {
            load_history(store, &mut snapshot, &mut self.state.nodes_ui.persisted_saved_at);
        }
        snapshot.messages.sort_by_key(|m| m.received_at);
        self.state.snapshot = snapshot;
        self.state.reconnect.on_connected();
        self.persist_last_active();
    }

    /// Global keyboard shortcuts handled before the frame's UI draws.
    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        let focus = egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::K);
        if ctx.input_mut(|i| i.consume_shortcut(&focus)) {
            self.state.focus_search = true;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            // Close the detail popup first (modal-ish), then fall through to
            // search clearing on the active tab the next Esc.
            if self.state.detail_node.is_some() {
                self.state.detail_node = None;
            } else {
                match self.state.active_tab {
                    Tab::Chat => self.state.chat_ui.search.clear(),
                    Tab::Nodes => self.state.nodes_ui.search.clear(),
                    Tab::Channels | Tab::Settings => {}
                }
            }
        }
    }

    fn persist_last_active(&mut self) {
        let key = self.state.reconnect.profile.as_ref().map(crate::domain::profile::ConnectionProfile::key);
        if key == self.state.reconnect.last_active {
            return;
        }
        self.state.reconnect.last_active = key;
        if let Some(store) = self.store.as_ref()
            && let Err(e) = store.save_last_active(self.state.reconnect.last_active.as_deref())
        {
            warn!(%e, "persist last-active profile failed");
        }
    }

    fn apply_node_updated(&mut self, node: crate::domain::node::Node) {
        if node.id == self.state.snapshot.my_node {
            if !node.long_name.is_empty() {
                self.state.snapshot.long_name.clone_from(&node.long_name);
            }
            if !node.short_name.is_empty() {
                self.state.snapshot.short_name.clone_from(&node.short_name);
            }
        }
        self.state.nodes_ui.mark_updated(node.id);
        let _ = self.state.nodes_ui.seen_live.insert(node.id);
        if let Some(store) = &self.store
            && let Err(e) = store.upsert_node(self.state.snapshot.my_node, &node)
        {
            warn!(%e, "persist node failed");
        }
        let _ = self.state.snapshot.nodes.insert(node.id, node);
    }

    fn apply_message_received(&mut self, m: crate::domain::message::TextMessage) {
        if let Some(store) = &self.store
            && let Err(e) = store.upsert_message(self.state.snapshot.my_node, &m)
        {
            warn!(%e, "persist message failed");
        }
        let from = m.from;
        self.state.snapshot.upsert_message(m);
        self.probe_node_if_unknown(from);
    }

    /// If the sender is a ghost in our `NodeDB`, fire one `NodeInfo` request over
    /// the mesh. The reply flows back as a normal `NodeUpdated` event and the
    /// chat auto-relabels from `!xxxxxxxx` to the node's display name.
    fn probe_node_if_unknown(&mut self, id: NodeId) {
        if id.0 == 0 || id == self.state.snapshot.my_node {
            return;
        }
        if self.state.snapshot.nodes.contains_key(&id) {
            return;
        }
        if !self.state.probed_nodes.insert(id) {
            return;
        }
        let _ = self.cmd_tx.send(Command::RequestNodeInfo { node: id });
    }

    fn refresh_storage_counts(&mut self) {
        let Some(store) = self.store.as_ref() else {
            self.state.settings_ui.stored_messages = None;
            self.state.settings_ui.stored_nodes = None;
            return;
        };
        let my = self.state.snapshot.my_node;
        self.state.settings_ui.stored_messages = store.message_count(my).ok();
        self.state.settings_ui.stored_nodes = store.node_count(my).ok();
    }

    fn handle_clear(&mut self, kind: settings::PendingClear) {
        use settings::PendingClear;
        let Some(store) = self.store.as_ref() else { return };
        let my = self.state.snapshot.my_node;
        let clear_messages = matches!(kind, PendingClear::Messages | PendingClear::All);
        let clear_nodes = matches!(kind, PendingClear::Nodes | PendingClear::All);
        if clear_messages {
            if let Err(e) = store.clear_messages(my) {
                warn!(%e, "clear messages failed");
            } else {
                self.state.snapshot.messages.clear();
            }
        }
        if clear_nodes {
            if let Err(e) = store.clear_nodes(my) {
                warn!(%e, "clear nodes failed");
            } else {
                self.state
                    .snapshot
                    .nodes
                    .retain(|id, _| self.state.nodes_ui.seen_live.contains(id));
                self.state.nodes_ui.persisted_saved_at.clear();
            }
        }
    }

    fn apply_state_changed(
        &mut self,
        id: crate::domain::ids::PacketId,
        state: &crate::domain::message::DeliveryState,
    ) {
        let applied = self
            .state
            .snapshot
            .messages
            .iter_mut()
            .find(|m| m.id == id)
            .is_some_and(|m| {
                if m.state.is_terminal() {
                    false
                } else {
                    m.state = state.clone();
                    true
                }
            });
        if applied
            && let Some(store) = &self.store
            && let Err(e) = store.update_message_state(self.state.snapshot.my_node, id, state)
        {
            warn!(%e, "persist state change failed");
        }
    }
}

fn load_history(
    store: &HistoryStore,
    snapshot: &mut DeviceSnapshot,
    persisted_saved_at: &mut std::collections::HashMap<NodeId, std::time::SystemTime>,
) {
    match store.load_messages(snapshot.my_node) {
        Ok(history) => {
            for m in history {
                snapshot.upsert_message(m);
            }
        }
        Err(e) => warn!(%e, "load message history failed"),
    }
    match store.load_nodes(snapshot.my_node) {
        Ok(saved_nodes) => {
            for persisted in saved_nodes {
                let id = persisted.node.id;
                snapshot.nodes.entry(id).or_insert(persisted.node);
                let _ = persisted_saved_at.insert(id, persisted.saved_at);
            }
        }
        Err(e) => warn!(%e, "load node history failed"),
    }
    for m in snapshot.messages.clone() {
        if let Err(e) = store.upsert_message(snapshot.my_node, &m) {
            warn!(%e, "persist handshake message failed");
        }
    }
    for node in snapshot.nodes.values() {
        if let Err(e) = store.upsert_node(snapshot.my_node, node) {
            warn!(%e, "persist handshake node failed");
        }
    }
}

const fn is_activity(ev: &Event) -> bool {
    matches!(
        ev,
        Event::Connected(_)
            | Event::NodeUpdated(_)
            | Event::ChannelUpdated(_)
            | Event::LoraUpdated(_)
            | Event::DeviceUpdated(_)
            | Event::PositionUpdated(_)
            | Event::PowerUpdated(_)
            | Event::NetworkUpdated(_)
            | Event::DisplayUpdated(_)
            | Event::BluetoothUpdated(_)
            | Event::MqttUpdated(_)
            | Event::TelemetryCfgUpdated(_)
            | Event::NeighborInfoUpdated(_)
            | Event::StoreForwardUpdated(_)
            | Event::SecurityUpdated(_)
            | Event::ExtNotifUpdated(_)
            | Event::CannedUpdated(_)
            | Event::RangeTestUpdated(_)
            | Event::StatsUpdated(_)
            | Event::TracerouteResult(_)
            | Event::TracerouteFailed { .. }
            | Event::MessageReceived(_)
            | Event::MessageStateChanged { .. }
    )
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.drain_events();
        self.handle_shortcuts(ctx);
        self.flush_profiles_dirty();
        self.render_reconnect(ctx);
        self.render_chrome(ctx);
        if !self.state.connected() {
            egui::CentralPanel::default().show(ctx, |ui| {
                connect::render(ui, &mut self.state, &self.cmd_tx);
            });
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
            return;
        }
        self.render_sidebar(ctx);
        self.render_active_tab(ctx);
        details::render_overlay(ctx, &mut self.state, &self.cmd_tx);
        remote_admin::render(
            ctx,
            &self.state.snapshot,
            &mut self.state.remote_admin,
            &self.cmd_tx,
        );
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }
}

impl App {
    fn flush_profiles_dirty(&mut self) {
        if !self.state.profiles_dirty {
            return;
        }
        if let Some(store) = self.store.as_ref()
            && let Err(e) = store.save_profiles(&self.state.profiles)
        {
            warn!(%e, "persist profiles failed");
        }
        self.state.profiles_dirty = false;
    }

    fn render_reconnect(&mut self, ctx: &egui::Context) {
        let now = Instant::now();
        let disconnected = matches!(self.state.status, SessionStatus::Disconnected);
        let connecting = matches!(self.state.status, SessionStatus::Connecting);
        let mut stop = false;
        reconnect::render_banner(
            ctx,
            &self.state.reconnect,
            disconnected,
            connecting,
            now,
            &mut stop,
        );
        if stop {
            self.state.reconnect.cancel();
        }
        reconnect::tick(&mut self.state.reconnect, disconnected, now, &self.cmd_tx);
    }

    fn render_chrome(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("status").show(ctx, |ui| status::render(ui, &self.state));
        if self.state.connected() {
            firmware::render_banner_if_old(ctx, &self.state.snapshot.firmware_version);
        }
        scan::render(
            ctx,
            &mut self.state.scan_ui,
            &self.cmd_tx,
            &mut self.state.profiles,
            &mut self.state.profiles_dirty,
            &mut self.state.reconnect,
        );
    }

    fn render_sidebar(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("sidebar").show(ctx, |ui| {
            if ui.button("Disconnect").clicked() {
                self.state.reconnect.mark_user_disconnect();
                let _ = self.cmd_tx.send(Command::Disconnect);
            }
            ui.separator();
            ui.selectable_value(&mut self.state.active_tab, Tab::Chat, "Chat");
            ui.selectable_value(&mut self.state.active_tab, Tab::Nodes, "Nodes");
            ui.selectable_value(&mut self.state.active_tab, Tab::Channels, "Channels");
            ui.selectable_value(&mut self.state.active_tab, Tab::Settings, "Settings");
        });
    }

    fn render_active_tab(&mut self, ctx: &egui::Context) {
        match self.state.active_tab {
            Tab::Chat => {
                egui::TopBottomPanel::bottom("chat_composer")
                    .resizable(false)
                    .min_height(68.0)
                    .show(ctx, |ui| {
                        chat::render_composer(ui, &mut self.state, &self.cmd_tx);
                    });
                egui::CentralPanel::default().show(ctx, |ui| {
                    chat::render_messages(ui, &mut self.state);
                });
            }
            Tab::Nodes => {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let AppState {
                        snapshot,
                        nodes_ui,
                        detail_node,
                        focus_search,
                        ..
                    } = &mut self.state;
                    nodes::render(ui, snapshot, nodes_ui, detail_node, focus_search);
                });
            }
            Tab::Channels => {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let AppState { snapshot, channels_ui, .. } = &mut self.state;
                    channels::render(ui, snapshot, channels_ui, &self.cmd_tx);
                });
            }
            Tab::Settings => {
                self.refresh_storage_counts();
                egui::CentralPanel::default().show(ctx, |ui| {
                    let AppState { snapshot, settings_ui, .. } = &mut self.state;
                    settings::render(ui, snapshot, settings_ui, &self.cmd_tx);
                });
                if let Some(kind) = self.state.settings_ui.pending_clear.take() {
                    self.handle_clear(kind);
                }
            }
        }
    }
}
