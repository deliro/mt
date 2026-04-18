use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::domain::profile::ConnectionProfile;
use crate::error::PersistError;

#[derive(Serialize, Deserialize, Default)]
struct File {
    #[serde(default)]
    profile: Vec<ConnectionProfile>,
    #[serde(default)]
    last_active: Option<String>,
}

#[derive(Default)]
pub struct Stored {
    pub profiles: Vec<ConnectionProfile>,
    pub last_active: Option<String>,
}

pub fn load_from(path: &Path) -> Result<Stored, PersistError> {
    match std::fs::read_to_string(path) {
        Ok(text) => {
            let file: File = toml::from_str(&text)?;
            Ok(Stored { profiles: file.profile, last_active: file.last_active })
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Stored::default()),
        Err(e) => Err(PersistError::Io(e)),
    }
}

pub fn save_to(
    path: &Path,
    profiles: &[ConnectionProfile],
    last_active: Option<&str>,
) -> Result<(), PersistError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let file = File {
        profile: profiles.to_vec(),
        last_active: last_active.map(ToOwned::to_owned),
    };
    let text = toml::to_string_pretty(&file)?;
    std::fs::write(path, text)?;
    Ok(())
}

pub fn default_path() -> PathBuf {
    directories::ProjectDirs::from("dev", "", "mt")
        .map_or_else(|| PathBuf::from("profiles.toml"), |d| d.config_dir().join("profiles.toml"))
}
