use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::domain::ids::BleAddress;

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TransportKind {
    Ble,
    Serial,
    Tcp,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum ConnectionProfile {
    Ble { name: String, address: BleAddress },
    Serial { name: String, path: PathBuf },
    Tcp { name: String, host: String, port: u16 },
}

impl ConnectionProfile {
    pub fn kind(&self) -> TransportKind {
        match self {
            Self::Ble { .. } => TransportKind::Ble,
            Self::Serial { .. } => TransportKind::Serial,
            Self::Tcp { .. } => TransportKind::Tcp,
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Self::Ble { name, .. } | Self::Serial { name, .. } | Self::Tcp { name, .. } => name,
        }
    }

    /// Stable identifier derived from the transport's addressable field, used
    /// to persist "last active" profile across restarts. The display `name`
    /// is user-editable and not a reliable key.
    pub fn key(&self) -> String {
        match self {
            Self::Ble { address, .. } => format!("ble:{}", address.as_str()),
            Self::Serial { path, .. } => format!("serial:{}", path.display()),
            Self::Tcp { host, port, .. } => format!("tcp:{host}:{port}"),
        }
    }
}
