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
use crate::error::ConnectError;
use crate::proto::meshtastic;
use crate::proto::port::{PortPayload, parse};
use crate::session::commands::Command;
use crate::session::handshake::{fragments_from_radio, node_from_proto};
use crate::transport::BoxedTransport;

pub const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(300);
pub const ACK_TIMEOUT: Duration = Duration::from_secs(30);
pub const MY_INFO_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Clone, Debug)]
pub enum Event {
    Connecting,
    Connected(Box<DeviceSnapshot>),
    NodeUpdated(Node),
    ChannelUpdated(Channel),
    LoraUpdated(crate::domain::config::LoraSettings),
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
                | Command::SetLora(_) => {}
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
                    | Command::SetLora(_),
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
            HandshakeFragment::ConfigComplete { .. }
            | HandshakeFragment::Message(_)
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
            messages: Vec::new(),
            lora: self.lora,
        }
    }
}

async fn run_ready_loop(
    sink: &mut Pin<Box<impl futures::Sink<Vec<u8>, Error = crate::transport::TransportError> + ?Sized>>,
    stream: &mut (impl futures::Stream<Item = Result<Vec<u8>, crate::transport::TransportError>>
              + Unpin),
    my_node: NodeId,
    rx: &mut mpsc::UnboundedReceiver<Command>,
    tx: &mpsc::Sender<Event>,
) {
    let mut heartbeat = interval(HEARTBEAT_INTERVAL);
    heartbeat.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let _ = heartbeat.tick().await;
    let mut pending: std::collections::HashSet<PacketId> =
        std::collections::HashSet::default();

    loop {
        let step = tokio::select! {
            cmd = rx.recv() => match cmd {
                Some(c) => handle_command(c, my_node, sink, tx, &mut pending).await,
                None => LoopStep::Channel,
            },
            _ = heartbeat.tick() => match send_heartbeat(sink).await {
                Ok(()) => LoopStep::Continue,
                Err(e) => LoopStep::Error(e.to_string()),
            },
            item = stream.next() => handle_incoming(item, tx, &mut pending).await,
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

enum LoopStep {
    Continue,
    Disconnect,
    Error(String),
    Channel,
}

async fn handle_command(
    cmd: Command,
    my_node: NodeId,
    sink: &mut Pin<Box<impl futures::Sink<Vec<u8>, Error = crate::transport::TransportError> + ?Sized>>,
    tx: &mpsc::Sender<Event>,
    pending: &mut std::collections::HashSet<PacketId>,
) -> LoopStep {
    match cmd {
        Command::Connect(_) => {
            warn!("ignoring Connect while already connected");
            LoopStep::Continue
        }
        Command::Disconnect => LoopStep::Disconnect,
        Command::SendText { channel, to, text, want_ack } => {
            let is_dm = matches!(to, Recipient::Node(_));
            let on_wire_want_ack = want_ack && is_dm;
            match send_text(sink, channel, to, &text, on_wire_want_ack).await {
                Ok(id) => {
                    let _ = pending.insert(id);
                    let _ = tx
                        .send(Event::MessageReceived(TextMessage {
                            id,
                            channel,
                            from: my_node,
                            to,
                            text,
                            received_at: SystemTime::now(),
                            direction: Direction::Outgoing,
                            state: DeliveryState::Queued,
                        }))
                        .await;
                    if is_dm {
                        spawn_ack_timeout(tx.clone(), id);
                    }
                    LoopStep::Continue
                }
                Err(e) => LoopStep::Error(e.to_string()),
            }
        }
        Command::AckTimeout(id) => {
            if pending.remove(&id) {
                let _ = tx
                    .send(Event::MessageStateChanged {
                        id,
                        state: DeliveryState::Failed("no ack".into()),
                    })
                    .await;
            }
            LoopStep::Continue
        }
        Command::SetOwner { long_name, short_name } => {
            match send_set_owner(sink, my_node, &long_name, &short_name).await {
                Ok(()) => LoopStep::Continue,
                Err(e) => LoopStep::Error(e.to_string()),
            }
        }
        Command::SetLora(settings) => match send_set_lora(sink, my_node, &settings).await {
            Ok(()) => LoopStep::Continue,
            Err(e) => LoopStep::Error(e.to_string()),
        },
    }
}

async fn send_set_owner(
    sink: &mut Pin<Box<impl futures::Sink<Vec<u8>, Error = crate::transport::TransportError> + ?Sized>>,
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
    sink: &mut Pin<Box<impl futures::Sink<Vec<u8>, Error = crate::transport::TransportError> + ?Sized>>,
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
    let cfg = meshtastic::Config {
        payload_variant: Some(meshtastic::config::PayloadVariant::Lora(lora)),
    };
    let admin = meshtastic::AdminMessage {
        payload_variant: Some(meshtastic::admin_message::PayloadVariant::SetConfig(cfg)),
        ..Default::default()
    };
    send_admin(sink, my_node, admin).await
}

async fn send_admin(
    sink: &mut Pin<Box<impl futures::Sink<Vec<u8>, Error = crate::transport::TransportError> + ?Sized>>,
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
    tx: &mpsc::Sender<Event>,
    pending: &mut std::collections::HashSet<PacketId>,
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
    for outcome in incoming_outcomes(msg) {
        match outcome {
            IncomingOutcome::Event(ev) => {
                let _ = tx.send(ev).await;
            }
            IncomingOutcome::QueueOk(id) => {
                if pending.contains(&id) {
                    let _ = tx
                        .send(Event::MessageStateChanged { id, state: DeliveryState::Sent })
                        .await;
                }
            }
            IncomingOutcome::Ack { id, state } => {
                if pending.remove(&id) {
                    let _ = tx.send(Event::MessageStateChanged { id, state }).await;
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
}

async fn send_want_config_id(
    sink: &mut Pin<Box<impl futures::Sink<Vec<u8>, Error = crate::transport::TransportError> + ?Sized>>,
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
    sink: &mut Pin<Box<impl futures::Sink<Vec<u8>, Error = crate::transport::TransportError> + ?Sized>>,
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
    sink: &mut Pin<Box<impl futures::Sink<Vec<u8>, Error = crate::transport::TransportError> + ?Sized>>,
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

fn spawn_ack_timeout(tx: mpsc::Sender<Event>, id: PacketId) {
    tokio::spawn(async move {
        sleep(ACK_TIMEOUT).await;
        let _ = tx
            .send(Event::MessageStateChanged {
                id,
                state: DeliveryState::Failed("no ack".into()),
            })
            .await;
    });
}

async fn emit_error_and_disconnect(tx: &mpsc::Sender<Event>, msg: &str) {
    let _ = tx.send(Event::Error(msg.into())).await;
    let _ = tx.send(Event::Disconnected).await;
}

fn incoming_outcomes(msg: meshtastic::FromRadio) -> Vec<IncomingOutcome> {
    use meshtastic::from_radio::PayloadVariant;
    let Some(variant) = msg.payload_variant else { return Vec::new() };
    match variant {
        PayloadVariant::Packet(packet) => packet_outcomes(packet),
        PayloadVariant::NodeInfo(ni) => {
            vec![IncomingOutcome::Event(Event::NodeUpdated(node_from_proto(&ni)))]
        }
        PayloadVariant::Channel(ch) => channel_to_events(ch)
            .into_iter()
            .map(IncomingOutcome::Event)
            .collect(),
        PayloadVariant::Config(cfg) => config_to_events(cfg),
        PayloadVariant::QueueStatus(qs) if qs.res == 0 => {
            vec![IncomingOutcome::QueueOk(PacketId(qs.mesh_packet_id))]
        }
        PayloadVariant::QueueStatus(qs) => vec![IncomingOutcome::Ack {
            id: PacketId(qs.mesh_packet_id),
            state: DeliveryState::Failed(format!("device queue rejected ({})", qs.res)),
        }],
        PayloadVariant::MyInfo(_)
        | PayloadVariant::ModuleConfig(_)
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
    let Some(variant) = cfg.payload_variant else { return Vec::new() };
    match variant {
        PayloadVariant::Lora(lora) => {
            vec![IncomingOutcome::Event(Event::LoraUpdated(lora_from_proto(&lora)))]
        }
        PayloadVariant::Device(_)
        | PayloadVariant::Position(_)
        | PayloadVariant::Power(_)
        | PayloadVariant::Network(_)
        | PayloadVariant::Display(_)
        | PayloadVariant::Bluetooth(_)
        | PayloadVariant::Security(_)
        | PayloadVariant::Sessionkey(_)
        | PayloadVariant::DeviceUi(_) => Vec::new(),
    }
}

fn lora_from_proto(lora: &meshtastic::config::LoRaConfig) -> crate::domain::config::LoraSettings {
    crate::domain::config::LoraSettings {
        region: lora.region(),
        modem_preset: lora.modem_preset(),
        use_preset: lora.use_preset,
        hop_limit: lora.hop_limit.min(7) as u8,
        tx_enabled: lora.tx_enabled,
        tx_power: lora.tx_power,
    }
}

fn packet_outcomes(p: meshtastic::MeshPacket) -> Vec<IncomingOutcome> {
    use meshtastic::mesh_packet::PayloadVariant;
    let Some(PayloadVariant::Decoded(data)) = p.payload_variant else { return Vec::new() };
    let request_id = data.request_id;
    let Ok(payload) = parse(data.portnum, &data.payload) else { return Vec::new() };
    let channel = ChannelIndex::new(p.channel as u8).unwrap_or_else(ChannelIndex::primary);
    match payload {
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
        PortPayload::Routing(r) => {
            let id = PacketId(request_id);
            let state = match r.variant {
                Some(meshtastic::routing::Variant::ErrorReason(0)) => DeliveryState::Acked,
                Some(meshtastic::routing::Variant::ErrorReason(code)) => {
                    DeliveryState::Failed(format!("routing error {code}"))
                }
                Some(
                    meshtastic::routing::Variant::RouteRequest(_)
                    | meshtastic::routing::Variant::RouteReply(_),
                )
                | None => return Vec::new(),
            };
            vec![IncomingOutcome::Ack { id, state }]
        }
        PortPayload::Position(_)
        | PortPayload::NodeInfo(_)
        | PortPayload::Telemetry(_)
        | PortPayload::Admin(_)
        | PortPayload::Unknown { .. } => Vec::new(),
    }
}

fn channel_to_events(ch: meshtastic::Channel) -> Vec<Event> {
    use crate::domain::channel::ChannelRole;
    let Some(index) = ChannelIndex::new(ch.index as u8) else { return Vec::new() };
    let role = match ch.role() {
        meshtastic::channel::Role::Primary => ChannelRole::Primary,
        meshtastic::channel::Role::Secondary => ChannelRole::Secondary,
        meshtastic::channel::Role::Disabled => ChannelRole::Disabled,
    };
    let (name, has_psk) = match ch.settings {
        Some(s) => (s.name, !s.psk.is_empty()),
        None => (String::new(), false),
    };
    vec![Event::ChannelUpdated(Channel { index, role, name, has_psk })]
}

