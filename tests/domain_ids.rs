#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mt::domain::ids::{BROADCAST_NODE, BleAddress, ChannelIndex, ConfigId, NodeId, PacketId};

#[test]
fn broadcast_node_is_all_ones() {
    assert_eq!(BROADCAST_NODE, NodeId(0xFFFF_FFFF));
}

#[test]
fn node_id_is_copy() {
    let a = NodeId(1);
    let b: NodeId = a;
    assert_eq!(a, b);
}

#[test]
fn channel_index_rejects_out_of_range() {
    assert!(ChannelIndex::new(0).is_some());
    assert!(ChannelIndex::new(ChannelIndex::MAX).is_some());
    assert!(ChannelIndex::new(ChannelIndex::MAX + 1).is_none());
}

#[test]
fn channel_index_primary_is_zero() {
    assert_eq!(ChannelIndex::primary().get(), 0);
}

#[test]
fn packet_and_config_ids_are_nonzero_random() {
    let p = PacketId::random();
    let c = ConfigId::random();
    assert_ne!(p.0, 0);
    assert_ne!(c.0, 0);
}

#[test]
fn ble_address_normalizes_case() {
    assert_eq!(BleAddress::new("aa:bb"), BleAddress::new("AA:BB"));
    assert_eq!(BleAddress::new("aa:bb").as_str(), "AA:BB");
}
