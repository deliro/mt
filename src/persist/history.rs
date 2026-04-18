use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension, params};

use crate::domain::ids::{BROADCAST_NODE, ChannelIndex, NodeId, PacketId};
use crate::domain::message::{DeliveryState, Direction, Recipient, TextMessage};
use crate::domain::node::{Node, NodeRole, Position};
use crate::domain::profile::ConnectionProfile;
use crate::error::PersistError;

const SETTING_PROFILES: &str = "profiles_json";
const SETTING_LAST_ACTIVE: &str = "last_active_key";
const SETTING_NODES_SORT: &str = "nodes_sort";

pub struct HistoryStore {
    conn: Connection,
}

pub struct PersistedNode {
    pub node: Node,
    pub saved_at: SystemTime,
}

impl HistoryStore {
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
                ON messages (my_node, received_ms);

            CREATE TABLE IF NOT EXISTS nodes (
                my_node       INTEGER NOT NULL,
                node_id       INTEGER NOT NULL,
                long_name     TEXT    NOT NULL,
                short_name    TEXT    NOT NULL,
                role          INTEGER NOT NULL,
                battery       INTEGER,
                voltage       REAL,
                snr           REAL,
                rssi          INTEGER,
                hops_away     INTEGER,
                last_heard_ms INTEGER,
                latitude      REAL,
                longitude     REAL,
                altitude      INTEGER,
                saved_at_ms   INTEGER NOT NULL,
                is_favorite   INTEGER NOT NULL DEFAULT 0,
                is_ignored    INTEGER NOT NULL DEFAULT 0,
                public_key    BLOB    NOT NULL DEFAULT x'',
                PRIMARY KEY (my_node, node_id)
            );

            CREATE TABLE IF NOT EXISTS settings (
                k TEXT PRIMARY KEY,
                v TEXT NOT NULL
            );",
        )?;
        Ok(Self { conn })
    }

    // ---- Profiles + settings ----

    pub fn load_profiles(&self) -> Result<Vec<ConnectionProfile>, PersistError> {
        let Some(blob) = self.load_setting(SETTING_PROFILES)? else {
            return Ok(Vec::new());
        };
        Ok(serde_json::from_str::<Vec<ConnectionProfile>>(&blob)?)
    }

    pub fn save_profiles(&self, profiles: &[ConnectionProfile]) -> Result<(), PersistError> {
        let blob = serde_json::to_string(profiles)?;
        self.save_setting(SETTING_PROFILES, Some(&blob))
    }

    pub fn load_last_active(&self) -> Result<Option<String>, PersistError> {
        self.load_setting(SETTING_LAST_ACTIVE)
    }

    pub fn save_last_active(&self, value: Option<&str>) -> Result<(), PersistError> {
        self.save_setting(SETTING_LAST_ACTIVE, value)
    }

    pub fn load_nodes_sort_json(&self) -> Result<Option<String>, PersistError> {
        self.load_setting(SETTING_NODES_SORT)
    }

    pub fn save_nodes_sort_json(&self, value: &str) -> Result<(), PersistError> {
        self.save_setting(SETTING_NODES_SORT, Some(value))
    }

    fn load_setting(&self, key: &str) -> Result<Option<String>, PersistError> {
        let value: Option<String> = self
            .conn
            .query_row("SELECT v FROM settings WHERE k = ?", [key], |row| row.get(0))
            .optional()?;
        Ok(value)
    }

    fn save_setting(&self, key: &str, value: Option<&str>) -> Result<(), PersistError> {
        match value {
            Some(v) => {
                self.conn.execute(
                    "INSERT INTO settings (k, v) VALUES (?, ?) \
                     ON CONFLICT(k) DO UPDATE SET v = excluded.v",
                    params![key, v],
                )?;
            }
            None => {
                self.conn.execute("DELETE FROM settings WHERE k = ?", [key])?;
            }
        }
        Ok(())
    }

    // ---- Messages ----

    pub fn load_messages(&self, my_node: NodeId) -> Result<Vec<TextMessage>, PersistError> {
        let mut stmt = self.conn.prepare(
            "SELECT packet_id, direction, from_node, to_node, channel, text, received_ms, \
                    state, state_arg
             FROM messages
             WHERE my_node = ?
             ORDER BY received_ms ASC",
        )?;
        let rows = stmt
            .query_map([my_node.0], |row| {
                Ok(StoredMessage {
                    packet_id: row.get(0)?,
                    direction: row.get(1)?,
                    from_node: row.get(2)?,
                    to_node: row.get(3)?,
                    channel: row.get(4)?,
                    text: row.get(5)?,
                    received_ms: row.get(6)?,
                    state: row.get(7)?,
                    state_arg: row.get(8)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows.into_iter().map(StoredMessage::into_message).collect()
    }

    pub fn upsert_message(&self, my_node: NodeId, msg: &TextMessage) -> Result<(), PersistError> {
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

    pub fn update_message_state(
        &self,
        my_node: NodeId,
        id: PacketId,
        state: &DeliveryState,
    ) -> Result<(), PersistError> {
        let (tag, arg) = encode_state(state);
        self.conn.execute(
            "UPDATE messages SET state = ?, state_arg = ?
             WHERE my_node = ? AND packet_id = ? AND direction = ?",
            params![tag, arg, my_node.0, id.0, direction_to_code(Direction::Outgoing)],
        )?;
        Ok(())
    }

    pub fn message_count(&self, my_node: NodeId) -> Result<i64, PersistError> {
        Ok(self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM messages WHERE my_node = ?",
                [my_node.0],
                |r| r.get::<_, i64>(0),
            )
            .optional()?
            .unwrap_or(0))
    }

    pub fn clear_messages(&self, my_node: NodeId) -> Result<usize, PersistError> {
        let rows = self.conn.execute("DELETE FROM messages WHERE my_node = ?", [my_node.0])?;
        Ok(rows)
    }

    // ---- Nodes ----

    pub fn load_nodes(&self, my_node: NodeId) -> Result<Vec<PersistedNode>, PersistError> {
        let mut stmt = self.conn.prepare(
            "SELECT node_id, long_name, short_name, role, battery, voltage, snr, rssi, \
                    hops_away, last_heard_ms, latitude, longitude, altitude, saved_at_ms, \
                    is_favorite, is_ignored, public_key
             FROM nodes
             WHERE my_node = ?
             ORDER BY saved_at_ms DESC",
        )?;
        let rows = stmt
            .query_map([my_node.0], |row| {
                Ok(StoredNode {
                    node_id: row.get(0)?,
                    long_name: row.get(1)?,
                    short_name: row.get(2)?,
                    role: row.get(3)?,
                    battery: row.get(4)?,
                    voltage: row.get(5)?,
                    snr: row.get(6)?,
                    rssi: row.get(7)?,
                    hops_away: row.get(8)?,
                    last_heard_ms: row.get(9)?,
                    latitude: row.get(10)?,
                    longitude: row.get(11)?,
                    altitude: row.get(12)?,
                    saved_at_ms: row.get(13)?,
                    is_favorite: row.get::<_, i64>(14)? != 0,
                    is_ignored: row.get::<_, i64>(15)? != 0,
                    public_key: row.get::<_, Vec<u8>>(16)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows.into_iter().map(StoredNode::into_persisted).collect())
    }

    pub fn upsert_node(&self, my_node: NodeId, node: &Node) -> Result<(), PersistError> {
        let saved_at_ms =
            i64::try_from(SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis())
                .unwrap_or(0);
        let last_heard_ms = node
            .last_heard
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .and_then(|d| i64::try_from(d.as_millis()).ok());
        let (lat, lon, alt) = node.position.as_ref().map_or((None, None, None), |p| {
            (Some(p.latitude_deg), Some(p.longitude_deg), p.altitude_m)
        });
        self.conn.execute(
            "INSERT INTO nodes (my_node, node_id, long_name, short_name, role, battery, voltage, \
                                snr, rssi, hops_away, last_heard_ms, latitude, longitude, \
                                altitude, saved_at_ms, is_favorite, is_ignored, public_key)
             VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)
             ON CONFLICT(my_node, node_id) DO UPDATE SET
                long_name     = excluded.long_name,
                short_name    = excluded.short_name,
                role          = excluded.role,
                battery       = excluded.battery,
                voltage       = excluded.voltage,
                snr           = excluded.snr,
                rssi          = excluded.rssi,
                hops_away     = excluded.hops_away,
                last_heard_ms = COALESCE(excluded.last_heard_ms, nodes.last_heard_ms),
                latitude      = COALESCE(excluded.latitude, nodes.latitude),
                longitude     = COALESCE(excluded.longitude, nodes.longitude),
                altitude      = COALESCE(excluded.altitude, nodes.altitude),
                saved_at_ms   = excluded.saved_at_ms,
                is_favorite   = excluded.is_favorite,
                is_ignored    = excluded.is_ignored,
                public_key    = CASE WHEN length(excluded.public_key) > 0 \
                                     THEN excluded.public_key \
                                     ELSE nodes.public_key END",
            params![
                my_node.0,
                node.id.0,
                node.long_name,
                node.short_name,
                node_role_to_wire(&node.role),
                node.battery_level.map(i64::from),
                node.voltage_v.map(f64::from),
                node.snr_db.map(f64::from),
                node.rssi_dbm,
                node.hops_away.map(i64::from),
                last_heard_ms,
                lat,
                lon,
                alt,
                saved_at_ms,
                i64::from(node.is_favorite),
                i64::from(node.is_ignored),
                node.public_key.clone(),
            ],
        )?;
        Ok(())
    }

    pub fn node_count(&self, my_node: NodeId) -> Result<i64, PersistError> {
        Ok(self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM nodes WHERE my_node = ?",
                [my_node.0],
                |r| r.get::<_, i64>(0),
            )
            .optional()?
            .unwrap_or(0))
    }

    pub fn clear_nodes(&self, my_node: NodeId) -> Result<usize, PersistError> {
        let rows = self.conn.execute("DELETE FROM nodes WHERE my_node = ?", [my_node.0])?;
        Ok(rows)
    }
}

pub fn default_path() -> PathBuf {
    directories::ProjectDirs::from("dev", "", "mt")
        .map_or_else(|| PathBuf::from("history.db"), |d| d.config_dir().join("history.db"))
}

// ---- Stored row helpers ----

struct StoredMessage {
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

impl StoredMessage {
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

struct StoredNode {
    node_id: u32,
    long_name: String,
    short_name: String,
    role: i32,
    battery: Option<i64>,
    voltage: Option<f64>,
    snr: Option<f64>,
    rssi: Option<i32>,
    hops_away: Option<i64>,
    last_heard_ms: Option<i64>,
    latitude: Option<f64>,
    longitude: Option<f64>,
    altitude: Option<i32>,
    saved_at_ms: i64,
    is_favorite: bool,
    is_ignored: bool,
    public_key: Vec<u8>,
}

impl StoredNode {
    fn into_persisted(self) -> PersistedNode {
        let last_heard = self.last_heard_ms.and_then(|ms| {
            let ms_u = u64::try_from(ms).ok()?;
            UNIX_EPOCH.checked_add(Duration::from_millis(ms_u))
        });
        let position = match (self.latitude, self.longitude) {
            (Some(lat), Some(lon)) => {
                Some(Position { latitude_deg: lat, longitude_deg: lon, altitude_m: self.altitude })
            }
            _ => None,
        };
        let saved_at = UNIX_EPOCH
            .checked_add(Duration::from_millis(u64::try_from(self.saved_at_ms).unwrap_or(0)))
            .unwrap_or(UNIX_EPOCH);
        let node = Node {
            id: NodeId(self.node_id),
            long_name: self.long_name,
            short_name: self.short_name,
            role: node_role_from_wire(self.role),
            battery_level: self.battery.and_then(|b| u8::try_from(b).ok()),
            voltage_v: self.voltage.map(|v| v as f32),
            snr_db: self.snr.map(|v| v as f32),
            rssi_dbm: self.rssi,
            hops_away: self.hops_away.and_then(|h| u8::try_from(h).ok()),
            last_heard,
            position,
            is_favorite: self.is_favorite,
            is_ignored: self.is_ignored,
            public_key: self.public_key,
        };
        PersistedNode { node, saved_at }
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

const fn node_role_to_wire(r: &NodeRole) -> i32 {
    match r {
        NodeRole::Client => 0,
        NodeRole::ClientMute => 1,
        NodeRole::Router => 2,
        NodeRole::RouterClient => 3,
        NodeRole::Repeater => 4,
        NodeRole::Tracker => 5,
        NodeRole::Sensor => 6,
        NodeRole::Tak => 7,
        NodeRole::ClientHidden => 8,
        NodeRole::LostAndFound => 9,
        NodeRole::TakTracker => 10,
        NodeRole::RouterLate => 11,
        NodeRole::ClientBase => 12,
        NodeRole::Unknown(v) => *v,
    }
}

const fn node_role_from_wire(v: i32) -> NodeRole {
    match v {
        0 => NodeRole::Client,
        1 => NodeRole::ClientMute,
        2 => NodeRole::Router,
        3 => NodeRole::RouterClient,
        4 => NodeRole::Repeater,
        5 => NodeRole::Tracker,
        6 => NodeRole::Sensor,
        7 => NodeRole::Tak,
        8 => NodeRole::ClientHidden,
        9 => NodeRole::LostAndFound,
        10 => NodeRole::TakTracker,
        11 => NodeRole::RouterLate,
        12 => NodeRole::ClientBase,
        v => NodeRole::Unknown(v),
    }
}
