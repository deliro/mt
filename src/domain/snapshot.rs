use std::collections::HashMap;

use crate::domain::channel::Channel;
use crate::domain::config::LoraSettings;
use crate::domain::ids::NodeId;
use crate::domain::message::TextMessage;
use crate::domain::node::Node;

#[derive(Clone, Debug, Default)]
pub struct DeviceSnapshot {
    pub my_node: NodeId,
    pub short_name: String,
    pub long_name: String,
    pub firmware_version: String,
    pub nodes: HashMap<NodeId, Node>,
    pub channels: Vec<Channel>,
    pub messages: Vec<TextMessage>,
    pub lora: Option<LoraSettings>,
}

impl DeviceSnapshot {
    pub fn upsert_channel(&mut self, channel: Channel) {
        match self.channels.iter_mut().find(|c| c.index == channel.index) {
            Some(existing) => *existing = channel,
            None => self.channels.push(channel),
        }
    }

    /// Insert a message, deduplicating on (from, id, direction). If a duplicate
    /// is found keep the existing entry but upgrade its `state` when the new
    /// state is strictly more terminal (Queued < Sent < Acked/Failed).
    pub fn upsert_message(&mut self, msg: crate::domain::message::TextMessage) {
        if let Some(existing) = self.messages.iter_mut().find(|m| {
            m.id == msg.id && m.from == msg.from && m.direction == msg.direction
        }) {
            if msg.state.rank() > existing.state.rank() {
                existing.state = msg.state;
            }
            return;
        }
        self.messages.push(msg);
    }
}
