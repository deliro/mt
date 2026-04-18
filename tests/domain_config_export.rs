#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mt::domain::channel::{Channel, ChannelRole};
use mt::domain::config::{
    BluetoothSettings, DeviceSettings, DisplaySettings, LoraSettings, MqttSettings,
    NeighborInfoSettings, NetworkSettings, PositionSettings, PowerSettings, RangeTestSettings,
    StoreForwardSettings, TelemetrySettings,
};
use mt::domain::config_export::{self, ConfigExport, Owner, EXPORT_VERSION};
use mt::domain::ids::ChannelIndex;
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
    assert_eq!(export.version, EXPORT_VERSION);
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
fn decode_rejects_wrong_version() {
    let json = r#"{"version":99,"owner":{"long_name":"","short_name":""},"channels":[]}"#;
    let err = config_export::decode(json).unwrap_err();
    assert!(format!("{err}").contains("unsupported export version 99"));
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
