pub mod commands;
pub mod handshake;

use std::pin::Pin;
use std::time::{Duration, SystemTime};

use futures::future::BoxFuture;
use futures::{SinkExt, StreamExt};
use prost::Message;
use tokio::sync::mpsc;
use tokio::time::{MissedTickBehavior, interval, sleep};
use tracing::{debug, warn};

use crate::domain::channel::Channel;
use crate::domain::ids::{BROADCAST_NODE, ChannelIndex, NodeId, PacketId};
use crate::domain::message::{DeliveryState, Direction, Recipient, TextMessage};
use crate::domain::node::Node;
use crate::domain::profile::{ConnectionProfile, TransportKind};
use crate::domain::session::HandshakeFragment;
use crate::domain::snapshot::DeviceSnapshot;
use crate::domain::stats::MeshStats;
use crate::error::ConnectError;
use crate::proto::meshtastic;
use crate::proto::port::{PortPayload, parse};
use crate::session::commands::Command;
use crate::session::handshake::{fragments_from_radio, node_from_proto};
use crate::transport::BoxedTransport;

pub const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(300);
pub const ACK_TIMEOUT: Duration = Duration::from_secs(30);
pub const MY_INFO_TIMEOUT: Duration = Duration::from_secs(15);
/// Kill the session if no frame arrives for this long.
///
/// The radio emits position / telemetry / node-info far more often than this on
/// any live link, so the watchdog only trips after a real disconnect — e.g.
/// laptop suspend/resume leaving a stale BLE handle that silently drops writes.
pub const RX_WATCHDOG: Duration = Duration::from_secs(420);
/// Cap on how long a single heartbeat write may block.
///
/// Protects against OS-level buffering where the underlying socket is dead but
/// `Sink::send` never errors.
const HEARTBEAT_SEND_TIMEOUT: Duration = Duration::from_secs(10);

trait FrameSink: futures::Sink<Vec<u8>, Error = crate::transport::TransportError> {}
impl<T: ?Sized + futures::Sink<Vec<u8>, Error = crate::transport::TransportError>> FrameSink for T {}

type SinkRef<'a, S> = &'a mut Pin<Box<S>>;

#[derive(Clone, Debug)]
pub enum Event {
    Connecting,
    Connected(Box<DeviceSnapshot>),
    NodeUpdated(Node),
    ChannelUpdated(Channel),
    LoraUpdated(crate::domain::config::LoraSettings),
    DeviceUpdated(crate::domain::config::DeviceSettings),
    PositionUpdated(crate::domain::config::PositionSettings),
    PowerUpdated(crate::domain::config::PowerSettings),
    NetworkUpdated(crate::domain::config::NetworkSettings),
    DisplayUpdated(crate::domain::config::DisplaySettings),
    BluetoothUpdated(crate::domain::config::BluetoothSettings),
    MqttUpdated(crate::domain::config::MqttSettings),
    TelemetryCfgUpdated(crate::domain::config::TelemetrySettings),
    NeighborInfoUpdated(crate::domain::config::NeighborInfoSettings),
    StatsUpdated(MeshStats),
    TracerouteResult(crate::domain::traceroute::TracerouteResult),
    TracerouteFailed { target: NodeId, reason: String },
    MessageReceived(TextMessage),
    MessageStateChanged { id: PacketId, state: DeliveryState },
    Disconnected,
    Error(String),
}

pub type Connector = Box<
    dyn Fn(ConnectionProfile) -> BoxFuture<'static, Result<(BoxedTransport, TransportKind), ConnectError>>
        + Send
        + Sync,
>;

pub struct DeviceSession {
    connect: Connector,
}

impl DeviceSession {
    pub fn new(connect: Connector) -> Self {
        Self { connect }
    }

    pub async fn run(
        self,
        mut rx: mpsc::UnboundedReceiver<Command>,
        tx: mpsc::Sender<Event>,
    ) {
        while let Some(cmd) = rx.recv().await {
            match cmd {
                Command::Connect(profile) => {
                    run_connection(&self.connect, profile, &mut rx, &tx).await;
                }
                Command::Disconnect
                | Command::SendText { .. }
                | Command::AckTimeout(_)
                | Command::SetOwner { .. }
                | Command::SetLora(_)
                | Command::SetDevice(_)
                | Command::SetPosition(_)
                | Command::SetPower(_)
                | Command::SetNetwork(_)
                | Command::SetDisplay(_)
                | Command::SetBluetooth(_)
                | Command::SetFixedPosition { .. }
                | Command::RemoveFixedPosition
                | Command::Admin(_)
                | Command::SetFavorite { .. }
                | Command::SetIgnored { .. }
                | Command::Traceroute { .. }
                | Command::SetChannel(_)
                | Command::SetMqtt(_)
                | Command::SetTelemetryCfg(_)
                | Command::SetNeighborInfo(_) => {}
            }
        }
    }
}

async fn run_connection(
    connect: &Connector,
    profile: ConnectionProfile,
    rx: &mut mpsc::UnboundedReceiver<Command>,
    tx: &mpsc::Sender<Event>,
) {
    let _ = tx.send(Event::Connecting).await;
    let Some((transport, _kind)) = open_with_cancel(connect, profile, rx, tx).await else {
        return;
    };
    let (sink, stream) = transport.split();
    let mut sink: Pin<Box<_>> = Box::pin(sink);
    let mut stream: Pin<Box<_>> = Box::pin(stream);

    if let Err(e) = send_want_config_id(&mut sink).await {
        emit_error_and_disconnect(tx, &e.to_string()).await;
        return;
    }

    let Some(my_node) = wait_for_my_info(&mut stream, rx, tx).await else {
        return;
    };

    run_ready_loop(&mut sink, &mut stream, my_node, rx, tx).await;
}

async fn open_with_cancel(
    connect: &Connector,
    profile: ConnectionProfile,
    rx: &mut mpsc::UnboundedReceiver<Command>,
    tx: &mpsc::Sender<Event>,
) -> Option<(BoxedTransport, TransportKind)> {
    let mut work = Box::pin((connect)(profile));
    loop {
        tokio::select! {
            biased;
            cmd = rx.recv() => match cmd {
                Some(Command::Disconnect) | None => {
                    let _ = tx.send(Event::Disconnected).await;
                    return None;
                }
                Some(Command::Connect(_)) => warn!("ignoring Connect while already connecting"),
                Some(
                    Command::SendText { .. }
                    | Command::AckTimeout(_)
                    | Command::SetOwner { .. }
                    | Command::SetLora(_)
                    | Command::SetDevice(_)
                    | Command::SetPosition(_)
                    | Command::SetPower(_)
                    | Command::SetNetwork(_)
                    | Command::SetDisplay(_)
                    | Command::SetBluetooth(_)
                    | Command::SetFixedPosition { .. }
                    | Command::RemoveFixedPosition
                    | Command::Admin(_)
                    | Command::SetFavorite { .. }
                    | Command::SetIgnored { .. }
                    | Command::Traceroute { .. }
                    | Command::SetChannel(_)
                    | Command::SetMqtt(_)
                    | Command::SetTelemetryCfg(_)
                    | Command::SetNeighborInfo(_),
                ) => {
                    debug!("ignoring command while connecting");
                }
            },
            result = &mut work => return match result {
                Ok(pair) => Some(pair),
                Err(e) => {
                    emit_error_and_disconnect(tx, &e.to_string()).await;
                    None
                }
            },
        }
    }
}

async fn wait_for_my_info(
    stream: &mut (impl futures::Stream<Item = Result<Vec<u8>, crate::transport::TransportError>>
              + Unpin),
    rx: &mut mpsc::UnboundedReceiver<Command>,
    tx: &mpsc::Sender<Event>,
) -> Option<NodeId> {
    let mut acc = InitAcc::default();
    let deadline = tokio::time::Instant::now()
        .checked_add(MY_INFO_TIMEOUT)
        .unwrap_or_else(tokio::time::Instant::now);

    loop {
        tokio::select! {
            biased;
            cmd = rx.recv() => match cmd {
                Some(Command::Disconnect) | None => {
                    let _ = tx.send(Event::Disconnected).await;
                    return None;
                }
                Some(_) => {}
            },
            () = tokio::time::sleep_until(deadline) => {
                emit_error_and_disconnect(tx, "timeout waiting for device").await;
                return None;
            }
            item = stream.next() => {
                let Some(item) = item else {
                    emit_error_and_disconnect(tx, "transport closed").await;
                    return None;
                };
                let frame = match item {
                    Ok(f) => f,
                    Err(e) => {
                        emit_error_and_disconnect(tx, &e.to_string()).await;
                        return None;
                    }
                };
                let Ok(msg) = meshtastic::FromRadio::decode(frame.as_slice()) else {
                    continue;
                };
                for fragment in fragments_from_radio(msg) {
                    acc.apply(fragment);
                }
                if let Some(my_node) = acc.my_node {
                    let snap = acc.into_snapshot(my_node);
                    let _ = tx.send(Event::Connected(Box::new(snap))).await;
                    return Some(my_node);
                }
            }
        }
    }
}

#[derive(Default)]
struct InitAcc {
    my_node: Option<NodeId>,
    firmware: String,
    nodes: std::collections::HashMap<NodeId, Node>,
    channels: Vec<Channel>,
    lora: Option<crate::domain::config::LoraSettings>,
    device: Option<crate::domain::config::DeviceSettings>,
    position: Option<crate::domain::config::PositionSettings>,
    power: Option<crate::domain::config::PowerSettings>,
    network: Option<crate::domain::config::NetworkSettings>,
    display: Option<crate::domain::config::DisplaySettings>,
    bluetooth: Option<crate::domain::config::BluetoothSettings>,
    mqtt: Option<crate::domain::config::MqttSettings>,
    telemetry: Option<crate::domain::config::TelemetrySettings>,
    neighbor_info: Option<crate::domain::config::NeighborInfoSettings>,
    messages: Vec<TextMessage>,
}

impl InitAcc {
    fn apply(&mut self, fragment: HandshakeFragment) {
        match fragment {
            HandshakeFragment::MyNode { id } => self.my_node = Some(id),
            HandshakeFragment::Firmware(v) => self.firmware = v,
            HandshakeFragment::Node(node) => {
                let _ = self.nodes.insert(node.id, node);
            }
            HandshakeFragment::Channel(ch) => match self.channels.iter_mut().find(|c| c.index == ch.index) {
                Some(slot) => *slot = ch,
                None => self.channels.push(ch),
            },
            HandshakeFragment::Lora(settings) => self.lora = Some(settings),
            HandshakeFragment::Device(settings) => self.device = Some(settings),
            HandshakeFragment::Position(settings) => self.position = Some(settings),
            HandshakeFragment::Power(settings) => self.power = Some(settings),
            HandshakeFragment::Network(settings) => self.network = Some(settings),
            HandshakeFragment::Display(settings) => self.display = Some(settings),
            HandshakeFragment::Bluetooth(settings) => self.bluetooth = Some(settings),
            HandshakeFragment::Mqtt(settings) => self.mqtt = Some(settings),
            HandshakeFragment::Telemetry(settings) => self.telemetry = Some(settings),
            HandshakeFragment::NeighborInfo(settings) => self.neighbor_info = Some(settings),
            HandshakeFragment::Message(msg) => self.messages.push(msg),
            HandshakeFragment::ConfigComplete { .. }
            | HandshakeFragment::MessageStateChanged { .. }
            | HandshakeFragment::NodeMetric { .. } => {}
        }
    }

    fn into_snapshot(self, my_node: NodeId) -> DeviceSnapshot {
        let (short, long) = self
            .nodes
            .get(&my_node)
            .map(|n| (n.short_name.clone(), n.long_name.clone()))
            .unwrap_or_default();
        DeviceSnapshot {
            my_node,
            short_name: short,
            long_name: long,
            firmware_version: self.firmware,
            nodes: self.nodes,
            channels: self.channels,
            messages: self.messages,
            lora: self.lora,
            device: self.device,
            position: self.position,
            power: self.power,
            network: self.network,
            stats: crate::domain::stats::MeshStats::default(),
            display: self.display,
            bluetooth: self.bluetooth,
            mqtt: self.mqtt,
            telemetry: self.telemetry,
            neighbor_info: self.neighbor_info,
        }
    }
}

async fn run_ready_loop(
    sink: SinkRef<'_, impl FrameSink + ?Sized>,
    stream: &mut (impl futures::Stream<Item = Result<Vec<u8>, crate::transport::TransportError>>
              + Unpin),
    my_node: NodeId,
    rx: &mut mpsc::UnboundedReceiver<Command>,
    tx: &mpsc::Sender<Event>,
) {
    let mut heartbeat = interval(HEARTBEAT_INTERVAL);
    heartbeat.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let _ = heartbeat.tick().await;
    let mut pending = PendingOps::default();
    let mut last_rx = tokio::time::Instant::now();

    loop {
        let watchdog_deadline = last_rx.checked_add(RX_WATCHDOG).unwrap_or(last_rx);
        let watchdog = tokio::time::sleep_until(watchdog_deadline);
        let step = tokio::select! {
            cmd = rx.recv() => match cmd {
                Some(c) => handle_command(c, my_node, sink, tx, &mut pending).await,
                None => LoopStep::Channel,
            },
            _ = heartbeat.tick() => heartbeat_step(sink, my_node).await,
            item = stream.next() => {
                last_rx = tokio::time::Instant::now();
                handle_incoming(item, my_node, tx, &mut pending).await
            },
            () = watchdog => LoopStep::Error(format!(
                "no data from device in {}s — connection appears stale",
                RX_WATCHDOG.as_secs(),
            )),
        };
        match step {
            LoopStep::Continue => {}
            LoopStep::Channel => return,
            LoopStep::Disconnect => {
                let _ = tx.send(Event::Disconnected).await;
                return;
            }
            LoopStep::Error(msg) => {
                emit_error_and_disconnect(tx, &msg).await;
                return;
            }
        }
    }
}

async fn heartbeat_step(
    sink: SinkRef<'_, impl FrameSink + ?Sized>,
    my_node: NodeId,
) -> LoopStep {
    let work = async move {
        send_heartbeat(&mut *sink).await?;
        send_metadata_probe(&mut *sink, my_node).await?;
        Ok::<(), ConnectError>(())
    };
    match tokio::time::timeout(HEARTBEAT_SEND_TIMEOUT, work).await {
        Ok(Ok(())) => LoopStep::Continue,
        Ok(Err(e)) => LoopStep::Error(e.to_string()),
        Err(_) => LoopStep::Error(format!(
            "heartbeat blocked >{}s — transport stuck",
            HEARTBEAT_SEND_TIMEOUT.as_secs(),
        )),
    }
}

async fn send_metadata_probe(
    sink: SinkRef<'_, impl FrameSink + ?Sized>,
    my_node: NodeId,
) -> Result<(), ConnectError> {
    let admin = meshtastic::AdminMessage {
        payload_variant: Some(meshtastic::admin_message::PayloadVariant::GetDeviceMetadataRequest(
            true,
        )),
        ..Default::default()
    };
    send_admin(sink, my_node, admin).await
}

enum LoopStep {
    Continue,
    Disconnect,
    Error(String),
    Channel,
}

#[derive(Default)]
struct PendingOps {
    text_sent: std::collections::HashSet<PacketId>,
    text_acks: std::collections::HashMap<PacketId, tokio::sync::oneshot::Sender<()>>,
    tracers: std::collections::HashMap<PacketId, tokio::sync::oneshot::Sender<()>>,
}

async fn handle_command(
    cmd: Command,
    my_node: NodeId,
    sink: SinkRef<'_, impl FrameSink + ?Sized>,
    tx: &mpsc::Sender<Event>,
    pending: &mut PendingOps,
) -> LoopStep {
    match cmd {
        Command::Connect(_) => {
            warn!("ignoring Connect while already connected");
            LoopStep::Continue
        }
        Command::Disconnect => LoopStep::Disconnect,
        Command::SendText { channel, to, text, want_ack } => {
            let req = SendTextRequest { channel, to, text, want_ack };
            handle_send_text(sink, my_node, tx, pending, req).await
        }
        Command::AckTimeout(id) => {
            if pending.text_acks.remove(&id).is_some() {
                let _ = pending.text_sent.remove(&id);
                let _ = tx
                    .send(Event::MessageStateChanged {
                        id,
                        state: DeliveryState::Failed("no ack".into()),
                    })
                    .await;
            }
            LoopStep::Continue
        }
        Command::Traceroute { node } => {
            handle_traceroute(sink, my_node, tx, pending, node).await
        }
        other => handle_config_command(other, my_node, sink).await,
    }
}

struct SendTextRequest {
    channel: ChannelIndex,
    to: Recipient,
    text: String,
    want_ack: bool,
}

async fn handle_send_text(
    sink: SinkRef<'_, impl FrameSink + ?Sized>,
    my_node: NodeId,
    tx: &mpsc::Sender<Event>,
    pending: &mut PendingOps,
    req: SendTextRequest,
) -> LoopStep {
    let is_dm = matches!(req.to, Recipient::Node(_));
    let on_wire_want_ack = req.want_ack && is_dm;
    match send_text(sink, req.channel, req.to, &req.text, on_wire_want_ack).await {
        Ok(id) => {
            let _ = pending.text_sent.insert(id);
            if is_dm {
                let cancel = spawn_ack_timeout(tx.clone(), id);
                let _ = pending.text_acks.insert(id, cancel);
            }
            let _ = tx
                .send(Event::MessageReceived(TextMessage {
                    id,
                    channel: req.channel,
                    from: my_node,
                    to: req.to,
                    text: req.text,
                    received_at: SystemTime::now(),
                    direction: Direction::Outgoing,
                    state: DeliveryState::Queued,
                }))
                .await;
            LoopStep::Continue
        }
        Err(e) => LoopStep::Error(e.to_string()),
    }
}

async fn handle_config_command(
    cmd: Command,
    my_node: NodeId,
    sink: SinkRef<'_, impl FrameSink + ?Sized>,
) -> LoopStep {
    let result = match cmd {
        Command::SetOwner { long_name, short_name } => {
            send_set_owner(sink, my_node, &long_name, &short_name).await
        }
        Command::SetLora(s) => send_set_lora(sink, my_node, &s).await,
        Command::SetDevice(s) => send_set_device(sink, my_node, &s).await,
        Command::SetPosition(s) => send_set_position(sink, my_node, &s).await,
        Command::SetPower(s) => send_set_power(sink, my_node, &s).await,
        Command::SetNetwork(s) => send_set_network(sink, my_node, &s).await,
        Command::SetDisplay(s) => send_set_display(sink, my_node, &s).await,
        Command::SetBluetooth(s) => send_set_bluetooth(sink, my_node, &s).await,
        Command::SetFixedPosition { latitude_deg, longitude_deg, altitude_m } => {
            send_set_fixed_position(sink, my_node, latitude_deg, longitude_deg, altitude_m).await
        }
        Command::RemoveFixedPosition => send_remove_fixed_position(sink, my_node).await,
        Command::Admin(action) => send_admin_action(sink, my_node, action).await,
        Command::SetFavorite { node, favorite } => {
            send_favorite(sink, my_node, node, favorite).await
        }
        Command::SetIgnored { node, ignored } => {
            send_ignored(sink, my_node, node, ignored).await
        }
        Command::SetChannel(channel) => send_set_channel(sink, my_node, &channel).await,
        Command::SetMqtt(s) => send_set_mqtt(sink, my_node, &s).await,
        Command::SetTelemetryCfg(s) => send_set_telemetry_cfg(sink, my_node, &s).await,
        Command::SetNeighborInfo(s) => send_set_neighbor_info(sink, my_node, &s).await,
        Command::Traceroute { .. } => Ok(()),
        Command::Connect(_)
        | Command::Disconnect
        | Command::SendText { .. }
        | Command::AckTimeout(_) => return LoopStep::Continue,
    };
    match result {
        Ok(()) => LoopStep::Continue,
        Err(e) => LoopStep::Error(e.to_string()),
    }
}

async fn send_set_owner(
    sink: SinkRef<'_, impl FrameSink + ?Sized>,
    my_node: NodeId,
    long_name: &str,
    short_name: &str,
) -> Result<(), ConnectError> {
    let admin = meshtastic::AdminMessage {
        payload_variant: Some(meshtastic::admin_message::PayloadVariant::SetOwner(
            meshtastic::User {
                id: format!("!{:08x}", my_node.0),
                long_name: long_name.into(),
                short_name: short_name.into(),
                ..Default::default()
            },
        )),
        ..Default::default()
    };
    send_admin(sink, my_node, admin).await
}

async fn send_set_lora(
    sink: SinkRef<'_, impl FrameSink + ?Sized>,
    my_node: NodeId,
    settings: &crate::domain::config::LoraSettings,
) -> Result<(), ConnectError> {
    let lora = meshtastic::config::LoRaConfig {
        use_preset: settings.use_preset,
        modem_preset: settings.modem_preset as i32,
        region: settings.region as i32,
        hop_limit: u32::from(settings.hop_limit.min(7)),
        tx_enabled: settings.tx_enabled,
        tx_power: settings.tx_power,
        ..Default::default()
    };
    send_config(sink, my_node, meshtastic::config::PayloadVariant::Lora(lora)).await
}

async fn send_set_device(
    sink: SinkRef<'_, impl FrameSink + ?Sized>,
    my_node: NodeId,
    s: &crate::domain::config::DeviceSettings,
) -> Result<(), ConnectError> {
    let device = meshtastic::config::DeviceConfig {
        role: s.role as i32,
        rebroadcast_mode: s.rebroadcast_mode as i32,
        node_info_broadcast_secs: s.node_info_broadcast_secs,
        disable_triple_click: s.disable_triple_click,
        led_heartbeat_disabled: s.led_heartbeat_disabled,
        tzdef: s.tzdef.clone(),
        ..Default::default()
    };
    send_config(sink, my_node, meshtastic::config::PayloadVariant::Device(device)).await
}

async fn send_set_position(
    sink: SinkRef<'_, impl FrameSink + ?Sized>,
    my_node: NodeId,
    s: &crate::domain::config::PositionSettings,
) -> Result<(), ConnectError> {
    let position = meshtastic::config::PositionConfig {
        position_broadcast_secs: s.broadcast_secs,
        position_broadcast_smart_enabled: s.smart_enabled,
        fixed_position: s.fixed_position,
        gps_update_interval: s.gps_update_interval,
        gps_mode: s.gps_mode as i32,
        broadcast_smart_minimum_distance: s.smart_min_distance_m,
        broadcast_smart_minimum_interval_secs: s.smart_min_interval_secs,
        ..Default::default()
    };
    send_config(sink, my_node, meshtastic::config::PayloadVariant::Position(position)).await
}

async fn send_set_power(
    sink: SinkRef<'_, impl FrameSink + ?Sized>,
    my_node: NodeId,
    s: &crate::domain::config::PowerSettings,
) -> Result<(), ConnectError> {
    let power = meshtastic::config::PowerConfig {
        is_power_saving: s.is_power_saving,
        on_battery_shutdown_after_secs: s.on_battery_shutdown_after_secs,
        wait_bluetooth_secs: s.wait_bluetooth_secs,
        ls_secs: s.ls_secs,
        min_wake_secs: s.min_wake_secs,
        ..Default::default()
    };
    send_config(sink, my_node, meshtastic::config::PayloadVariant::Power(power)).await
}

async fn send_set_network(
    sink: SinkRef<'_, impl FrameSink + ?Sized>,
    my_node: NodeId,
    s: &crate::domain::config::NetworkSettings,
) -> Result<(), ConnectError> {
    let network = meshtastic::config::NetworkConfig {
        wifi_enabled: s.wifi_enabled,
        wifi_ssid: s.wifi_ssid.clone(),
        wifi_psk: s.wifi_psk.clone(),
        ntp_server: s.ntp_server.clone(),
        eth_enabled: s.eth_enabled,
        ..Default::default()
    };
    send_config(sink, my_node, meshtastic::config::PayloadVariant::Network(network)).await
}

async fn send_set_display(
    sink: SinkRef<'_, impl FrameSink + ?Sized>,
    my_node: NodeId,
    s: &crate::domain::config::DisplaySettings,
) -> Result<(), ConnectError> {
    use crate::domain::config::{ClockFormat, ScreenOrientation};
    let display = meshtastic::config::DisplayConfig {
        screen_on_secs: s.screen_on_secs,
        auto_screen_carousel_secs: s.auto_carousel_secs,
        flip_screen: matches!(s.orientation, ScreenOrientation::Flipped),
        units: s.units as i32,
        heading_bold: s.heading_bold,
        wake_on_tap_or_motion: s.wake_on_tap_or_motion,
        use_12h_clock: matches!(s.clock, ClockFormat::H12),
        ..Default::default()
    };
    send_config(sink, my_node, meshtastic::config::PayloadVariant::Display(display)).await
}

async fn send_set_bluetooth(
    sink: SinkRef<'_, impl FrameSink + ?Sized>,
    my_node: NodeId,
    s: &crate::domain::config::BluetoothSettings,
) -> Result<(), ConnectError> {
    let bt = meshtastic::config::BluetoothConfig {
        enabled: s.enabled,
        mode: s.mode as i32,
        fixed_pin: s.fixed_pin,
    };
    send_config(sink, my_node, meshtastic::config::PayloadVariant::Bluetooth(bt)).await
}

async fn send_set_fixed_position(
    sink: SinkRef<'_, impl FrameSink + ?Sized>,
    my_node: NodeId,
    lat: f64,
    lon: f64,
    alt: i32,
) -> Result<(), ConnectError> {
    let lat_i = (lat * 1e7).round() as i32;
    let lon_i = (lon * 1e7).round() as i32;
    let pos = meshtastic::Position {
        latitude_i: Some(lat_i),
        longitude_i: Some(lon_i),
        altitude: Some(alt),
        ..Default::default()
    };
    let admin = meshtastic::AdminMessage {
        payload_variant: Some(meshtastic::admin_message::PayloadVariant::SetFixedPosition(pos)),
        ..Default::default()
    };
    send_admin(sink, my_node, admin).await
}

async fn send_remove_fixed_position(
    sink: SinkRef<'_, impl FrameSink + ?Sized>,
    my_node: NodeId,
) -> Result<(), ConnectError> {
    let admin = meshtastic::AdminMessage {
        payload_variant: Some(meshtastic::admin_message::PayloadVariant::RemoveFixedPosition(
            true,
        )),
        ..Default::default()
    };
    send_admin(sink, my_node, admin).await
}

const TRACEROUTE_TIMEOUT: Duration = Duration::from_secs(60);

async fn handle_traceroute(
    sink: SinkRef<'_, impl FrameSink + ?Sized>,
    my_node: NodeId,
    tx: &mpsc::Sender<Event>,
    pending: &mut PendingOps,
    target: NodeId,
) -> LoopStep {
    match send_traceroute(sink, my_node, target).await {
        Ok(id) => {
            let cancel = spawn_traceroute_timeout(tx.clone(), id, target);
            let _ = pending.tracers.insert(id, cancel);
            LoopStep::Continue
        }
        Err(e) => LoopStep::Error(e.to_string()),
    }
}

const TRACEROUTE_HOP_LIMIT: u32 = 7;

async fn send_traceroute(
    sink: SinkRef<'_, impl FrameSink + ?Sized>,
    my_node: NodeId,
    target: NodeId,
) -> Result<PacketId, ConnectError> {
    let id = PacketId::random();
    let rd = meshtastic::RouteDiscovery::default();
    let mut rd_buf = Vec::with_capacity(rd.encoded_len());
    rd.encode(&mut rd_buf)?;
    let data = meshtastic::Data {
        portnum: meshtastic::PortNum::TracerouteApp as i32,
        payload: rd_buf,
        want_response: true,
        ..Default::default()
    };
    let packet = meshtastic::MeshPacket {
        from: my_node.0,
        to: target.0,
        channel: 0,
        id: id.0,
        want_ack: false,
        hop_limit: TRACEROUTE_HOP_LIMIT,
        hop_start: TRACEROUTE_HOP_LIMIT,
        payload_variant: Some(meshtastic::mesh_packet::PayloadVariant::Decoded(data)),
        ..Default::default()
    };
    let msg = meshtastic::ToRadio {
        payload_variant: Some(meshtastic::to_radio::PayloadVariant::Packet(packet)),
    };
    let mut buf = Vec::with_capacity(msg.encoded_len());
    msg.encode(&mut buf)?;
    sink.send(buf).await?;
    Ok(id)
}

fn spawn_traceroute_timeout(
    tx: mpsc::Sender<Event>,
    id: PacketId,
    target: NodeId,
) -> tokio::sync::oneshot::Sender<()> {
    let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        tokio::select! {
            () = sleep(TRACEROUTE_TIMEOUT) => {
                let _ = tx
                    .send(Event::TracerouteFailed {
                        target,
                        reason: format!("no reply in {}s", TRACEROUTE_TIMEOUT.as_secs()),
                    })
                    .await;
                let _ = id;
            }
            _ = cancel_rx => {}
        }
    });
    cancel_tx
}

async fn send_favorite(
    sink: SinkRef<'_, impl FrameSink + ?Sized>,
    my_node: NodeId,
    node: NodeId,
    favorite: bool,
) -> Result<(), ConnectError> {
    use meshtastic::admin_message::PayloadVariant;
    let variant = if favorite {
        PayloadVariant::SetFavoriteNode(node.0)
    } else {
        PayloadVariant::RemoveFavoriteNode(node.0)
    };
    let admin = meshtastic::AdminMessage {
        payload_variant: Some(variant),
        ..Default::default()
    };
    send_admin(sink, my_node, admin).await
}

async fn send_set_mqtt(
    sink: SinkRef<'_, impl FrameSink + ?Sized>,
    my_node: NodeId,
    s: &crate::domain::config::MqttSettings,
) -> Result<(), ConnectError> {
    let mqtt = meshtastic::module_config::MqttConfig {
        enabled: s.enabled,
        address: s.address.clone(),
        username: s.username.clone(),
        password: s.password.clone(),
        encryption_enabled: s.payload.encrypted,
        json_enabled: s.payload.json,
        tls_enabled: s.tls_enabled,
        root: s.root.clone(),
        proxy_to_client_enabled: s.proxy_to_client_enabled,
        map_reporting_enabled: s.map.enabled,
        map_report_settings: Some(meshtastic::module_config::MapReportSettings {
            publish_interval_secs: s.map.publish_interval_secs,
            position_precision: s.map.position_precision,
            should_report_location: s.map.publish_location,
        }),
    };
    send_module_config(
        sink,
        my_node,
        meshtastic::module_config::PayloadVariant::Mqtt(mqtt),
    )
    .await
}

async fn send_set_telemetry_cfg(
    sink: SinkRef<'_, impl FrameSink + ?Sized>,
    my_node: NodeId,
    s: &crate::domain::config::TelemetrySettings,
) -> Result<(), ConnectError> {
    let t = meshtastic::module_config::TelemetryConfig {
        device_update_interval: s.device.update_interval_secs,
        device_telemetry_enabled: s.device.enabled,
        environment_update_interval: s.environment.update_interval_secs,
        environment_measurement_enabled: s.environment.measurement_enabled,
        environment_screen_enabled: s.environment.screen_enabled,
        environment_display_fahrenheit: s.environment.display_fahrenheit,
        air_quality_interval: s.air_quality.update_interval_secs,
        air_quality_enabled: s.air_quality.measurement_enabled,
        air_quality_screen_enabled: s.air_quality.screen_enabled,
        power_update_interval: s.power.update_interval_secs,
        power_measurement_enabled: s.power.measurement_enabled,
        power_screen_enabled: s.power.screen_enabled,
        health_update_interval: s.health.update_interval_secs,
        health_measurement_enabled: s.health.measurement_enabled,
        health_screen_enabled: s.health.screen_enabled,
    };
    send_module_config(
        sink,
        my_node,
        meshtastic::module_config::PayloadVariant::Telemetry(t),
    )
    .await
}

async fn send_set_neighbor_info(
    sink: SinkRef<'_, impl FrameSink + ?Sized>,
    my_node: NodeId,
    s: &crate::domain::config::NeighborInfoSettings,
) -> Result<(), ConnectError> {
    let ni = meshtastic::module_config::NeighborInfoConfig {
        enabled: s.enabled,
        update_interval: s.update_interval_secs,
        transmit_over_lora: s.transmit_over_lora,
    };
    send_module_config(
        sink,
        my_node,
        meshtastic::module_config::PayloadVariant::NeighborInfo(ni),
    )
    .await
}

async fn send_module_config(
    sink: SinkRef<'_, impl FrameSink + ?Sized>,
    my_node: NodeId,
    payload: meshtastic::module_config::PayloadVariant,
) -> Result<(), ConnectError> {
    let cfg = meshtastic::ModuleConfig { payload_variant: Some(payload) };
    let admin = meshtastic::AdminMessage {
        payload_variant: Some(meshtastic::admin_message::PayloadVariant::SetModuleConfig(cfg)),
        ..Default::default()
    };
    send_admin(sink, my_node, admin).await
}

async fn send_set_channel(
    sink: SinkRef<'_, impl FrameSink + ?Sized>,
    my_node: NodeId,
    ch: &crate::domain::channel::Channel,
) -> Result<(), ConnectError> {
    use crate::domain::channel::ChannelRole;
    let role = match ch.role {
        ChannelRole::Primary => meshtastic::channel::Role::Primary,
        ChannelRole::Secondary => meshtastic::channel::Role::Secondary,
        ChannelRole::Disabled => meshtastic::channel::Role::Disabled,
    };
    let settings = meshtastic::ChannelSettings {
        name: ch.name.clone(),
        psk: ch.psk.clone(),
        uplink_enabled: ch.uplink_enabled,
        downlink_enabled: ch.downlink_enabled,
        module_settings: Some(meshtastic::ModuleSettings {
            position_precision: ch.position_precision,
            is_muted: false,
        }),
        ..Default::default()
    };
    let channel = meshtastic::Channel {
        index: i32::from(ch.index.get()),
        settings: Some(settings),
        role: role as i32,
    };
    let admin = meshtastic::AdminMessage {
        payload_variant: Some(meshtastic::admin_message::PayloadVariant::SetChannel(channel)),
        ..Default::default()
    };
    send_admin(sink, my_node, admin).await
}

async fn send_ignored(
    sink: SinkRef<'_, impl FrameSink + ?Sized>,
    my_node: NodeId,
    node: NodeId,
    ignored: bool,
) -> Result<(), ConnectError> {
    use meshtastic::admin_message::PayloadVariant;
    let variant = if ignored {
        PayloadVariant::SetIgnoredNode(node.0)
    } else {
        PayloadVariant::RemoveIgnoredNode(node.0)
    };
    let admin = meshtastic::AdminMessage {
        payload_variant: Some(variant),
        ..Default::default()
    };
    send_admin(sink, my_node, admin).await
}

async fn send_admin_action(
    sink: SinkRef<'_, impl FrameSink + ?Sized>,
    my_node: NodeId,
    action: crate::session::commands::AdminAction,
) -> Result<(), ConnectError> {
    use crate::session::commands::AdminAction;
    use meshtastic::admin_message::PayloadVariant;
    let variant = match action {
        AdminAction::Reboot { seconds } => PayloadVariant::RebootSeconds(seconds),
        AdminAction::Shutdown { seconds } => PayloadVariant::ShutdownSeconds(seconds),
        AdminAction::RebootOta { seconds } => PayloadVariant::RebootOtaSeconds(seconds),
        AdminAction::FactoryResetDevice => PayloadVariant::FactoryResetDevice(1),
        AdminAction::FactoryResetConfig => PayloadVariant::FactoryResetConfig(1),
        AdminAction::NodedbReset => PayloadVariant::NodedbReset(true),
    };
    let admin = meshtastic::AdminMessage {
        payload_variant: Some(variant),
        ..Default::default()
    };
    send_admin(sink, my_node, admin).await
}

async fn send_config(
    sink: SinkRef<'_, impl FrameSink + ?Sized>,
    my_node: NodeId,
    payload: meshtastic::config::PayloadVariant,
) -> Result<(), ConnectError> {
    let cfg = meshtastic::Config { payload_variant: Some(payload) };
    let admin = meshtastic::AdminMessage {
        payload_variant: Some(meshtastic::admin_message::PayloadVariant::SetConfig(cfg)),
        ..Default::default()
    };
    send_admin(sink, my_node, admin).await
}

async fn send_admin(
    sink: SinkRef<'_, impl FrameSink + ?Sized>,
    my_node: NodeId,
    admin: meshtastic::AdminMessage,
) -> Result<(), ConnectError> {
    let mut payload = Vec::with_capacity(admin.encoded_len());
    admin.encode(&mut payload)?;
    let data = meshtastic::Data {
        portnum: meshtastic::PortNum::AdminApp as i32,
        payload,
        want_response: true,
        ..Default::default()
    };
    let packet = meshtastic::MeshPacket {
        from: my_node.0,
        to: my_node.0,
        channel: 0,
        id: PacketId::random().0,
        want_ack: true,
        payload_variant: Some(meshtastic::mesh_packet::PayloadVariant::Decoded(data)),
        ..Default::default()
    };
    let msg = meshtastic::ToRadio {
        payload_variant: Some(meshtastic::to_radio::PayloadVariant::Packet(packet)),
    };
    let mut buf = Vec::with_capacity(msg.encoded_len());
    msg.encode(&mut buf)?;
    sink.send(buf).await?;
    Ok(())
}

async fn handle_incoming(
    item: Option<Result<Vec<u8>, crate::transport::TransportError>>,
    my_node: NodeId,
    tx: &mpsc::Sender<Event>,
    pending: &mut PendingOps,
) -> LoopStep {
    let Some(item) = item else { return LoopStep::Disconnect };
    let frame = match item {
        Ok(f) => f,
        Err(e) => return LoopStep::Error(e.to_string()),
    };
    let msg = match meshtastic::FromRadio::decode(frame.as_slice()) {
        Ok(m) => m,
        Err(e) => {
            warn!(%e, "bad FromRadio");
            return LoopStep::Continue;
        }
    };
    for outcome in incoming_outcomes(msg, my_node) {
        match outcome {
            IncomingOutcome::Event(ev) => {
                let _ = tx.send(ev).await;
            }
            IncomingOutcome::QueueOk(id) => {
                if pending.text_sent.remove(&id) {
                    let _ = tx
                        .send(Event::MessageStateChanged { id, state: DeliveryState::Sent })
                        .await;
                }
            }
            IncomingOutcome::Ack { id, state } => {
                if let Some(cancel) = pending.text_acks.remove(&id) {
                    let _ = cancel.send(());
                    let _ = pending.text_sent.remove(&id);
                    let _ = tx.send(Event::MessageStateChanged { id, state }).await;
                }
            }
            IncomingOutcome::RouteReply { request_id, route } => {
                if let Some(cancel) = pending.tracers.remove(&request_id) {
                    let _ = cancel.send(());
                    let _ = tx.send(Event::TracerouteResult(route)).await;
                }
            }
        }
    }
    LoopStep::Continue
}

enum IncomingOutcome {
    Event(Event),
    QueueOk(PacketId),
    Ack { id: PacketId, state: DeliveryState },
    RouteReply { request_id: PacketId, route: crate::domain::traceroute::TracerouteResult },
}

async fn send_want_config_id(
    sink: SinkRef<'_, impl FrameSink + ?Sized>,
) -> Result<(), ConnectError> {
    let id = crate::domain::ids::ConfigId::random().0;
    let msg = meshtastic::ToRadio {
        payload_variant: Some(meshtastic::to_radio::PayloadVariant::WantConfigId(id)),
    };
    let mut buf = Vec::with_capacity(msg.encoded_len());
    msg.encode(&mut buf)?;
    sink.send(buf).await?;
    Ok(())
}

async fn send_text(
    sink: SinkRef<'_, impl FrameSink + ?Sized>,
    channel: ChannelIndex,
    to: Recipient,
    text: &str,
    want_ack: bool,
) -> Result<PacketId, ConnectError> {
    let id = PacketId::random();
    let dest = match to {
        Recipient::Broadcast => BROADCAST_NODE.0,
        Recipient::Node(n) => n.0,
    };
    let data = meshtastic::Data {
        portnum: meshtastic::PortNum::TextMessageApp as i32,
        payload: text.as_bytes().to_vec(),
        ..Default::default()
    };
    let packet = meshtastic::MeshPacket {
        from: 0,
        to: dest,
        channel: channel.get() as u32,
        id: id.0,
        want_ack,
        payload_variant: Some(meshtastic::mesh_packet::PayloadVariant::Decoded(data)),
        ..Default::default()
    };
    let msg = meshtastic::ToRadio {
        payload_variant: Some(meshtastic::to_radio::PayloadVariant::Packet(packet)),
    };
    let mut buf = Vec::with_capacity(msg.encoded_len());
    msg.encode(&mut buf)?;
    sink.send(buf).await?;
    Ok(id)
}

async fn send_heartbeat(
    sink: SinkRef<'_, impl FrameSink + ?Sized>,
) -> Result<(), ConnectError> {
    let msg = meshtastic::ToRadio {
        payload_variant: Some(meshtastic::to_radio::PayloadVariant::Heartbeat(
            meshtastic::Heartbeat { nonce: 0 },
        )),
    };
    let mut buf = Vec::with_capacity(msg.encoded_len());
    msg.encode(&mut buf)?;
    sink.send(buf).await?;
    Ok(())
}

fn spawn_ack_timeout(
    tx: mpsc::Sender<Event>,
    id: PacketId,
) -> tokio::sync::oneshot::Sender<()> {
    let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        tokio::select! {
            () = sleep(ACK_TIMEOUT) => {
                let _ = tx
                    .send(Event::MessageStateChanged {
                        id,
                        state: DeliveryState::Failed("no ack".into()),
                    })
                    .await;
            }
            _ = cancel_rx => {}
        }
    });
    cancel_tx
}

async fn emit_error_and_disconnect(tx: &mpsc::Sender<Event>, msg: &str) {
    let _ = tx.send(Event::Error(msg.into())).await;
    let _ = tx.send(Event::Disconnected).await;
}

fn incoming_outcomes(msg: meshtastic::FromRadio, my_node: NodeId) -> Vec<IncomingOutcome> {
    use meshtastic::from_radio::PayloadVariant;
    let Some(variant) = msg.payload_variant else { return Vec::new() };
    match variant {
        PayloadVariant::Packet(packet) => packet_outcomes(packet, my_node),
        PayloadVariant::NodeInfo(ni) => node_info_outcomes(&ni, my_node),
        PayloadVariant::Channel(ch) => channel_to_events(ch)
            .into_iter()
            .map(IncomingOutcome::Event)
            .collect(),
        PayloadVariant::Config(cfg) => config_to_events(cfg),
        PayloadVariant::ModuleConfig(cfg) => module_config_to_events(cfg),
        PayloadVariant::QueueStatus(qs) if qs.res == 0 => {
            vec![IncomingOutcome::QueueOk(PacketId(qs.mesh_packet_id))]
        }
        PayloadVariant::QueueStatus(qs) => vec![IncomingOutcome::Ack {
            id: PacketId(qs.mesh_packet_id),
            state: DeliveryState::Failed(format!("device queue rejected ({})", qs.res)),
        }],
        PayloadVariant::MyInfo(_)
        | PayloadVariant::ConfigCompleteId(_)
        | PayloadVariant::Rebooted(_)
        | PayloadVariant::XmodemPacket(_)
        | PayloadVariant::Metadata(_)
        | PayloadVariant::FileInfo(_)
        | PayloadVariant::LogRecord(_)
        | PayloadVariant::MqttClientProxyMessage(_)
        | PayloadVariant::ClientNotification(_)
        | PayloadVariant::DeviceuiConfig(_) => Vec::new(),
    }
}

fn config_to_events(cfg: meshtastic::Config) -> Vec<IncomingOutcome> {
    use meshtastic::config::PayloadVariant;
    use crate::session::handshake::{
        bluetooth_from_proto, device_from_proto, display_from_proto, lora_from_proto,
        network_from_proto, position_from_proto, power_from_proto,
    };
    let Some(variant) = cfg.payload_variant else { return Vec::new() };
    match variant {
        PayloadVariant::Lora(lora) => {
            vec![IncomingOutcome::Event(Event::LoraUpdated(lora_from_proto(&lora)))]
        }
        PayloadVariant::Device(d) => {
            vec![IncomingOutcome::Event(Event::DeviceUpdated(device_from_proto(&d)))]
        }
        PayloadVariant::Position(p) => {
            vec![IncomingOutcome::Event(Event::PositionUpdated(position_from_proto(&p)))]
        }
        PayloadVariant::Power(p) => {
            vec![IncomingOutcome::Event(Event::PowerUpdated(power_from_proto(&p)))]
        }
        PayloadVariant::Network(n) => {
            vec![IncomingOutcome::Event(Event::NetworkUpdated(network_from_proto(&n)))]
        }
        PayloadVariant::Display(d) => {
            vec![IncomingOutcome::Event(Event::DisplayUpdated(display_from_proto(&d)))]
        }
        PayloadVariant::Bluetooth(b) => {
            vec![IncomingOutcome::Event(Event::BluetoothUpdated(bluetooth_from_proto(&b)))]
        }
        PayloadVariant::Security(_) | PayloadVariant::Sessionkey(_) | PayloadVariant::DeviceUi(_) => {
            Vec::new()
        }
    }
}

fn module_config_to_events(cfg: meshtastic::ModuleConfig) -> Vec<IncomingOutcome> {
    use meshtastic::module_config::PayloadVariant;
    use crate::session::handshake::{mqtt_from_proto, neighbor_info_from_proto, telemetry_from_proto};
    let Some(variant) = cfg.payload_variant else { return Vec::new() };
    match variant {
        PayloadVariant::Mqtt(m) => {
            vec![IncomingOutcome::Event(Event::MqttUpdated(mqtt_from_proto(&m)))]
        }
        PayloadVariant::Telemetry(t) => {
            vec![IncomingOutcome::Event(Event::TelemetryCfgUpdated(telemetry_from_proto(&t)))]
        }
        PayloadVariant::NeighborInfo(n) => {
            vec![IncomingOutcome::Event(Event::NeighborInfoUpdated(neighbor_info_from_proto(n)))]
        }
        PayloadVariant::Serial(_)
        | PayloadVariant::ExternalNotification(_)
        | PayloadVariant::StoreForward(_)
        | PayloadVariant::RangeTest(_)
        | PayloadVariant::CannedMessage(_)
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

fn node_info_outcomes(ni: &meshtastic::NodeInfo, my_node: NodeId) -> Vec<IncomingOutcome> {
    let mut out = vec![IncomingOutcome::Event(Event::NodeUpdated(node_from_proto(ni)))];
    if NodeId(ni.num) == my_node
        && let Some(metrics) = ni.device_metrics.as_ref()
    {
        out.push(IncomingOutcome::Event(Event::StatsUpdated(stats_from_device_metrics(metrics))));
    }
    out
}

fn packet_outcomes(p: meshtastic::MeshPacket, my_node: NodeId) -> Vec<IncomingOutcome> {
    use meshtastic::mesh_packet::PayloadVariant;
    let Some(PayloadVariant::Decoded(data)) = p.payload_variant else { return Vec::new() };
    let request_id = data.request_id;
    let Ok(payload) = parse(data.portnum, &data.payload) else { return Vec::new() };
    let channel = ChannelIndex::new(p.channel as u8).unwrap_or_else(ChannelIndex::primary);
    let from_self = NodeId(p.from) == my_node;
    match payload {
        PortPayload::Telemetry(t) => {
            if from_self {
                telemetry_outcomes(t)
            } else {
                Vec::new()
            }
        }
        PortPayload::Text(text) => vec![IncomingOutcome::Event(Event::MessageReceived(TextMessage {
            id: PacketId(p.id),
            channel,
            from: NodeId(p.from),
            to: if p.to == BROADCAST_NODE.0 {
                Recipient::Broadcast
            } else {
                Recipient::Node(NodeId(p.to))
            },
            text,
            received_at: SystemTime::now(),
            direction: Direction::Incoming,
            state: DeliveryState::Acked,
        }))],
        PortPayload::Routing(r) => routing_to_outcomes(PacketId(request_id), NodeId(p.from), r),
        PortPayload::Traceroute(rd) => vec![IncomingOutcome::RouteReply {
            request_id: PacketId(request_id),
            route: route_discovery_to_result(NodeId(p.from), &rd),
        }],
        PortPayload::Position(_)
        | PortPayload::NodeInfo(_)
        | PortPayload::Admin(_)
        | PortPayload::Unknown { .. } => Vec::new(),
    }
}

fn routing_to_outcomes(
    request_id: PacketId,
    from: NodeId,
    r: meshtastic::Routing,
) -> Vec<IncomingOutcome> {
    use meshtastic::routing::Variant;
    match r.variant {
        Some(Variant::ErrorReason(0)) => {
            vec![IncomingOutcome::Ack { id: request_id, state: DeliveryState::Acked }]
        }
        Some(Variant::ErrorReason(code)) => vec![IncomingOutcome::Ack {
            id: request_id,
            state: DeliveryState::Failed(format!("routing error {code}")),
        }],
        Some(Variant::RouteReply(rd)) => {
            vec![IncomingOutcome::RouteReply {
                request_id,
                route: route_discovery_to_result(from, &rd),
            }]
        }
        Some(Variant::RouteRequest(_)) | None => Vec::new(),
    }
}

fn route_discovery_to_result(
    target: NodeId,
    rd: &meshtastic::RouteDiscovery,
) -> crate::domain::traceroute::TracerouteResult {
    crate::domain::traceroute::TracerouteResult {
        target,
        route: rd.route.iter().copied().map(NodeId).collect(),
        snr_towards_db: rd.snr_towards.iter().map(|x| *x as f32 * 0.25).collect(),
        route_back: rd.route_back.iter().copied().map(NodeId).collect(),
        snr_back_db: rd.snr_back.iter().map(|x| *x as f32 * 0.25).collect(),
        completed_at: std::time::SystemTime::now(),
    }
}

fn telemetry_outcomes(t: meshtastic::Telemetry) -> Vec<IncomingOutcome> {
    use meshtastic::telemetry::Variant;
    let Some(variant) = t.variant else { return Vec::new() };
    match variant {
        Variant::DeviceMetrics(m) => {
            vec![IncomingOutcome::Event(Event::StatsUpdated(stats_from_device_metrics(&m)))]
        }
        Variant::LocalStats(s) => {
            vec![IncomingOutcome::Event(Event::StatsUpdated(stats_from_local_stats(&s)))]
        }
        Variant::EnvironmentMetrics(_)
        | Variant::AirQualityMetrics(_)
        | Variant::PowerMetrics(_)
        | Variant::HealthMetrics(_)
        | Variant::HostMetrics(_)
        | Variant::TrafficManagementStats(_) => Vec::new(),
    }
}

fn stats_from_device_metrics(m: &meshtastic::DeviceMetrics) -> MeshStats {
    MeshStats {
        battery_level: m.battery_level.and_then(|v| u8::try_from(v).ok()),
        voltage_v: m.voltage,
        channel_utilization: m.channel_utilization,
        air_util_tx: m.air_util_tx,
        uptime_seconds: m.uptime_seconds,
        ..MeshStats::default()
    }
}

fn stats_from_local_stats(s: &meshtastic::LocalStats) -> MeshStats {
    MeshStats {
        uptime_seconds: Some(s.uptime_seconds),
        channel_utilization: Some(s.channel_utilization),
        air_util_tx: Some(s.air_util_tx),
        num_packets_tx: Some(s.num_packets_tx),
        num_packets_rx: Some(s.num_packets_rx),
        num_tx_relay: Some(s.num_tx_relay),
        num_online_nodes: Some(s.num_online_nodes),
        ..MeshStats::default()
    }
}

fn channel_to_events(ch: meshtastic::Channel) -> Vec<Event> {
    use crate::domain::session::HandshakeFragment;
    crate::session::handshake::channel_to_domain(ch)
        .into_iter()
        .filter_map(|f| match f {
            HandshakeFragment::Channel(c) => Some(Event::ChannelUpdated(c)),
            _ => None,
        })
        .collect()
}

