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
    let mut out = Vec::with_capacity(HEADER_LEN.saturating_add(payload.len()));
    out.extend_from_slice(&MAGIC);
    let len_u16 = u16::try_from(payload.len()).unwrap_or(u16::MAX);
    out.extend_from_slice(&len_u16.to_be_bytes());
    out.extend_from_slice(payload);
    Ok(out)
}

pub fn decode(bytes: &[u8]) -> Result<(Vec<u8>, usize), FrameError> {
    let Some(header) = bytes.get(..HEADER_LEN) else {
        if let (Some(&b0), Some(&b1)) = (bytes.first(), bytes.get(1))
            && (b0 != MAGIC[0] || b1 != MAGIC[1])
        {
            return Err(FrameError::BadMagic(b0, b1));
        }
        return Err(FrameError::NeedMore(HEADER_LEN.saturating_sub(bytes.len())));
    };
    let b0 = header.first().copied().unwrap_or(0);
    let b1 = header.get(1).copied().unwrap_or(0);
    if b0 != MAGIC[0] || b1 != MAGIC[1] {
        return Err(FrameError::BadMagic(b0, b1));
    }
    let len_hi = header.get(2).copied().unwrap_or(0);
    let len_lo = header.get(3).copied().unwrap_or(0);
    let len = u16::from_be_bytes([len_hi, len_lo]) as usize;
    if len > MAX_FRAME_PAYLOAD {
        return Err(FrameError::TooLarge(len));
    }
    let total = HEADER_LEN.saturating_add(len);
    let Some(body) = bytes.get(HEADER_LEN..total) else {
        return Err(FrameError::NeedMore(total.saturating_sub(bytes.len())));
    };
    Ok((body.to_vec(), total))
}

#[derive(Default)]
pub struct FrameCodec;

impl Decoder for FrameCodec {
    type Item = Vec<u8>;
    type Error = FrameError;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        while src.first().is_some_and(|&b| b != MAGIC[0]) {
            let _ = src.split_to(1);
        }
        if src.len() < 2 {
            return Ok(None);
        }
        if src.get(1).copied() != Some(MAGIC[1]) {
            let _ = src.split_to(1);
            return self.decode(src);
        }
        if src.len() < HEADER_LEN {
            return Ok(None);
        }
        let len_hi = src.get(2).copied().unwrap_or(0);
        let len_lo = src.get(3).copied().unwrap_or(0);
        let len = u16::from_be_bytes([len_hi, len_lo]) as usize;
        if len > MAX_FRAME_PAYLOAD {
            return Err(FrameError::TooLarge(len));
        }
        let total = HEADER_LEN.saturating_add(len);
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
        dst.reserve(HEADER_LEN.saturating_add(item.len()));
        dst.extend_from_slice(&MAGIC);
        let len_u16 = u16::try_from(item.len()).unwrap_or(u16::MAX);
        dst.extend_from_slice(&len_u16.to_be_bytes());
        dst.extend_from_slice(&item);
        Ok(())
    }
}
