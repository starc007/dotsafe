//! Per-machine settings: where the vault lives and this device's id.

use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    pub vault_path: PathBuf,
    /// Identifies this machine in change stamps.
    pub device_id: Uuid,
}

impl Config {
    pub fn load() -> Result<Config> {
        let path = config_path()?;
        let bytes = fs::read(&path).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => anyhow!("no vault set up yet; run `dotsafe init` first"),
            _ => anyhow!(e).context(format!("reading {}", path.display())),
        })?;
        serde_json::from_slice(&bytes).with_context(|| format!("parsing {}", path.display()))
    }

    /// The existing device id, or a new one if this machine has none yet.
    pub fn device_id() -> Uuid {
        Config::load().map(|c| c.device_id).unwrap_or_else(|_| Uuid::new_v4())
    }

    pub fn save(&self) -> Result<()> {
        let path = config_path()?;
        fs::create_dir_all(path.parent().expect("config path has a parent"))?;
        fs::write(&path, serde_json::to_vec_pretty(self)?)
            .with_context(|| format!("writing {}", path.display()))
    }
}

fn config_path() -> Result<PathBuf> {
    let dir = match std::env::var_os("DOTSAFE_CONFIG_DIR") {
        Some(dir) => PathBuf::from(dir),
        None => dirs::config_dir().context("no config directory on this OS")?.join("dotsafe"),
    };
    Ok(dir.join("config.json"))
}

/// Where `dotsafe init` puts a new vault: iCloud Drive when it's available,
/// so it syncs out of the box, otherwise the local data directory.
pub fn default_vault_path() -> Result<PathBuf> {
    if let Some(home) = dirs::home_dir() {
        let icloud = home.join("Library/Mobile Documents/com~apple~CloudDocs");
        if icloud.is_dir() {
            return Ok(icloud.join("dotsafe"));
        }
    }
    Ok(dirs::data_dir().context("no data directory on this OS")?.join("dotsafe").join("vault"))
}
