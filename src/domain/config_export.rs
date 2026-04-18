use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::domain::channel::Channel;
use crate::domain::config::{
    BluetoothSettings, CannedMessageSettings, ConsoleAccess, DeviceSettings, DisplaySettings,
    ExternalNotificationSettings, LoraSettings, MqttSettings, NeighborInfoSettings,
    NetworkSettings, PositionSettings, PowerSettings, RangeTestSettings, SecuritySettings,
    StoreForwardSettings, TelemetrySettings,
};
use crate::domain::snapshot::DeviceSnapshot;

pub const EXPORT_VERSION: u32 = 2;

/// Human-editable JSON document capturing every configuration surface the UI
/// can write back to the device.
///
/// Deliberately omits the cryptographic keypair inside `SecuritySettings`
/// (`public_key` + `private_key`) — they are tied to the device identity and
/// cloning them across radios breaks DM encryption. The remaining security
/// policy (`admin_keys`, `is_managed`, `admin_channel_enabled`, console flags)
/// IS exported so an admin can clone "who may manage me" across a fleet.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConfigExport {
    pub version: u32,
    pub owner: Owner,
    pub lora: Option<LoraSettings>,
    pub device: Option<DeviceSettings>,
    pub position: Option<PositionSettings>,
    pub fixed_position: Option<FixedPosition>,
    pub power: Option<PowerSettings>,
    pub network: Option<NetworkSettings>,
    pub display: Option<DisplaySettings>,
    pub bluetooth: Option<BluetoothSettings>,
    pub security_policy: Option<SecurityPolicy>,
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

/// Fixed GPS position of our own node. Exported only when
/// `position.fixed_position` is on — otherwise the coords are tracked from a
/// live GPS and replaying them would make no sense.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct FixedPosition {
    pub latitude_deg: f64,
    pub longitude_deg: f64,
    pub altitude_m: i32,
}

/// Security policy without the keypair: who may administer this node and
/// what kinds of local access are permitted.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SecurityPolicy {
    pub admin_keys: Vec<Vec<u8>>,
    pub is_managed: bool,
    pub admin_channel_enabled: bool,
    pub console: ConsoleAccess,
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
        fixed_position: fixed_position_from_snapshot(snapshot),
        power: snapshot.power.clone(),
        network: snapshot.network.clone(),
        display: snapshot.display.clone(),
        bluetooth: snapshot.bluetooth.clone(),
        security_policy: snapshot.security.as_ref().map(security_policy_from),
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

fn fixed_position_from_snapshot(snapshot: &DeviceSnapshot) -> Option<FixedPosition> {
    let enabled = snapshot.position.as_ref().is_some_and(|p| p.fixed_position);
    if !enabled {
        return None;
    }
    let me = snapshot.nodes.get(&snapshot.my_node)?;
    let pos = me.position.as_ref()?;
    Some(FixedPosition {
        latitude_deg: pos.latitude_deg,
        longitude_deg: pos.longitude_deg,
        altitude_m: pos.altitude_m.unwrap_or(0),
    })
}

fn security_policy_from(s: &SecuritySettings) -> SecurityPolicy {
    SecurityPolicy {
        admin_keys: s.admin_keys.clone(),
        is_managed: s.is_managed,
        admin_channel_enabled: s.admin_channel_enabled,
        console: s.console.clone(),
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
