#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::Duration;

use mt::domain::ids::{ConfigId, NodeId};
use mt::domain::profile::TransportKind;
use mt::proto::meshtastic;
use mt::session::handshake::run_handshake;
use mt::transport::BoxedTransport;
use mt::transport::mock::{MockTransport, Script};
use prost::Message;

fn frame_from_radio(m: &meshtastic::FromRadio) -> Vec<u8> {
    let mut buf = Vec::with_capacity(m.encoded_len());
    m.encode(&mut buf).expect("encode FromRadio");
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

fn node_info(num: u32, long: &str, short: &str) -> meshtastic::FromRadio {
    meshtastic::FromRadio {
        id: 2,
        payload_variant: Some(meshtastic::from_radio::PayloadVariant::NodeInfo(
            meshtastic::NodeInfo {
                num,
                user: Some(meshtastic::User {
                    id: format!("!{num:08x}"),
                    long_name: long.into(),
                    short_name: short.into(),
                    ..Default::default()
                }),
                ..Default::default()
            },
        )),
    }
}

fn config_complete(id: u32) -> meshtastic::FromRadio {
    meshtastic::FromRadio {
        id: 3,
        payload_variant: Some(meshtastic::from_radio::PayloadVariant::ConfigCompleteId(id)),
    }
}

#[tokio::test(flavor = "current_thread")]
async fn completes_when_config_complete_matches_and_populates_my_names() {
    let cfg = ConfigId(12345);
    let (transport, _handle) = MockTransport::new(Script::from_frames(vec![
        frame_from_radio(&my_info(77)),
        frame_from_radio(&node_info(77, "My Node", "MN")),
        frame_from_radio(&node_info(123, "Other", "OT")),
        frame_from_radio(&config_complete(12345)),
    ]));
    let boxed: BoxedTransport = Box::pin(transport);

    let (snapshot, _transport) = tokio::time::timeout(
        Duration::from_millis(500),
        run_handshake(boxed, TransportKind::Tcp, cfg),
    )
    .await
    .expect("no timeout")
    .expect("handshake ok");

    assert_eq!(snapshot.my_node, NodeId(77));
    assert_eq!(snapshot.long_name, "My Node");
    assert_eq!(snapshot.short_name, "MN");
    assert_eq!(snapshot.nodes.len(), 2);
}

#[tokio::test(flavor = "current_thread")]
async fn reports_timeout_when_config_complete_mismatches() {
    let (transport, handle) = MockTransport::new(Script::from_frames(vec![
        frame_from_radio(&my_info(1)),
        frame_from_radio(&config_complete(9999)),
    ]));
    let boxed: BoxedTransport = Box::pin(transport);
    handle.close();

    match run_handshake(boxed, TransportKind::Tcp, ConfigId(42)).await {
        Ok(_) => panic!("should not complete with mismatching config id"),
        Err(err) => assert!(format!("{err}").contains("handshake")),
    }
}
