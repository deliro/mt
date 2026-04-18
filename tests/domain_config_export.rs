#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::HashMap;

use mt::domain::channel::{Channel, ChannelRole};
use mt::domain::config::{
    BluetoothSettings, DeviceSettings, DisplaySettings, LoraSettings, MqttSettings,
    NeighborInfoSettings, NetworkSettings, PositionSettings, PowerSettings, RangeTestSettings,
    SecuritySettings, StoreForwardSettings, TelemetrySettings,
};
use mt::domain::config_export::{self, ConfigExport, Owner};
use mt::domain::ids::{ChannelIndex, NodeId};
use mt::domain::node::{Node, NodeRole, Position};
use mt::domain::snapshot::DeviceSnapshot;

fn channel(index: u8, role: ChannelRole, name: &str) -> Channel {
    Channel {
        index: ChannelIndex::new(index).unwrap(),
        role,
        name: name.into(),
        psk: vec![1, 2, 3, 4],
        uplink_enabled: true,
        downlink_enabled: false,
        position_precision: 12,
    }
}

#[test]
fn encode_decode_preserves_every_populated_section() {
    let snap = DeviceSnapshot {
        long_name: "Roman".into(),
        short_name: "RK".into(),
        lora: Some(LoraSettings::default()),
        device: Some(DeviceSettings::default()),
        position: Some(PositionSettings::default()),
        power: Some(PowerSettings::default()),
        network: Some(NetworkSettings {
            wifi_enabled: true,
            wifi_ssid: "home".into(),
            wifi_psk: "pass".into(),
            ntp_server: "pool.ntp.org".into(),
            eth_enabled: false,
        }),
        display: Some(DisplaySettings::default()),
        bluetooth: Some(BluetoothSettings::default()),
        mqtt: Some(MqttSettings {
            enabled: true,
            address: "mqtt.example.com".into(),
            ..MqttSettings::default()
        }),
        telemetry: Some(TelemetrySettings::default()),
        neighbor_info: Some(NeighborInfoSettings {
            enabled: true,
            transmit_over_lora: false,
            update_interval_secs: 14_400,
        }),
        store_forward: Some(StoreForwardSettings::default()),
        range_test: Some(RangeTestSettings::default()),
        channels: vec![
            channel(0, ChannelRole::Primary, "Primary"),
            channel(1, ChannelRole::Secondary, "Scouts"),
        ],
        ..DeviceSnapshot::default()
    };

    let export = config_export::export_snapshot(&snap);
    let json = config_export::encode(&export);
    assert!(json.contains("\"long_name\": \"Roman\""));

    let round: ConfigExport = config_export::decode(&json).expect("roundtrip decode");
    assert_eq!(round.owner.long_name, "Roman");
    assert_eq!(round.owner.short_name, "RK");
    assert_eq!(round.channels.len(), 2);
    assert_eq!(round.network.as_ref().map(|n| n.wifi_ssid.as_str()), Some("home"));
    assert_eq!(round.mqtt.as_ref().map(|m| m.address.as_str()), Some("mqtt.example.com"));
    assert_eq!(round.neighbor_info.as_ref().map(|n| n.update_interval_secs), Some(14_400));
}

#[test]
fn fixed_position_only_exported_when_flag_on() {
    let mut snap = snapshot_with_my_node(NodeId(0x1234), 55.7, 37.6, Some(150));
    // fixed_position flag OFF → coords not exported even though node has them
    snap.position = Some(PositionSettings { fixed_position: false, ..Default::default() });
    assert!(config_export::export_snapshot(&snap).fixed_position.is_none());

    // fixed_position flag ON → coords get captured
    snap.position = Some(PositionSettings { fixed_position: true, ..Default::default() });
    let export = config_export::export_snapshot(&snap);
    let fp = export.fixed_position.expect("fixed position populated");
    assert!((fp.latitude_deg - 55.7).abs() < 1e-9);
    assert!((fp.longitude_deg - 37.6).abs() < 1e-9);
    assert_eq!(fp.altitude_m, 150);
}

#[test]
fn security_policy_excludes_keypair() {
    let snap = DeviceSnapshot {
        security: Some(SecuritySettings {
            public_key: vec![0xAA; 32],
            private_key: vec![0xBB; 32],
            admin_keys: vec![vec![0xCC; 32]],
            is_managed: true,
            admin_channel_enabled: false,
            ..SecuritySettings::default()
        }),
        ..DeviceSnapshot::default()
    };

    let export = config_export::export_snapshot(&snap);
    let json = config_export::encode(&export);
    assert!(!json.contains("public_key"), "export leaked public_key: {json}");
    assert!(!json.contains("private_key"), "export leaked private_key: {json}");
    assert!(json.contains("admin_keys"));

    let policy = export.security_policy.expect("policy populated");
    assert_eq!(policy.admin_keys.len(), 1);
    assert!(policy.is_managed);
}

fn snapshot_with_my_node(id: NodeId, lat: f64, lon: f64, alt: Option<i32>) -> DeviceSnapshot {
    let me = Node {
        id,
        long_name: "me".into(),
        short_name: "me".into(),
        role: NodeRole::Client,
        battery_level: None,
        voltage_v: None,
        snr_db: None,
        rssi_dbm: None,
        hops_away: None,
        last_heard: None,
        position: Some(Position { latitude_deg: lat, longitude_deg: lon, altitude_m: alt }),
        is_favorite: false,
        is_ignored: false,
        public_key: Vec::new(),
    };
    let mut nodes = HashMap::new();
    nodes.insert(id, me);
    DeviceSnapshot { my_node: id, nodes, ..DeviceSnapshot::default() }
}

#[test]
fn decode_rejects_garbage() {
    let err = config_export::decode("not json").unwrap_err();
    assert!(format!("{err}").contains("not a valid config export"));
}

#[test]
fn empty_snapshot_roundtrips_with_all_options_none() {
    let snap = DeviceSnapshot::default();
    let _owner = Owner::default();
    let export = config_export::export_snapshot(&snap);
    let json = config_export::encode(&export);
    let round = config_export::decode(&json).expect("decode");
    assert!(round.lora.is_none());
    assert!(round.mqtt.is_none());
    assert!(round.channels.is_empty());
}
