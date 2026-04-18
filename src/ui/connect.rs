use std::path::PathBuf;

use eframe::egui;
use tokio::sync::mpsc;

use crate::domain::ids::BleAddress;
use crate::domain::profile::{ConnectionProfile, TransportKind};
use crate::session::commands::Command;
use crate::ui::AppState;

#[derive(Default)]
pub struct ConnectUi {
    pub add: AddForm,
}

#[derive(Default)]
pub struct AddForm {
    pub open: bool,
    pub kind: Option<TransportKind>,
    pub name: String,
    pub host: String,
    pub port: String,
    pub path: String,
    pub address: String,
}

pub fn render(ui: &mut egui::Ui, state: &mut AppState, cmd: &mpsc::UnboundedSender<Command>) {
    ui.heading("Meshtastic");
    let busy = matches!(state.status, crate::ui::SessionStatus::Connecting);
    if busy {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label("Connecting…");
            if ui.button("Cancel").clicked() {
                state.reconnect.mark_user_disconnect();
                let _ = cmd.send(Command::Disconnect);
            }
        });
        ui.separator();
    }
    ui.horizontal(|ui| {
        if ui.add_enabled(!busy, egui::Button::new("Add profile")).clicked() {
            state.connect_ui.add = AddForm { open: true, ..AddForm::default() };
        }
        if ui.add_enabled(!busy, egui::Button::new("Scan BLE")).clicked() {
            crate::ui::scan::open(&mut state.scan_ui);
        }
    });
    ui.separator();
    list_profiles(ui, state, cmd, busy);
    if state.connect_ui.add.open {
        add_dialog(ui.ctx(), state);
    }
}

fn list_profiles(
    ui: &mut egui::Ui,
    state: &mut AppState,
    cmd: &mpsc::UnboundedSender<Command>,
    busy: bool,
) {
    let mut delete_idx: Option<usize> = None;
    let profiles = state.profiles.clone();
    for (idx, profile) in profiles.iter().enumerate() {
        ui.horizontal(|ui| {
            ui.label(format!("[{:?}] {}", profile.kind(), profile.name()));
            if ui.add_enabled(!busy, egui::Button::new("Connect")).clicked() {
                state.reconnect.mark_user_connect(profile);
                let _ = cmd.send(Command::Connect(profile.clone()));
            }
            if ui.add_enabled(!busy, egui::Button::new("Delete")).clicked() {
                delete_idx = Some(idx);
            }
        });
    }
    if let Some(i) = delete_idx
        && i < state.profiles.len()
    {
        let _ = state.profiles.remove(i);
        state.profiles_dirty = true;
    }
}

fn add_dialog(ctx: &egui::Context, state: &mut AppState) {
    let mut close = false;
    let mut save = false;
    egui::Window::new("Add profile").collapsible(false).show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut state.connect_ui.add.kind, Some(TransportKind::Ble), "BLE");
            ui.selectable_value(
                &mut state.connect_ui.add.kind,
                Some(TransportKind::Serial),
                "Serial",
            );
            ui.selectable_value(&mut state.connect_ui.add.kind, Some(TransportKind::Tcp), "TCP");
        });
        ui.horizontal(|ui| {
            ui.label("Name:");
            ui.text_edit_singleline(&mut state.connect_ui.add.name);
        });
        match state.connect_ui.add.kind {
            Some(TransportKind::Ble) => {
                ui.horizontal(|ui| {
                    ui.label("Address:");
                    ui.text_edit_singleline(&mut state.connect_ui.add.address);
                });
            }
            Some(TransportKind::Serial) => {
                ui.horizontal(|ui| {
                    ui.label("Path:");
                    ui.text_edit_singleline(&mut state.connect_ui.add.path);
                });
            }
            Some(TransportKind::Tcp) => {
                ui.horizontal(|ui| {
                    ui.label("Host:");
                    ui.text_edit_singleline(&mut state.connect_ui.add.host);
                });
                ui.horizontal(|ui| {
                    ui.label("Port:");
                    ui.text_edit_singleline(&mut state.connect_ui.add.port);
                });
            }
            None => {
                ui.label("Pick a transport");
            }
        }
        ui.horizontal(|ui| {
            if ui.button("Cancel").clicked() {
                close = true;
            }
            if ui.button("Save").clicked() {
                save = true;
            }
        });
    });
    if save
        && let Some(profile) = build_profile(&state.connect_ui.add) {
            state.profiles.push(profile);
            state.profiles_dirty = true;
            close = true;
        }
    if close {
        state.connect_ui.add = AddForm::default();
    }
}

fn build_profile(form: &AddForm) -> Option<ConnectionProfile> {
    match form.kind? {
        TransportKind::Ble => Some(ConnectionProfile::Ble {
            name: form.name.clone(),
            address: BleAddress::new(form.address.clone()),
        }),
        TransportKind::Serial => Some(ConnectionProfile::Serial {
            name: form.name.clone(),
            path: PathBuf::from(form.path.clone()),
        }),
        TransportKind::Tcp => Some(ConnectionProfile::Tcp {
            name: form.name.clone(),
            host: form.host.clone(),
            port: form.port.parse().ok()?,
        }),
    }
}
