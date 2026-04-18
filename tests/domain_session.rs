#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mt::domain::channel::{Channel, ChannelRole};
use mt::domain::ids::{ChannelIndex, ConfigId, NodeId, PacketId};
use mt::domain::message::{DeliveryState, Direction, Recipient, TextMessage};
use mt::domain::node::{Node, NodeRole};
use mt::domain::profile::TransportKind;
use mt::domain::session::{HandshakeFragment, NodeMetric, SessionState, apply, start_handshake};

fn node(id: u32, name: &str) -> Node {
    Node {
        id: NodeId(id),
        long_name: name.into(),
        short_name: name.chars().take(2).collect(),
        role: NodeRole::Client,
        battery_level: None,
        voltage_v: None,
        snr_db: None,
        rssi_dbm: None,
        hops_away: None,
        last_heard: None,
        position: None,
    }
}

fn primary_channel() -> Channel {
    Channel {
        index: ChannelIndex::primary(),
        role: ChannelRole::Primary,
        name: "Primary".into(),
        has_psk: true,
    }
}

#[test]
fn handshake_collects_fragments_and_completes() {
    let cfg = ConfigId(42);
    let s = start_handshake(TransportKind::Tcp, cfg);
    let s = apply(s, HandshakeFragment::MyNode { id: NodeId(1) });
    let s = apply(s, HandshakeFragment::Firmware("2.7".into()));
    let s = apply(s, HandshakeFragment::Node(node(1, "self")));
    let s = apply(s, HandshakeFragment::Node(node(2, "n2")));
    let s = apply(s, HandshakeFragment::Channel(primary_channel()));
    let s = apply(s, HandshakeFragment::ConfigComplete { id: cfg });

    match s {
        SessionState::Ready(snap) => {
            assert_eq!(snap.my_node, NodeId(1));
            assert_eq!(snap.firmware_version, "2.7");
            assert_eq!(snap.nodes.len(), 2);
            assert_eq!(snap.channels.len(), 1);
            assert_eq!(snap.long_name, "self");
        }
        other => panic!("should be Ready, got {other:?}"),
    }
}

#[test]
fn config_complete_with_wrong_id_keeps_handshake() {
    let s = start_handshake(TransportKind::Tcp, ConfigId(1));
    let s = apply(s, HandshakeFragment::MyNode { id: NodeId(1) });
    let s = apply(s, HandshakeFragment::ConfigComplete { id: ConfigId(999) });
    assert!(matches!(s, SessionState::Handshake(_)));
}

#[test]
fn config_complete_without_my_node_stays_in_handshake() {
    let s = start_handshake(TransportKind::Tcp, ConfigId(1));
    let s = apply(s, HandshakeFragment::ConfigComplete { id: ConfigId(1) });
    assert!(matches!(s, SessionState::Handshake(_)));
}

#[test]
fn ready_applies_incoming_text_and_state_changes() {
    let s = start_handshake(TransportKind::Tcp, ConfigId(1));
    let s = apply(s, HandshakeFragment::MyNode { id: NodeId(1) });
    let s = apply(s, HandshakeFragment::ConfigComplete { id: ConfigId(1) });

    let msg = TextMessage {
        id: PacketId(5),
        channel: ChannelIndex::primary(),
        from: NodeId(2),
        to: Recipient::Broadcast,
        text: "hello".into(),
        received_at: std::time::SystemTime::UNIX_EPOCH,
        direction: Direction::Incoming,
        state: DeliveryState::Delivered,
    };
    let s = apply(s, HandshakeFragment::Message(msg));
    let s = apply(
        s,
        HandshakeFragment::MessageStateChanged {
            id: PacketId(5),
            state: DeliveryState::Failed("no ack".into()),
        },
    );
    match s {
        SessionState::Ready(snap) => {
            assert_eq!(snap.messages.len(), 1);
            assert_eq!(
                snap.messages.first().map(|m| m.state.clone()),
                Some(DeliveryState::Failed("no ack".into())),
            );
        }
        other => panic!("should be Ready, got {other:?}"),
    }
}

#[test]
fn ready_updates_node_metrics() {
    let s = start_handshake(TransportKind::Tcp, ConfigId(1));
    let s = apply(s, HandshakeFragment::MyNode { id: NodeId(1) });
    let s = apply(s, HandshakeFragment::Node(node(2, "n")));
    let s = apply(s, HandshakeFragment::ConfigComplete { id: ConfigId(1) });
    let s = apply(
        s,
        HandshakeFragment::NodeMetric { id: NodeId(2), update: NodeMetric::Battery(73) },
    );
    match s {
        SessionState::Ready(snap) => {
            assert_eq!(snap.nodes.get(&NodeId(2)).and_then(|n| n.battery_level), Some(73));
        }
        other => panic!("should be Ready, got {other:?}"),
    }
}
