#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::SystemTime;

use mt::domain::ids::{ChannelIndex, NodeId, PacketId};
use mt::domain::message::{DeliveryState, Direction, Recipient, TextMessage};
use mt::persist::messages::MessageStore;

fn sample(id: u32, text: &str, state: DeliveryState) -> TextMessage {
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

#[test]
fn round_trip_and_dedup() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history.db");
    let store = MessageStore::open(&path).unwrap();
    let my = NodeId(7);

    store.upsert(my, &sample(1, "hello", DeliveryState::Acked)).unwrap();
    store.upsert(my, &sample(2, "world", DeliveryState::Acked)).unwrap();
    // Re-insert with same id: replaces row but remains unique.
    store.upsert(my, &sample(1, "hello v2", DeliveryState::Acked)).unwrap();

    let loaded = store.load(my).unwrap();
    assert_eq!(loaded.len(), 2);
    let first = loaded.iter().find(|m| m.id == PacketId(1)).unwrap();
    assert_eq!(first.text, "hello v2");
}

#[test]
fn update_state_changes_outgoing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history.db");
    let store = MessageStore::open(&path).unwrap();
    let my = NodeId(7);
    let mut msg = sample(10, "ping", DeliveryState::Queued);
    msg.direction = Direction::Outgoing;
    store.upsert(my, &msg).unwrap();
    store.update_state(my, PacketId(10), &DeliveryState::Acked).unwrap();
    let loaded = store.load(my).unwrap();
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded.first().map(|m| m.state.clone()), Some(DeliveryState::Acked));
}

#[test]
fn state_failed_preserves_reason() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history.db");
    let store = MessageStore::open(&path).unwrap();
    let my = NodeId(7);
    let mut msg = sample(11, "oops", DeliveryState::Failed("no ack".into()));
    msg.direction = Direction::Outgoing;
    store.upsert(my, &msg).unwrap();
    let loaded = store.load(my).unwrap();
    assert_eq!(
        loaded.first().map(|m| m.state.clone()),
        Some(DeliveryState::Failed("no ack".into())),
    );
}
