use std::collections::HashMap;
use std::time::Instant;

use crate::domain::channel::Channel;
use crate::domain::ids::{ConfigId, NodeId, PacketId};
use crate::domain::message::{DeliveryState, TextMessage};
use crate::domain::node::Node;
use crate::domain::profile::TransportKind;
use crate::domain::snapshot::DeviceSnapshot;

#[derive(Clone, Debug)]
pub enum SessionState {
    Disconnected,
    Connecting { transport: TransportKind, started: Instant },
    Handshake(HandshakeAcc),
    Ready(DeviceSnapshot),
    Failed { reason: String },
}

#[derive(Clone, Debug)]
pub struct HandshakeAcc {
    pub transport: TransportKind,
    pub config_id: ConfigId,
    pub my_node: Option<NodeId>,
    pub short_name: String,
    pub long_name: String,
    pub firmware: String,
    pub nodes: HashMap<NodeId, Node>,
    pub channels: Vec<Channel>,
    pub lora: Option<crate::domain::config::LoraSettings>,
    pub device: Option<crate::domain::config::DeviceSettings>,
    pub position: Option<crate::domain::config::PositionSettings>,
    pub power: Option<crate::domain::config::PowerSettings>,
    pub network: Option<crate::domain::config::NetworkSettings>,
    pub display: Option<crate::domain::config::DisplaySettings>,
    pub bluetooth: Option<crate::domain::config::BluetoothSettings>,
    pub mqtt: Option<crate::domain::config::MqttSettings>,
    pub telemetry: Option<crate::domain::config::TelemetrySettings>,
    pub neighbor_info: Option<crate::domain::config::NeighborInfoSettings>,
    pub store_forward: Option<crate::domain::config::StoreForwardSettings>,
    pub security: Option<crate::domain::config::SecuritySettings>,
}

pub fn start_handshake(transport: TransportKind, config_id: ConfigId) -> SessionState {
    SessionState::Handshake(HandshakeAcc {
        transport,
        config_id,
        my_node: None,
        short_name: String::new(),
        long_name: String::new(),
        firmware: String::new(),
        nodes: HashMap::new(),
        channels: Vec::new(),
        lora: None,
        device: None,
        position: None,
        power: None,
        network: None,
        display: None,
        bluetooth: None,
        mqtt: None,
        telemetry: None,
        neighbor_info: None,
        store_forward: None,
        security: None,
    })
}

#[derive(Clone, Debug)]
pub enum HandshakeFragment {
    MyNode { id: NodeId },
    Firmware(String),
    Node(Node),
    Channel(Channel),
    Lora(crate::domain::config::LoraSettings),
    Device(crate::domain::config::DeviceSettings),
    Position(crate::domain::config::PositionSettings),
    Power(crate::domain::config::PowerSettings),
    Network(crate::domain::config::NetworkSettings),
    Display(crate::domain::config::DisplaySettings),
    Bluetooth(crate::domain::config::BluetoothSettings),
    Mqtt(crate::domain::config::MqttSettings),
    Telemetry(crate::domain::config::TelemetrySettings),
    NeighborInfo(crate::domain::config::NeighborInfoSettings),
    StoreForward(crate::domain::config::StoreForwardSettings),
    Security(crate::domain::config::SecuritySettings),
    ConfigComplete { id: ConfigId },
    Message(TextMessage),
    MessageStateChanged { id: PacketId, state: DeliveryState },
    NodeMetric { id: NodeId, update: NodeMetric },
}

#[derive(Clone, Debug)]
pub enum NodeMetric {
    Battery(u8),
    Voltage(f32),
    Snr(f32),
    Rssi(i32),
}

pub fn apply(state: SessionState, event: HandshakeFragment) -> SessionState {
    match state {
        SessionState::Disconnected => SessionState::Disconnected,
        SessionState::Connecting { transport, started } => {
            SessionState::Connecting { transport, started }
        }
        SessionState::Failed { reason } => SessionState::Failed { reason },
        SessionState::Handshake(acc) => apply_handshake(acc, event),
        SessionState::Ready(snap) => apply_ready(snap, event),
    }
}

fn apply_handshake(mut acc: HandshakeAcc, event: HandshakeFragment) -> SessionState {
    match event {
        HandshakeFragment::MyNode { id } => acc.my_node = Some(id),
        HandshakeFragment::Firmware(version) => acc.firmware = version,
        HandshakeFragment::Node(node) => {
            acc.nodes.insert(node.id, node);
        }
        HandshakeFragment::Channel(channel) => upsert_channel(&mut acc.channels, channel),
        HandshakeFragment::Lora(settings) => acc.lora = Some(settings),
        HandshakeFragment::Device(settings) => acc.device = Some(settings),
        HandshakeFragment::Position(settings) => acc.position = Some(settings),
        HandshakeFragment::Power(settings) => acc.power = Some(settings),
        HandshakeFragment::Network(settings) => acc.network = Some(settings),
        HandshakeFragment::Display(settings) => acc.display = Some(settings),
        HandshakeFragment::Bluetooth(settings) => acc.bluetooth = Some(settings),
        HandshakeFragment::Mqtt(settings) => acc.mqtt = Some(settings),
        HandshakeFragment::Telemetry(settings) => acc.telemetry = Some(settings),
        HandshakeFragment::NeighborInfo(settings) => acc.neighbor_info = Some(settings),
        HandshakeFragment::StoreForward(settings) => acc.store_forward = Some(settings),
        HandshakeFragment::Security(settings) => acc.security = Some(settings),
        HandshakeFragment::ConfigComplete { id } => return finalise_handshake(acc, id),
        HandshakeFragment::Message(_)
        | HandshakeFragment::MessageStateChanged { .. }
        | HandshakeFragment::NodeMetric { .. } => {}
    }
    SessionState::Handshake(acc)
}

fn finalise_handshake(acc: HandshakeAcc, id: ConfigId) -> SessionState {
    if id != acc.config_id {
        return SessionState::Handshake(acc);
    }
    let Some(my_node) = acc.my_node else {
        return SessionState::Handshake(acc);
    };
    let (short_name, long_name) = match acc.nodes.get(&my_node) {
        Some(n) if !n.short_name.is_empty() || !n.long_name.is_empty() => {
            (n.short_name.clone(), n.long_name.clone())
        }
        _ => (acc.short_name, acc.long_name),
    };
    SessionState::Ready(DeviceSnapshot {
        my_node,
        short_name,
        long_name,
        firmware_version: acc.firmware,
        nodes: acc.nodes,
        channels: acc.channels,
        messages: Vec::new(),
        lora: acc.lora,
        device: acc.device,
        position: acc.position,
        power: acc.power,
        network: acc.network,
        display: acc.display,
        bluetooth: acc.bluetooth,
        mqtt: acc.mqtt,
        telemetry: acc.telemetry,
        neighbor_info: acc.neighbor_info,
        store_forward: acc.store_forward,
        security: acc.security,
        stats: crate::domain::stats::MeshStats::default(),
    })
}

fn apply_ready(mut snap: DeviceSnapshot, event: HandshakeFragment) -> SessionState {
    match event {
        HandshakeFragment::MyNode { .. }
        | HandshakeFragment::Firmware(_)
        | HandshakeFragment::ConfigComplete { .. } => SessionState::Ready(snap),
        HandshakeFragment::Lora(settings) => {
            snap.lora = Some(settings);
            SessionState::Ready(snap)
        }
        HandshakeFragment::Device(settings) => {
            snap.device = Some(settings);
            SessionState::Ready(snap)
        }
        HandshakeFragment::Position(settings) => {
            snap.position = Some(settings);
            SessionState::Ready(snap)
        }
        HandshakeFragment::Power(settings) => {
            snap.power = Some(settings);
            SessionState::Ready(snap)
        }
        HandshakeFragment::Network(settings) => {
            snap.network = Some(settings);
            SessionState::Ready(snap)
        }
        HandshakeFragment::Display(settings) => {
            snap.display = Some(settings);
            SessionState::Ready(snap)
        }
        HandshakeFragment::Bluetooth(settings) => {
            snap.bluetooth = Some(settings);
            SessionState::Ready(snap)
        }
        HandshakeFragment::Mqtt(settings) => {
            snap.mqtt = Some(settings);
            SessionState::Ready(snap)
        }
        HandshakeFragment::Telemetry(settings) => {
            snap.telemetry = Some(settings);
            SessionState::Ready(snap)
        }
        HandshakeFragment::NeighborInfo(settings) => {
            snap.neighbor_info = Some(settings);
            SessionState::Ready(snap)
        }
        HandshakeFragment::StoreForward(settings) => {
            snap.store_forward = Some(settings);
            SessionState::Ready(snap)
        }
        HandshakeFragment::Security(settings) => {
            snap.security = Some(settings);
            SessionState::Ready(snap)
        }
        HandshakeFragment::Node(node) => {
            snap.nodes.insert(node.id, node);
            SessionState::Ready(snap)
        }
        HandshakeFragment::Channel(channel) => {
            snap.upsert_channel(channel);
            SessionState::Ready(snap)
        }
        HandshakeFragment::Message(msg) => {
            snap.messages.push(msg);
            SessionState::Ready(snap)
        }
        HandshakeFragment::MessageStateChanged { id, state } => {
            if let Some(m) = snap.messages.iter_mut().find(|m| m.id == id) {
                m.state = state;
            }
            SessionState::Ready(snap)
        }
        HandshakeFragment::NodeMetric { id, update } => {
            if let Some(node) = snap.nodes.get_mut(&id) {
                apply_metric(node, &update);
            }
            SessionState::Ready(snap)
        }
    }
}

fn upsert_channel(channels: &mut Vec<Channel>, channel: Channel) {
    match channels.iter_mut().find(|c| c.index == channel.index) {
        Some(existing) => *existing = channel,
        None => channels.push(channel),
    }
}

fn apply_metric(node: &mut Node, metric: &NodeMetric) {
    match *metric {
        NodeMetric::Battery(b) => node.battery_level = Some(b),
        NodeMetric::Voltage(v) => node.voltage_v = Some(v),
        NodeMetric::Snr(s) => node.snr_db = Some(s),
        NodeMetric::Rssi(r) => node.rssi_dbm = Some(r),
    }
}
