use mt::codec::error::{FrameError, MAX_FRAME_PAYLOAD};
use mt::codec::frame::{decode, encode};

#[test]
fn encodes_empty_payload_as_header_only() {
    let out = encode(&[]).expect("empty payload is valid");
    assert_eq!(out, vec![0x94, 0xC3, 0x00, 0x00]);
}

#[test]
fn encodes_payload_with_big_endian_length() {
    let payload = vec![1u8, 2, 3, 4, 5];
    let out = encode(&payload).expect("encode");
    assert_eq!(out[..4], [0x94, 0xC3, 0x00, 0x05]);
    assert_eq!(&out[4..], &payload[..]);
}

#[test]
fn rejects_oversized_payload() {
    let big = vec![0u8; MAX_FRAME_PAYLOAD + 1];
    let err = encode(&big).expect_err("should reject");
    assert_eq!(err, FrameError::TooLarge(big.len()));
}

#[test]
fn decodes_round_trip() {
    let payload: Vec<u8> = (0u8..200).collect();
    let framed = encode(&payload).expect("encode");
    let (out, consumed) = decode(&framed).expect("decode");
    assert_eq!(out, payload);
    assert_eq!(consumed, framed.len());
}

#[test]
fn decode_rejects_bad_magic() {
    let err = decode(&[0x00, 0x00, 0x00, 0x00]).expect_err("bad magic");
    assert_eq!(err, FrameError::BadMagic(0x00, 0x00));
}

#[test]
fn decode_needs_more_when_short() {
    assert_eq!(decode(&[]).expect_err("empty"), FrameError::NeedMore(4));
    assert_eq!(decode(&[0x94]).expect_err("one"), FrameError::NeedMore(3));
    assert_eq!(decode(&[0x94, 0xC3]).expect_err("two"), FrameError::NeedMore(2));
    assert_eq!(
        decode(&[0x94, 0xC3, 0x00, 0x05, 1, 2]).expect_err("partial"),
        FrameError::NeedMore(3),
    );
}
