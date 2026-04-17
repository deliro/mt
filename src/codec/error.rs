use thiserror::Error;

pub const MAX_FRAME_PAYLOAD: usize = 512;

#[derive(Debug, Error)]
pub enum FrameError {
    #[error("bad magic: {0:#04x} {1:#04x}")]
    BadMagic(u8, u8),
    #[error("frame payload too large: {0} bytes (max {MAX_FRAME_PAYLOAD})")]
    TooLarge(usize),
    #[error("need {0} more bytes")]
    NeedMore(usize),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

impl PartialEq for FrameError {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::BadMagic(a1, b1), Self::BadMagic(a2, b2)) => a1 == a2 && b1 == b2,
            (Self::TooLarge(a), Self::TooLarge(b)) | (Self::NeedMore(a), Self::NeedMore(b)) => {
                a == b
            }
            (Self::Io(a), Self::Io(b)) => a.kind() == b.kind(),
            (Self::BadMagic(..) | Self::TooLarge(_) | Self::NeedMore(_) | Self::Io(_), _) => false,
        }
    }
}

impl Eq for FrameError {}
