use crate::domain::ids::{ChannelIndex, PacketId};
use crate::domain::message::Recipient;
use crate::domain::profile::ConnectionProfile;

#[derive(Clone, Debug)]
pub enum Command {
    Connect(ConnectionProfile),
    Disconnect,
    SendText { channel: ChannelIndex, to: Recipient, text: String, want_ack: bool },
    AckTimeout(PacketId),
}
