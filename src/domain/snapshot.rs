use std::collections::HashMap;

use crate::domain::channel::Channel;
use crate::domain::config::{
    BluetoothSettings, CannedMessageSettings, DeviceSettings, DisplaySettings,
    ExternalNotificationSettings, LoraSettings, MqttSettings, NeighborInfoSettings,
    NetworkSettings, PositionSettings, PowerSettings, RangeTestSettings, SecuritySettings,
    StoreForwardSettings, TelemetrySettings,
};
use crate::domain::ids::NodeId;
use crate::domain::message::TextMessage;
use crate::domain::node::Node;
use crate::domain::stats::MeshStats;

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
    pub device: Option<DeviceSettings>,
    pub position: Option<PositionSettings>,
    pub power: Option<PowerSettings>,
    pub network: Option<NetworkSettings>,
    pub display: Option<DisplaySettings>,
    pub bluetooth: Option<BluetoothSettings>,
    pub mqtt: Option<MqttSettings>,
    pub telemetry: Option<TelemetrySettings>,
    pub neighbor_info: Option<NeighborInfoSettings>,
    pub store_forward: Option<StoreForwardSettings>,
    pub security: Option<SecuritySettings>,
    pub ext_notif: Option<ExternalNotificationSettings>,
    pub canned: Option<CannedMessageSettings>,
    pub range_test: Option<RangeTestSettings>,
    pub stats: MeshStats,
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
