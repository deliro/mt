use crate::proto::meshtastic::config::bluetooth_config::PairingMode;
use crate::proto::meshtastic::config::device_config::{RebroadcastMode, Role as DeviceRole};
use crate::proto::meshtastic::config::display_config::DisplayUnits;
use crate::proto::meshtastic::config::lo_ra_config::{ModemPreset, RegionCode};
use crate::proto::meshtastic::config::position_config::GpsMode;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoraSettings {
    pub region: RegionCode,
    pub modem_preset: ModemPreset,
    pub use_preset: bool,
    pub hop_limit: u8,
    pub tx_enabled: bool,
    pub tx_power: i32,
}

impl Default for LoraSettings {
    fn default() -> Self {
        Self {
            region: RegionCode::Unset,
            modem_preset: ModemPreset::LongFast,
            use_preset: true,
            hop_limit: 3,
            tx_enabled: true,
            tx_power: 0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceSettings {
    pub role: DeviceRole,
    pub rebroadcast_mode: RebroadcastMode,
    pub node_info_broadcast_secs: u32,
    pub disable_triple_click: bool,
    pub led_heartbeat_disabled: bool,
    pub tzdef: String,
}

impl Default for DeviceSettings {
    fn default() -> Self {
        Self {
            role: DeviceRole::Client,
            rebroadcast_mode: RebroadcastMode::All,
            node_info_broadcast_secs: 10_800,
            disable_triple_click: false,
            led_heartbeat_disabled: false,
            tzdef: String::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PositionSettings {
    pub broadcast_secs: u32,
    pub smart_enabled: bool,
    pub fixed_position: bool,
    pub gps_update_interval: u32,
    pub gps_mode: GpsMode,
    pub smart_min_distance_m: u32,
    pub smart_min_interval_secs: u32,
}

impl Default for PositionSettings {
    fn default() -> Self {
        Self {
            broadcast_secs: 900,
            smart_enabled: true,
            fixed_position: false,
            gps_update_interval: 30,
            gps_mode: GpsMode::Enabled,
            smart_min_distance_m: 0,
            smart_min_interval_secs: 0,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PowerSettings {
    pub is_power_saving: bool,
    pub on_battery_shutdown_after_secs: u32,
    pub wait_bluetooth_secs: u32,
    pub ls_secs: u32,
    pub min_wake_secs: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NetworkSettings {
    pub wifi_enabled: bool,
    pub wifi_ssid: String,
    pub wifi_psk: String,
    pub ntp_server: String,
    pub eth_enabled: bool,
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum ScreenOrientation {
    #[default]
    Normal,
    Flipped,
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum ClockFormat {
    #[default]
    H24,
    H12,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DisplaySettings {
    pub screen_on_secs: u32,
    pub auto_carousel_secs: u32,
    pub orientation: ScreenOrientation,
    pub units: DisplayUnits,
    pub clock: ClockFormat,
    pub heading_bold: bool,
    pub wake_on_tap_or_motion: bool,
}

impl Default for DisplaySettings {
    fn default() -> Self {
        Self {
            screen_on_secs: 0,
            auto_carousel_secs: 0,
            orientation: ScreenOrientation::default(),
            units: DisplayUnits::Metric,
            clock: ClockFormat::default(),
            heading_bold: false,
            wake_on_tap_or_motion: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BluetoothSettings {
    pub enabled: bool,
    pub mode: PairingMode,
    pub fixed_pin: u32,
}

impl Default for BluetoothSettings {
    fn default() -> Self {
        Self { enabled: true, mode: PairingMode::RandomPin, fixed_pin: 0 }
    }
}

// ---- Label tables ----

pub const REGION_CHOICES: &[RegionCode] = &[
    RegionCode::Unset,
    RegionCode::Us,
    RegionCode::Eu433,
    RegionCode::Eu868,
    RegionCode::Cn,
    RegionCode::Jp,
    RegionCode::Anz,
    RegionCode::Anz433,
    RegionCode::Kr,
    RegionCode::Tw,
    RegionCode::Ru,
    RegionCode::In,
    RegionCode::Nz865,
    RegionCode::Th,
    RegionCode::Lora24,
    RegionCode::Ua433,
    RegionCode::Ua868,
    RegionCode::My433,
    RegionCode::My919,
    RegionCode::Sg923,
    RegionCode::Ph433,
    RegionCode::Ph868,
    RegionCode::Ph915,
    RegionCode::Kz433,
    RegionCode::Kz863,
    RegionCode::Np865,
    RegionCode::Br902,
];

pub const MODEM_PRESET_CHOICES: &[ModemPreset] = &[
    ModemPreset::LongFast,
    ModemPreset::LongModerate,
    ModemPreset::LongSlow,
    ModemPreset::MediumFast,
    ModemPreset::MediumSlow,
    ModemPreset::ShortFast,
    ModemPreset::ShortSlow,
    ModemPreset::ShortTurbo,
    ModemPreset::LongTurbo,
    ModemPreset::VeryLongSlow,
];

pub const DEVICE_ROLE_CHOICES: &[DeviceRole] = &[
    DeviceRole::Client,
    DeviceRole::ClientMute,
    DeviceRole::ClientHidden,
    DeviceRole::ClientBase,
    DeviceRole::Router,
    DeviceRole::RouterClient,
    DeviceRole::RouterLate,
    DeviceRole::Repeater,
    DeviceRole::Tracker,
    DeviceRole::Sensor,
    DeviceRole::Tak,
    DeviceRole::TakTracker,
    DeviceRole::LostAndFound,
];

pub const REBROADCAST_CHOICES: &[RebroadcastMode] = &[
    RebroadcastMode::All,
    RebroadcastMode::AllSkipDecoding,
    RebroadcastMode::LocalOnly,
    RebroadcastMode::KnownOnly,
    RebroadcastMode::None,
    RebroadcastMode::CorePortnumsOnly,
];

pub const GPS_MODE_CHOICES: &[GpsMode] =
    &[GpsMode::Disabled, GpsMode::Enabled, GpsMode::NotPresent];

pub const DISPLAY_UNITS_CHOICES: &[DisplayUnits] = &[DisplayUnits::Metric, DisplayUnits::Imperial];

pub const ORIENTATION_CHOICES: &[ScreenOrientation] =
    &[ScreenOrientation::Normal, ScreenOrientation::Flipped];

pub const CLOCK_CHOICES: &[ClockFormat] = &[ClockFormat::H24, ClockFormat::H12];

pub const fn orientation_label(o: ScreenOrientation) -> &'static str {
    match o {
        ScreenOrientation::Normal => "Normal",
        ScreenOrientation::Flipped => "Flipped",
    }
}

pub const fn clock_label(c: ClockFormat) -> &'static str {
    match c {
        ClockFormat::H24 => "24-hour",
        ClockFormat::H12 => "12-hour",
    }
}

pub const PAIRING_MODE_CHOICES: &[PairingMode] =
    &[PairingMode::RandomPin, PairingMode::FixedPin, PairingMode::NoPin];

pub fn region_label(region: RegionCode) -> &'static str {
    match region {
        RegionCode::Unset => "Unset",
        RegionCode::Us => "US",
        RegionCode::Eu433 => "EU 433",
        RegionCode::Eu868 => "EU 868",
        RegionCode::Cn => "China",
        RegionCode::Jp => "Japan",
        RegionCode::Anz => "Australia / NZ",
        RegionCode::Anz433 => "Australia / NZ 433",
        RegionCode::Kr => "Korea",
        RegionCode::Tw => "Taiwan",
        RegionCode::Ru => "Russia",
        RegionCode::In => "India",
        RegionCode::Nz865 => "New Zealand 865",
        RegionCode::Th => "Thailand",
        RegionCode::Lora24 => "LoRa 2.4 GHz",
        RegionCode::Ua433 => "Ukraine 433",
        RegionCode::Ua868 => "Ukraine 868",
        RegionCode::My433 => "Malaysia 433",
        RegionCode::My919 => "Malaysia 919",
        RegionCode::Sg923 => "Singapore 923",
        RegionCode::Ph433 => "Philippines 433",
        RegionCode::Ph868 => "Philippines 868",
        RegionCode::Ph915 => "Philippines 915",
        RegionCode::Kz433 => "Kazakhstan 433",
        RegionCode::Kz863 => "Kazakhstan 863",
        RegionCode::Np865 => "Nepal 865",
        RegionCode::Br902 => "Brazil 902",
    }
}

pub fn modem_preset_label(preset: ModemPreset) -> &'static str {
    match preset {
        ModemPreset::LongFast => "Long Fast",
        ModemPreset::LongModerate => "Long Moderate",
        ModemPreset::LongSlow => "Long Slow",
        ModemPreset::MediumFast => "Medium Fast",
        ModemPreset::MediumSlow => "Medium Slow",
        ModemPreset::ShortFast => "Short Fast",
        ModemPreset::ShortSlow => "Short Slow",
        ModemPreset::ShortTurbo => "Short Turbo",
        ModemPreset::LongTurbo => "Long Turbo",
        ModemPreset::VeryLongSlow => "Very Long Slow",
    }
}

pub const fn device_role_label(role: DeviceRole) -> &'static str {
    match role {
        DeviceRole::Client => "Client",
        DeviceRole::ClientMute => "Client Mute",
        DeviceRole::ClientHidden => "Client Hidden",
        DeviceRole::ClientBase => "Client Base",
        DeviceRole::Router => "Router",
        DeviceRole::RouterClient => "Router / Client",
        DeviceRole::RouterLate => "Router Late",
        DeviceRole::Repeater => "Repeater",
        DeviceRole::Tracker => "Tracker",
        DeviceRole::Sensor => "Sensor",
        DeviceRole::Tak => "TAK",
        DeviceRole::TakTracker => "TAK Tracker",
        DeviceRole::LostAndFound => "Lost and Found",
    }
}

pub const fn rebroadcast_label(mode: RebroadcastMode) -> &'static str {
    match mode {
        RebroadcastMode::All => "All",
        RebroadcastMode::AllSkipDecoding => "All (skip decoding)",
        RebroadcastMode::LocalOnly => "Local only",
        RebroadcastMode::KnownOnly => "Known only",
        RebroadcastMode::None => "None",
        RebroadcastMode::CorePortnumsOnly => "Core portnums only",
    }
}

pub const fn gps_mode_label(mode: GpsMode) -> &'static str {
    match mode {
        GpsMode::Disabled => "Disabled",
        GpsMode::Enabled => "Enabled",
        GpsMode::NotPresent => "Not present",
    }
}

pub const fn display_units_label(u: DisplayUnits) -> &'static str {
    match u {
        DisplayUnits::Metric => "Metric",
        DisplayUnits::Imperial => "Imperial",
    }
}

pub const fn pairing_mode_label(mode: PairingMode) -> &'static str {
    match mode {
        PairingMode::RandomPin => "Random PIN (on screen)",
        PairingMode::FixedPin => "Fixed PIN",
        PairingMode::NoPin => "No PIN",
    }
}
