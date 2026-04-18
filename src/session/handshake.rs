use crate::domain::channel::{Channel, ChannelRole};
use crate::domain::ids::{ChannelIndex, ConfigId, NodeId};
use crate::domain::node::{Node, NodeRole, Position};
use crate::domain::session::HandshakeFragment;
use crate::proto::meshtastic;

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
        PayloadVariant::ConfigCompleteId(id) => {
            vec![HandshakeFragment::ConfigComplete { id: ConfigId(id) }]
        }
        PayloadVariant::Packet(_)
        | PayloadVariant::Config(_)
        | PayloadVariant::ModuleConfig(_)
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

pub fn node_from_proto(ni: &meshtastic::NodeInfo) -> Node {
    Node {
        id: NodeId(ni.num),
        long_name: ni.user.as_ref().map(|u| u.long_name.clone()).unwrap_or_default(),
        short_name: ni.user.as_ref().map(|u| u.short_name.clone()).unwrap_or_default(),
        role: NodeRole::Client,
        battery_level: ni.device_metrics.as_ref().map(|m| m.battery_level() as u8),
        voltage_v: ni.device_metrics.as_ref().map(meshtastic::DeviceMetrics::voltage),
        snr_db: Some(ni.snr),
        rssi_dbm: None,
        hops_away: Some(ni.hops_away() as u8),
        last_heard: None,
        position: ni.position.as_ref().map(|p| Position {
            latitude_deg: p.latitude_i() as f64 * 1e-7,
            longitude_deg: p.longitude_i() as f64 * 1e-7,
            altitude_m: Some(p.altitude()),
        }),
    }
}

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
