use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::domain::channel::Channel;
use crate::domain::config::{
    BluetoothSettings, CannedMessageSettings, DeviceSettings, DisplaySettings,
    ExternalNotificationSettings, LoraSettings, MqttSettings, NeighborInfoSettings,
    NetworkSettings, PositionSettings, PowerSettings, RangeTestSettings, StoreForwardSettings,
    TelemetrySettings,
};
use crate::domain::snapshot::DeviceSnapshot;

pub const EXPORT_VERSION: u32 = 1;

/// Human-editable JSON document capturing every configuration surface the UI
/// can write back to the device.
///
/// Deliberately omits `SecuritySettings` — public/private keys are tied to
/// the device identity and cloning them across radios breaks DM encryption
/// and remote-admin trust.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConfigExport {
    pub version: u32,
    pub owner: Owner,
    pub lora: Option<LoraSettings>,
    pub device: Option<DeviceSettings>,
    pub position: Option<PositionSettings>,
    pub power: Option<PowerSettings>,
    pub network: Option<NetworkSettings>,
    pub display: Option<DisplaySettings>,
    pub bluetooth: Option<BluetoothSettings>,
    pub mqtt: Option<MqttSettings>,
    pub telemetry: Option<TelemetrySettings>,
    pub neighbor_info: Option<NeighborInfoSettings>,
    pub store_forward: Option<StoreForwardSettings>,
    pub ext_notif: Option<ExternalNotificationSettings>,
    pub canned: Option<CannedMessageSettings>,
    pub range_test: Option<RangeTestSettings>,
    pub channels: Vec<Channel>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Owner {
    pub long_name: String,
    pub short_name: String,
}

#[derive(Debug, Error)]
pub enum ImportError {
    #[error("not a valid config export: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("unsupported export version {found}; this build understands v{}", EXPORT_VERSION)]
    Version { found: u32 },
}

pub fn export_snapshot(snapshot: &DeviceSnapshot) -> ConfigExport {
    ConfigExport {
        version: EXPORT_VERSION,
        owner: Owner {
            long_name: snapshot.long_name.clone(),
            short_name: snapshot.short_name.clone(),
        },
        lora: snapshot.lora.clone(),
        device: snapshot.device.clone(),
        position: snapshot.position.clone(),
        power: snapshot.power.clone(),
        network: snapshot.network.clone(),
        display: snapshot.display.clone(),
        bluetooth: snapshot.bluetooth.clone(),
        mqtt: snapshot.mqtt.clone(),
        telemetry: snapshot.telemetry.clone(),
        neighbor_info: snapshot.neighbor_info.clone(),
        store_forward: snapshot.store_forward.clone(),
        ext_notif: snapshot.ext_notif.clone(),
        canned: snapshot.canned.clone(),
        range_test: snapshot.range_test.clone(),
        channels: snapshot.channels.clone(),
    }
}

pub fn encode(export: &ConfigExport) -> String {
    serde_json::to_string_pretty(export).unwrap_or_default()
}

pub fn decode(src: &str) -> Result<ConfigExport, ImportError> {
    let parsed: ConfigExport = serde_json::from_str(src)?;
    if parsed.version != EXPORT_VERSION {
        return Err(ImportError::Version { found: parsed.version });
    }
    Ok(parsed)
}
