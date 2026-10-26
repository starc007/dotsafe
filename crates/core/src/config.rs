//! Per-machine settings shared by the app and CLI: where the vault lives and
//! this device's id. Stored in the OS config directory, never in the vault.

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::license::License;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    /// A folder vault's location. For a vault in S3, the local cache.
    pub vault_path: PathBuf,
    /// Set when the vault lives in an S3-compatible bucket.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub s3: Option<S3Location>,
    /// Identifies this machine in change stamps.
    pub device_id: Uuid,
    /// The license activated on this machine, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<License>,
    /// Approve agents' requests with a click alone, without Touch ID or the
    /// login password. Off unless the user turns it on.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub agent_approvals_without_auth: bool,
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

    /// A config pointing at `vault_path`, keeping this machine's device id and
    /// license from any existing config.
    pub fn for_vault(vault_path: PathBuf) -> Config {
        match Config::load() {
            Ok(Some(existing)) => Config { vault_path, s3: None, ..existing },
            _ => Config {
                vault_path,
                s3: None,
                device_id: Uuid::new_v4(),
                license: None,
                agent_approvals_without_auth: false,
            },
        }
    }

    /// A config for a vault in `s3`, cached locally, keeping this machine's
    /// device id and license.
    pub fn for_s3(s3: S3Location) -> Result<Config> {
        let mut config = Config::for_vault(cache_dir(&s3)?);
        config.s3 = Some(s3);
        Ok(config)
    }

    /// Where the vault lives, for display.
    pub fn describe_location(&self) -> String {
        match &self.s3 {
            Some(s3) => s3.describe(),
            None => self.vault_path.display().to_string(),
        }
    }

    pub fn save(&self) -> Result<()> {
        let path = config_path()?;
        fs::create_dir_all(path.parent().expect("config path has a parent"))?;
        fs::write(&path, serde_json::to_vec_pretty(self)?)?;
        Ok(())
    }
}

/// A bucket on AWS S3, Cloudflare R2, Backblaze B2, MinIO or any other
/// S3-compatible service. Credentials live in the OS keychain, never here.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct S3Location {
    /// For example `https://<account>.r2.cloudflarestorage.com` or
    /// `https://s3.eu-west-1.amazonaws.com`.
    pub endpoint: String,
    /// `auto` for R2.
    pub region: String,
    pub bucket: String,
    /// Folder inside the bucket, without slashes at either end. May be empty.
    #[serde(default)]
    pub prefix: String,
}

impl S3Location {
    pub fn describe(&self) -> String {
        match self.prefix.as_str() {
            "" => format!("s3://{}", self.bucket),
            prefix => format!("s3://{}/{prefix}", self.bucket),
        }
    }

    /// A stable name for this location, for cache folders and keychain entries.
    pub fn id(&self) -> String {
        format!("{}|{}|{}", self.endpoint.trim_end_matches('/'), self.bucket, self.prefix)
    }
}

/// Where the local cache for a vault in `s3` goes.
pub fn cache_dir(s3: &S3Location) -> Result<PathBuf> {
    use sha2::{Digest, Sha256};
    let hash: String = Sha256::digest(s3.id().as_bytes()).iter().take(8).map(|b| format!("{b:02x}")).collect();
    Ok(dirs::data_dir()
        .ok_or_else(|| Error::Format("no data directory on this OS".into()))?
        .join("dotsafe")
        .join("cache")
        .join(hash))
}

/// This machine's dotsafe config directory. `DOTSAFE_CONFIG_DIR` overrides
/// the location, for tests and portable setups.
pub fn config_dir() -> Result<PathBuf> {
    match std::env::var_os("DOTSAFE_CONFIG_DIR") {
        Some(dir) => Ok(PathBuf::from(dir)),
        None => Ok(dirs::config_dir()
            .ok_or_else(|| Error::Format("no config directory on this OS".into()))?
            .join("dotsafe")),
    }
}

fn config_path() -> Result<PathBuf> {
    Ok(config_dir()?.join("config.json"))
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
