pub mod commands;
pub mod handshake;

use std::pin::Pin;
use std::time::{Duration, SystemTime};

use futures::future::BoxFuture;
use futures::{SinkExt, StreamExt};
use prost::Message;
use tokio::sync::mpsc;
use tokio::time::{MissedTickBehavior, interval, sleep};
use tracing::warn;

use crate::codec::frame::encode as encode_frame;
use crate::domain::channel::{Channel, ChannelRole};
use crate::domain::ids::{BROADCAST_NODE, ChannelIndex, ConfigId, NodeId, PacketId};
use crate::domain::message::{DeliveryState, Direction, Recipient, TextMessage};
use crate::domain::node::{Node, NodeRole, Position};
use crate::domain::profile::{ConnectionProfile, TransportKind};
use crate::domain::snapshot::DeviceSnapshot;
use crate::error::ConnectError;
use crate::proto::meshtastic;
use crate::proto::port::{PortPayload, parse};
use crate::session::commands::Command;
use crate::session::handshake::run_handshake;
use crate::transport::BoxedTransport;

pub const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(300);
pub const ACK_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, Debug)]
pub enum Event {
    Connecting,
    Connected(Box<DeviceSnapshot>),
    NodeUpdated(Node),
    ChannelUpdated(Channel),
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
                Command::Disconnect | Command::SendText { .. } | Command::AckTimeout(_) => {}
            }
        }
    }
}

#[allow(clippy::cognitive_complexity, clippy::too_many_lines)]
async fn run_connection(
    connect: &Connector,
    profile: ConnectionProfile,
    rx: &mut mpsc::UnboundedReceiver<Command>,
    tx: &mpsc::Sender<Event>,
) {
    let _ = tx.send(Event::Connecting).await;
    let (transport, kind) = match (connect)(profile).await {
        Ok(pair) => pair,
        Err(e) => {
            let _ = tx.send(Event::Error(e.to_string())).await;
            let _ = tx.send(Event::Disconnected).await;
            return;
        }
    };
    let config_id = ConfigId::random();
    let (snapshot, transport) = match run_handshake(transport, kind, config_id).await {
        Ok(pair) => pair,
        Err(e) => {
            let _ = tx.send(Event::Error(e.to_string())).await;
            let _ = tx.send(Event::Disconnected).await;
            return;
        }
    };
    let _ = tx.send(Event::Connected(Box::new(snapshot))).await;

    let (sink, stream) = transport.split();
    let mut sink: Pin<Box<_>> = Box::pin(sink);
    let mut stream: Pin<Box<_>> = Box::pin(stream);

    let mut heartbeat = interval(HEARTBEAT_INTERVAL);
    heartbeat.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let _ = heartbeat.tick().await;

    loop {
        tokio::select! {
            cmd = rx.recv() => {
                let Some(cmd) = cmd else { break };
                match cmd {
                    Command::Connect(_) => {
                        warn!("ignoring Connect while already connected");
                    }
                    Command::Disconnect => {
                        let _ = tx.send(Event::Disconnected).await;
                        return;
                    }
                    Command::SendText { channel, to, text, want_ack } => {
                        match send_text(&mut sink, channel, to, &text, want_ack).await {
                            Ok(id) => {
                                let _ = tx.send(Event::MessageReceived(TextMessage {
                                    id,
                                    channel,
                                    from: NodeId(0),
                                    to,
                                    text,
                                    received_at: SystemTime::now(),
                                    direction: Direction::Outgoing,
                                    state: DeliveryState::Pending,
                                })).await;
                                spawn_ack_timeout(tx.clone(), id);
                            }
                            Err(e) => {
                                let _ = tx.send(Event::Error(e.to_string())).await;
                                let _ = tx.send(Event::Disconnected).await;
                                return;
                            }
                        }
                    }
                    Command::AckTimeout(id) => {
                        let _ = tx.send(Event::MessageStateChanged {
                            id,
                            state: DeliveryState::Failed("no ack".into()),
                        }).await;
                    }
                }
            }
            _ = heartbeat.tick() => {
                if let Err(e) = send_heartbeat(&mut sink).await {
                    let _ = tx.send(Event::Error(e.to_string())).await;
                    let _ = tx.send(Event::Disconnected).await;
                    return;
                }
            }
            item = stream.next() => {
                let Some(item) = item else {
                    let _ = tx.send(Event::Disconnected).await;
                    return;
                };
                let frame = match item {
                    Ok(f) => f,
                    Err(e) => {
                        let _ = tx.send(Event::Error(e.to_string())).await;
                        let _ = tx.send(Event::Disconnected).await;
                        return;
                    }
                };
                let msg = match meshtastic::FromRadio::decode(frame.as_slice()) {
                    Ok(m) => m,
                    Err(e) => { warn!(%e, "bad FromRadio"); continue; }
                };
                for ev in events_from_radio(msg) {
                    let _ = tx.send(ev).await;
                }
            }
        }
    }
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
        want_response: want_ack,
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
    let frame = encode_frame(&buf)?;
    sink.send(frame).await?;
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
    let frame = encode_frame(&buf)?;
    sink.send(frame).await?;
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

fn events_from_radio(msg: meshtastic::FromRadio) -> Vec<Event> {
    use meshtastic::from_radio::PayloadVariant;
    let Some(variant) = msg.payload_variant else { return Vec::new() };
    match variant {
        PayloadVariant::Packet(packet) => packet_to_events(packet),
        PayloadVariant::NodeInfo(ni) => vec![Event::NodeUpdated(node_from_proto(&ni))],
        PayloadVariant::Channel(ch) => channel_to_events(ch),
        PayloadVariant::MyInfo(_)
        | PayloadVariant::Config(_)
        | PayloadVariant::ModuleConfig(_)
        | PayloadVariant::ConfigCompleteId(_)
        | PayloadVariant::Rebooted(_)
        | PayloadVariant::QueueStatus(_)
        | PayloadVariant::XmodemPacket(_)
        | PayloadVariant::Metadata(_)
        | PayloadVariant::FileInfo(_)
        | PayloadVariant::LogRecord(_)
        | PayloadVariant::MqttClientProxyMessage(_)
        | PayloadVariant::ClientNotification(_)
        | PayloadVariant::DeviceuiConfig(_) => Vec::new(),
    }
}

fn packet_to_events(p: meshtastic::MeshPacket) -> Vec<Event> {
    use meshtastic::mesh_packet::PayloadVariant;
    let Some(PayloadVariant::Decoded(data)) = p.payload_variant else { return Vec::new() };
    let request_id = data.request_id;
    let Ok(payload) = parse(data.portnum, &data.payload) else { return Vec::new() };
    let channel = ChannelIndex::new(p.channel as u8).unwrap_or_else(ChannelIndex::primary);
    match payload {
        PortPayload::Text(text) => vec![Event::MessageReceived(TextMessage {
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
            state: DeliveryState::Delivered,
        })],
        PortPayload::Routing(r) => {
            let id = PacketId(request_id);
            let state = match r.variant {
                Some(meshtastic::routing::Variant::ErrorReason(0)) => DeliveryState::Delivered,
                Some(meshtastic::routing::Variant::ErrorReason(code)) => {
                    DeliveryState::Failed(format!("routing error {code}"))
                }
                Some(
                    meshtastic::routing::Variant::RouteRequest(_)
                    | meshtastic::routing::Variant::RouteReply(_),
                )
                | None => return Vec::new(),
            };
            vec![Event::MessageStateChanged { id, state }]
        }
        PortPayload::Position(_)
        | PortPayload::NodeInfo(_)
        | PortPayload::Telemetry(_)
        | PortPayload::Admin(_)
        | PortPayload::Unknown { .. } => Vec::new(),
    }
}

fn node_from_proto(ni: &meshtastic::NodeInfo) -> Node {
    Node {
        id: NodeId(ni.num),
        long_name: ni.user.as_ref().map(|u| u.long_name.clone()).unwrap_or_default(),
        short_name: ni.user.as_ref().map(|u| u.short_name.clone()).unwrap_or_default(),
        role: NodeRole::Client,
        battery_level: ni.device_metrics.as_ref().map(|m| m.battery_level() as u8),
        voltage_v: ni.device_metrics.as_ref().map(meshtastic::DeviceMetrics::voltage),
        snr_db: Some(ni.snr),
        rssi_dbm: None,
        hops_away: Some(ni.hops_away() as u8),
        last_heard: None,
        position: ni.position.as_ref().map(|p| Position {
            latitude_deg: p.latitude_i() as f64 * 1e-7,
            longitude_deg: p.longitude_i() as f64 * 1e-7,
            altitude_m: Some(p.altitude()),
        }),
    }
}

fn channel_to_events(ch: meshtastic::Channel) -> Vec<Event> {
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

