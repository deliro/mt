#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use mt::domain::channel::{Channel, ChannelRole};
use mt::domain::channel_url;
use mt::domain::ids::ChannelIndex;

fn ch(index: u8, role: ChannelRole, name: &str, psk: Vec<u8>) -> Channel {
    let index = ChannelIndex::new(index).unwrap_or_else(ChannelIndex::primary);
    Channel {
        index,
        role,
        name: name.into(),
        psk,
        uplink_enabled: false,
        downlink_enabled: false,
        position_precision: 0,
    }
}

#[test]
fn encode_decode_roundtrip_preserves_name_and_psk() {
    let channels = vec![
        ch(0, ChannelRole::Primary, "LongFast", vec![1]),
        ch(1, ChannelRole::Secondary, "Scouts", vec![0xAA; 32]),
    ];
    let url = channel_url::encode(&channels);
    assert!(url.starts_with("https://meshtastic.org/e/#"));
    let decoded = channel_url::decode(&url).expect("decode should succeed");
    assert_eq!(decoded.len(), 2);
    let first = decoded.first().expect("primary exists");
    assert_eq!(first.name, "LongFast");
    assert_eq!(first.psk, vec![1]);
    assert_eq!(first.role, ChannelRole::Primary);
    let second = decoded.get(1).expect("secondary exists");
    assert_eq!(second.name, "Scouts");
    assert_eq!(second.psk, vec![0xAA; 32]);
    assert_eq!(second.role, ChannelRole::Secondary);
}

#[test]
fn encode_skips_disabled_channels() {
    let channels = vec![
        ch(0, ChannelRole::Primary, "P", vec![1]),
        ch(1, ChannelRole::Disabled, "X", vec![]),
        ch(2, ChannelRole::Secondary, "S", vec![2]),
    ];
    let url = channel_url::encode(&channels);
    let decoded = channel_url::decode(&url).expect("decode");
    assert_eq!(decoded.len(), 2);
    assert_eq!(decoded.first().map(|c| c.name.as_str()), Some("P"));
    assert_eq!(decoded.get(1).map(|c| c.name.as_str()), Some("S"));
}

#[test]
fn decode_rejects_non_meshtastic_url() {
    let err = channel_url::decode("https://example.com/x").unwrap_err();
    assert!(format!("{err}").contains("does not look like"));
}

#[test]
fn decode_accepts_url_without_scheme() {
    let full = channel_url::encode(&[ch(0, ChannelRole::Primary, "p", vec![1])]);
    let bare = full.strip_prefix("https://").expect("prefix");
    let decoded = channel_url::decode(bare).expect("bare url parses");
    assert_eq!(decoded.first().map(|c| c.name.as_str()), Some("p"));
}
