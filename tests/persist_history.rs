#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use mt::domain::ids::{ChannelIndex, NodeId, PacketId};
use mt::domain::message::{DeliveryState, Direction, Recipient, TextMessage};
use mt::domain::node::{Node, NodeRole, Position};
use mt::persist::history::HistoryStore;

fn sample_msg(id: u32, text: &str, state: DeliveryState) -> TextMessage {
    TextMessage {
        id: PacketId(id),
        channel: ChannelIndex::primary(),
        from: NodeId(42),
        to: Recipient::Broadcast,
        text: text.into(),
        received_at: SystemTime::UNIX_EPOCH,
        direction: Direction::Incoming,
        state,
    }
}

fn sample_node(id: u32, name: &str, last_heard: Option<SystemTime>) -> Node {
    Node {
        id: NodeId(id),
        long_name: name.into(),
        short_name: name.chars().take(4).collect(),
        role: NodeRole::Router,
        battery_level: Some(88),
        voltage_v: Some(4.01),
        snr_db: Some(6.5),
        rssi_dbm: Some(-48),
        hops_away: Some(1),
        last_heard,
        position: Some(Position {
            latitude_deg: 48.145,
            longitude_deg: 17.102,
            altitude_m: Some(217),
        }),
    }
}

#[test]
fn messages_round_trip_and_dedup() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history.db");
    let store = HistoryStore::open(&path).unwrap();
    let my = NodeId(7);

    store.upsert_message(my, &sample_msg(1, "hello", DeliveryState::Acked)).unwrap();
    store.upsert_message(my, &sample_msg(2, "world", DeliveryState::Acked)).unwrap();
    store.upsert_message(my, &sample_msg(1, "hello v2", DeliveryState::Acked)).unwrap();

    let loaded = store.load_messages(my).unwrap();
    assert_eq!(loaded.len(), 2);
    let first = loaded.iter().find(|m| m.id == PacketId(1)).unwrap();
    assert_eq!(first.text, "hello v2");
}

#[test]
fn update_state_changes_outgoing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history.db");
    let store = HistoryStore::open(&path).unwrap();
    let my = NodeId(7);
    let mut msg = sample_msg(10, "ping", DeliveryState::Queued);
    msg.direction = Direction::Outgoing;
    store.upsert_message(my, &msg).unwrap();
    store.update_message_state(my, PacketId(10), &DeliveryState::Acked).unwrap();
    let loaded = store.load_messages(my).unwrap();
    assert_eq!(loaded.first().map(|m| m.state.clone()), Some(DeliveryState::Acked));
}

#[test]
fn state_failed_preserves_reason() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history.db");
    let store = HistoryStore::open(&path).unwrap();
    let my = NodeId(7);
    let mut msg = sample_msg(11, "oops", DeliveryState::Failed("no ack".into()));
    msg.direction = Direction::Outgoing;
    store.upsert_message(my, &msg).unwrap();
    let loaded = store.load_messages(my).unwrap();
    assert_eq!(
        loaded.first().map(|m| m.state.clone()),
        Some(DeliveryState::Failed("no ack".into())),
    );
}

#[test]
fn nodes_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history.db");
    let store = HistoryStore::open(&path).unwrap();
    let my = NodeId(7);
    let last = UNIX_EPOCH + Duration::from_secs(1_700_000_000);
    let node = sample_node(100, "Base station", Some(last));
    store.upsert_node(my, &node).unwrap();
    let loaded = store.load_nodes(my).unwrap();
    assert_eq!(loaded.len(), 1);
    let p = loaded.into_iter().next().unwrap();
    assert_eq!(p.node.id, NodeId(100));
    assert_eq!(p.node.long_name, "Base station");
    assert_eq!(p.node.role, NodeRole::Router);
    assert_eq!(p.node.battery_level, Some(88));
    assert_eq!(p.node.last_heard, Some(last));
    assert!(p.node.position.is_some());
    assert!(p.saved_at >= UNIX_EPOCH);
}

#[test]
fn clear_scopes_to_my_node() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history.db");
    let store = HistoryStore::open(&path).unwrap();
    let a = NodeId(1);
    let b = NodeId(2);
    store.upsert_message(a, &sample_msg(1, "a1", DeliveryState::Acked)).unwrap();
    store.upsert_message(b, &sample_msg(2, "b1", DeliveryState::Acked)).unwrap();
    store.upsert_node(a, &sample_node(10, "n-a", None)).unwrap();
    store.upsert_node(b, &sample_node(20, "n-b", None)).unwrap();

    let removed_msgs = store.clear_messages(a).unwrap();
    let removed_nodes = store.clear_nodes(a).unwrap();
    assert_eq!(removed_msgs, 1);
    assert_eq!(removed_nodes, 1);
    assert_eq!(store.load_messages(a).unwrap().len(), 0);
    assert_eq!(store.load_nodes(a).unwrap().len(), 0);
    assert_eq!(store.load_messages(b).unwrap().len(), 1);
    assert_eq!(store.load_nodes(b).unwrap().len(), 1);
}

#[test]
fn counts_reflect_data() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history.db");
    let store = HistoryStore::open(&path).unwrap();
    let my = NodeId(1);
    assert_eq!(store.message_count(my).unwrap(), 0);
    assert_eq!(store.node_count(my).unwrap(), 0);
    store.upsert_message(my, &sample_msg(1, "hi", DeliveryState::Acked)).unwrap();
    store.upsert_node(my, &sample_node(42, "x", None)).unwrap();
    assert_eq!(store.message_count(my).unwrap(), 1);
    assert_eq!(store.node_count(my).unwrap(), 1);
}
