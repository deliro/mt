use std::time::SystemTime;

use crate::domain::ids::NodeId;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NodeRole {
    Client,
    ClientMute,
    Router,
    RouterClient,
    Repeater,
    Tracker,
    Sensor,
    Tak,
    TakTracker,
    LostAndFound,
    ClientHidden,
    Unknown(i32),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Position {
    pub latitude_deg: f64,
    pub longitude_deg: f64,
    pub altitude_m: Option<i32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    pub id: NodeId,
    pub long_name: String,
    pub short_name: String,
    pub role: NodeRole,
    pub battery_level: Option<u8>,
    pub voltage_v: Option<f32>,
    pub snr_db: Option<f32>,
    pub rssi_dbm: Option<i32>,
    pub hops_away: Option<u8>,
    pub last_heard: Option<SystemTime>,
    pub position: Option<Position>,
}
