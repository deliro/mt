use thiserror::Error;

#[derive(Debug, Error)]
pub enum TransportError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("serial: {0}")]
    Serial(#[from] tokio_serial::Error),
    #[error("ble: {0}")]
    Ble(String),
    #[error("frame: {0}")]
    Frame(#[from] crate::codec::error::FrameError),
    #[error("closed")]
    Closed,
}
