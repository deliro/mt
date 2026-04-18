use thiserror::Error;

pub use crate::transport::error::TransportError;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("connect: {0}")]
    Connect(#[from] ConnectError),
    #[error("transport: {0}")]
    Transport(#[from] TransportError),
    #[error("persist: {0}")]
    Persist(#[from] PersistError),
}

#[derive(Debug, Error)]
pub enum ConnectError {
    #[error("BLE adapter unavailable")]
    BleAdapterUnavailable,
    #[error("BLE device not found: {0}")]
    BleDeviceNotFound(String),
    #[error("BLE pairing required ({0:?})")]
    BlePairingRequired(PairingHint),
    #[error("BLE pairing failed: {0}")]
    BlePairingFailed(String),
    #[error("BLE GATT: {0}")]
    BleGatt(String),
    #[error("serial: {0}")]
    Serial(#[from] tokio_serial::Error),
    #[error("tcp: {0}")]
    Tcp(std::io::Error),
    #[error("handshake timeout")]
    HandshakeTimeout,
    #[error("codec: {0}")]
    Codec(#[from] crate::codec::error::FrameError),
    #[error("transport: {0}")]
    Transport(#[from] TransportError),
    #[error("encode: {0}")]
    Encode(#[from] prost::EncodeError),
    #[error("decode: {0}")]
    Decode(#[from] prost::DecodeError),
}

#[derive(Debug, Copy, Clone)]
pub enum PairingHint {
    Macos,
    Windows,
    LinuxBluetoothctl,
}

#[derive(Debug, Error)]
pub enum PersistError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("parse: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("serialize: {0}")]
    Serialize(#[from] toml::ser::Error),
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("state decode: {0}")]
    StateDecode(String),
}
