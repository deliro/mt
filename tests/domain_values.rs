#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic, clippy::pedantic, clippy::nursery, clippy::cargo, clippy::indexing_slicing, clippy::integer_division, clippy::collapsible_if, clippy::byte_char_slices, clippy::redundant_pattern_matching)]

use std::path::PathBuf;

use mt::domain::channel::{Channel, ChannelRole};
use mt::domain::ids::{BleAddress, ChannelIndex, NodeId, PacketId};
use mt::domain::message::{DeliveryState, Direction, Recipient, TextMessage};
use mt::domain::node::{Node, NodeRole};
use mt::domain::profile::{ConnectionProfile, TransportKind};
use mt::domain::snapshot::DeviceSnapshot;

#[test]
fn position_optional_altitude() {
    use mt::domain::node::Position;
    let p = Position { latitude_deg: 48.14, longitude_deg: 17.11, altitude_m: None };
    assert!(p.altitude_m.is_none());
}

#[test]
fn profile_kind_and_name_match_variant() {
    let p = ConnectionProfile::Tcp { name: "home".into(), host: "h".into(), port: 4403 };
    assert_eq!(p.kind(), TransportKind::Tcp);
    assert_eq!(p.name(), "home");
}

#[test]
fn text_message_records_direction_and_state() {
    let m = TextMessage {
        id: PacketId(1),
        channel: ChannelIndex::primary(),
        from: NodeId(10),
        to: Recipient::Broadcast,
        text: "hi".into(),
        received_at: std::time::SystemTime::UNIX_EPOCH,
        direction: Direction::Outgoing,
        state: DeliveryState::Pending,
    };
    assert_eq!(m.direction, Direction::Outgoing);
    assert_eq!(m.state, DeliveryState::Pending);
}

#[test]
fn device_snapshot_upserts_channels() {
    let mut snap = DeviceSnapshot::default();
    snap.upsert_channel(Channel {
        index: ChannelIndex::primary(),
        role: ChannelRole::Primary,
        name: "LongFast".into(),
        has_psk: true,
    });
    snap.upsert_channel(Channel {
        index: ChannelIndex::primary(),
        role: ChannelRole::Primary,
        name: "Renamed".into(),
        has_psk: true,
    });
    assert_eq!(snap.channels.len(), 1);
    assert_eq!(snap.channels[0].name, "Renamed");
}

#[test]
fn node_and_profile_compose_without_leaking_proto() {
    let _ = Node {
        id: NodeId(1),
        long_name: "n".into(),
        short_name: "n".into(),
        role: NodeRole::Client,
        battery_level: None,
        voltage_v: None,
        snr_db: None,
        rssi_dbm: None,
        hops_away: None,
        last_heard: None,
        position: None,
    };
    let _ = ConnectionProfile::Ble { name: "r".into(), address: BleAddress::new("AA:BB") };
    let _ = ConnectionProfile::Serial { name: "s".into(), path: PathBuf::from("/dev/ttyUSB0") };
}
