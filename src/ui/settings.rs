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
use crate::session::commands::Command;

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

#[derive(Default)]
pub struct SettingsUi {
    pub draft: Draft,
    pub dirty: DirtySet,
    pub last_save: Option<String>,
}

#[derive(Default, Clone)]
pub struct Draft {
    pub long_name: String,
    pub short_name: String,
    pub lora: LoraSettings,
    pub device: DeviceSettings,
    pub position: PositionSettings,
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
    if let Some(saved) = &s.last_save {
        ui.separator();
        ui.colored_label(
            egui::Color32::LIGHT_GREEN,
            format!("{saved} applied (device may reboot)"),
        );
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
    ui.horizontal(|ui| {
        ui.label("Long name:");
        if ui.text_edit_singleline(&mut s.draft.long_name).changed() {
            s.dirty.mark(Section::Owner);
        }
    });
    ui.horizontal(|ui| {
        ui.label("Short name:");
        let resp =
            ui.add(egui::TextEdit::singleline(&mut s.draft.short_name).desired_width(80.0));
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

// ---- LoRa ----

fn lora_section(ui: &mut egui::Ui, s: &mut SettingsUi, cmd: &mpsc::UnboundedSender<Command>) {
    let mut dirty = s.dirty.is(Section::Lora);
    combo(ui, "Region", &mut s.draft.lora.region, REGION_CHOICES, region_label, &mut dirty);
    checkbox(ui, "Use preset", &mut s.draft.lora.use_preset, &mut dirty);
    combo(
        ui,
        "Modem preset",
        &mut s.draft.lora.modem_preset,
        MODEM_PRESET_CHOICES,
        modem_preset_label,
        &mut dirty,
    );
    u8_slider(ui, "Max hops", &mut s.draft.lora.hop_limit, 1..=7, &mut dirty);
    checkbox(ui, "TX enabled", &mut s.draft.lora.tx_enabled, &mut dirty);
    i32_drag(ui, "TX power (dBm, 0=default)", &mut s.draft.lora.tx_power, 0..=30, &mut dirty);
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
    );
    combo(
        ui,
        "Rebroadcast",
        &mut s.draft.device.rebroadcast_mode,
        REBROADCAST_CHOICES,
        rebroadcast_label,
        &mut dirty,
    );
    u32_drag(
        ui,
        "NodeInfo broadcast (s)",
        &mut s.draft.device.node_info_broadcast_secs,
        0..=86_400,
        &mut dirty,
    );
    checkbox(ui, "Disable triple click", &mut s.draft.device.disable_triple_click, &mut dirty);
    checkbox(ui, "LED heartbeat disabled", &mut s.draft.device.led_heartbeat_disabled, &mut dirty);
    text_line(ui, "Timezone (POSIX TZ)", &mut s.draft.device.tzdef, &mut dirty);
    commit(
        s,
        Section::Device,
        dirty,
        ui,
        "Save Device",
        cmd,
        |d| Command::SetDevice(d.device.clone()),
    );
}

// ---- Position ----

fn position_section(
    ui: &mut egui::Ui,
    s: &mut SettingsUi,
    cmd: &mpsc::UnboundedSender<Command>,
) {
    let mut dirty = s.dirty.is(Section::Position);
    u32_drag(ui, "Broadcast (s)", &mut s.draft.position.broadcast_secs, 0..=86_400, &mut dirty);
    checkbox(ui, "Smart broadcast", &mut s.draft.position.smart_enabled, &mut dirty);
    checkbox(ui, "Fixed position", &mut s.draft.position.fixed_position, &mut dirty);
    u32_drag(
        ui,
        "GPS update interval (s)",
        &mut s.draft.position.gps_update_interval,
        0..=3_600,
        &mut dirty,
    );
    combo(
        ui,
        "GPS mode",
        &mut s.draft.position.gps_mode,
        GPS_MODE_CHOICES,
        gps_mode_label,
        &mut dirty,
    );
    u32_drag(
        ui,
        "Smart min distance (m)",
        &mut s.draft.position.smart_min_distance_m,
        0..=10_000,
        &mut dirty,
    );
    u32_drag(
        ui,
        "Smart min interval (s)",
        &mut s.draft.position.smart_min_interval_secs,
        0..=3_600,
        &mut dirty,
    );
    commit(
        s,
        Section::Position,
        dirty,
        ui,
        "Save Position",
        cmd,
        |d| Command::SetPosition(d.position.clone()),
    );
}

// ---- Power ----

fn power_section(ui: &mut egui::Ui, s: &mut SettingsUi, cmd: &mpsc::UnboundedSender<Command>) {
    let mut dirty = s.dirty.is(Section::Power);
    checkbox(ui, "Power saving", &mut s.draft.power.is_power_saving, &mut dirty);
    u32_drag(
        ui,
        "Shutdown after (s, 0=off)",
        &mut s.draft.power.on_battery_shutdown_after_secs,
        0..=604_800,
        &mut dirty,
    );
    u32_drag(
        ui,
        "Wait Bluetooth (s)",
        &mut s.draft.power.wait_bluetooth_secs,
        0..=3_600,
        &mut dirty,
    );
    u32_drag(ui, "Light sleep (s)", &mut s.draft.power.ls_secs, 0..=86_400, &mut dirty);
    u32_drag(ui, "Min wake (s)", &mut s.draft.power.min_wake_secs, 0..=3_600, &mut dirty);
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
    checkbox(ui, "Wi-Fi enabled", &mut s.draft.network.wifi_enabled, &mut dirty);
    text_line(ui, "SSID", &mut s.draft.network.wifi_ssid, &mut dirty);
    secret_line(ui, "PSK", &mut s.draft.network.wifi_psk, &mut dirty);
    text_line(ui, "NTP server", &mut s.draft.network.ntp_server, &mut dirty);
    checkbox(ui, "Ethernet enabled", &mut s.draft.network.eth_enabled, &mut dirty);
    commit(
        s,
        Section::Network,
        dirty,
        ui,
        "Save Network",
        cmd,
        |d| Command::SetNetwork(d.network.clone()),
    );
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
    );
    u32_drag(
        ui,
        "Auto-carousel (s)",
        &mut s.draft.display.auto_carousel_secs,
        0..=3_600,
        &mut dirty,
    );
    combo(
        ui,
        "Orientation",
        &mut s.draft.display.orientation,
        ORIENTATION_CHOICES,
        orientation_label,
        &mut dirty,
    );
    combo(
        ui,
        "Units",
        &mut s.draft.display.units,
        DISPLAY_UNITS_CHOICES,
        display_units_label,
        &mut dirty,
    );
    combo(
        ui,
        "Clock",
        &mut s.draft.display.clock,
        CLOCK_CHOICES,
        clock_label,
        &mut dirty,
    );
    checkbox(ui, "Heading bold", &mut s.draft.display.heading_bold, &mut dirty);
    checkbox(ui, "Wake on tap/motion", &mut s.draft.display.wake_on_tap_or_motion, &mut dirty);
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
    checkbox(ui, "Enabled", &mut s.draft.bluetooth.enabled, &mut dirty);
    combo(
        ui,
        "Pairing mode",
        &mut s.draft.bluetooth.mode,
        PAIRING_MODE_CHOICES,
        pairing_mode_label,
        &mut dirty,
    );
    u32_drag(ui, "Fixed PIN", &mut s.draft.bluetooth.fixed_pin, 0..=999_999, &mut dirty);
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
) {
    ui.horizontal(|ui| {
        ui.label(format!("{label}:"));
        egui::ComboBox::from_id_salt(label).selected_text(to_label(*value)).show_ui(ui, |ui| {
            for choice in choices {
                if ui.selectable_value(value, *choice, to_label(*choice)).changed() {
                    *dirty = true;
                }
            }
        });
    });
}

fn checkbox(ui: &mut egui::Ui, label: &str, value: &mut bool, dirty: &mut bool) {
    ui.horizontal(|ui| {
        ui.label(format!("{label}:"));
        if ui.checkbox(value, "").changed() {
            *dirty = true;
        }
    });
}

fn text_line(ui: &mut egui::Ui, label: &str, value: &mut String, dirty: &mut bool) {
    ui.horizontal(|ui| {
        ui.label(format!("{label}:"));
        if ui.text_edit_singleline(value).changed() {
            *dirty = true;
        }
    });
}

fn secret_line(ui: &mut egui::Ui, label: &str, value: &mut String, dirty: &mut bool) {
    ui.horizontal(|ui| {
        ui.label(format!("{label}:"));
        if ui.add(egui::TextEdit::singleline(value).password(true)).changed() {
            *dirty = true;
        }
    });
}

fn u32_drag(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut u32,
    range: std::ops::RangeInclusive<u32>,
    dirty: &mut bool,
) {
    ui.horizontal(|ui| {
        ui.label(format!("{label}:"));
        if ui.add(egui::DragValue::new(value).range(range)).changed() {
            *dirty = true;
        }
    });
}

fn i32_drag(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut i32,
    range: std::ops::RangeInclusive<i32>,
    dirty: &mut bool,
) {
    ui.horizontal(|ui| {
        ui.label(format!("{label}:"));
        if ui.add(egui::DragValue::new(value).range(range)).changed() {
            *dirty = true;
        }
    });
}

fn u8_slider(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut u8,
    range: std::ops::RangeInclusive<u8>,
    dirty: &mut bool,
) {
    ui.horizontal(|ui| {
        ui.label(format!("{label}:"));
        let mut tmp = u32::from(*value);
        let (start, end) = (u32::from(*range.start()), u32::from(*range.end()));
        if ui.add(egui::Slider::new(&mut tmp, start..=end)).changed() {
            *value = u8::try_from(tmp).unwrap_or(*value);
            *dirty = true;
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
}

fn sync_section<T: Clone>(src: Option<&T>, draft: &mut T, dirty: bool) {
    if dirty {
        return;
    }
    if let Some(value) = src {
        *draft = value.clone();
    }
}
