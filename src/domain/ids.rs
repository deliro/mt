use rand::RngExt;
use serde::{Deserialize, Serialize};

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default, Serialize, Deserialize)]
pub struct NodeId(pub u32);

pub const BROADCAST_NODE: NodeId = NodeId(0xFFFF_FFFF);

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub struct ChannelIndex(u8);

impl ChannelIndex {
    pub const MAX: u8 = 7;

    pub fn new(value: u8) -> Option<Self> {
        if value <= Self::MAX { Some(Self(value)) } else { None }
    }

    pub const fn primary() -> Self {
        Self(0)
    }

    pub fn get(self) -> u8 {
        self.0
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct PacketId(pub u32);

impl PacketId {
    pub fn random() -> Self {
        Self(rand::rng().random_range(1..=u32::MAX))
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct ConfigId(pub u32);

impl ConfigId {
    pub fn random() -> Self {
        Self(rand::rng().random_range(1..=u32::MAX))
    }
}

#[derive(Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub struct BleAddress(String);

impl BleAddress {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into().to_ascii_uppercase())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}
