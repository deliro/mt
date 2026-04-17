use std::time::Duration;

use futures::FutureExt;
use mt::domain::ids::{ChannelIndex, NodeId};
use mt::domain::message::{Direction, Recipient};
use mt::domain::profile::{ConnectionProfile, TransportKind};
use mt::proto::meshtastic;
use mt::session::commands::Command;
use mt::session::{DeviceSession, Event};
use mt::transport::BoxedTransport;
use mt::transport::mock::{MockTransport, Script};
use prost::Message;
use tokio::sync::mpsc;
use tokio::time::timeout;

fn enc_from(m: meshtastic::FromRadio) -> Vec<u8> {
    let mut buf = Vec::with_capacity(m.encoded_len());
    m.encode(&mut buf).expect("encode");
    buf
}

fn my_info(num: u32) -> meshtastic::FromRadio {
    meshtastic::FromRadio {
        id: 1,
        payload_variant: Some(meshtastic::from_radio::PayloadVariant::MyInfo(
            meshtastic::MyNodeInfo { my_node_num: num, ..Default::default() },
        )),
    }
}

fn config_complete(id: u32) -> meshtastic::FromRadio {
    meshtastic::FromRadio {
        id: 2,
        payload_variant: Some(meshtastic::from_radio::PayloadVariant::ConfigCompleteId(id)),
    }
}

fn text_packet(from: u32, id: u32, text: &str) -> meshtastic::FromRadio {
    let data = meshtastic::Data {
        portnum: meshtastic::PortNum::TextMessageApp as i32,
        payload: text.as_bytes().to_vec(),
        ..Default::default()
    };
    let packet = meshtastic::MeshPacket {
        from,
        to: 0xFFFF_FFFF,
        channel: 0,
        id,
        want_ack: false,
        payload_variant: Some(meshtastic::mesh_packet::PayloadVariant::Decoded(data)),
        ..Default::default()
    };
    meshtastic::FromRadio {
        id: 3,
        payload_variant: Some(meshtastic::from_radio::PayloadVariant::Packet(packet)),
    }
}

async fn next_matching<F>(rx: &mut mpsc::Receiver<Event>, pred: F) -> Option<Event>
where
    F: Fn(&Event) -> bool,
{
    for _ in 0..32 {
        let ev = timeout(Duration::from_millis(500), rx.recv()).await.ok().flatten()?;
        if pred(&ev) {
            return Some(ev);
        }
    }
    None
}

#[tokio::test(flavor = "current_thread")]
async fn receives_text_after_connect() {
    let connector: mt::session::Connector = Box::new(move |_profile: ConnectionProfile| {
        async move {
            let (transport, handle) = MockTransport::new(Script::from_frames(Vec::new()));
            let echo = handle.clone();
            tokio::spawn(async move {
                for _ in 0..50 {
                    if let Some(frame) = echo.captured().first() {
                        if frame.len() > 4 {
                            if let Ok(msg) = meshtastic::ToRadio::decode(&frame[4..]) {
                                if let Some(
                                    meshtastic::to_radio::PayloadVariant::WantConfigId(id),
                                ) = msg.payload_variant
                                {
                                    echo.inject(enc_from(my_info(7)));
                                    echo.inject(enc_from(config_complete(id)));
                                    echo.inject(enc_from(text_packet(42, 555, "hello")));
                                    return;
                                }
                            }
                        }
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            });
            let boxed: BoxedTransport = Box::pin(transport);
            Ok((boxed, TransportKind::Tcp))
        }
        .boxed()
    });

    let session = DeviceSession::new(connector);
    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel::<Command>();
    let (ev_tx, mut ev_rx) = mpsc::channel::<Event>(64);
    let join = tokio::spawn(session.run(cmd_rx, ev_tx));

    cmd_tx
        .send(Command::Connect(ConnectionProfile::Tcp {
            name: "mock".into(),
            host: "h".into(),
            port: 1,
        }))
        .expect("send Connect");

    let connected =
        next_matching(&mut ev_rx, |ev| matches!(ev, Event::Connected(_))).await.expect("connected");
    match connected {
        Event::Connected(snap) => assert_eq!(snap.my_node, NodeId(7)),
        other => panic!("expected Connected, got {other:?}"),
    }

    let message = next_matching(&mut ev_rx, |ev| matches!(ev, Event::MessageReceived(_)))
        .await
        .expect("message received");
    match message {
        Event::MessageReceived(m) => {
            assert_eq!(m.text, "hello");
            assert_eq!(m.from, NodeId(42));
            assert_eq!(m.channel, ChannelIndex::primary());
            assert_eq!(m.direction, Direction::Incoming);
            assert_eq!(m.to, Recipient::Broadcast);
        }
        other => panic!("expected MessageReceived, got {other:?}"),
    }

    drop(cmd_tx);
    let _ = timeout(Duration::from_millis(500), join).await;
}
