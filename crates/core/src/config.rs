//! Per-machine settings shared by the app and CLI: where the vault lives and
//! this device's id. Stored in the OS config directory, never in the vault.

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{Error, Result};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    pub vault_path: PathBuf,
    /// Identifies this machine in change stamps.
    pub device_id: Uuid,
}

impl Config {
    /// The saved config, or `None` if this machine hasn't set up a vault.
    pub fn load() -> Result<Option<Config>> {
        match fs::read(config_path()?) {
            Ok(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// The existing device id, or a new one if this machine has none yet.
    pub fn device_id() -> Uuid {
        match Config::load() {
            Ok(Some(c)) => c.device_id,
            _ => Uuid::new_v4(),
        }
    }

    pub fn save(&self) -> Result<()> {
        let path = config_path()?;
        fs::create_dir_all(path.parent().expect("config path has a parent"))?;
        fs::write(&path, serde_json::to_vec_pretty(self)?)?;
        Ok(())
    }
}

/// `DOTSAFE_CONFIG_DIR` overrides the location, for tests and portable setups.
fn config_path() -> Result<PathBuf> {
    let dir = match std::env::var_os("DOTSAFE_CONFIG_DIR") {
        Some(dir) => PathBuf::from(dir),
        None => dirs::config_dir()
            .ok_or_else(|| Error::Format("no config directory on this OS".into()))?
            .join("dotsafe"),
    };
    Ok(dir.join("config.json"))
}

/// Where a new vault goes by default: iCloud Drive when it's available, so it
/// syncs out of the box, otherwise the local data directory.
pub fn default_vault_path() -> Result<PathBuf> {
    if let Some(home) = dirs::home_dir() {
        let icloud = home.join("Library/Mobile Documents/com~apple~CloudDocs");
        if icloud.is_dir() {
            return Ok(icloud.join("dotsafe"));
        }
    }
    Ok(dirs::data_dir()
        .ok_or_else(|| Error::Format("no data directory on this OS".into()))?
        .join("dotsafe")
        .join("vault"))
}
