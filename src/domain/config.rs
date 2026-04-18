use crate::proto::meshtastic::config::lo_ra_config::{ModemPreset, RegionCode};

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
