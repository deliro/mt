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
    pub has_psk: bool,
}
