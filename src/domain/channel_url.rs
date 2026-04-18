use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use prost::Message;
use thiserror::Error;

use crate::domain::channel::{Channel, ChannelRole};
use crate::domain::ids::ChannelIndex;
use crate::proto::meshtastic;

/// Meshtastic channel-share URL: `https://meshtastic.org/e/#<base64url(ChannelSet)>`.
const URL_PREFIX: &str = "https://meshtastic.org/e/#";
const URL_PREFIX_ALT: &str = "meshtastic.org/e/#";

#[derive(Debug, Error)]
pub enum ChannelUrlError {
    #[error("URL is empty")]
    Empty,
    #[error("URL does not look like a meshtastic channel share link")]
    BadPrefix,
    #[error("fragment is not valid base64url: {0}")]
    Base64(#[from] base64::DecodeError),
    #[error("fragment is not a valid ChannelSet proto: {0}")]
    Proto(#[from] prost::DecodeError),
    #[error("share URL carries no channels")]
    NoChannels,
}

pub fn encode(channels: &[Channel]) -> String {
    let settings: Vec<_> = channels
        .iter()
        .filter(|c| !matches!(c.role, ChannelRole::Disabled))
        .map(settings_from_domain)
        .collect();
    let set = meshtastic::ChannelSet { settings, lora_config: None };
    let mut buf = Vec::with_capacity(set.encoded_len());
    let _ = set.encode(&mut buf);
    format!("{URL_PREFIX}{}", URL_SAFE_NO_PAD.encode(&buf))
}

pub fn decode(url: &str) -> Result<Vec<Channel>, ChannelUrlError> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return Err(ChannelUrlError::Empty);
    }
    let fragment = strip_prefix(trimmed).ok_or(ChannelUrlError::BadPrefix)?;
    let bytes = URL_SAFE_NO_PAD.decode(fragment.as_bytes())?;
    let set = meshtastic::ChannelSet::decode(bytes.as_slice())?;
    if set.settings.is_empty() {
        return Err(ChannelUrlError::NoChannels);
    }
    Ok(channels_from_set(&set))
}

fn strip_prefix(url: &str) -> Option<&str> {
    url.strip_prefix(URL_PREFIX).or_else(|| url.strip_prefix(URL_PREFIX_ALT))
}

fn settings_from_domain(c: &Channel) -> meshtastic::ChannelSettings {
    meshtastic::ChannelSettings {
        psk: c.psk.clone(),
        name: c.name.clone(),
        uplink_enabled: c.uplink_enabled,
        downlink_enabled: c.downlink_enabled,
        module_settings: Some(meshtastic::ModuleSettings {
            position_precision: c.position_precision,
            is_muted: false,
        }),
        ..Default::default()
    }
}

fn channels_from_set(set: &meshtastic::ChannelSet) -> Vec<Channel> {
    set.settings
        .iter()
        .enumerate()
        .filter_map(|(i, s)| {
            let index = ChannelIndex::new(u8::try_from(i).ok()?)?;
            let role = if i == 0 { ChannelRole::Primary } else { ChannelRole::Secondary };
            let position_precision = s
                .module_settings
                .as_ref()
                .map_or(0, |m| m.position_precision);
            Some(Channel {
                index,
                role,
                name: s.name.clone(),
                psk: s.psk.clone(),
                uplink_enabled: s.uplink_enabled,
                downlink_enabled: s.downlink_enabled,
                position_precision,
            })
        })
        .collect()
}
