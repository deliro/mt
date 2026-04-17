#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic, clippy::pedantic, clippy::nursery, clippy::cargo, clippy::indexing_slicing, clippy::integer_division, clippy::collapsible_if, clippy::byte_char_slices, clippy::redundant_pattern_matching)]

use mt::proto::meshtastic::PortNum;
use mt::proto::port::{PortPayload, parse};

#[test]
fn unknown_port_preserves_bytes() {
    let payload = parse(9999, b"\x01\x02\x03").expect("parse");
    match payload {
        PortPayload::Unknown { port, bytes } => {
            assert_eq!(port, 9999);
            assert_eq!(bytes.as_ref(), b"\x01\x02\x03");
        }
        other => panic!("expected Unknown, got {other:?}"),
    }
}

#[test]
fn text_port_parses_utf8() {
    let p = parse(PortNum::TextMessageApp as i32, "hi".as_bytes()).expect("parse");
    match p {
        PortPayload::Text(t) => assert_eq!(t, "hi"),
        other => panic!("expected Text, got {other:?}"),
    }
}

#[test]
fn text_port_rejects_invalid_utf8() {
    let err = parse(PortNum::TextMessageApp as i32, &[0xFF, 0xFE]).expect_err("utf-8");
    assert!(format!("{err}").contains("utf-8"));
}
