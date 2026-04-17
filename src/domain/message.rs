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
    Pending,
    Delivered,
    Failed(String),
}

#[derive(Clone, Debug, PartialEq)]
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
