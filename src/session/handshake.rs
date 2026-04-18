use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::domain::channel::{Channel, ChannelRole};
use crate::domain::config::LoraSettings;
use crate::domain::ids::{BROADCAST_NODE, ChannelIndex, ConfigId, NodeId, PacketId};
use crate::domain::message::{DeliveryState, Direction, Recipient, TextMessage};
use crate::domain::node::{Node, NodeRole, Position};
use crate::domain::session::HandshakeFragment;
use crate::proto::meshtastic;
use crate::proto::port::{PortPayload, parse};

pub fn fragments_from_radio(msg: meshtastic::FromRadio) -> Vec<HandshakeFragment> {
    use meshtastic::from_radio::PayloadVariant;
    let Some(variant) = msg.payload_variant else { return Vec::new() };
    match variant {
        PayloadVariant::MyInfo(info) => {
            vec![HandshakeFragment::MyNode { id: NodeId(info.my_node_num) }]
        }
        PayloadVariant::NodeInfo(ni) => vec![HandshakeFragment::Node(node_from_proto(&ni))],
        PayloadVariant::Channel(ch) => channel_fragments(ch),
        PayloadVariant::Metadata(meta) => vec![HandshakeFragment::Firmware(meta.firmware_version)],
        PayloadVariant::Config(cfg) => config_fragments(cfg),
        PayloadVariant::ConfigCompleteId(id) => {
            vec![HandshakeFragment::ConfigComplete { id: ConfigId(id) }]
        }
        PayloadVariant::Packet(p) => packet_fragments(p),
        PayloadVariant::ModuleConfig(_)
        | PayloadVariant::Rebooted(_)
        | PayloadVariant::QueueStatus(_)
        | PayloadVariant::XmodemPacket(_)
        | PayloadVariant::FileInfo(_)
        | PayloadVariant::LogRecord(_)
        | PayloadVariant::MqttClientProxyMessage(_)
        | PayloadVariant::ClientNotification(_)
        | PayloadVariant::DeviceuiConfig(_) => Vec::new(),
    }
}

fn packet_fragments(p: meshtastic::MeshPacket) -> Vec<HandshakeFragment> {
    use meshtastic::mesh_packet::PayloadVariant;
    let Some(PayloadVariant::Decoded(data)) = p.payload_variant else { return Vec::new() };
    let Ok(payload) = parse(data.portnum, &data.payload) else { return Vec::new() };
    let channel = ChannelIndex::new(p.channel as u8).unwrap_or_else(ChannelIndex::primary);
    match payload {
        PortPayload::Text(text) => vec![HandshakeFragment::Message(TextMessage {
            id: PacketId(p.id),
            channel,
            from: NodeId(p.from),
            to: if p.to == BROADCAST_NODE.0 {
                Recipient::Broadcast
            } else {
                Recipient::Node(NodeId(p.to))
            },
            text,
            received_at: packet_time(p.rx_time),
            direction: Direction::Incoming,
            state: DeliveryState::Acked,
        })],
        PortPayload::Position(_)
        | PortPayload::NodeInfo(_)
        | PortPayload::Telemetry(_)
        | PortPayload::Routing(_)
        | PortPayload::Admin(_)
        | PortPayload::Unknown { .. } => Vec::new(),
    }
}

fn packet_time(rx_time_secs: u32) -> SystemTime {
    if rx_time_secs == 0 {
        SystemTime::now()
    } else {
        UNIX_EPOCH
            .checked_add(Duration::from_secs(u64::from(rx_time_secs)))
            .unwrap_or_else(SystemTime::now)
    }
}

pub fn node_from_proto(ni: &meshtastic::NodeInfo) -> Node {
    let last_heard = if ni.last_heard == 0 {
        None
    } else {
        UNIX_EPOCH.checked_add(Duration::from_secs(u64::from(ni.last_heard)))
    };
    Node {
        id: NodeId(ni.num),
        long_name: ni.user.as_ref().map(|u| u.long_name.clone()).unwrap_or_default(),
        short_name: ni.user.as_ref().map(|u| u.short_name.clone()).unwrap_or_default(),
        role: ni.user.as_ref().map_or(NodeRole::Client, |u| role_from_proto(u.role())),
        battery_level: ni.device_metrics.as_ref().map(|m| m.battery_level() as u8),
        voltage_v: ni.device_metrics.as_ref().map(meshtastic::DeviceMetrics::voltage),
        snr_db: Some(ni.snr),
        rssi_dbm: None,
        hops_away: Some(ni.hops_away() as u8),
        last_heard,
        position: ni.position.as_ref().map(|p| Position {
            latitude_deg: p.latitude_i() as f64 * 1e-7,
            longitude_deg: p.longitude_i() as f64 * 1e-7,
            altitude_m: Some(p.altitude()),
        }),
    }
}

fn role_from_proto(role: meshtastic::config::device_config::Role) -> NodeRole {
    use meshtastic::config::device_config::Role;
    match role {
        Role::Client => NodeRole::Client,
        Role::ClientMute => NodeRole::ClientMute,
        Role::ClientHidden => NodeRole::ClientHidden,
        Role::ClientBase => NodeRole::ClientBase,
        Role::Router => NodeRole::Router,
        Role::RouterClient => NodeRole::RouterClient,
        Role::RouterLate => NodeRole::RouterLate,
        Role::Repeater => NodeRole::Repeater,
        Role::Tracker => NodeRole::Tracker,
        Role::Sensor => NodeRole::Sensor,
        Role::Tak => NodeRole::Tak,
        Role::TakTracker => NodeRole::TakTracker,
        Role::LostAndFound => NodeRole::LostAndFound,
    }
}

#[allow(dead_code)]
fn _touch_system_time(_: SystemTime) {}

fn channel_fragments(ch: meshtastic::Channel) -> Vec<HandshakeFragment> {
    let Some(index) = ChannelIndex::new(ch.index as u8) else {
        return Vec::new();
    };
    let role = match ch.role() {
        meshtastic::channel::Role::Primary => ChannelRole::Primary,
        meshtastic::channel::Role::Secondary => ChannelRole::Secondary,
        meshtastic::channel::Role::Disabled => ChannelRole::Disabled,
    };
    let (name, has_psk) = match ch.settings {
        Some(s) => (s.name, !s.psk.is_empty()),
        None => (String::new(), false),
    };
    vec![HandshakeFragment::Channel(Channel { index, role, name, has_psk })]
}

fn config_fragments(cfg: meshtastic::Config) -> Vec<HandshakeFragment> {
    use meshtastic::config::PayloadVariant;
    let Some(variant) = cfg.payload_variant else { return Vec::new() };
    match variant {
        PayloadVariant::Lora(lora) => vec![HandshakeFragment::Lora(lora_from_proto(&lora))],
        PayloadVariant::Device(_)
        | PayloadVariant::Position(_)
        | PayloadVariant::Power(_)
        | PayloadVariant::Network(_)
        | PayloadVariant::Display(_)
        | PayloadVariant::Bluetooth(_)
        | PayloadVariant::Security(_)
        | PayloadVariant::Sessionkey(_)
        | PayloadVariant::DeviceUi(_) => Vec::new(),
    }
}

fn lora_from_proto(lora: &meshtastic::config::LoRaConfig) -> LoraSettings {
    LoraSettings {
        region: lora.region(),
        modem_preset: lora.modem_preset(),
        use_preset: lora.use_preset,
        hop_limit: lora.hop_limit.min(7) as u8,
        tx_enabled: lora.tx_enabled,
        tx_power: lora.tx_power,
    }
}
