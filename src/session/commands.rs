use crate::domain::config::{
    BluetoothSettings, DeviceSettings, DisplaySettings, LoraSettings, NetworkSettings,
    PositionSettings, PowerSettings,
};
use crate::domain::ids::{ChannelIndex, PacketId};
use crate::domain::message::Recipient;
use crate::domain::profile::ConnectionProfile;

#[derive(Clone, Debug)]
pub enum Command {
    Connect(ConnectionProfile),
    Disconnect,
    SendText { channel: ChannelIndex, to: Recipient, text: String, want_ack: bool },
    AckTimeout(PacketId),
    SetOwner { long_name: String, short_name: String },
    SetLora(LoraSettings),
    SetDevice(DeviceSettings),
    SetPosition(PositionSettings),
    SetPower(PowerSettings),
    SetNetwork(NetworkSettings),
    SetDisplay(DisplaySettings),
    SetBluetooth(BluetoothSettings),
    SetFixedPosition { latitude_deg: f64, longitude_deg: f64, altitude_m: i32 },
    RemoveFixedPosition,
    Admin(AdminAction),
    SetFavorite { node: crate::domain::ids::NodeId, favorite: bool },
    SetIgnored { node: crate::domain::ids::NodeId, ignored: bool },
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum AdminAction {
    Reboot { seconds: i32 },
    Shutdown { seconds: i32 },
    RebootOta { seconds: i32 },
    FactoryResetDevice,
    FactoryResetConfig,
    NodedbReset,
}

impl AdminAction {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Reboot { .. } => "Reboot",
            Self::Shutdown { .. } => "Shutdown",
            Self::RebootOta { .. } => "Reboot to OTA",
            Self::FactoryResetDevice => "Factory reset (device)",
            Self::FactoryResetConfig => "Factory reset (config only)",
            Self::NodedbReset => "Reset NodeDB",
        }
    }

    pub const fn is_destructive(self) -> bool {
        matches!(
            self,
            Self::FactoryResetDevice
                | Self::FactoryResetConfig
                | Self::NodedbReset
                | Self::Shutdown { .. }
                | Self::RebootOta { .. }
        )
    }

    pub const fn warning(self) -> &'static str {
        match self {
            Self::Reboot { .. } => "The device will reboot in a few seconds.",
            Self::Shutdown { .. } => {
                "The device will power off. You'll need to press its button to turn it back on."
            }
            Self::RebootOta { .. } => {
                "Reboots into OTA update mode. Firmware flashing happens in a separate tool."
            }
            Self::FactoryResetDevice => {
                "Wipes ALL configuration, channels, the NodeDB and keys. The device returns to factory defaults — this is irreversible."
            }
            Self::FactoryResetConfig => {
                "Wipes all device configuration (device/LoRa/position/etc) and channels. Keeps the NodeDB. Irreversible."
            }
            Self::NodedbReset => {
                "Clears the device's list of known nodes. They'll repopulate as nodes reappear on the mesh."
            }
        }
    }
}
