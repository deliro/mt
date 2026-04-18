use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::domain::channel::{Channel, ChannelRole};
use crate::domain::config::{
    BluetoothSettings, CannedMessageSettings, ConsoleAccess, DeviceSettings, DisplaySettings,
    ExtNotifAlerts, ExtNotifOutputs, ExtNotifSound, ExtNotifTargets, ExternalNotificationSettings,
    LoraSettings, MqttSettings, NeighborInfoSettings, NetworkSettings, PositionSettings,
    PowerSettings, RangeTestSettings, SecuritySettings, StoreForwardSettings, TelemetrySettings,
};
use crate::domain::ids::{BROADCAST_NODE, ChannelIndex, ConfigId, NodeId, PacketId};
use crate::domain::message::{DeliveryState, Direction, Recipient, TextMessage};
use crate::domain::node::{Node, NodeRole, Position};
use crate::domain::session::HandshakeFragment;
use crate::proto::meshtastic;
use crate::proto::port::{PortPayload, parse};

pub fn fragments_from_radio(msg: meshtastic::FromRadio) -> Vec<HandshakeFragment> {
    use meshtastic::from_radio::PayloadVariant;
    let Some(variant) = msg.payload_variant else { return Vec::new() };
    match variant {
        PayloadVariant::MyInfo(info) => {
            vec![HandshakeFragment::MyNode { id: NodeId(info.my_node_num) }]
        }
        PayloadVariant::NodeInfo(ni) => vec![HandshakeFragment::Node(node_from_proto(&ni))],
        PayloadVariant::Channel(ch) => channel_fragments(ch),
        PayloadVariant::Metadata(meta) => vec![HandshakeFragment::Firmware(meta.firmware_version)],
        PayloadVariant::Config(cfg) => config_fragments(cfg),
        PayloadVariant::ConfigCompleteId(id) => {
            vec![HandshakeFragment::ConfigComplete { id: ConfigId(id) }]
        }
        PayloadVariant::Packet(p) => packet_fragments(p),
        PayloadVariant::ModuleConfig(cfg) => module_config_fragments(cfg),
        PayloadVariant::Rebooted(_)
        | PayloadVariant::QueueStatus(_)
        | PayloadVariant::XmodemPacket(_)
        | PayloadVariant::FileInfo(_)
        | PayloadVariant::LogRecord(_)
        | PayloadVariant::MqttClientProxyMessage(_)
        | PayloadVariant::ClientNotification(_)
        | PayloadVariant::DeviceuiConfig(_) => Vec::new(),
    }
}

fn packet_fragments(p: meshtastic::MeshPacket) -> Vec<HandshakeFragment> {
    use meshtastic::mesh_packet::PayloadVariant;
    let Some(PayloadVariant::Decoded(data)) = p.payload_variant else { return Vec::new() };
    let Ok(payload) = parse(data.portnum, &data.payload) else { return Vec::new() };
    let channel = ChannelIndex::new(p.channel as u8).unwrap_or_else(ChannelIndex::primary);
    match payload {
        PortPayload::Text(text) => vec![HandshakeFragment::Message(TextMessage {
            id: PacketId(p.id),
            channel,
            from: NodeId(p.from),
            to: if p.to == BROADCAST_NODE.0 {
                Recipient::Broadcast
            } else {
                Recipient::Node(NodeId(p.to))
            },
            text,
            received_at: packet_time(p.rx_time),
            direction: Direction::Incoming,
            state: DeliveryState::Acked,
        })],
        PortPayload::Position(_)
        | PortPayload::NodeInfo(_)
        | PortPayload::Telemetry(_)
        | PortPayload::Routing(_)
        | PortPayload::Admin(_)
        | PortPayload::Traceroute(_)
        | PortPayload::Unknown { .. } => Vec::new(),
    }
}

fn packet_time(rx_time_secs: u32) -> SystemTime {
    if rx_time_secs == 0 {
        SystemTime::now()
    } else {
        UNIX_EPOCH
            .checked_add(Duration::from_secs(u64::from(rx_time_secs)))
            .unwrap_or_else(SystemTime::now)
    }
}

pub fn node_from_proto(ni: &meshtastic::NodeInfo) -> Node {
    let last_heard = if ni.last_heard == 0 {
        None
    } else {
        UNIX_EPOCH.checked_add(Duration::from_secs(u64::from(ni.last_heard)))
    };
    Node {
        id: NodeId(ni.num),
        long_name: ni.user.as_ref().map(|u| u.long_name.clone()).unwrap_or_default(),
        short_name: ni.user.as_ref().map(|u| u.short_name.clone()).unwrap_or_default(),
        role: ni.user.as_ref().map_or(NodeRole::Client, |u| role_from_proto(u.role())),
        battery_level: ni.device_metrics.as_ref().map(|m| m.battery_level() as u8),
        voltage_v: ni.device_metrics.as_ref().map(meshtastic::DeviceMetrics::voltage),
        snr_db: Some(ni.snr),
        rssi_dbm: None,
        hops_away: Some(ni.hops_away() as u8),
        last_heard,
        position: ni.position.as_ref().map(|p| Position {
            latitude_deg: p.latitude_i() as f64 * 1e-7,
            longitude_deg: p.longitude_i() as f64 * 1e-7,
            altitude_m: Some(p.altitude()),
        }),
        is_favorite: ni.is_favorite,
        is_ignored: ni.is_ignored,
        public_key: ni.user.as_ref().map(|u| u.public_key.clone()).unwrap_or_default(),
    }
}

fn role_from_proto(role: meshtastic::config::device_config::Role) -> NodeRole {
    use meshtastic::config::device_config::Role;
    match role {
        Role::Client => NodeRole::Client,
        Role::ClientMute => NodeRole::ClientMute,
        Role::ClientHidden => NodeRole::ClientHidden,
        Role::ClientBase => NodeRole::ClientBase,
        Role::Router => NodeRole::Router,
        Role::RouterClient => NodeRole::RouterClient,
        Role::RouterLate => NodeRole::RouterLate,
        Role::Repeater => NodeRole::Repeater,
        Role::Tracker => NodeRole::Tracker,
        Role::Sensor => NodeRole::Sensor,
        Role::Tak => NodeRole::Tak,
        Role::TakTracker => NodeRole::TakTracker,
        Role::LostAndFound => NodeRole::LostAndFound,
    }
}

#[allow(dead_code)]
fn _touch_system_time(_: SystemTime) {}

pub fn channel_to_domain(ch: meshtastic::Channel) -> Vec<HandshakeFragment> {
    channel_fragments(ch)
}

fn channel_fragments(ch: meshtastic::Channel) -> Vec<HandshakeFragment> {
    let Some(index) = ChannelIndex::new(ch.index as u8) else {
        return Vec::new();
    };
    let role = match ch.role() {
        meshtastic::channel::Role::Primary => ChannelRole::Primary,
        meshtastic::channel::Role::Secondary => ChannelRole::Secondary,
        meshtastic::channel::Role::Disabled => ChannelRole::Disabled,
    };
    let (name, psk, uplink, downlink, position_precision) = match ch.settings {
        Some(s) => (
            s.name,
            s.psk,
            s.uplink_enabled,
            s.downlink_enabled,
            s.module_settings.map_or(0, |m| m.position_precision),
        ),
        None => (String::new(), Vec::new(), false, false, 0),
    };
    vec![HandshakeFragment::Channel(Channel {
        index,
        role,
        name,
        psk,
        uplink_enabled: uplink,
        downlink_enabled: downlink,
        position_precision,
    })]
}

fn module_config_fragments(cfg: meshtastic::ModuleConfig) -> Vec<HandshakeFragment> {
    use meshtastic::module_config::PayloadVariant;
    let Some(variant) = cfg.payload_variant else { return Vec::new() };
    match variant {
        PayloadVariant::Mqtt(m) => vec![HandshakeFragment::Mqtt(mqtt_from_proto(&m))],
        PayloadVariant::Telemetry(t) => {
            vec![HandshakeFragment::Telemetry(telemetry_from_proto(&t))]
        }
        PayloadVariant::NeighborInfo(n) => {
            vec![HandshakeFragment::NeighborInfo(neighbor_info_from_proto(n))]
        }
        PayloadVariant::StoreForward(sf) => {
            vec![HandshakeFragment::StoreForward(store_forward_from_proto(sf))]
        }
        PayloadVariant::ExternalNotification(e) => {
            vec![HandshakeFragment::ExtNotif(ext_notif_from_proto(&e))]
        }
        PayloadVariant::CannedMessage(c) => {
            vec![HandshakeFragment::Canned(canned_from_proto(&c))]
        }
        PayloadVariant::RangeTest(r) => {
            vec![HandshakeFragment::RangeTest(range_test_from_proto(r))]
        }
        PayloadVariant::Serial(_)
        | PayloadVariant::Audio(_)
        | PayloadVariant::RemoteHardware(_)
        | PayloadVariant::AmbientLighting(_)
        | PayloadVariant::DetectionSensor(_)
        | PayloadVariant::Paxcounter(_)
        | PayloadVariant::Statusmessage(_)
        | PayloadVariant::TrafficManagement(_)
        | PayloadVariant::Tak(_) => Vec::new(),
    }
}

pub const fn neighbor_info_from_proto(
    n: meshtastic::module_config::NeighborInfoConfig,
) -> NeighborInfoSettings {
    NeighborInfoSettings {
        enabled: n.enabled,
        transmit_over_lora: n.transmit_over_lora,
        update_interval_secs: n.update_interval,
    }
}

pub const fn ext_notif_from_proto(
    e: &meshtastic::module_config::ExternalNotificationConfig,
) -> ExternalNotificationSettings {
    ExternalNotificationSettings {
        enabled: e.enabled,
        output_ms: e.output_ms,
        nag_timeout_secs: e.nag_timeout,
        outputs: ExtNotifOutputs {
            output_pin: e.output,
            output_vibra_pin: e.output_vibra,
            output_buzzer_pin: e.output_buzzer,
            active_high: e.active,
        },
        alerts: ExtNotifAlerts {
            message: ExtNotifTargets {
                led: e.alert_message,
                vibra: e.alert_message_vibra,
                buzzer: e.alert_message_buzzer,
            },
            bell: ExtNotifTargets {
                led: e.alert_bell,
                vibra: e.alert_bell_vibra,
                buzzer: e.alert_bell_buzzer,
            },
        },
        sound: ExtNotifSound {
            use_pwm: e.use_pwm,
            use_i2s_as_buzzer: e.use_i2s_as_buzzer,
        },
    }
}

pub fn canned_from_proto(
    c: &meshtastic::module_config::CannedMessageConfig,
) -> CannedMessageSettings {
    CannedMessageSettings {
        rotary1_enabled: c.rotary1_enabled,
        updown1_enabled: c.updown1_enabled,
        send_bell: c.send_bell,
        rotary_pin_a: c.inputbroker_pin_a,
        rotary_pin_b: c.inputbroker_pin_b,
        rotary_pin_press: c.inputbroker_pin_press,
    }
}

pub const fn range_test_from_proto(
    r: meshtastic::module_config::RangeTestConfig,
) -> RangeTestSettings {
    RangeTestSettings {
        enabled: r.enabled,
        sender_secs: r.sender,
        save: r.save,
        clear_on_reboot: r.clear_on_reboot,
    }
}

pub const fn store_forward_from_proto(
    s: meshtastic::module_config::StoreForwardConfig,
) -> StoreForwardSettings {
    StoreForwardSettings {
        enabled: s.enabled,
        is_server: s.is_server,
        heartbeat: s.heartbeat,
        records: s.records,
        history_return_max: s.history_return_max,
        history_return_window_secs: s.history_return_window,
    }
}

pub fn telemetry_from_proto(t: &meshtastic::module_config::TelemetryConfig) -> TelemetrySettings {
    use crate::domain::config::{
        AirQualityMetricsCfg, DeviceMetricsCfg, EnvironmentMetricsCfg, HealthMetricsCfg,
        PowerMetricsCfg,
    };
    TelemetrySettings {
        device: DeviceMetricsCfg {
            enabled: t.device_telemetry_enabled,
            update_interval_secs: t.device_update_interval,
        },
        environment: EnvironmentMetricsCfg {
            measurement_enabled: t.environment_measurement_enabled,
            screen_enabled: t.environment_screen_enabled,
            display_fahrenheit: t.environment_display_fahrenheit,
            update_interval_secs: t.environment_update_interval,
        },
        air_quality: AirQualityMetricsCfg {
            measurement_enabled: t.air_quality_enabled,
            screen_enabled: t.air_quality_screen_enabled,
            update_interval_secs: t.air_quality_interval,
        },
        power: PowerMetricsCfg {
            measurement_enabled: t.power_measurement_enabled,
            screen_enabled: t.power_screen_enabled,
            update_interval_secs: t.power_update_interval,
        },
        health: HealthMetricsCfg {
            measurement_enabled: t.health_measurement_enabled,
            screen_enabled: t.health_screen_enabled,
            update_interval_secs: t.health_update_interval,
        },
    }
}

pub fn mqtt_from_proto(m: &meshtastic::module_config::MqttConfig) -> MqttSettings {
    use crate::domain::config::{MqttMapReport, MqttPayloadOptions};
    let (pub_secs, pos_prec, report_loc) = m
        .map_report_settings
        .as_ref()
        .map_or((0, 0, false), |s| {
            (s.publish_interval_secs, s.position_precision, s.should_report_location)
        });
    MqttSettings {
        enabled: m.enabled,
        address: m.address.clone(),
        username: m.username.clone(),
        password: m.password.clone(),
        root: m.root.clone(),
        tls_enabled: m.tls_enabled,
        proxy_to_client_enabled: m.proxy_to_client_enabled,
        payload: MqttPayloadOptions {
            encrypted: m.encryption_enabled,
            json: m.json_enabled,
        },
        map: MqttMapReport {
            enabled: m.map_reporting_enabled,
            publish_location: report_loc,
            publish_interval_secs: pub_secs,
            position_precision: pos_prec,
        },
    }
}

fn config_fragments(cfg: meshtastic::Config) -> Vec<HandshakeFragment> {
    use meshtastic::config::PayloadVariant;
    let Some(variant) = cfg.payload_variant else { return Vec::new() };
    match variant {
        PayloadVariant::Lora(lora) => vec![HandshakeFragment::Lora(lora_from_proto(&lora))],
        PayloadVariant::Device(d) => vec![HandshakeFragment::Device(device_from_proto(&d))],
        PayloadVariant::Position(p) => vec![HandshakeFragment::Position(position_from_proto(&p))],
        PayloadVariant::Power(p) => vec![HandshakeFragment::Power(power_from_proto(&p))],
        PayloadVariant::Network(n) => vec![HandshakeFragment::Network(network_from_proto(&n))],
        PayloadVariant::Display(d) => vec![HandshakeFragment::Display(display_from_proto(&d))],
        PayloadVariant::Bluetooth(b) => {
            vec![HandshakeFragment::Bluetooth(bluetooth_from_proto(&b))]
        }
        PayloadVariant::Security(s) => {
            vec![HandshakeFragment::Security(security_from_proto(&s))]
        }
        PayloadVariant::Sessionkey(_) | PayloadVariant::DeviceUi(_) => Vec::new(),
    }
}

pub fn security_from_proto(s: &meshtastic::config::SecurityConfig) -> SecuritySettings {
    SecuritySettings {
        public_key: s.public_key.clone(),
        private_key: s.private_key.clone(),
        admin_keys: s.admin_key.clone(),
        is_managed: s.is_managed,
        admin_channel_enabled: s.admin_channel_enabled,
        console: ConsoleAccess {
            serial_enabled: s.serial_enabled,
            debug_log_api_enabled: s.debug_log_api_enabled,
        },
    }
}

pub fn lora_from_proto(lora: &meshtastic::config::LoRaConfig) -> LoraSettings {
    LoraSettings {
        region: lora.region(),
        modem_preset: lora.modem_preset(),
        use_preset: lora.use_preset,
        hop_limit: lora.hop_limit.min(7) as u8,
        tx_enabled: lora.tx_enabled,
        tx_power: lora.tx_power,
    }
}

pub fn device_from_proto(d: &meshtastic::config::DeviceConfig) -> DeviceSettings {
    DeviceSettings {
        role: d.role(),
        rebroadcast_mode: d.rebroadcast_mode(),
        node_info_broadcast_secs: d.node_info_broadcast_secs,
        disable_triple_click: d.disable_triple_click,
        led_heartbeat_disabled: d.led_heartbeat_disabled,
        tzdef: d.tzdef.clone(),
    }
}

pub fn position_from_proto(p: &meshtastic::config::PositionConfig) -> PositionSettings {
    PositionSettings {
        broadcast_secs: p.position_broadcast_secs,
        smart_enabled: p.position_broadcast_smart_enabled,
        fixed_position: p.fixed_position,
        gps_update_interval: p.gps_update_interval,
        gps_mode: p.gps_mode(),
        smart_min_distance_m: p.broadcast_smart_minimum_distance,
        smart_min_interval_secs: p.broadcast_smart_minimum_interval_secs,
    }
}

pub fn power_from_proto(p: &meshtastic::config::PowerConfig) -> PowerSettings {
    PowerSettings {
        is_power_saving: p.is_power_saving,
        on_battery_shutdown_after_secs: p.on_battery_shutdown_after_secs,
        wait_bluetooth_secs: p.wait_bluetooth_secs,
        ls_secs: p.ls_secs,
        min_wake_secs: p.min_wake_secs,
    }
}

pub fn network_from_proto(n: &meshtastic::config::NetworkConfig) -> NetworkSettings {
    NetworkSettings {
        wifi_enabled: n.wifi_enabled,
        wifi_ssid: n.wifi_ssid.clone(),
        wifi_psk: n.wifi_psk.clone(),
        ntp_server: n.ntp_server.clone(),
        eth_enabled: n.eth_enabled,
    }
}

pub fn display_from_proto(d: &meshtastic::config::DisplayConfig) -> DisplaySettings {
    use crate::domain::config::{ClockFormat, ScreenOrientation};
    DisplaySettings {
        screen_on_secs: d.screen_on_secs,
        auto_carousel_secs: d.auto_screen_carousel_secs,
        orientation: if d.flip_screen { ScreenOrientation::Flipped } else { ScreenOrientation::Normal },
        units: d.units(),
        clock: if d.use_12h_clock { ClockFormat::H12 } else { ClockFormat::H24 },
        heading_bold: d.heading_bold,
        wake_on_tap_or_motion: d.wake_on_tap_or_motion,
    }
}

pub fn bluetooth_from_proto(b: &meshtastic::config::BluetoothConfig) -> BluetoothSettings {
    BluetoothSettings { enabled: b.enabled, mode: b.mode(), fixed_pin: b.fixed_pin }
}
