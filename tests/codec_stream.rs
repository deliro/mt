use bytes::BytesMut;
use mt::codec::frame::FrameCodec;
use tokio_util::codec::{Decoder, Encoder};

#[test]
fn decodes_one_frame_across_partial_reads() {
    let mut codec = FrameCodec;
    let mut buf = BytesMut::new();

    buf.extend_from_slice(&[0x94, 0xC3]);
    assert!(codec.decode(&mut buf).expect("decode").is_none());

    buf.extend_from_slice(&[0x00, 0x03, b'h']);
    assert!(codec.decode(&mut buf).expect("decode").is_none());

    buf.extend_from_slice(&[b'i', b'!']);
    let got = codec.decode(&mut buf).expect("decode").expect("frame ready");
    assert_eq!(got.as_slice(), b"hi!");
    assert!(buf.is_empty());
}

#[test]
fn decodes_two_frames_back_to_back() {
    let mut codec = FrameCodec;
    let mut buf = BytesMut::new();
    buf.extend_from_slice(&[
        0x94, 0xC3, 0x00, 0x01, b'a', 0x94, 0xC3, 0x00, 0x02, b'b', b'c',
    ]);
    let first = codec.decode(&mut buf).expect("decode").expect("first");
    assert_eq!(first.as_slice(), b"a");
    let second = codec.decode(&mut buf).expect("decode").expect("second");
    assert_eq!(second.as_slice(), b"bc");
    assert!(buf.is_empty());
}

#[test]
fn decoder_skips_garbage_before_magic() {
    let mut codec = FrameCodec;
    let mut buf = BytesMut::new();
    buf.extend_from_slice(&[0xFF, 0xAA, 0x94, 0xC3, 0x00, 0x01, b'x']);
    let f = codec.decode(&mut buf).expect("decode").expect("skipped");
    assert_eq!(f.as_slice(), b"x");
    assert!(buf.is_empty());
}

#[test]
fn encodes_through_framed_api() {
    let mut codec = FrameCodec;
    let mut out = BytesMut::new();
    codec.encode(b"ping".to_vec(), &mut out).expect("encode");
    assert_eq!(out.as_ref(), &[0x94, 0xC3, 0x00, 0x04, b'p', b'i', b'n', b'g']);
}
