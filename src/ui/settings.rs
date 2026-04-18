use std::collections::HashSet;

use eframe::egui;
use tokio::sync::mpsc;

use crate::domain::config::{
    BluetoothSettings, CLOCK_CHOICES, DEVICE_ROLE_CHOICES, DISPLAY_UNITS_CHOICES, DeviceSettings,
    DisplaySettings, GPS_MODE_CHOICES, LoraSettings, MODEM_PRESET_CHOICES, NetworkSettings,
    ORIENTATION_CHOICES, PAIRING_MODE_CHOICES, PositionSettings, PowerSettings,
    REBROADCAST_CHOICES, REGION_CHOICES, clock_label, device_role_label, display_units_label,
    gps_mode_label, modem_preset_label, orientation_label, pairing_mode_label, rebroadcast_label,
    region_label,
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
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum PendingClear {
    Messages,
    Nodes,
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
    pub previous_fixed_enabled: bool,
    pub pending_admin: Option<AdminAction>,
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
    cmd: &mpsc::UnboundedSender<Command>,
) {
    sync_from_snapshot(snapshot, settings_ui);
    egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
        sections(ui, settings_ui, cmd);
    });
}

fn sections(ui: &mut egui::Ui, s: &mut SettingsUi, cmd: &mpsc::UnboundedSender<Command>) {
    collapsible(ui, "Owner", |ui| owner_section(ui, s, cmd));
    collapsible(ui, "LoRa", |ui| lora_section(ui, s, cmd));
    collapsible(ui, "Device", |ui| device_section(ui, s, cmd));
    collapsible(ui, "Position", |ui| position_section(ui, s, cmd));
    collapsible(ui, "Power", |ui| power_section(ui, s, cmd));
    collapsible(ui, "Network", |ui| network_section(ui, s, cmd));
    collapsible(ui, "Display", |ui| display_section(ui, s, cmd));
    collapsible(ui, "Bluetooth", |ui| bluetooth_section(ui, s, cmd));
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
    ui.label(format!("Stored messages for this device: {msgs}"));
    ui.label(format!("Stored nodes for this device: {nodes}"));
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        if ui.button("Clear messages").clicked() {
            s.pending_clear = Some(PendingClear::Messages);
        }
        if ui.button("Clear nodes").clicked() {
            s.pending_clear = Some(PendingClear::Nodes);
        }
        if ui.button("Clear everything").clicked() {
            s.pending_clear = Some(PendingClear::All);
        }
    });
    ui.weak("Clears only the on-disk cache for this connected device. The mesh keeps its own copy.");
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

fn position_section(
    ui: &mut egui::Ui,
    s: &mut SettingsUi,
    cmd: &mpsc::UnboundedSender<Command>,
) {
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
    commit(
        s,
        Section::Power,
        dirty,
        ui,
        "Save Power",
        cmd,
        |d| Command::SetPower(d.power.clone()),
    );
}

// ---- Network ----

fn network_section(
    ui: &mut egui::Ui,
    s: &mut SettingsUi,
    cmd: &mpsc::UnboundedSender<Command>,
) {
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

fn display_section(
    ui: &mut egui::Ui,
    s: &mut SettingsUi,
    cmd: &mpsc::UnboundedSender<Command>,
) {
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
    commit(
        s,
        Section::Display,
        dirty,
        ui,
        "Save Display",
        cmd,
        |d| Command::SetDisplay(d.display.clone()),
    );
}

// ---- Bluetooth ----

fn bluetooth_section(
    ui: &mut egui::Ui,
    s: &mut SettingsUi,
    cmd: &mpsc::UnboundedSender<Command>,
) {
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
    commit(
        s,
        Section::Bluetooth,
        dirty,
        ui,
        "Save Bluetooth",
        cmd,
        |d| Command::SetBluetooth(d.bluetooth.clone()),
    );
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
    if !s.dirty.is(Section::Position) {
        if let Some(pos) = snapshot.nodes.get(&snapshot.my_node).and_then(|n| n.position.as_ref()) {
            s.draft.fixed_lat = pos.latitude_deg;
            s.draft.fixed_lon = pos.longitude_deg;
            s.draft.fixed_alt = pos.altitude_m.unwrap_or(0);
        }
        s.previous_fixed_enabled =
            snapshot.position.as_ref().is_some_and(|p| p.fixed_position);
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
