use std::sync::Arc;
use std::time::Duration;

use eframe::egui;
use parking_lot::Mutex;
use tokio::sync::mpsc;

use crate::domain::ids::BleAddress;
use crate::domain::profile::ConnectionProfile;
use crate::session::commands::Command;
use crate::transport::ble::{Discovered, scan_stream};

#[derive(Default)]
pub struct ScanUi {
    pub open: bool,
    pub results: Arc<Mutex<Vec<DiscoveredRow>>>,
    pub scanning: Arc<Mutex<bool>>,
}

#[derive(Clone)]
pub struct DiscoveredRow {
    pub name: String,
    pub address: BleAddress,
    pub rssi_dbm: Option<i16>,
    pub is_connected: bool,
}

impl From<Discovered> for DiscoveredRow {
    fn from(d: Discovered) -> Self {
        Self {
            name: d.name,
            address: d.address,
            rssi_dbm: d.rssi_dbm,
            is_connected: d.is_paired,
        }
    }
}

pub fn open(ui: &mut ScanUi) {
    ui.open = true;
    ui.results.lock().clear();
    *ui.scanning.lock() = true;

    let results = ui.results.clone();
    let scanning = ui.scanning.clone();
    let (tx, mut rx) = mpsc::unbounded_channel::<Discovered>();

    tokio::spawn(async move {
        while let Some(d) = rx.recv().await {
            let mut rows = results.lock();
            let address = d.address.clone();
            let row: DiscoveredRow = d.into();
            if let Some(slot) = rows.iter_mut().find(|r| r.address == address) {
                *slot = row;
            } else {
                rows.push(row);
            }
        }
        *scanning.lock() = false;
    });

    tokio::spawn(async move {
        let _ = scan_stream(Duration::from_secs(15), tx).await;
    });
}

pub fn render(
    ctx: &egui::Context,
    ui_state: &mut ScanUi,
    cmd: &mpsc::UnboundedSender<Command>,
    profiles: &mut Vec<ConnectionProfile>,
    profiles_dirty: &mut bool,
    reconnect: &mut crate::ui::reconnect::ReconnectUi,
) {
    if !ui_state.open {
        return;
    }
    let mut close = false;
    let mut rescan = false;
    let mut start_connect: Option<(String, BleAddress)> = None;
    let mut save_profile: Option<(String, BleAddress)> = None;

    egui::Window::new("BLE Scan").collapsible(false).show(ctx, |ui| {
        let scanning = *ui_state.scanning.lock();
        ui.horizontal(|ui| {
            if scanning {
                ui.spinner();
                ui.label("Scanning…");
            }
            if ui.add_enabled(!scanning, egui::Button::new("Rescan")).clicked() {
                rescan = true;
            }
        });
        ui.separator();

        let rows = ui_state.results.lock().clone();
        if rows.is_empty() && !scanning {
            ui.weak("No Meshtastic devices found.");
        } else if rows.is_empty() {
            ui.weak("Looking for devices…");
        }
        for row in rows {
            ui.horizontal(|ui| {
                ui.label(&row.name);
                ui.monospace(row.address.as_str());
                if let Some(r) = row.rssi_dbm {
                    ui.label(format!("{r} dBm"));
                }
                if row.is_connected {
                    ui.colored_label(egui::Color32::LIGHT_GREEN, "connected");
                }
                if ui.button("Connect").clicked() {
                    start_connect = Some((row.name.clone(), row.address.clone()));
                }
                if ui.button("Save").clicked() {
                    save_profile = Some((row.name.clone(), row.address.clone()));
                }
            });
        }
        ui.separator();
        if ui.button("Close").clicked() {
            close = true;
        }
    });

    if rescan {
        open(ui_state);
    }
    if let Some((name, addr)) = start_connect {
        let profile = ConnectionProfile::Ble { name, address: addr };
        // Persist the profile as soon as the user chooses it — otherwise
        // auto-reconnect would have nothing to match last_active against on
        // restart.
        if !profiles.iter().any(|p| p.key() == profile.key()) {
            profiles.push(profile.clone());
            *profiles_dirty = true;
        }
        reconnect.mark_user_connect(&profile);
        let _ = cmd.send(Command::Connect(profile));
        close = true;
    }
    if let Some((name, addr)) = save_profile {
        let profile = ConnectionProfile::Ble { name, address: addr };
        if !profiles.iter().any(|p| p.key() == profile.key()) {
            profiles.push(profile);
            *profiles_dirty = true;
        }
    }
    if close {
        ui_state.open = false;
    }
}
