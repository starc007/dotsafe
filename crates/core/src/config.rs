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
    /// Set when the vault lives on a WebDAV server (Nextcloud, ownCloud, a NAS).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub webdav: Option<WebDavLocation>,
    /// Identifies this machine in change stamps.
    pub device_id: Uuid,
    /// The license activated on this machine, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<License>,
    /// Approve agents' requests with a click alone, without Touch ID or the
    /// login password. Off unless the user turns it on.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub agent_approvals_without_auth: bool,
    /// This machine unlocks with Touch ID: its device key is in the keychain.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub device_unlock: bool,
    /// Unix milliseconds of the last unlock with the password (or recovery
    /// key). Touch ID unlock asks for the password again after a while, so
    /// it isn't forgotten.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_password_unlock: Option<u64>,
    /// Set after removing a device from this machine: everything that device
    /// could read should be rotated. See [`crate::integration::exposed_since`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotation_queue: Option<RotationQueue>,
    /// Where dated backup snapshots go. Not synced: each machine chooses.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backup_dir: Option<PathBuf>,
    /// Unix milliseconds of the last backup snapshot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_backup: Option<u64>,
    /// Minutes of inactivity before the app locks. Not synced: each machine
    /// chooses. Unset means [`DEFAULT_AUTO_LOCK_MINUTES`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_lock_minutes: Option<u32>,
}

/// How long the app waits, idle, before locking, unless changed in Settings.
pub const DEFAULT_AUTO_LOCK_MINUTES: u32 = 15;

/// What to rotate after a device was removed: every value it could have
/// read, which is every value older than the removal.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RotationQueue {
    /// Unix milliseconds of the removal.
    pub since: u64,
    /// The removed device's name, for display.
    pub device: String,
}

impl Config {
    /// How long the app waits, idle, before locking.
    pub fn auto_lock(&self) -> std::time::Duration {
        let minutes = self.auto_lock_minutes.unwrap_or(DEFAULT_AUTO_LOCK_MINUTES);
        std::time::Duration::from_secs(u64::from(minutes) * 60)
    }

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
            // A queue belongs to the vault it was made for.
            Ok(Some(existing)) => Config { vault_path, s3: None, webdav: None, rotation_queue: None, ..existing },
            _ => Config {
                vault_path,
                s3: None,
                webdav: None,
                device_id: Uuid::new_v4(),
                license: None,
                agent_approvals_without_auth: false,
                device_unlock: false,
                last_password_unlock: None,
                rotation_queue: None,
                backup_dir: None,
                last_backup: None,
                auto_lock_minutes: None,
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

    /// A config for a vault on a WebDAV server, cached locally.
    pub fn for_webdav(webdav: WebDavLocation) -> Result<Config> {
        let mut config = Config::for_vault(cache_dir_for(&webdav.id())?);
        config.webdav = Some(webdav);
        Ok(config)
    }

    /// The vault is on a server (S3 or WebDAV) behind a local cache, so it
    /// syncs, rather than a folder a sync client keeps up to date.
    pub fn is_remote(&self) -> bool {
        self.s3.is_some() || self.webdav.is_some()
    }

    /// A stable name for a remote vault's location, for keychain entries.
    pub fn remote_id(&self) -> Option<String> {
        self.s3.as_ref().map(S3Location::id).or_else(|| self.webdav.as_ref().map(WebDavLocation::id))
    }

    /// Where the vault lives, for display.
    pub fn describe_location(&self) -> String {
        match (&self.s3, &self.webdav) {
            (Some(s3), _) => s3.describe(),
            (None, Some(webdav)) => webdav.describe(),
            (None, None) => self.vault_path.display().to_string(),
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

/// A folder on a WebDAV server. The password (an app password, ideally) is
/// kept in the OS keychain, never here.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebDavLocation {
    /// The folder's URL, for example
    /// `https://cloud.example.com/remote.php/dav/files/me/dotsafe/`.
    pub url: String,
    pub username: String,
}

impl WebDavLocation {
    pub fn describe(&self) -> String {
        self.url.trim_end_matches('/').to_string()
    }

    /// A stable name for this location, for cache folders and keychain entries.
    pub fn id(&self) -> String {
        format!("webdav|{}|{}", self.url.trim_end_matches('/'), self.username)
    }
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
    cache_dir_for(&s3.id())
}

/// The local cache folder for a remote location, by its id.
pub fn cache_dir_for(id: &str) -> Result<PathBuf> {
    use sha2::{Digest, Sha256};
    let hash: String = Sha256::digest(id.as_bytes()).iter().take(8).map(|b| format!("{b:02x}")).collect();
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
