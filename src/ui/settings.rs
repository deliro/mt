use std::collections::HashSet;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use eframe::egui;
use tokio::sync::mpsc;

use crate::domain::config::{
    BluetoothSettings, CLOCK_CHOICES, CannedMessageSettings, DEVICE_ROLE_CHOICES,
    DISPLAY_UNITS_CHOICES, DeviceSettings, DisplaySettings, ExternalNotificationSettings,
    GPS_MODE_CHOICES, LoraSettings, MODEM_PRESET_CHOICES, MqttSettings, NeighborInfoSettings,
    NetworkSettings, ORIENTATION_CHOICES, PAIRING_MODE_CHOICES, PositionSettings, PowerSettings,
    REBROADCAST_CHOICES, REGION_CHOICES, RangeTestSettings, SecuritySettings, StoreForwardSettings,
    TelemetrySettings, clock_label, device_role_label, display_units_label, gps_mode_label,
    modem_preset_label, orientation_label, pairing_mode_label, rebroadcast_label, region_label,
};
use crate::domain::snapshot::DeviceSnapshot;
use crate::session::commands::{AdminAction, Command};

#[derive(Copy, Clone, Debug, Eq, Hash, PartialEq)]
pub enum Section {
    Owner,
    Lora,
    Device,
    Position,
    Power,
    Network,
    Display,
    Bluetooth,
    Mqtt,
    Telemetry,
    NeighborInfo,
    StoreForward,
    Security,
    ExtNotif,
    Canned,
    RangeTest,
    Backup,
    Alerts,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum PendingClear {
    Messages,
    Nodes,
    Tiles,
    All,
}

#[derive(Default)]
pub struct SettingsUi {
    pub draft: Draft,
    pub dirty: DirtySet,
    pub last_save: Option<String>,
    pub pending_clear: Option<PendingClear>,
    pub stored_messages: Option<i64>,
    pub stored_nodes: Option<i64>,
    pub stored_tile_bytes: Option<u64>,
    pub previous_fixed_enabled: bool,
    pub pending_admin: Option<AdminAction>,
    pub security_new_admin_key: String,
    pub backup_import_open: bool,
    pub backup_import_text: String,
    pub backup_import_error: Option<String>,
    pub backup_last_action: Option<String>,
}

#[derive(Default, Clone)]
pub struct Draft {
    pub long_name: String,
    pub short_name: String,
    pub lora: LoraSettings,
    pub device: DeviceSettings,
    pub position: PositionSettings,
    pub fixed_lat: f64,
    pub fixed_lon: f64,
    pub fixed_alt: i32,
    pub power: PowerSettings,
    pub network: NetworkSettings,
    pub display: DisplaySettings,
    pub bluetooth: BluetoothSettings,
    pub mqtt: MqttSettings,
    pub telemetry: TelemetrySettings,
    pub neighbor_info: NeighborInfoSettings,
    pub store_forward: StoreForwardSettings,
    pub security: SecuritySettings,
    pub ext_notif: ExternalNotificationSettings,
    pub canned: CannedMessageSettings,
    pub range_test: RangeTestSettings,
}

#[derive(Default, Clone)]
pub struct DirtySet {
    sections: HashSet<Section>,
}

impl DirtySet {
    pub fn is(&self, s: Section) -> bool {
        self.sections.contains(&s)
    }
    pub fn mark(&mut self, s: Section) {
        let _ = self.sections.insert(s);
    }
    pub fn clear(&mut self, s: Section) {
        let _ = self.sections.remove(&s);
    }
}

pub fn render(
    ui: &mut egui::Ui,
    snapshot: &DeviceSnapshot,
    settings_ui: &mut SettingsUi,
    alerts_state: AlertsCtx<'_>,
    cmd: &mpsc::UnboundedSender<Command>,
) {
    sync_from_snapshot(snapshot, settings_ui);
    egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
        sections(ui, snapshot, settings_ui, alerts_state, cmd);
    });
}

pub struct AlertsCtx<'a> {
    pub config: &'a mut crate::ui::alerts::AlertConfig,
    pub dirty: &'a mut bool,
}

fn sections(
    ui: &mut egui::Ui,
    snapshot: &DeviceSnapshot,
    s: &mut SettingsUi,
    alerts_state: AlertsCtx<'_>,
    cmd: &mpsc::UnboundedSender<Command>,
) {
    collapsible(ui, "Owner", |ui| owner_section(ui, s, cmd));
    collapsible(ui, "LoRa", |ui| lora_section(ui, s, cmd));
    collapsible(ui, "Device", |ui| device_section(ui, s, cmd));
    collapsible(ui, "Position", |ui| position_section(ui, s, cmd));
    collapsible(ui, "Power", |ui| power_section(ui, s, cmd));
    collapsible(ui, "Network", |ui| network_section(ui, s, cmd));
    collapsible(ui, "Display", |ui| display_section(ui, s, cmd));
    collapsible(ui, "Bluetooth", |ui| bluetooth_section(ui, s, cmd));
    collapsible(ui, "MQTT", |ui| mqtt_section(ui, s, cmd));
    collapsible(ui, "Telemetry module", |ui| telemetry_section(ui, s, cmd));
    collapsible(ui, "Neighbor Info", |ui| neighbor_info_section(ui, s, cmd));
    collapsible(ui, "Store & Forward", |ui| store_forward_section(ui, s, cmd));
    collapsible(ui, "Security", |ui| security_section(ui, s, cmd));
    collapsible(ui, "External Notification", |ui| ext_notif_section(ui, s, cmd));
    collapsible(ui, "Canned Messages", |ui| canned_section(ui, s, cmd));
    collapsible(ui, "Range Test", |ui| range_test_section(ui, s, cmd));
    collapsible(ui, "Backup / restore", |ui| backup_section(ui, snapshot, s, cmd));
    collapsible(ui, "Alerts", |ui| alerts_section(ui, snapshot, alerts_state));
    collapsible(ui, "Admin", |ui| admin_section(ui, s));
    collapsible(ui, "Storage", |ui| storage_section(ui, s));
    admin_confirm_modal(ui.ctx(), s, cmd);
    if let Some(saved) = &s.last_save {
        ui.separator();
        ui.colored_label(
            egui::Color32::LIGHT_GREEN,
            format!("{saved} applied (device may reboot)"),
        );
    }
}

fn admin_section(ui: &mut egui::Ui, s: &mut SettingsUi) {
    ui.weak("These commands affect the connected device. Destructive ones ask for confirmation.");
    ui.add_space(4.0);
    admin_button(ui, s, AdminAction::Reboot { seconds: 5 });
    admin_button(ui, s, AdminAction::Shutdown { seconds: 5 });
    admin_button(ui, s, AdminAction::RebootOta { seconds: 5 });
    ui.separator();
    admin_button(ui, s, AdminAction::NodedbReset);
    admin_button(ui, s, AdminAction::FactoryResetConfig);
    admin_button(ui, s, AdminAction::FactoryResetDevice);
}

fn admin_button(ui: &mut egui::Ui, s: &mut SettingsUi, action: AdminAction) {
    ui.horizontal(|ui| {
        let tinted = if action.is_destructive() {
            egui::Button::new(action.label()).fill(egui::Color32::from_rgb(120, 30, 30))
        } else {
            egui::Button::new(action.label())
        };
        let resp = ui.add(tinted).on_hover_text(action.warning());
        if resp.clicked() {
            s.pending_admin = Some(action);
        }
    });
}

fn admin_confirm_modal(
    ctx: &egui::Context,
    s: &mut SettingsUi,
    cmd: &mpsc::UnboundedSender<Command>,
) {
    let Some(action) = s.pending_admin else { return };
    let mut confirm = false;
    let mut cancel = false;
    egui::Window::new(format!("Confirm: {}", action.label()))
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            ui.set_min_width(360.0);
            if action.is_destructive() {
                ui.colored_label(egui::Color32::LIGHT_RED, "⚠ Destructive action");
            }
            ui.label(action.warning());
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Cancel").clicked() {
                    cancel = true;
                }
                let confirm_btn = if action.is_destructive() {
                    egui::Button::new("Yes, do it").fill(egui::Color32::from_rgb(150, 40, 40))
                } else {
                    egui::Button::new("Confirm")
                };
                if ui.add(confirm_btn).clicked() {
                    confirm = true;
                }
            });
        });
    if confirm {
        let _ = cmd.send(Command::Admin(action));
        s.last_save = Some(action.label().into());
        s.pending_admin = None;
    } else if cancel {
        s.pending_admin = None;
    }
}

fn storage_section(ui: &mut egui::Ui, s: &mut SettingsUi) {
    let msgs = s.stored_messages.map_or_else(|| "?".into(), |n| n.to_string());
    let nodes = s.stored_nodes.map_or_else(|| "?".into(), |n| n.to_string());
    let tiles = s.stored_tile_bytes.map_or_else(|| "?".into(), format_bytes);
    ui.label(format!("Stored messages for this device: {msgs}"));
    ui.label(format!("Stored nodes for this device: {nodes}"));
    ui.label(format!("Map-tile cache: {tiles}"));
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        if ui.button("Clear messages").clicked() {
            s.pending_clear = Some(PendingClear::Messages);
        }
        if ui.button("Clear nodes").clicked() {
            s.pending_clear = Some(PendingClear::Nodes);
        }
        if ui.button("Clear map tiles").clicked() {
            s.pending_clear = Some(PendingClear::Tiles);
        }
        if ui.button("Clear everything").clicked() {
            s.pending_clear = Some(PendingClear::All);
        }
    });
    ui.weak(
        "Clears only the on-disk cache for this connected device. The mesh keeps its own copy.",
    );
}

fn format_bytes(n: u64) -> String {
    const K: u64 = 1024;
    const M: u64 = K * K;
    const G: u64 = M * K;
    if n >= G {
        format!("{:.1} GB", n as f64 / G as f64)
    } else if n >= M {
        format!("{:.1} MB", n as f64 / M as f64)
    } else if n >= K {
        format!("{:.1} KB", n as f64 / K as f64)
    } else {
        format!("{n} B")
    }
}

fn collapsible<R>(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui) -> R) {
    egui::CollapsingHeader::new(title).default_open(true).show(ui, add);
}

fn save_row(ui: &mut egui::Ui, dirty: bool, label: &str, enabled: bool) -> bool {
    let mut clicked = false;
    ui.horizontal(|ui| {
        if ui.add_enabled(enabled, egui::Button::new(label)).clicked() {
            clicked = true;
        }
        if dirty {
            ui.weak("unsaved changes");
        }
    });
    clicked
}

// ---- Owner ----

fn owner_section(ui: &mut egui::Ui, s: &mut SettingsUi, cmd: &mpsc::UnboundedSender<Command>) {
    text_line(
        ui,
        "Long name",
        &mut s.draft.long_name,
        &mut mark_on_change(&mut s.dirty, Section::Owner),
        "Full display name for this node. Shown on other Meshtastic devices and in the mesh node list.",
    );
    ui.horizontal(|ui| {
        ui.label("Short name:");
        let resp = ui
            .add(egui::TextEdit::singleline(&mut s.draft.short_name).desired_width(80.0))
            .on_hover_text(
                "Up to 4 characters shown on device OLED / heard-from summaries. Often initials or a nickname.",
            );
        if resp.changed() {
            s.dirty.mark(Section::Owner);
        }
        if s.draft.short_name.chars().count() > 4 {
            ui.colored_label(egui::Color32::LIGHT_RED, "4 chars max");
        }
    });
    let dirty = s.dirty.is(Section::Owner);
    let can_save = dirty && owner_valid(s);
    if save_row(ui, dirty, "Save owner", can_save) {
        let _ = cmd.send(Command::SetOwner {
            long_name: s.draft.long_name.clone(),
            short_name: s.draft.short_name.clone(),
        });
        s.dirty.clear(Section::Owner);
        s.last_save = Some("owner".into());
    }
}

fn owner_valid(s: &SettingsUi) -> bool {
    !s.draft.long_name.trim().is_empty()
        && !s.draft.short_name.trim().is_empty()
        && s.draft.short_name.chars().count() <= 4
}

fn mark_on_change(dirty: &mut DirtySet, section: Section) -> impl FnMut() + '_ {
    move || dirty.mark(section)
}

// ---- LoRa ----

fn lora_section(ui: &mut egui::Ui, s: &mut SettingsUi, cmd: &mpsc::UnboundedSender<Command>) {
    let mut dirty = s.dirty.is(Section::Lora);
    combo(
        ui,
        "Region",
        &mut s.draft.lora.region,
        REGION_CHOICES,
        region_label,
        &mut dirty,
        "Regulatory domain: must match where you physically operate. Controls legal frequencies and TX power. Default: Unset (device refuses to transmit until this is set).",
    );
    checkbox(
        ui,
        "Use preset",
        &mut s.draft.lora.use_preset,
        &mut dirty,
        "When on, the modem preset picks bandwidth/spread-factor/coding-rate. Recommended. Turn off only if you know what you're doing.",
    );
    combo(
        ui,
        "Modem preset",
        &mut s.draft.lora.modem_preset,
        MODEM_PRESET_CHOICES,
        modem_preset_label,
        &mut dirty,
        "Trade-off between range and throughput. LongFast is the mesh default and the most widely interoperable choice. Short* presets are faster but shorter range.",
    );
    u8_slider(
        ui,
        "Max hops",
        &mut s.draft.lora.hop_limit,
        1..=7,
        &mut dirty,
        "Maximum number of retransmissions a packet may make across the mesh. Higher = better coverage but more airtime cost. Default: 3. Max: 7.",
    );
    checkbox(
        ui,
        "TX enabled",
        &mut s.draft.lora.tx_enabled,
        &mut dirty,
        "Transmit is enabled. Turn off only for receive-only setups, antenna tests, or when silence is required. Default: on.",
    );
    i32_drag(
        ui,
        "TX power (dBm, 0=default)",
        &mut s.draft.lora.tx_power,
        0..=30,
        &mut dirty,
        "Antenna output power in dBm. 0 keeps the regional default (the safe, compliant value). Set manually only if the hardware supports it and you need reduced power for lab/testing.",
    );
    commit(s, Section::Lora, dirty, ui, "Save LoRa", cmd, |d| Command::SetLora(d.lora.clone()));
}

// ---- Device ----

fn device_section(ui: &mut egui::Ui, s: &mut SettingsUi, cmd: &mpsc::UnboundedSender<Command>) {
    let mut dirty = s.dirty.is(Section::Device);
    combo(
        ui,
        "Role",
        &mut s.draft.device.role,
        DEVICE_ROLE_CHOICES,
        device_role_label,
        &mut dirty,
        "Defines how the node participates in the mesh. 'Client' is the default for handhelds. 'Router'/'Repeater' should only be picked for fixed infrastructure nodes with good antennas — setting this incorrectly hurts the whole mesh.",
    );
    combo(
        ui,
        "Rebroadcast",
        &mut s.draft.device.rebroadcast_mode,
        REBROADCAST_CHOICES,
        rebroadcast_label,
        &mut dirty,
        "Which packets this node will forward. 'All' is the default. 'LocalOnly' restricts forwarding to your private channels; 'KnownOnly' further restricts to NodeDB entries. Setting to 'None' is only valid for Sensor/Tracker/TakTracker roles.",
    );
    u32_drag(
        ui,
        "NodeInfo broadcast (s)",
        &mut s.draft.device.node_info_broadcast_secs,
        0..=86_400,
        &mut dirty,
        "How often to broadcast our node identity (long/short name, etc.) to the mesh. Default: 10800 seconds (3 hours). Lower values give faster NodeDB sync at higher airtime cost.",
    );
    checkbox(
        ui,
        "Disable triple click",
        &mut s.draft.device.disable_triple_click,
        &mut dirty,
        "Disables the triple-press-of-user-button shortcut that toggles GPS power. Turn on if accidental presses keep disabling GPS. Default: off.",
    );
    checkbox(
        ui,
        "LED heartbeat disabled",
        &mut s.draft.device.led_heartbeat_disabled,
        &mut dirty,
        "Turns off the default blinking LED (LED_PIN) used as a liveness indicator. Useful for stealth or battery savings. Default: off.",
    );
    text_line(
        ui,
        "Timezone (POSIX TZ)",
        &mut s.draft.device.tzdef,
        &mut mark_on_change(&mut s.dirty, Section::Device),
        "POSIX timezone string for the device clock (e.g. 'EET-2EEST,M3.5.0/3,M10.5.0/4'). Leave empty to use UTC. See github.com/nayarsystems/posix_tz_db.",
    );
    let dirty = s.dirty.is(Section::Device);
    if save_row(ui, dirty, "Save Device", dirty) {
        let _ = cmd.send(Command::SetDevice(s.draft.device.clone()));
        s.dirty.clear(Section::Device);
        s.last_save = Some("Device".into());
    }
}

// ---- Position ----

fn position_section(ui: &mut egui::Ui, s: &mut SettingsUi, cmd: &mpsc::UnboundedSender<Command>) {
    let mut dirty = s.dirty.is(Section::Position);
    u32_drag(
        ui,
        "Broadcast (s)",
        &mut s.draft.position.broadcast_secs,
        0..=86_400,
        &mut dirty,
        "Interval between position broadcasts on the mesh, if the position changed meaningfully. Default: 900 seconds (15 minutes). 0 disables regular broadcasts.",
    );
    checkbox(
        ui,
        "Smart broadcast",
        &mut s.draft.position.smart_enabled,
        &mut dirty,
        "Adaptive broadcast: skip updates when stationary, send more often when moving. Recommended for handhelds. Default: on.",
    );
    checkbox(
        ui,
        "Fixed position",
        &mut s.draft.position.fixed_position,
        &mut dirty,
        "Treat this node as stationary at the coordinates below. The GPS is not required — the device broadcasts the saved coordinates. Useful for fixed base stations and repeaters.",
    );
    if s.draft.position.fixed_position {
        render_fixed_position(ui, s, &mut dirty);
    }
    u32_drag(
        ui,
        "GPS update interval (s)",
        &mut s.draft.position.gps_update_interval,
        0..=3_600,
        &mut dirty,
        "How often the GPS module tries to compute a fix (seconds). 0 = default of 30s. Very large values (e.g. 86400) keep the GPS off except at boot.",
    );
    combo(
        ui,
        "GPS mode",
        &mut s.draft.position.gps_mode,
        GPS_MODE_CHOICES,
        gps_mode_label,
        &mut dirty,
        "Whether GPS is powered on. Pick 'Disabled' for indoor/stationary use with Fixed position, 'Enabled' for mobile, 'Not present' for boards without a GPS module.",
    );
    u32_drag(
        ui,
        "Smart min distance (m)",
        &mut s.draft.position.smart_min_distance_m,
        0..=10_000,
        &mut dirty,
        "Minimum movement in meters before smart-broadcast will send an update. 0 uses firmware default. Only matters when Smart broadcast is on.",
    );
    u32_drag(
        ui,
        "Smart min interval (s)",
        &mut s.draft.position.smart_min_interval_secs,
        0..=3_600,
        &mut dirty,
        "Minimum seconds between smart-broadcast updates, even when moving. 0 uses firmware default. Only matters when Smart broadcast is on.",
    );
    s.dirty.sections.extend(dirty.then_some(Section::Position));
    let dirty = s.dirty.is(Section::Position);
    if save_row(ui, dirty, "Save Position", dirty) {
        save_position(s, cmd);
    }
}

fn render_fixed_position(ui: &mut egui::Ui, s: &mut SettingsUi, dirty: &mut bool) {
    ui.indent("fixed_pos", |ui| {
        ui.horizontal(|ui| {
            ui.label("Latitude:");
            let resp = ui
                .add(
                    egui::DragValue::new(&mut s.draft.fixed_lat)
                        .range(-90.0..=90.0)
                        .speed(0.0001)
                        .max_decimals(6)
                        .suffix("°"),
                )
                .on_hover_text("Decimal degrees, positive north.");
            if resp.changed() {
                *dirty = true;
            }
        });
        ui.horizontal(|ui| {
            ui.label("Longitude:");
            let resp = ui
                .add(
                    egui::DragValue::new(&mut s.draft.fixed_lon)
                        .range(-180.0..=180.0)
                        .speed(0.0001)
                        .max_decimals(6)
                        .suffix("°"),
                )
                .on_hover_text("Decimal degrees, positive east.");
            if resp.changed() {
                *dirty = true;
            }
        });
        ui.horizontal(|ui| {
            ui.label("Altitude:");
            let resp = ui
                .add(egui::DragValue::new(&mut s.draft.fixed_alt).range(-500..=9_000).suffix(" m"))
                .on_hover_text("Meters above mean sea level. Set 0 if unknown.");
            if resp.changed() {
                *dirty = true;
            }
        });
        ui.weak("Coordinates are sent via set_fixed_position on save.");
    });
}

fn save_position(s: &mut SettingsUi, cmd: &mpsc::UnboundedSender<Command>) {
    let _ = cmd.send(Command::SetPosition(s.draft.position.clone()));
    if s.draft.position.fixed_position {
        let _ = cmd.send(Command::SetFixedPosition {
            latitude_deg: s.draft.fixed_lat,
            longitude_deg: s.draft.fixed_lon,
            altitude_m: s.draft.fixed_alt,
        });
    } else if s.previous_fixed_enabled {
        let _ = cmd.send(Command::RemoveFixedPosition);
    }
    s.previous_fixed_enabled = s.draft.position.fixed_position;
    s.dirty.clear(Section::Position);
    s.last_save = Some("Position".into());
}

// ---- Power ----

fn power_section(ui: &mut egui::Ui, s: &mut SettingsUi, cmd: &mpsc::UnboundedSender<Command>) {
    let mut dirty = s.dirty.is(Section::Power);
    checkbox(
        ui,
        "Power saving",
        &mut s.draft.power.is_power_saving,
        &mut dirty,
        "Aggressively sleeps everything including the LoRa radio when idle. Only safe for Sensor/Tracker roles. Do NOT use if you rely on the phone app staying connected.",
    );
    u32_drag(
        ui,
        "Shutdown after (s, 0=off)",
        &mut s.draft.power.on_battery_shutdown_after_secs,
        0..=604_800,
        &mut dirty,
        "Automatically powers off this many seconds after external power is removed. 0 disables auto-shutdown. Useful for trackers that should not drain a small battery.",
    );
    u32_drag(
        ui,
        "Wait Bluetooth (s)",
        &mut s.draft.power.wait_bluetooth_secs,
        0..=3_600,
        &mut dirty,
        "How long to keep BLE awake after activity before sleeping it. 0 uses default (1 minute). ESP32 only.",
    );
    u32_drag(
        ui,
        "Light sleep (s)",
        &mut s.draft.power.ls_secs,
        0..=86_400,
        &mut dirty,
        "Seconds of idle before entering light sleep (CPU paused, LoRa on, BLE off, GPS on). 0 = default 300s. ESP32 only.",
    );
    u32_drag(
        ui,
        "Min wake (s)",
        &mut s.draft.power.min_wake_secs,
        0..=3_600,
        &mut dirty,
        "Once woken from light sleep by a LoRa packet, stay awake at least this long before sleeping again. 0 = default 10s.",
    );
    commit(s, Section::Power, dirty, ui, "Save Power", cmd, |d| Command::SetPower(d.power.clone()));
}

// ---- Network ----

fn network_section(ui: &mut egui::Ui, s: &mut SettingsUi, cmd: &mpsc::UnboundedSender<Command>) {
    let mut dirty = s.dirty.is(Section::Network);
    checkbox(
        ui,
        "Wi-Fi enabled",
        &mut s.draft.network.wifi_enabled,
        &mut dirty,
        "Enables the Wi-Fi radio on supported boards. Note: enabling Wi-Fi disables Bluetooth on the same device.",
    );
    text_line(
        ui,
        "SSID",
        &mut s.draft.network.wifi_ssid,
        &mut mark_on_change(&mut s.dirty, Section::Network),
        "Network name to join. The Meshtastic firmware does not expose a scan of nearby networks over the phone API — type the SSID manually.",
    );
    secret_line(
        ui,
        "PSK",
        &mut s.draft.network.wifi_psk,
        &mut mark_on_change(&mut s.dirty, Section::Network),
        "Wi-Fi password (WPA2). Stored on device; we never log it.",
    );
    text_line(
        ui,
        "NTP server",
        &mut s.draft.network.ntp_server,
        &mut mark_on_change(&mut s.dirty, Section::Network),
        "Host used to set device clock over Wi-Fi. Default: meshtastic.pool.ntp.org.",
    );
    checkbox(
        ui,
        "Ethernet enabled",
        &mut s.draft.network.eth_enabled,
        &mut dirty,
        "Enables the Ethernet interface on boards that have it (e.g. RAK Wireless gateway).",
    );
    let dirty = s.dirty.is(Section::Network);
    if save_row(ui, dirty, "Save Network", dirty) {
        let _ = cmd.send(Command::SetNetwork(s.draft.network.clone()));
        s.dirty.clear(Section::Network);
        s.last_save = Some("Network".into());
    }
}

// ---- Display ----

fn display_section(ui: &mut egui::Ui, s: &mut SettingsUi, cmd: &mpsc::UnboundedSender<Command>) {
    let mut dirty = s.dirty.is(Section::Display);
    u32_drag(
        ui,
        "Screen on (s, 0=default)",
        &mut s.draft.display.screen_on_secs,
        0..=3_600,
        &mut dirty,
        "How long the OLED stays on after a button press or incoming message. 0 = default 60s. Use a very large value to keep it always on (drains battery).",
    );
    u32_drag(
        ui,
        "Auto-carousel (s)",
        &mut s.draft.display.auto_carousel_secs,
        0..=3_600,
        &mut dirty,
        "Automatically rotate through screens every N seconds. 0 disables. Helpful for buttonless boards.",
    );
    combo(
        ui,
        "Orientation",
        &mut s.draft.display.orientation,
        ORIENTATION_CHOICES,
        orientation_label,
        &mut dirty,
        "Flip the screen vertically for upside-down mounting.",
    );
    combo(
        ui,
        "Units",
        &mut s.draft.display.units,
        DISPLAY_UNITS_CHOICES,
        display_units_label,
        &mut dirty,
        "Metric (meters, °C) or Imperial (feet, °F).",
    );
    combo(
        ui,
        "Clock",
        &mut s.draft.display.clock,
        CLOCK_CHOICES,
        clock_label,
        &mut dirty,
        "24-hour (default, international) or 12-hour AM/PM.",
    );
    checkbox(
        ui,
        "Heading bold",
        &mut s.draft.display.heading_bold,
        &mut dirty,
        "Render the first line of each screen in a bolder style.",
    );
    checkbox(
        ui,
        "Wake on tap/motion",
        &mut s.draft.display.wake_on_tap_or_motion,
        &mut dirty,
        "Wake the OLED when the accelerometer detects a tap or motion. Requires a supported IMU chip on the board.",
    );
    commit(s, Section::Display, dirty, ui, "Save Display", cmd, |d| {
        Command::SetDisplay(d.display.clone())
    });
}

// ---- Bluetooth ----

fn bluetooth_section(ui: &mut egui::Ui, s: &mut SettingsUi, cmd: &mpsc::UnboundedSender<Command>) {
    let mut dirty = s.dirty.is(Section::Bluetooth);
    checkbox(
        ui,
        "Enabled",
        &mut s.draft.bluetooth.enabled,
        &mut dirty,
        "Enable the BLE radio. Required for the phone app to talk to the device.",
    );
    combo(
        ui,
        "Pairing mode",
        &mut s.draft.bluetooth.mode,
        PAIRING_MODE_CHOICES,
        pairing_mode_label,
        &mut dirty,
        "RandomPin: device shows a fresh 6-digit PIN on screen each pair (recommended). FixedPin: always uses the PIN below (easier but weaker). NoPin: no PIN at all (insecure, legacy).",
    );
    u32_drag(
        ui,
        "Fixed PIN",
        &mut s.draft.bluetooth.fixed_pin,
        0..=999_999,
        &mut dirty,
        "6-digit PIN used when Pairing mode is FixedPin. Ignored otherwise.",
    );
    commit(s, Section::Bluetooth, dirty, ui, "Save Bluetooth", cmd, |d| {
        Command::SetBluetooth(d.bluetooth.clone())
    });
}

// ---- MQTT ----

fn mqtt_section(ui: &mut egui::Ui, s: &mut SettingsUi, cmd: &mpsc::UnboundedSender<Command>) {
    let mut dirty = s.dirty.is(Section::Mqtt);
    mqtt_transport_fields(ui, s, &mut dirty);
    mqtt_broker_fields(ui, s);
    mqtt_payload_fields(ui, s, &mut dirty);
    ui.separator();
    ui.label(egui::RichText::new("Map reporting").strong());
    mqtt_map_fields(ui, s, &mut dirty);
    commit(s, Section::Mqtt, dirty, ui, "Save MQTT", cmd, |d| Command::SetMqtt(d.mqtt.clone()));
}

fn mqtt_transport_fields(ui: &mut egui::Ui, s: &mut SettingsUi, dirty: &mut bool) {
    checkbox(
        ui,
        "Enabled",
        &mut s.draft.mqtt.enabled,
        dirty,
        "Enable the MQTT gateway module. When on, the device bridges channels marked Uplink/Downlink to an MQTT broker.",
    );
    checkbox(
        ui,
        "Proxy via phone",
        &mut s.draft.mqtt.proxy_to_client_enabled,
        dirty,
        "Use the connected phone/client as the MQTT transport instead of the device's own Wi-Fi. Handy when the node has no internet of its own.",
    );
    checkbox(
        ui,
        "TLS",
        &mut s.draft.mqtt.tls_enabled,
        dirty,
        "Connect to the broker over TLS. Required for most hosted brokers on port 8883.",
    );
}

fn mqtt_broker_fields(ui: &mut egui::Ui, s: &mut SettingsUi) {
    text_line(
        ui,
        "Broker address",
        &mut s.draft.mqtt.address,
        &mut mark_on_change(&mut s.dirty, Section::Mqtt),
        "Host[:port] of the MQTT broker. Leave empty to use the Meshtastic public server (mqtt.meshtastic.org).",
    );
    text_line(
        ui,
        "Username",
        &mut s.draft.mqtt.username,
        &mut mark_on_change(&mut s.dirty, Section::Mqtt),
        "Broker username. For the public server leave empty to use the default.",
    );
    secret_line(
        ui,
        "Password",
        &mut s.draft.mqtt.password,
        &mut mark_on_change(&mut s.dirty, Section::Mqtt),
        "Broker password. For the public server leave empty to use the default.",
    );
    text_line(
        ui,
        "Root topic",
        &mut s.draft.mqtt.root,
        &mut mark_on_change(&mut s.dirty, Section::Mqtt),
        "Topic prefix for all published messages. Default is 'msh'. Change only if you host multiple meshes on the same broker.",
    );
}

fn mqtt_payload_fields(ui: &mut egui::Ui, s: &mut SettingsUi, dirty: &mut bool) {
    checkbox(
        ui,
        "Publish encrypted payloads",
        &mut s.draft.mqtt.payload.encrypted,
        dirty,
        "When on, MQTT sees the same encrypted bytes the mesh sees — only holders of your channel PSK can decode. Off = broker sees plaintext (handy for your own dashboards).",
    );
    checkbox(
        ui,
        "Publish JSON too",
        &mut s.draft.mqtt.payload.json,
        dirty,
        "Also publish a human-readable JSON version of each packet on a parallel topic. Convenient for integrations, but doubles airtime bandwidth on the MQTT side.",
    );
}

fn mqtt_map_fields(ui: &mut egui::Ui, s: &mut SettingsUi, dirty: &mut bool) {
    checkbox(
        ui,
        "Report to public map",
        &mut s.draft.mqtt.map.enabled,
        dirty,
        "Periodically publish an unencrypted node-info packet so this device appears on map.meshtastic.org. Off by default — opt-in only.",
    );
    u32_drag(
        ui,
        "Publish every (s)",
        &mut s.draft.mqtt.map.publish_interval_secs,
        0..=86_400,
        dirty,
        "How often to send the map-report packet. 0 = firmware default (about once an hour). Respect the airtime budget.",
    );
    u32_drag(
        ui,
        "Position precision (bits)",
        &mut s.draft.mqtt.map.position_precision,
        0..=32,
        dirty,
        "Bits of latitude/longitude precision sent to the map. 32 = full, 0 = nothing. Lower values round coordinates; ~12-14 is a neighbourhood-level fuzz.",
    );
    checkbox(
        ui,
        "Share location on map",
        &mut s.draft.mqtt.map.publish_location,
        dirty,
        "If off, the map-report omits position even when map reporting itself is on. Use if you want the node listed without pinpointing its location.",
    );
}

// ---- Telemetry ----

fn telemetry_section(ui: &mut egui::Ui, s: &mut SettingsUi, cmd: &mpsc::UnboundedSender<Command>) {
    let mut dirty = s.dirty.is(Section::Telemetry);
    ui.label(egui::RichText::new("Device").strong());
    telemetry_device_fields(ui, s, &mut dirty);
    ui.separator();
    ui.label(egui::RichText::new("Environment").strong());
    telemetry_environment_fields(ui, s, &mut dirty);
    ui.separator();
    ui.label(egui::RichText::new("Air quality").strong());
    telemetry_family_fields(
        ui,
        "air_quality",
        &mut s.draft.telemetry.air_quality.measurement_enabled,
        &mut s.draft.telemetry.air_quality.screen_enabled,
        &mut s.draft.telemetry.air_quality.update_interval_secs,
        &mut dirty,
        "particulate / CO₂ sensor measurements",
    );
    ui.separator();
    ui.label(egui::RichText::new("Power").strong());
    telemetry_family_fields(
        ui,
        "power",
        &mut s.draft.telemetry.power.measurement_enabled,
        &mut s.draft.telemetry.power.screen_enabled,
        &mut s.draft.telemetry.power.update_interval_secs,
        &mut dirty,
        "INA219 / INA260 power metrics (bus voltage, current, shunt)",
    );
    ui.separator();
    ui.label(egui::RichText::new("Health").strong());
    telemetry_family_fields(
        ui,
        "health",
        &mut s.draft.telemetry.health.measurement_enabled,
        &mut s.draft.telemetry.health.screen_enabled,
        &mut s.draft.telemetry.health.update_interval_secs,
        &mut dirty,
        "heart-rate / SpO₂ / body-temp sensors",
    );
    commit(s, Section::Telemetry, dirty, ui, "Save Telemetry", cmd, |d| {
        Command::SetTelemetryCfg(d.telemetry.clone())
    });
}

fn telemetry_device_fields(ui: &mut egui::Ui, s: &mut SettingsUi, dirty: &mut bool) {
    checkbox(
        ui,
        "Broadcast device metrics",
        &mut s.draft.telemetry.device.enabled,
        dirty,
        "Periodically broadcast battery / voltage / airtime / channel utilization to the mesh. Off = send only to the phone, not over LoRa.",
    );
    u32_drag(
        ui,
        "Update every (s)",
        &mut s.draft.telemetry.device.update_interval_secs,
        0..=86_400,
        dirty,
        "How often device metrics are broadcast. 0 = firmware default (about 30 minutes).",
    );
}

fn telemetry_environment_fields(ui: &mut egui::Ui, s: &mut SettingsUi, dirty: &mut bool) {
    checkbox(
        ui,
        "Measurement enabled",
        &mut s.draft.telemetry.environment.measurement_enabled,
        dirty,
        "Enable reading temperature / humidity / pressure sensors and broadcasting them.",
    );
    checkbox(
        ui,
        "Show on device screen",
        &mut s.draft.telemetry.environment.screen_enabled,
        dirty,
        "Include the environment page in the rotating OLED carousel.",
    );
    checkbox(
        ui,
        "Display in Fahrenheit",
        &mut s.draft.telemetry.environment.display_fahrenheit,
        dirty,
        "Sensor is always read in °C; this toggle only controls the on-device display unit.",
    );
    u32_drag(
        ui,
        "Update every (s)",
        &mut s.draft.telemetry.environment.update_interval_secs,
        0..=86_400,
        dirty,
        "How often environment metrics are broadcast. 0 = firmware default.",
    );
}

fn telemetry_family_fields(
    ui: &mut egui::Ui,
    id_scope: &str,
    measurement: &mut bool,
    screen: &mut bool,
    interval: &mut u32,
    dirty: &mut bool,
    hint: &str,
) {
    ui.push_id(id_scope, |ui| {
        checkbox(
            ui,
            "Measurement enabled",
            measurement,
            dirty,
            &format!("Collect and broadcast {hint}. Off = stop reading the sensor entirely."),
        );
        checkbox(
            ui,
            "Show on device screen",
            screen,
            dirty,
            "Include this page in the rotating OLED carousel.",
        );
        u32_drag(
            ui,
            "Update every (s)",
            interval,
            0..=86_400,
            dirty,
            "How often these metrics are broadcast. 0 = firmware default.",
        );
    });
}

// ---- Neighbor Info ----

fn neighbor_info_section(
    ui: &mut egui::Ui,
    s: &mut SettingsUi,
    cmd: &mpsc::UnboundedSender<Command>,
) {
    let mut dirty = s.dirty.is(Section::NeighborInfo);
    checkbox(
        ui,
        "Enabled",
        &mut s.draft.neighbor_info.enabled,
        &mut dirty,
        "Run the NeighborInfo module. When on, the node periodically broadcasts which neighbours it hears, which helps mesh topology tools.",
    );
    checkbox(
        ui,
        "Transmit over LoRa",
        &mut s.draft.neighbor_info.transmit_over_lora,
        &mut dirty,
        "Also broadcast NeighborInfo on the mesh (not just MQTT / phone API). Note: firmware forbids this on a channel using the default key+name.",
    );
    u32_drag(
        ui,
        "Update every (s)",
        &mut s.draft.neighbor_info.update_interval_secs,
        0..=86_400,
        &mut dirty,
        "How often NeighborInfo is broadcast. Firmware enforces a minimum of 14400 (4 h) — smaller values are clamped to protect the airtime budget.",
    );
    commit(s, Section::NeighborInfo, dirty, ui, "Save Neighbor Info", cmd, |d| {
        Command::SetNeighborInfo(d.neighbor_info.clone())
    });
}

// ---- Store & Forward ----

fn store_forward_section(
    ui: &mut egui::Ui,
    s: &mut SettingsUi,
    cmd: &mpsc::UnboundedSender<Command>,
) {
    let mut dirty = s.dirty.is(Section::StoreForward);
    checkbox(
        ui,
        "Enabled",
        &mut s.draft.store_forward.enabled,
        &mut dirty,
        "Run the Store-and-Forward module. Required on both server and client sides to use message buffering.",
    );
    checkbox(
        ui,
        "Act as a server",
        &mut s.draft.store_forward.is_server,
        &mut dirty,
        "This node stores incoming messages and replays them to peers that request history. Server mode needs PSRAM — enabling it on a bare ESP32 without PSRAM is a no-op.",
    );
    checkbox(
        ui,
        "Broadcast heartbeat",
        &mut s.draft.store_forward.heartbeat,
        &mut dirty,
        "Servers periodically announce their presence so clients know whom to ask for history. Off = clients must know the server node-id.",
    );
    u32_drag(
        ui,
        "Server: buffer size (records)",
        &mut s.draft.store_forward.records,
        0..=10_000,
        &mut dirty,
        "How many recent packets the server keeps in its ring buffer. 0 = firmware default. Higher = more history, more PSRAM used.",
    );
    u32_drag(
        ui,
        "Client: max records per reply",
        &mut s.draft.store_forward.history_return_max,
        0..=10_000,
        &mut dirty,
        "When asking a server for history, cap on how many packets it may send back in one go. 0 = firmware default.",
    );
    u32_drag(
        ui,
        "Client: history window (s)",
        &mut s.draft.store_forward.history_return_window_secs,
        0..=604_800,
        &mut dirty,
        "Ask the server for messages no older than this many seconds. 0 = firmware default (typically 1h).",
    );
    commit(s, Section::StoreForward, dirty, ui, "Save Store & Forward", cmd, |d| {
        Command::SetStoreForward(d.store_forward.clone())
    });
}

// ---- Security ----

fn security_section(ui: &mut egui::Ui, s: &mut SettingsUi, cmd: &mpsc::UnboundedSender<Command>) {
    let mut dirty = s.dirty.is(Section::Security);
    security_identity(ui, &s.draft.security);
    ui.separator();
    security_admin_keys(ui, s, &mut dirty);
    ui.separator();
    security_flags(ui, s, &mut dirty);
    commit(s, Section::Security, dirty, ui, "Save Security", cmd, |d| {
        Command::SetSecurity(d.security.clone())
    });
}

fn security_identity(ui: &mut egui::Ui, sec: &SecuritySettings) {
    ui.label(egui::RichText::new("This node's identity").strong());
    ui.horizontal(|ui| {
        ui.label("Public key:");
        if sec.public_key.is_empty() {
            ui.colored_label(egui::Color32::YELLOW, "<firmware hasn't generated one yet>")
                .on_hover_text(
                    "Firmware ≥ 2.5 generates a Curve25519 keypair on first boot. If empty, \
                     the device may be older or hasn't finished its first boot.",
                );
        } else {
            let b64 = STANDARD.encode(&sec.public_key);
            ui.monospace(shorten(&b64, 24)).on_hover_text(b64.as_str());
            if ui.small_button("Copy").clicked() {
                ui.ctx().copy_text(b64);
            }
        }
    });
    ui.label(
        egui::RichText::new(
            "Your public key identifies this node. Share it with anyone who should be able to \
         remote-admin this node (they paste it into their Admin keys list) or send you \
         direct messages on firmware ≥ 2.5.",
        )
        .weak(),
    );
}

fn security_admin_keys(ui: &mut egui::Ui, s: &mut SettingsUi, dirty: &mut bool) {
    ui.label(egui::RichText::new("Admin keys (remote admin allowlist)").strong());
    ui.label(
        egui::RichText::new(
            "Up to 3 public keys that may administer this node over the mesh. If empty, the \
         only way to admin this node is a direct physical connection (BLE / serial / TCP).",
        )
        .weak(),
    );
    let mut remove: Option<usize> = None;
    for (idx, key) in s.draft.security.admin_keys.iter().enumerate() {
        ui.horizontal(|ui| {
            let b64 = STANDARD.encode(key);
            ui.monospace(format!("{}. {}", idx.saturating_add(1), shorten(&b64, 24)))
                .on_hover_text(b64);
            if ui.small_button("Remove").clicked() {
                remove = Some(idx);
            }
        });
    }
    if let Some(i) = remove
        && i < s.draft.security.admin_keys.len()
    {
        let _ = s.draft.security.admin_keys.remove(i);
        *dirty = true;
    }
    if s.draft.security.admin_keys.len() < 3 {
        ui.horizontal(|ui| {
            ui.label("Add:");
            ui.add(
                egui::TextEdit::singleline(&mut s.security_new_admin_key)
                    .hint_text("paste base64 public key")
                    .desired_width(280.0),
            );
            let parsed: Option<Vec<u8>> = if s.security_new_admin_key.trim().is_empty() {
                None
            } else {
                STANDARD.decode(s.security_new_admin_key.trim().as_bytes()).ok()
            };
            let valid = parsed.as_ref().is_some_and(|b| b.len() == 32);
            if ui.add_enabled(valid, egui::Button::new("Add")).clicked()
                && let Some(bytes) = parsed
            {
                s.draft.security.admin_keys.push(bytes);
                s.security_new_admin_key.clear();
                *dirty = true;
            }
            if !s.security_new_admin_key.trim().is_empty() && !valid {
                ui.colored_label(egui::Color32::LIGHT_RED, "expected 32-byte base64 key");
            }
        });
    } else {
        ui.label(egui::RichText::new("Maximum of 3 admin keys reached.").weak());
    }
}

fn security_flags(ui: &mut egui::Ui, s: &mut SettingsUi, dirty: &mut bool) {
    checkbox(
        ui,
        "Managed device",
        &mut s.draft.security.is_managed,
        dirty,
        "When on, the device refuses local configuration changes (over BLE / serial / TCP). \
         Only remote-admin via a key in the list above can change settings. Turn this on \
         only for deployments where you're sure remote admin still works.",
    );
    checkbox(
        ui,
        "Admin channel (legacy)",
        &mut s.draft.security.admin_channel_enabled,
        dirty,
        "Old PSK-based admin mechanism. Anyone on a channel named 'admin' can administer \
         this node. Off is strongly recommended on firmware ≥ 2.5 — use admin_key instead.",
    );
    checkbox(
        ui,
        "Serial console",
        &mut s.draft.security.console.serial_enabled,
        dirty,
        "Exposes a serial console on the USB port. Independent from the Meshtastic phone \
         API; meant for firmware-level debugging.",
    );
    checkbox(
        ui,
        "Debug log over API",
        &mut s.draft.security.console.debug_log_api_enabled,
        dirty,
        "Stream verbose firmware logs back to the phone / client. Normally suppressed as \
         soon as a client connects to keep the LoRa link quiet.",
    );
}

fn shorten(s: &str, take: usize) -> String {
    if s.chars().count() <= take {
        return s.to_owned();
    }
    let mut out: String = s.chars().take(take).collect();
    out.push('…');
    out
}

// ---- External Notification ----

fn ext_notif_section(ui: &mut egui::Ui, s: &mut SettingsUi, cmd: &mpsc::UnboundedSender<Command>) {
    let mut dirty = s.dirty.is(Section::ExtNotif);
    checkbox(
        ui,
        "Enabled",
        &mut s.draft.ext_notif.enabled,
        &mut dirty,
        "Run the external notification module — drives an LED / vibra / buzzer pin on incoming messages.",
    );
    ext_notif_timing(ui, s, &mut dirty);
    ui.separator();
    ui.label(egui::RichText::new("Output pins (board-specific)").strong());
    ext_notif_outputs(ui, s, &mut dirty);
    ui.separator();
    ui.label(egui::RichText::new("Alerts").strong());
    ext_notif_alerts(ui, s, &mut dirty);
    ui.separator();
    ui.label(egui::RichText::new("Sound").strong());
    ext_notif_sound(ui, s, &mut dirty);
    commit(s, Section::ExtNotif, dirty, ui, "Save External Notification", cmd, |d| {
        Command::SetExtNotif(d.ext_notif.clone())
    });
}

fn ext_notif_timing(ui: &mut egui::Ui, s: &mut SettingsUi, dirty: &mut bool) {
    u32_drag(
        ui,
        "Output on (ms)",
        &mut s.draft.ext_notif.output_ms,
        0..=60_000,
        dirty,
        "How long the output stays active per alert. Default 1000 ms.",
    );
    u32_drag(
        ui,
        "Nag timeout (s)",
        &mut s.draft.ext_notif.nag_timeout_secs,
        0..=3_600,
        dirty,
        "Keep pulsing until acknowledged for this many seconds. 0 = only one pulse.",
    );
}

fn ext_notif_outputs(ui: &mut egui::Ui, s: &mut SettingsUi, dirty: &mut bool) {
    u32_drag(
        ui,
        "LED pin",
        &mut s.draft.ext_notif.outputs.output_pin,
        0..=64,
        dirty,
        "GPIO that drives the status LED. 0 = use the board's default.",
    );
    u32_drag(
        ui,
        "Vibra pin",
        &mut s.draft.ext_notif.outputs.output_vibra_pin,
        0..=64,
        dirty,
        "GPIO for an optional vibration motor. 0 = unused.",
    );
    u32_drag(
        ui,
        "Buzzer pin",
        &mut s.draft.ext_notif.outputs.output_buzzer_pin,
        0..=64,
        dirty,
        "GPIO for an optional active buzzer. 0 = unused.",
    );
    checkbox(
        ui,
        "Active-high output",
        &mut s.draft.ext_notif.outputs.active_high,
        dirty,
        "If on, the LED pin goes HIGH when alerting. Off = LOW.",
    );
}

fn ext_notif_alerts(ui: &mut egui::Ui, s: &mut SettingsUi, dirty: &mut bool) {
    checkbox(
        ui,
        "On message: LED",
        &mut s.draft.ext_notif.alerts.message.led,
        dirty,
        "Pulse the LED when a text message arrives.",
    );
    checkbox(
        ui,
        "On message: vibra",
        &mut s.draft.ext_notif.alerts.message.vibra,
        dirty,
        "Pulse the vibration motor on new text messages.",
    );
    checkbox(
        ui,
        "On message: buzzer",
        &mut s.draft.ext_notif.alerts.message.buzzer,
        dirty,
        "Beep the buzzer on new text messages.",
    );
    checkbox(
        ui,
        "On bell: LED",
        &mut s.draft.ext_notif.alerts.bell.led,
        dirty,
        "Pulse the LED when a bell character (\\x07) arrives — used by Canned Messages with 'send bell'.",
    );
    checkbox(
        ui,
        "On bell: vibra",
        &mut s.draft.ext_notif.alerts.bell.vibra,
        dirty,
        "Pulse the vibra on bell characters.",
    );
    checkbox(
        ui,
        "On bell: buzzer",
        &mut s.draft.ext_notif.alerts.bell.buzzer,
        dirty,
        "Beep the buzzer on bell characters.",
    );
}

fn ext_notif_sound(ui: &mut egui::Ui, s: &mut SettingsUi, dirty: &mut bool) {
    checkbox(
        ui,
        "Use PWM tone",
        &mut s.draft.ext_notif.sound.use_pwm,
        dirty,
        "Drive the device.buzzer_gpio as a PWM tone instead of a simple on/off. Ignores the output_ms / active / pin fields above.",
    );
    checkbox(
        ui,
        "Use I²S speaker as buzzer",
        &mut s.draft.ext_notif.sound.use_i2s_as_buzzer,
        dirty,
        "On boards with native audio (T-Watch S3, T-Deck) play RTTTL melodies over the speaker instead of driving a buzzer pin.",
    );
}

// ---- Canned Messages ----

fn canned_section(ui: &mut egui::Ui, s: &mut SettingsUi, cmd: &mpsc::UnboundedSender<Command>) {
    let mut dirty = s.dirty.is(Section::Canned);
    ui.label(
        egui::RichText::new(
            "Input device for selecting pre-canned message presets on headless devices.",
        )
        .weak(),
    );
    checkbox(
        ui,
        "Rotary encoder",
        &mut s.draft.canned.rotary1_enabled,
        &mut dirty,
        "Enable a dumb rotary encoder producing A/B pulses. Use with devices that have a knob and press button.",
    );
    checkbox(
        ui,
        "Up / Down buttons",
        &mut s.draft.canned.updown1_enabled,
        &mut dirty,
        "Enable a 3-button up/down/select setup (e.g. RAK rotary encoder dev board or three buttons).",
    );
    checkbox(
        ui,
        "Send bell character",
        &mut s.draft.canned.send_bell,
        &mut dirty,
        "Append a bell (\\x07) to outgoing canned messages — receiving nodes with External Notification 'on bell' will alert.",
    );
    ui.separator();
    ui.label(egui::RichText::new("GPIO pins").strong());
    u32_drag(ui, "Pin A", &mut s.draft.canned.rotary_pin_a, 0..=64, &mut dirty, "Rotary A signal.");
    u32_drag(ui, "Pin B", &mut s.draft.canned.rotary_pin_b, 0..=64, &mut dirty, "Rotary B signal.");
    u32_drag(
        ui,
        "Press pin",
        &mut s.draft.canned.rotary_pin_press,
        0..=64,
        &mut dirty,
        "Encoder press-button pin.",
    );
    commit(s, Section::Canned, dirty, ui, "Save Canned Messages", cmd, |d| {
        Command::SetCanned(d.canned.clone())
    });
}

// ---- Range Test ----

fn range_test_section(ui: &mut egui::Ui, s: &mut SettingsUi, cmd: &mpsc::UnboundedSender<Command>) {
    let mut dirty = s.dirty.is(Section::RangeTest);
    ui.colored_label(
        egui::Color32::YELLOW,
        "⚠ RangeTest burns airtime. Enable only for deliberate RF experiments.",
    );
    checkbox(
        ui,
        "Enabled",
        &mut s.draft.range_test.enabled,
        &mut dirty,
        "Turn the range-test module on. No broadcasts happen until the 'Send every (s)' interval is non-zero.",
    );
    u32_drag(
        ui,
        "Send every (s)",
        &mut s.draft.range_test.sender_secs,
        0..=3_600,
        &mut dirty,
        "How often this node broadcasts a range-test packet. 0 = receive-only (this node logs incoming tests but doesn't transmit).",
    );
    checkbox(
        ui,
        "Save to CSV (ESP32)",
        &mut s.draft.range_test.save,
        &mut dirty,
        "Log received tests to RangeTest.csv on the device's filesystem. ESP32 only.",
    );
    checkbox(
        ui,
        "Clear CSV on reboot",
        &mut s.draft.range_test.clear_on_reboot,
        &mut dirty,
        "Wipe RangeTest.csv on every boot. Use for fresh test runs.",
    );
    commit(s, Section::RangeTest, dirty, ui, "Save Range Test", cmd, |d| {
        Command::SetRangeTest(d.range_test.clone())
    });
}

// ---- Backup / restore ----

fn backup_section(
    ui: &mut egui::Ui,
    snapshot: &DeviceSnapshot,
    s: &mut SettingsUi,
    cmd: &mpsc::UnboundedSender<Command>,
) {
    ui.label(
        egui::RichText::new(
            "Export the current device configuration to JSON (for backup / cloning), or import a \
         previous export. Security keys are intentionally omitted — they stay device-specific.",
        )
        .weak(),
    );
    ui.horizontal(|ui| {
        if ui
            .button("Copy config JSON")
            .on_hover_text(
                "Serialise every core + module config section and the channel list into a JSON \
                 document, then copy it to the clipboard. Safe to paste into a file and keep as \
                 a backup.",
            )
            .clicked()
        {
            let export = crate::domain::config_export::export_snapshot(snapshot);
            ui.ctx().copy_text(crate::domain::config_export::encode(&export));
            s.backup_last_action = Some("Exported to clipboard".into());
        }
        if ui
            .button("Import config JSON…")
            .on_hover_text(
                "Paste a previously exported JSON document to replay every non-empty section \
                 onto this device. Each section goes through its own admin round-trip.",
            )
            .clicked()
        {
            s.backup_import_open = true;
            s.backup_import_error = None;
            s.backup_last_action = None;
        }
    });
    if let Some(msg) = s.backup_last_action.as_ref() {
        ui.colored_label(egui::Color32::LIGHT_GREEN, msg);
    }
    backup_import_modal(ui.ctx(), snapshot, s, cmd);
    if s.dirty.is(Section::Backup) {
        s.dirty.clear(Section::Backup);
    }
}

fn backup_import_modal(
    ctx: &egui::Context,
    snapshot: &DeviceSnapshot,
    s: &mut SettingsUi,
    cmd: &mpsc::UnboundedSender<Command>,
) {
    if !s.backup_import_open {
        return;
    }
    let mut open = s.backup_import_open;
    let mut apply = false;
    egui::Window::new("Import config JSON")
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .default_width(520.0)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            ui.colored_label(
                egui::Color32::LIGHT_RED,
                "⚠ Every non-empty section in the JSON overwrites the device's current config.",
            );
            ui.label("Paste the JSON here:");
            egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                ui.add(
                    egui::TextEdit::multiline(&mut s.backup_import_text)
                        .desired_rows(12)
                        .desired_width(f32::INFINITY),
                );
            });
            if let Some(err) = s.backup_import_error.as_ref() {
                ui.colored_label(egui::Color32::LIGHT_RED, err);
            }
            ui.horizontal(|ui| {
                if ui.button("Cancel").clicked() {
                    s.backup_import_open = false;
                }
                if ui
                    .add(egui::Button::new("Apply").fill(egui::Color32::from_rgb(150, 40, 40)))
                    .clicked()
                {
                    apply = true;
                }
            });
        });
    s.backup_import_open = open && !apply;
    if apply {
        apply_import(snapshot, s, cmd);
    }
}

fn apply_import(
    snapshot: &DeviceSnapshot,
    s: &mut SettingsUi,
    cmd: &mpsc::UnboundedSender<Command>,
) {
    match crate::domain::config_export::decode(&s.backup_import_text) {
        Ok(export) => {
            dispatch_import(snapshot, &export, cmd);
            s.backup_import_open = false;
            s.backup_import_text.clear();
            s.backup_import_error = None;
            s.backup_last_action =
                Some("Import dispatched — watch for each Save to echo back".into());
        }
        Err(e) => {
            s.backup_import_error = Some(e.to_string());
        }
    }
}

fn dispatch_import(
    snapshot: &DeviceSnapshot,
    export: &crate::domain::config_export::ConfigExport,
    cmd: &mpsc::UnboundedSender<Command>,
) {
    if !export.owner.long_name.is_empty() || !export.owner.short_name.is_empty() {
        let _ = cmd.send(Command::SetOwner {
            long_name: export.owner.long_name.clone(),
            short_name: export.owner.short_name.clone(),
        });
    }
    dispatch_core_import(export, cmd);
    dispatch_security_import(snapshot, export, cmd);
    dispatch_module_import(export, cmd);
    dispatch_fixed_position_import(export, cmd);
    for channel in &export.channels {
        let _ = cmd.send(Command::SetChannel(channel.clone()));
    }
}

fn dispatch_core_import(
    export: &crate::domain::config_export::ConfigExport,
    cmd: &mpsc::UnboundedSender<Command>,
) {
    if let Some(v) = export.lora.clone() {
        let _ = cmd.send(Command::SetLora(v));
    }
    if let Some(v) = export.device.clone() {
        let _ = cmd.send(Command::SetDevice(v));
    }
    if let Some(v) = export.position.clone() {
        let _ = cmd.send(Command::SetPosition(v));
    }
    if let Some(v) = export.power.clone() {
        let _ = cmd.send(Command::SetPower(v));
    }
    if let Some(v) = export.network.clone() {
        let _ = cmd.send(Command::SetNetwork(v));
    }
    if let Some(v) = export.display.clone() {
        let _ = cmd.send(Command::SetDisplay(v));
    }
    if let Some(v) = export.bluetooth.clone() {
        let _ = cmd.send(Command::SetBluetooth(v));
    }
}

fn dispatch_security_import(
    snapshot: &DeviceSnapshot,
    export: &crate::domain::config_export::ConfigExport,
    cmd: &mpsc::UnboundedSender<Command>,
) {
    let Some(policy) = export.security_policy.as_ref() else { return };
    // Preserve the device's own keypair — the export never carried it, and
    // SetConfig(Security) with an empty keypair would wipe it on the target.
    let (public_key, private_key) = snapshot.security.as_ref().map_or_else(
        || (Vec::new(), Vec::new()),
        |s| (s.public_key.clone(), s.private_key.clone()),
    );
    let _ = cmd.send(Command::SetSecurity(crate::domain::config::SecuritySettings {
        public_key,
        private_key,
        admin_keys: policy.admin_keys.clone(),
        is_managed: policy.is_managed,
        admin_channel_enabled: policy.admin_channel_enabled,
        console: policy.console.clone(),
    }));
}

fn dispatch_module_import(
    export: &crate::domain::config_export::ConfigExport,
    cmd: &mpsc::UnboundedSender<Command>,
) {
    if let Some(v) = export.mqtt.clone() {
        let _ = cmd.send(Command::SetMqtt(v));
    }
    if let Some(v) = export.telemetry.clone() {
        let _ = cmd.send(Command::SetTelemetryCfg(v));
    }
    if let Some(v) = export.neighbor_info.clone() {
        let _ = cmd.send(Command::SetNeighborInfo(v));
    }
    if let Some(v) = export.store_forward.clone() {
        let _ = cmd.send(Command::SetStoreForward(v));
    }
    if let Some(v) = export.ext_notif.clone() {
        let _ = cmd.send(Command::SetExtNotif(v));
    }
    if let Some(v) = export.canned.clone() {
        let _ = cmd.send(Command::SetCanned(v));
    }
    if let Some(v) = export.range_test.clone() {
        let _ = cmd.send(Command::SetRangeTest(v));
    }
}

fn dispatch_fixed_position_import(
    export: &crate::domain::config_export::ConfigExport,
    cmd: &mpsc::UnboundedSender<Command>,
) {
    let Some(fp) = export.fixed_position.as_ref() else { return };
    let _ = cmd.send(Command::SetFixedPosition {
        latitude_deg: fp.latitude_deg,
        longitude_deg: fp.longitude_deg,
        altitude_m: fp.altitude_m,
    });
}

// ---- Alerts ----

fn alerts_section(ui: &mut egui::Ui, snapshot: &DeviceSnapshot, ctx: AlertsCtx<'_>) {
    ui.label(
        egui::RichText::new(
            "Fires native OS notifications on chosen mesh events. Runs entirely in this app; \
         the device doesn't know about these rules.",
        )
        .weak(),
    );
    let AlertsCtx { config, dirty } = ctx;
    if ui.checkbox(&mut config.enabled, "Alerts enabled").changed() {
        *dirty = true;
    }
    if ui.checkbox(&mut config.notify_on_dm, "Notify on direct messages").changed() {
        *dirty = true;
    }
    ui.separator();
    alerts_keywords(ui, config, dirty);
    ui.separator();
    alerts_battery_rules(ui, snapshot, config, dirty);
}

fn alerts_keywords(
    ui: &mut egui::Ui,
    config: &mut crate::ui::alerts::AlertConfig,
    dirty: &mut bool,
) {
    ui.label(egui::RichText::new("Keywords").strong());
    ui.label(
        egui::RichText::new(
            "Any message (DM or broadcast) containing one of these words fires an alert. \
         Matching is case-insensitive.",
        )
        .weak(),
    );
    let mut remove: Option<usize> = None;
    for (idx, kw) in config.keywords.iter().enumerate() {
        ui.horizontal(|ui| {
            ui.monospace(kw);
            if ui.small_button("Remove").clicked() {
                remove = Some(idx);
            }
        });
    }
    if let Some(i) = remove
        && i < config.keywords.len()
    {
        let _ = config.keywords.remove(i);
        *dirty = true;
    }
    ui.horizontal(|ui| {
        let id = ui.id().with("alerts_kw_input");
        let mut input = ui.data(|d| d.get_temp::<String>(id)).unwrap_or_default();
        ui.add(
            egui::TextEdit::singleline(&mut input).hint_text("add keyword").desired_width(200.0),
        );
        if ui.button("Add").clicked() && !input.trim().is_empty() {
            config.keywords.push(input.trim().to_owned());
            input.clear();
            *dirty = true;
        }
        ui.data_mut(|d| d.insert_temp(id, input));
    });
}

fn alerts_battery_rules(
    ui: &mut egui::Ui,
    snapshot: &DeviceSnapshot,
    config: &mut crate::ui::alerts::AlertConfig,
    dirty: &mut bool,
) {
    ui.label(egui::RichText::new("Battery thresholds").strong());
    ui.label(
        egui::RichText::new(
            "Alert when the battery level of a tracked node drops below the threshold. Fires \
         once per crossing (no spam at every telemetry update).",
        )
        .weak(),
    );
    let mut remove: Option<usize> = None;
    for (idx, rule) in config.battery_rules.iter().enumerate() {
        ui.horizontal(|ui| {
            let name = snapshot
                .nodes
                .get(&rule.node)
                .map_or_else(|| format!("!{:08x}", rule.node.0), node_display_name);
            ui.monospace(format!("{name} < {}%", rule.threshold_percent));
            if ui.small_button("Remove").clicked() {
                remove = Some(idx);
            }
        });
    }
    if let Some(i) = remove
        && i < config.battery_rules.len()
    {
        let _ = config.battery_rules.remove(i);
        *dirty = true;
    }
    alerts_battery_add_row(ui, snapshot, config, dirty);
}

fn alerts_battery_add_row(
    ui: &mut egui::Ui,
    snapshot: &DeviceSnapshot,
    config: &mut crate::ui::alerts::AlertConfig,
    dirty: &mut bool,
) {
    let node_id_input_key = ui.id().with("alerts_bat_node");
    let threshold_key = ui.id().with("alerts_bat_threshold");
    let mut node_id_str: String =
        ui.data(|d| d.get_temp::<String>(node_id_input_key)).unwrap_or_default();
    let mut threshold: u8 = ui.data(|d| d.get_temp::<u8>(threshold_key)).unwrap_or(20);
    let node_options: Vec<(crate::domain::ids::NodeId, String)> = {
        let mut v: Vec<_> = snapshot.nodes.values().map(|n| (n.id, node_display_name(n))).collect();
        v.sort_by(|a, b| a.1.cmp(&b.1));
        v
    };
    ui.horizontal(|ui| {
        ui.label("Add rule:");
        egui::ComboBox::from_id_salt("alerts_bat_pick")
            .selected_text(if node_id_str.is_empty() {
                "(pick node)".into()
            } else {
                node_id_str.clone()
            })
            .show_ui(ui, |ui| {
                for (id, name) in &node_options {
                    let label = format!("{name} !{:08x}", id.0);
                    if ui.selectable_label(node_id_str == label, &label).clicked() {
                        node_id_str.clone_from(&label);
                    }
                }
            });
        ui.add(egui::DragValue::new(&mut threshold).range(1..=100).suffix("%"));
        if ui.button("Add").clicked()
            && let Some((id, _)) = node_options.iter().find(|(id, n)| {
                let label = format!("{n} !{:08x}", id.0);
                label == node_id_str
            })
        {
            config
                .battery_rules
                .push(crate::ui::alerts::BatteryRule { node: *id, threshold_percent: threshold });
            node_id_str.clear();
            *dirty = true;
        }
    });
    ui.data_mut(|d| d.insert_temp(node_id_input_key, node_id_str));
    ui.data_mut(|d| d.insert_temp(threshold_key, threshold));
}

fn node_display_name(node: &crate::domain::node::Node) -> String {
    if !node.long_name.is_empty() {
        node.long_name.clone()
    } else if !node.short_name.is_empty() {
        node.short_name.clone()
    } else {
        format!("!{:08x}", node.id.0)
    }
}

// ---- Commit helper ----

fn commit(
    s: &mut SettingsUi,
    section: Section,
    dirty: bool,
    ui: &mut egui::Ui,
    button: &str,
    cmd: &mpsc::UnboundedSender<Command>,
    make_cmd: impl FnOnce(&Draft) -> Command,
) {
    if dirty {
        s.dirty.mark(section);
    }
    let is_dirty = s.dirty.is(section);
    if save_row(ui, is_dirty, button, is_dirty) {
        let _ = cmd.send(make_cmd(&s.draft));
        s.dirty.clear(section);
        s.last_save = Some(section_label(section).into());
    }
}

const fn section_label(section: Section) -> &'static str {
    match section {
        Section::Owner => "owner",
        Section::Lora => "LoRa",
        Section::Device => "Device",
        Section::Position => "Position",
        Section::Power => "Power",
        Section::Network => "Network",
        Section::Display => "Display",
        Section::Bluetooth => "Bluetooth",
        Section::Mqtt => "MQTT",
        Section::Telemetry => "Telemetry module",
        Section::NeighborInfo => "Neighbor Info",
        Section::StoreForward => "Store & Forward",
        Section::Security => "Security",
        Section::ExtNotif => "External Notification",
        Section::Canned => "Canned Messages",
        Section::RangeTest => "Range Test",
        Section::Backup => "Backup",
        Section::Alerts => "Alerts",
    }
}

// ---- Generic field helpers ----

fn combo<T: Copy + PartialEq>(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut T,
    choices: &[T],
    to_label: impl Fn(T) -> &'static str,
    dirty: &mut bool,
    hint: &str,
) {
    ui.horizontal(|ui| {
        ui.label(label).on_hover_text(hint);
        let resp = egui::ComboBox::from_id_salt(label)
            .selected_text(to_label(*value))
            .show_ui(ui, |ui| {
                for choice in choices {
                    if ui.selectable_value(value, *choice, to_label(*choice)).changed() {
                        *dirty = true;
                    }
                }
            })
            .response;
        if !hint.is_empty() {
            let _ = resp.on_hover_text(hint);
        }
    });
}

fn checkbox(ui: &mut egui::Ui, label: &str, value: &mut bool, dirty: &mut bool, hint: &str) {
    ui.horizontal(|ui| {
        ui.label(label).on_hover_text(hint);
        let resp = ui.checkbox(value, "");
        if resp.changed() {
            *dirty = true;
        }
        if !hint.is_empty() {
            let _ = resp.on_hover_text(hint);
        }
    });
}

fn text_line(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut String,
    mut on_change: impl FnMut(),
    hint: &str,
) {
    ui.horizontal(|ui| {
        ui.label(label).on_hover_text(hint);
        let resp = ui.text_edit_singleline(value);
        if resp.changed() {
            on_change();
        }
        if !hint.is_empty() {
            let _ = resp.on_hover_text(hint);
        }
    });
}

fn secret_line(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut String,
    mut on_change: impl FnMut(),
    hint: &str,
) {
    ui.horizontal(|ui| {
        ui.label(label).on_hover_text(hint);
        let resp = ui.add(egui::TextEdit::singleline(value).password(true));
        if resp.changed() {
            on_change();
        }
        if !hint.is_empty() {
            let _ = resp.on_hover_text(hint);
        }
    });
}

fn u32_drag(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut u32,
    range: std::ops::RangeInclusive<u32>,
    dirty: &mut bool,
    hint: &str,
) {
    ui.horizontal(|ui| {
        ui.label(label).on_hover_text(hint);
        let resp = ui.add(egui::DragValue::new(value).range(range));
        if resp.changed() {
            *dirty = true;
        }
        if !hint.is_empty() {
            let _ = resp.on_hover_text(hint);
        }
    });
}

fn i32_drag(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut i32,
    range: std::ops::RangeInclusive<i32>,
    dirty: &mut bool,
    hint: &str,
) {
    ui.horizontal(|ui| {
        ui.label(label).on_hover_text(hint);
        let resp = ui.add(egui::DragValue::new(value).range(range));
        if resp.changed() {
            *dirty = true;
        }
        if !hint.is_empty() {
            let _ = resp.on_hover_text(hint);
        }
    });
}

fn u8_slider(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut u8,
    range: std::ops::RangeInclusive<u8>,
    dirty: &mut bool,
    hint: &str,
) {
    ui.horizontal(|ui| {
        ui.label(label).on_hover_text(hint);
        let mut tmp = u32::from(*value);
        let (start, end) = (u32::from(*range.start()), u32::from(*range.end()));
        let resp = ui.add(egui::Slider::new(&mut tmp, start..=end));
        if resp.changed() {
            *value = u8::try_from(tmp).unwrap_or(*value);
            *dirty = true;
        }
        if !hint.is_empty() {
            let _ = resp.on_hover_text(hint);
        }
    });
}

// ---- Sync from snapshot ----

fn sync_from_snapshot(snapshot: &DeviceSnapshot, s: &mut SettingsUi) {
    if !s.dirty.is(Section::Owner) {
        let me = snapshot.nodes.get(&snapshot.my_node);
        s.draft.long_name = me.map_or_else(|| snapshot.long_name.clone(), |n| n.long_name.clone());
        s.draft.short_name =
            me.map_or_else(|| snapshot.short_name.clone(), |n| n.short_name.clone());
    }
    sync_section(snapshot.lora.as_ref(), &mut s.draft.lora, s.dirty.is(Section::Lora));
    sync_section(snapshot.device.as_ref(), &mut s.draft.device, s.dirty.is(Section::Device));
    sync_section(snapshot.position.as_ref(), &mut s.draft.position, s.dirty.is(Section::Position));
    sync_section(snapshot.power.as_ref(), &mut s.draft.power, s.dirty.is(Section::Power));
    sync_section(snapshot.network.as_ref(), &mut s.draft.network, s.dirty.is(Section::Network));
    sync_section(snapshot.display.as_ref(), &mut s.draft.display, s.dirty.is(Section::Display));
    sync_section(
        snapshot.bluetooth.as_ref(),
        &mut s.draft.bluetooth,
        s.dirty.is(Section::Bluetooth),
    );
    sync_section(snapshot.mqtt.as_ref(), &mut s.draft.mqtt, s.dirty.is(Section::Mqtt));
    sync_section(
        snapshot.telemetry.as_ref(),
        &mut s.draft.telemetry,
        s.dirty.is(Section::Telemetry),
    );
    sync_section(
        snapshot.neighbor_info.as_ref(),
        &mut s.draft.neighbor_info,
        s.dirty.is(Section::NeighborInfo),
    );
    sync_section(
        snapshot.store_forward.as_ref(),
        &mut s.draft.store_forward,
        s.dirty.is(Section::StoreForward),
    );
    sync_section(snapshot.security.as_ref(), &mut s.draft.security, s.dirty.is(Section::Security));
    sync_section(
        snapshot.ext_notif.as_ref(),
        &mut s.draft.ext_notif,
        s.dirty.is(Section::ExtNotif),
    );
    sync_section(snapshot.canned.as_ref(), &mut s.draft.canned, s.dirty.is(Section::Canned));
    sync_section(
        snapshot.range_test.as_ref(),
        &mut s.draft.range_test,
        s.dirty.is(Section::RangeTest),
    );
    if !s.dirty.is(Section::Position) {
        if let Some(pos) = snapshot.nodes.get(&snapshot.my_node).and_then(|n| n.position.as_ref()) {
            s.draft.fixed_lat = pos.latitude_deg;
            s.draft.fixed_lon = pos.longitude_deg;
            s.draft.fixed_alt = pos.altitude_m.unwrap_or(0);
        }
        s.previous_fixed_enabled = snapshot.position.as_ref().is_some_and(|p| p.fixed_position);
    }
}

fn sync_section<T: Clone>(src: Option<&T>, draft: &mut T, dirty: bool) {
    if dirty {
        return;
    }
    if let Some(value) = src {
        *draft = value.clone();
    }
}
