pub mod chat;
pub mod connect;
pub mod nodes;
pub mod scan;
pub mod status;

use std::path::PathBuf;

use eframe::egui;
use tokio::sync::mpsc;

use crate::domain::profile::ConnectionProfile;
use crate::domain::snapshot::DeviceSnapshot;
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
    pub connect_ui: connect::ConnectUi,
    pub scan_ui: scan::ScanUi,
    pub chat_ui: chat::ChatUi,
    pub nodes_ui: nodes::NodesUi,
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
}

pub struct App {
    state: AppState,
    cmd_tx: mpsc::UnboundedSender<Command>,
    ev_rx: mpsc::Receiver<Event>,
    profiles_path: PathBuf,
}

impl App {
    pub fn new(
        profiles: Vec<ConnectionProfile>,
        profiles_path: PathBuf,
        cmd_tx: mpsc::UnboundedSender<Command>,
        ev_rx: mpsc::Receiver<Event>,
    ) -> Self {
        Self {
            state: AppState { profiles, ..AppState::default() },
            cmd_tx,
            ev_rx,
            profiles_path,
        }
    }

    fn drain_events(&mut self) {
        while let Ok(ev) = self.ev_rx.try_recv() {
            self.reduce(ev);
        }
    }

    fn reduce(&mut self, ev: Event) {
        match ev {
            Event::Connecting => {
                self.state.status = SessionStatus::Connecting;
                self.state.last_error = None;
            }
            Event::Connected(snap) => {
                self.state.status = SessionStatus::Connected;
                self.state.snapshot = *snap;
            }
            Event::NodeUpdated(node) => {
                if node.id == self.state.snapshot.my_node {
                    if !node.long_name.is_empty() {
                        self.state.snapshot.long_name.clone_from(&node.long_name);
                    }
                    if !node.short_name.is_empty() {
                        self.state.snapshot.short_name.clone_from(&node.short_name);
                    }
                }
                let _ = self.state.snapshot.nodes.insert(node.id, node);
            }
            Event::ChannelUpdated(ch) => {
                self.state.snapshot.upsert_channel(ch);
            }
            Event::MessageReceived(m) => {
                self.state.snapshot.messages.push(m);
            }
            Event::MessageStateChanged { id, state } => {
                if let Some(m) = self.state.snapshot.messages.iter_mut().find(|m| m.id == id) {
                    m.state = state;
                }
            }
            Event::Disconnected => {
                self.state.status = SessionStatus::Disconnected;
            }
            Event::Error(msg) => {
                self.state.last_error = Some(msg);
            }
        }
    }
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
        });
        match self.state.active_tab {
            Tab::Chat => {
                egui::CentralPanel::default().show(ctx, |ui| {
                    chat::render(ui, &mut self.state, &self.cmd_tx);
                });
            }
            Tab::Nodes => {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let AppState { snapshot, nodes_ui, .. } = &mut self.state;
                    nodes::render(ui, snapshot, nodes_ui);
                });
            }
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }
}
