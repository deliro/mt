use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension, params};

use crate::domain::ids::{BROADCAST_NODE, ChannelIndex, NodeId, PacketId};
use crate::domain::message::{DeliveryState, Direction, Recipient, TextMessage};
use crate::error::PersistError;

pub struct MessageStore {
    conn: Connection,
}

impl MessageStore {
    pub fn open(path: &Path) -> Result<Self, PersistError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS messages (
                my_node     INTEGER NOT NULL,
                packet_id   INTEGER NOT NULL,
                direction   INTEGER NOT NULL,
                from_node   INTEGER NOT NULL,
                to_node     INTEGER NOT NULL,
                channel     INTEGER NOT NULL,
                text        TEXT    NOT NULL,
                received_ms INTEGER NOT NULL,
                state       TEXT    NOT NULL,
                state_arg   TEXT,
                PRIMARY KEY (my_node, packet_id, direction)
            );
            CREATE INDEX IF NOT EXISTS idx_messages_received
                ON messages (my_node, received_ms);",
        )?;
        Ok(Self { conn })
    }

    pub fn load(&self, my_node: NodeId) -> Result<Vec<TextMessage>, PersistError> {
        let mut stmt = self.conn.prepare(
            "SELECT packet_id, direction, from_node, to_node, channel, text, received_ms, \
                    state, state_arg
             FROM messages
             WHERE my_node = ?
             ORDER BY received_ms ASC",
        )?;
        let rows = stmt
            .query_map([my_node.0], |row| {
                let packet_id: u32 = row.get(0)?;
                let direction: u8 = row.get(1)?;
                let from_node: u32 = row.get(2)?;
                let to_node: u32 = row.get(3)?;
                let channel: u8 = row.get(4)?;
                let text: String = row.get(5)?;
                let received_ms: i64 = row.get(6)?;
                let state: String = row.get(7)?;
                let state_arg: Option<String> = row.get(8)?;
                Ok(StoredRow {
                    packet_id,
                    direction,
                    from_node,
                    to_node,
                    channel,
                    text,
                    received_ms,
                    state,
                    state_arg,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        rows.into_iter().map(StoredRow::into_message).collect()
    }

    pub fn upsert(&self, my_node: NodeId, msg: &TextMessage) -> Result<(), PersistError> {
        let received_ms = i64::try_from(
            msg.received_at.duration_since(UNIX_EPOCH).unwrap_or_default().as_millis(),
        )
        .unwrap_or(0);
        let direction = direction_to_code(msg.direction);
        let to_node = match msg.to {
            Recipient::Broadcast => BROADCAST_NODE.0,
            Recipient::Node(n) => n.0,
        };
        let (state_tag, state_arg) = encode_state(&msg.state);
        self.conn.execute(
            "INSERT INTO messages (my_node, packet_id, direction, from_node, to_node, channel, \
                                   text, received_ms, state, state_arg)
             VALUES (?,?,?,?,?,?,?,?,?,?)
             ON CONFLICT(my_node, packet_id, direction) DO UPDATE SET
                from_node   = excluded.from_node,
                to_node     = excluded.to_node,
                channel     = excluded.channel,
                text        = excluded.text,
                received_ms = excluded.received_ms,
                state       = excluded.state,
                state_arg   = excluded.state_arg",
            params![
                my_node.0,
                msg.id.0,
                direction,
                msg.from.0,
                to_node,
                msg.channel.get(),
                msg.text,
                received_ms,
                state_tag,
                state_arg,
            ],
        )?;
        Ok(())
    }

    pub fn update_state(
        &self,
        my_node: NodeId,
        id: PacketId,
        state: &DeliveryState,
    ) -> Result<(), PersistError> {
        let (tag, arg) = encode_state(state);
        let rows = self.conn.execute(
            "UPDATE messages SET state = ?, state_arg = ?
             WHERE my_node = ? AND packet_id = ? AND direction = ?",
            params![tag, arg, my_node.0, id.0, direction_to_code(Direction::Outgoing)],
        )?;
        let _ = rows;
        Ok(())
    }

    pub fn last_received_ms(&self, my_node: NodeId) -> Result<Option<i64>, PersistError> {
        let value: Option<i64> = self
            .conn
            .query_row(
                "SELECT MAX(received_ms) FROM messages WHERE my_node = ?",
                [my_node.0],
                |r| r.get(0),
            )
            .optional()?
            .flatten();
        Ok(value)
    }
}

pub fn default_path() -> PathBuf {
    directories::ProjectDirs::from("dev", "", "mt")
        .map_or_else(|| PathBuf::from("history.db"), |d| d.config_dir().join("history.db"))
}

struct StoredRow {
    packet_id: u32,
    direction: u8,
    from_node: u32,
    to_node: u32,
    channel: u8,
    text: String,
    received_ms: i64,
    state: String,
    state_arg: Option<String>,
}

impl StoredRow {
    fn into_message(self) -> Result<TextMessage, PersistError> {
        let direction = code_to_direction(self.direction)
            .ok_or_else(|| PersistError::StateDecode(format!("bad direction {}", self.direction)))?;
        let channel = ChannelIndex::new(self.channel)
            .ok_or_else(|| PersistError::StateDecode(format!("bad channel {}", self.channel)))?;
        let to = if self.to_node == BROADCAST_NODE.0 {
            Recipient::Broadcast
        } else {
            Recipient::Node(NodeId(self.to_node))
        };
        let received_at = UNIX_EPOCH
            .checked_add(Duration::from_millis(u64::try_from(self.received_ms).unwrap_or(0)))
            .unwrap_or(UNIX_EPOCH);
        let state = decode_state(&self.state, self.state_arg.as_deref())?;
        Ok(TextMessage {
            id: PacketId(self.packet_id),
            channel,
            from: NodeId(self.from_node),
            to,
            text: self.text,
            received_at,
            direction,
            state,
        })
    }
}

const fn direction_to_code(d: Direction) -> u8 {
    match d {
        Direction::Incoming => 0,
        Direction::Outgoing => 1,
    }
}

const fn code_to_direction(c: u8) -> Option<Direction> {
    match c {
        0 => Some(Direction::Incoming),
        1 => Some(Direction::Outgoing),
        _ => None,
    }
}

fn encode_state(state: &DeliveryState) -> (&'static str, Option<String>) {
    match state {
        DeliveryState::Queued => ("queued", None),
        DeliveryState::Sent => ("sent", None),
        DeliveryState::Acked => ("acked", None),
        DeliveryState::Failed(reason) => ("failed", Some(reason.clone())),
    }
}

fn decode_state(tag: &str, arg: Option<&str>) -> Result<DeliveryState, PersistError> {
    match tag {
        "queued" => Ok(DeliveryState::Queued),
        "sent" => Ok(DeliveryState::Sent),
        "acked" => Ok(DeliveryState::Acked),
        "failed" => Ok(DeliveryState::Failed(arg.unwrap_or("").to_owned())),
        other => Err(PersistError::StateDecode(format!("unknown state '{other}'"))),
    }
}
