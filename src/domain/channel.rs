use crate::domain::ids::ChannelIndex;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ChannelRole {
    Primary,
    Secondary,
    Disabled,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Channel {
    pub index: ChannelIndex,
    pub role: ChannelRole,
    pub name: String,
    pub psk: Vec<u8>,
    pub uplink_enabled: bool,
    pub downlink_enabled: bool,
    pub position_precision: u32,
}

impl Channel {
    pub fn has_psk(&self) -> bool {
        !self.psk.is_empty()
    }

    pub fn psk_summary(&self) -> PskSummary {
        match self.psk.as_slice() {
            [] => PskSummary::None,
            [n] => PskSummary::Preset(*n),
            k if k.len() == 16 => PskSummary::Aes128,
            k if k.len() == 32 => PskSummary::Aes256,
            k => PskSummary::Other(k.len()),
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum PskSummary {
    None,
    Preset(u8),
    Aes128,
    Aes256,
    Other(usize),
}

impl PskSummary {
    pub fn label(self) -> String {
        match self {
            Self::None => "no encryption".into(),
            Self::Preset(n) => format!("default preset #{n}"),
            Self::Aes128 => "AES128".into(),
            Self::Aes256 => "AES256".into(),
            Self::Other(n) => format!("{n}B"),
        }
    }
}
