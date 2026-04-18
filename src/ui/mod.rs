pub mod chat;
pub mod connect;
pub mod details;
pub mod nodes;
pub mod scan;
pub mod settings;
pub mod status;

use std::path::PathBuf;
use std::time::Instant;

use eframe::egui;
use tokio::sync::mpsc;

use tracing::warn;

use crate::domain::ids::NodeId;
use crate::domain::profile::ConnectionProfile;
use crate::domain::snapshot::DeviceSnapshot;
use crate::persist::messages::MessageStore;
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
    Settings,
}

pub struct App {
    state: AppState,
    cmd_tx: mpsc::UnboundedSender<Command>,
    ev_rx: mpsc::Receiver<Event>,
    profiles_path: PathBuf,
    store: Option<MessageStore>,
}

impl App {
    pub fn new(
        profiles: Vec<ConnectionProfile>,
        profiles_path: PathBuf,
        cmd_tx: mpsc::UnboundedSender<Command>,
        ev_rx: mpsc::Receiver<Event>,
        store: Option<MessageStore>,
    ) -> Self {
        Self {
            state: AppState { profiles, ..AppState::default() },
            cmd_tx,
            ev_rx,
            profiles_path,
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
            Event::MessageReceived(m) => self.apply_message_received(m),
            Event::MessageStateChanged { id, state } => self.apply_state_changed(id, &state),
            Event::Disconnected => {
                self.state.status = SessionStatus::Disconnected;
                self.state.last_activity = None;
            }
            Event::Error(msg) => {
                self.state.last_error = Some(msg);
            }
        }
    }

    fn apply_connected(&mut self, snap: DeviceSnapshot) {
        self.state.status = SessionStatus::Connected;
        let mut snapshot = snap;
        if let Some(store) = &self.store {
            match store.load(snapshot.my_node) {
                Ok(history) => {
                    for m in history {
                        snapshot.upsert_message(m);
                    }
                }
                Err(e) => warn!(%e, "load message history failed"),
            }
            for m in snapshot.messages.clone() {
                if let Err(e) = store.upsert(snapshot.my_node, &m) {
                    warn!(%e, "persist handshake message failed");
                }
            }
        }
        snapshot.messages.sort_by_key(|m| m.received_at);
        self.state.snapshot = snapshot;
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
        let _ = self.state.snapshot.nodes.insert(node.id, node);
    }

    fn apply_message_received(&mut self, m: crate::domain::message::TextMessage) {
        if let Some(store) = &self.store
            && let Err(e) = store.upsert(self.state.snapshot.my_node, &m)
        {
            warn!(%e, "persist message failed");
        }
        self.state.snapshot.upsert_message(m);
    }

    fn apply_state_changed(
        &mut self,
        id: crate::domain::ids::PacketId,
        state: &crate::domain::message::DeliveryState,
    ) {
        if let Some(m) = self.state.snapshot.messages.iter_mut().find(|m| m.id == id) {
            m.state = state.clone();
        }
        if let Some(store) = &self.store
            && let Err(e) = store.update_state(self.state.snapshot.my_node, id, state)
        {
            warn!(%e, "persist state change failed");
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
            | Event::MessageReceived(_)
            | Event::MessageStateChanged { .. }
    )
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.drain_events();

        egui::TopBottomPanel::top("status").show(ctx, |ui| status::render(ui, &self.state));
        scan::render(ctx, &mut self.state.scan_ui, &self.cmd_tx, &mut self.state.profiles);

        if !self.state.connected() {
            egui::CentralPanel::default().show(ctx, |ui| {
                connect::render(ui, &mut self.state, &self.cmd_tx, &self.profiles_path);
            });
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
            return;
        }
        egui::SidePanel::left("sidebar").show(ctx, |ui| {
            if ui.button("Disconnect").clicked() {
                let _ = self.cmd_tx.send(Command::Disconnect);
            }
            ui.separator();
            ui.selectable_value(&mut self.state.active_tab, Tab::Chat, "Chat");
            ui.selectable_value(&mut self.state.active_tab, Tab::Nodes, "Nodes");
            ui.selectable_value(&mut self.state.active_tab, Tab::Settings, "Settings");
        });
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
                    let AppState { snapshot, nodes_ui, detail_node, .. } = &mut self.state;
                    nodes::render(ui, snapshot, nodes_ui, detail_node);
                });
            }
            Tab::Settings => {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let AppState { snapshot, settings_ui, .. } = &mut self.state;
                    settings::render(ui, snapshot, settings_ui, &self.cmd_tx);
                });
            }
        }
        details::render_overlay(ctx, &self.state.snapshot, &mut self.state.detail_node);
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }
}
