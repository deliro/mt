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
}
