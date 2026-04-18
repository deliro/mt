use std::time::SystemTime;

use crate::domain::ids::{ChannelIndex, NodeId, PacketId};

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Direction {
    Incoming,
    Outgoing,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Recipient {
    Broadcast,
    Node(NodeId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeliveryState {
    Queued,
    Sent,
    Acked,
    Failed(String),
}

impl DeliveryState {
    pub fn rank(&self) -> u8 {
        match self {
            Self::Queued => 0,
            Self::Sent => 1,
            Self::Acked | Self::Failed(_) => 2,
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Acked | Self::Failed(_))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextMessage {
    pub id: PacketId,
    pub channel: ChannelIndex,
    pub from: NodeId,
    pub to: Recipient,
    pub text: String,
    pub received_at: SystemTime,
    pub direction: Direction,
    pub state: DeliveryState,
}
