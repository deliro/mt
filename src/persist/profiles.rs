use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::domain::profile::ConnectionProfile;
use crate::error::PersistError;

#[derive(Serialize, Deserialize, Default)]
struct File {
    #[serde(default)]
    profile: Vec<ConnectionProfile>,
}

pub fn load_from(path: &Path) -> Result<Vec<ConnectionProfile>, PersistError> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(toml::from_str::<File>(&text)?.profile),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(PersistError::Io(e)),
    }
}

pub fn save_to(path: &Path, profiles: &[ConnectionProfile]) -> Result<(), PersistError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let file = File { profile: profiles.to_vec() };
    let text = toml::to_string_pretty(&file)?;
    std::fs::write(path, text)?;
    Ok(())
}

pub fn default_path() -> PathBuf {
    directories::ProjectDirs::from("dev", "", "mt")
        .map_or_else(|| PathBuf::from("profiles.toml"), |d| d.config_dir().join("profiles.toml"))
}
