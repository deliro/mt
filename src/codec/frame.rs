use bytes::BytesMut;
use tokio_util::codec::{Decoder, Encoder};

use crate::codec::error::FrameError;

pub use crate::codec::error::MAX_FRAME_PAYLOAD;

pub const MAGIC: [u8; 2] = [0x94, 0xC3];
const HEADER_LEN: usize = 4;

pub fn encode(payload: &[u8]) -> Result<Vec<u8>, FrameError> {
    if payload.len() > MAX_FRAME_PAYLOAD {
        return Err(FrameError::TooLarge(payload.len()));
    }
    let mut out = Vec::with_capacity(HEADER_LEN + payload.len());
    out.extend_from_slice(&MAGIC);
    out.extend_from_slice(&(payload.len() as u16).to_be_bytes());
    out.extend_from_slice(payload);
    Ok(out)
}

pub fn decode(bytes: &[u8]) -> Result<(Vec<u8>, usize), FrameError> {
    if bytes.is_empty() {
        return Err(FrameError::NeedMore(HEADER_LEN));
    }
    if bytes[0] != MAGIC[0] {
        return Err(FrameError::BadMagic(bytes[0], bytes.get(1).copied().unwrap_or(0)));
    }
    if bytes.len() < 2 {
        return Err(FrameError::NeedMore(HEADER_LEN - bytes.len()));
    }
    if bytes[1] != MAGIC[1] {
        return Err(FrameError::BadMagic(bytes[0], bytes[1]));
    }
    if bytes.len() < HEADER_LEN {
        return Err(FrameError::NeedMore(HEADER_LEN - bytes.len()));
    }
    let len = u16::from_be_bytes([bytes[2], bytes[3]]) as usize;
    if len > MAX_FRAME_PAYLOAD {
        return Err(FrameError::TooLarge(len));
    }
    let total = HEADER_LEN + len;
    if bytes.len() < total {
        return Err(FrameError::NeedMore(total - bytes.len()));
    }
    Ok((bytes[HEADER_LEN..total].to_vec(), total))
}

#[derive(Default)]
pub struct FrameCodec;

impl Decoder for FrameCodec {
    type Item = Vec<u8>;
    type Error = FrameError;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        while !src.is_empty() && src[0] != MAGIC[0] {
            let _ = src.split_to(1);
        }
        if src.len() < 2 {
            return Ok(None);
        }
        if src[1] != MAGIC[1] {
            let _ = src.split_to(1);
            return self.decode(src);
        }
        if src.len() < HEADER_LEN {
            return Ok(None);
        }
        let len = u16::from_be_bytes([src[2], src[3]]) as usize;
        if len > MAX_FRAME_PAYLOAD {
            return Err(FrameError::TooLarge(len));
        }
        let total = HEADER_LEN + len;
        if src.len() < total {
            return Ok(None);
        }
        let _ = src.split_to(HEADER_LEN);
        let payload = src.split_to(len).to_vec();
        Ok(Some(payload))
    }
}

impl Encoder<Vec<u8>> for FrameCodec {
    type Error = FrameError;

    fn encode(&mut self, item: Vec<u8>, dst: &mut BytesMut) -> Result<(), Self::Error> {
        if item.len() > MAX_FRAME_PAYLOAD {
            return Err(FrameError::TooLarge(item.len()));
        }
        dst.reserve(HEADER_LEN + item.len());
        dst.extend_from_slice(&MAGIC);
        dst.extend_from_slice(&(item.len() as u16).to_be_bytes());
        dst.extend_from_slice(&item);
        Ok(())
    }
}
