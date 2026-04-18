#[derive(Clone, Debug, Default, PartialEq)]
pub struct MeshStats {
    pub battery_level: Option<u8>,
    pub voltage_v: Option<f32>,
    pub channel_utilization: Option<f32>,
    pub air_util_tx: Option<f32>,
    pub uptime_seconds: Option<u32>,
    pub num_tx_relay: Option<u32>,
    pub num_packets_tx: Option<u32>,
    pub num_packets_rx: Option<u32>,
    pub num_online_nodes: Option<u32>,
}

impl MeshStats {
    pub fn merge(&mut self, other: &Self) {
        if let Some(v) = other.battery_level {
            self.battery_level = Some(v);
        }
        if let Some(v) = other.voltage_v {
            self.voltage_v = Some(v);
        }
        if let Some(v) = other.channel_utilization {
            self.channel_utilization = Some(v);
        }
        if let Some(v) = other.air_util_tx {
            self.air_util_tx = Some(v);
        }
        if let Some(v) = other.uptime_seconds {
            self.uptime_seconds = Some(v);
        }
        if let Some(v) = other.num_tx_relay {
            self.num_tx_relay = Some(v);
        }
        if let Some(v) = other.num_packets_tx {
            self.num_packets_tx = Some(v);
        }
        if let Some(v) = other.num_packets_rx {
            self.num_packets_rx = Some(v);
        }
        if let Some(v) = other.num_online_nodes {
            self.num_online_nodes = Some(v);
        }
    }
}
