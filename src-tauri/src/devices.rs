//! Devices: Touch ID unlock, adding a device with a one-time code, and
//! removing a device (which re-keys the vault).
//!
//! This Mac's device key lives in the login keychain, and dotsafe asks for
//! Touch ID (LocalAuthentication, in Rust) before reading it. Binding the key
//! to Touch ID in hardware needs the signed app; until then the gate is
//! dotsafe's own check. macOS only: elsewhere there's no presence check, so
//! a device key would mean unlocking with nothing at all.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use dotsafe_core::{
    CachedStore, Config, DeviceKey, EnrollmentCode, Error, FolderStore, KdfParams, Store, Vault, config,
};
use dotsafe_stores::S3Store;
use serde::Serialize;
use tauri::State;
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::commands::BucketInput;
use crate::state::{AppState, CmdError, CmdResult, ErrorKind, Session};

const KEYCHAIN_SERVICE: &str = "app.dotsafe.device";
/// Touch ID unlock asks for the password again after this long.
const PASSWORD_EVERY_MS: u64 = 14 * 24 * 60 * 60 * 1000;

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

fn device_name() -> String {
    let name = whoami::devicename().unwrap_or_default();
    if name.trim().is_empty() { "This Mac".into() } else { name.trim().chars().take(60).collect() }
}

fn entry(device_id: Uuid) -> CmdResult<keyring::Entry> {
    keyring::Entry::new(KEYCHAIN_SERVICE, &device_id.to_string())
        .map_err(|e| CmdError::other(format!("Couldn't open the keychain: {e}")))
}

fn save_key(device_id: Uuid, key: &DeviceKey) -> CmdResult<()> {
    entry(device_id)?
        .set_secret(key.as_bytes())
        .map_err(|e| CmdError::other(format!("Couldn't save to the keychain: {e}")))
}

fn load_key(device_id: Uuid) -> CmdResult<DeviceKey> {
    let bytes = Zeroizing::new(
        entry(device_id)?
            .get_secret()
            .map_err(|_| CmdError::other("This Mac's Touch ID key is missing. Use your password."))?,
    );
    let bytes: [u8; 32] =
        bytes.as_slice().try_into().map_err(|_| CmdError::other("This Mac's Touch ID key is damaged."))?;
    Ok(DeviceKey::from_bytes(bytes))
}

fn delete_key(device_id: Uuid) {
    if let Ok(entry) = entry(device_id) {
        let _ = entry.delete_credential();
    }
}

/// Whether Touch ID unlock should be offered right now.
pub fn touch_id_offered(config: &Config) -> bool {
    config.device_unlock && crate::presence::available() && !password_due(config)
}

pub fn password_due(config: &Config) -> bool {
    config.last_password_unlock.is_none_or(|at| now_ms().saturating_sub(at) >= PASSWORD_EVERY_MS)
}

/// After an unlock with the password or recovery key: note when, and if
/// this Mac uses Touch ID but its slot went stale (the vault was re-keyed) or
/// is missing, enroll it again. Failing to refresh isn't fatal.
pub fn after_password_unlock(s: &mut Session) -> CmdResult<()> {
    let mut config = s.config()?.clone();
    config.last_password_unlock = Some(now_ms());
    if config.device_unlock {
        let device_id = config.device_id;
        let current = s.vault()?.devices().iter().any(|d| d.device_id == device_id && !d.stale);
        if !current && let Ok(key) = s.vault_mut()?.enroll_device(device_id, &device_name()) {
            let _ = save_key(device_id, &key);
        }
    }
    config.save()?;
    s.config = Some(config);
    Ok(())
}

async fn confirm(reason: &'static str) -> CmdResult<()> {
    tauri::async_runtime::spawn_blocking(move || crate::presence::confirm(reason))
        .await
        .map_err(|e| CmdError::other(e.to_string()))?
        .map_err(|why| CmdError::other(format!("Not confirmed: {why}")))
}

#[tauri::command]
pub async fn unlock_with_touch_id(state: State<'_, AppState>) -> CmdResult<()> {
    let config = state.run(|s| Ok(s.config()?.clone())).await?;
    if !touch_id_offered(&config) {
        return Err(CmdError::other("Use your password to unlock."));
    }
    confirm("unlock your vault").await?;
    let vault = tauri::async_runtime::spawn_blocking(move || -> CmdResult<Vault> {
        let key = load_key(config.device_id)?;
        let store = dotsafe_stores::open(&config)?;
        Vault::unlock_with_device(store, config.device_id, &key).map_err(|e| match e {
            Error::DeviceStale => CmdError {
                kind: ErrorKind::InvalidPassword,
                message:
                    "A device was removed, so the vault's key changed. Enter your password once to use Touch ID again."
                        .into(),
            },
            e => e.into(),
        })
    })
    .await
    .map_err(|e| CmdError::other(e.to_string()))??;
    state
        .run(move |s| {
            s.vault = Some(Arc::new(vault));
            s.sync = Default::default();
            s.must_change_password = false;
            Ok(())
        })
        .await
}

/// Turns Touch ID unlock on or off for this Mac. Turning it on asks for
/// Touch ID once, so it's known to work.
#[tauri::command]
pub async fn set_touch_id_unlock(state: State<'_, AppState>, enabled: bool) -> CmdResult<()> {
    if enabled {
        if !crate::presence::available() {
            return Err(CmdError::other("Touch ID unlock needs a Mac with Touch ID or a login password."));
        }
        confirm("turn on Touch ID unlock").await?;
    }
    state
        .run(move |s| {
            let mut config = s.config()?.clone();
            let device_id = config.device_id;
            if enabled {
                let key = s.vault_mut()?.enroll_device(device_id, &device_name())?;
                save_key(device_id, &key)?;
                config.last_password_unlock.get_or_insert(now_ms());
            } else {
                s.vault_mut()?.forget_device(device_id)?;
                delete_key(device_id);
            }
            config.device_unlock = enabled;
            config.save()?;
            s.config = Some(config);
            Ok(())
        })
        .await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceView {
    device_id: Uuid,
    name: String,
    added: u64,
    /// Needs the password once, after a re-key.
    stale: bool,
    this_device: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DevicesView {
    devices: Vec<DeviceView>,
    touch_id_unlock: bool,
    touch_id_available: bool,
}

#[tauri::command]
pub async fn list_devices(state: State<'_, AppState>) -> CmdResult<DevicesView> {
    state
        .run(|s| {
            let config = s.config()?;
            let mut devices: Vec<DeviceView> = s
                .vault()?
                .devices()
                .into_iter()
                .map(|d| DeviceView {
                    this_device: d.device_id == config.device_id,
                    device_id: d.device_id,
                    name: d.name,
                    added: d.added,
                    stale: d.stale,
                })
                .collect();
            devices.sort_by_key(|d| (!d.this_device, d.added));
            Ok(DevicesView {
                devices,
                touch_id_unlock: config.device_unlock,
                touch_id_available: crate::presence::available(),
            })
        })
        .await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnrollmentView {
    code: String,
    expires_at: u64,
}

#[tauri::command]
pub async fn create_enrollment_code(state: State<'_, AppState>) -> CmdResult<EnrollmentView> {
    state
        .run(|s| {
            let code = s.vault_mut()?.create_enrollment_code()?;
            Ok(EnrollmentView {
                code: code.to_display_string(),
                expires_at: now_ms() + dotsafe_core::header::ENROLLMENT_TTL_MS,
            })
        })
        .await
}

/// Removes another device and re-keys the vault. Returns the new recovery
/// key, to show once.
#[tauri::command]
pub async fn revoke_device(state: State<'_, AppState>, device_id: Uuid, password: String) -> CmdResult<String> {
    let password = Zeroizing::new(password);
    state
        .run(move |s| {
            let own = s.config()?.device_id;
            if device_id == own {
                return Err(CmdError::other("To stop this Mac using Touch ID, turn off Touch ID unlock."));
            }
            let recovery =
                s.vault_mut()?.revoke_device(device_id, &password, KdfParams::default()).map_err(|e| match e {
                    Error::InvalidCredentials => {
                        CmdError { kind: ErrorKind::InvalidPassword, message: "Wrong password.".into() }
                    }
                    e => e.into(),
                })?;
            // This Mac's own slot went stale with the re-key; refresh it.
            after_password_unlock(s)?;
            Ok(recovery.to_display_string())
        })
        .await
}

/// Connects this Mac to a vault with a code from a device that's already
/// set up, instead of the password. `path` for a folder vault, `bucket` for
/// one in S3 or R2.
#[tauri::command]
pub async fn connect_with_code(
    state: State<'_, AppState>,
    path: Option<String>,
    bucket: Option<BucketInput>,
    code: String,
) -> CmdResult<()> {
    if !crate::presence::available() {
        return Err(CmdError::other("Joining with a code needs Touch ID or a login password on this Mac."));
    }
    let code = EnrollmentCode::parse(&code)
        .map_err(|_| CmdError::other("That isn't a code. It's 7 groups of letters and numbers."))?;
    let bucket = bucket.map(BucketInput::parse).transpose()?;
    state
        .run(move |s| {
            let (store, mut config, credentials): (Box<dyn Store>, Config, _) = match (&path, bucket) {
                (Some(path), None) => {
                    let path = std::path::PathBuf::from(path.trim());
                    (Box::new(FolderStore::new(&path)), Config::for_vault(path), None)
                }
                (None, Some((location, credentials))) => {
                    let store =
                        CachedStore::open(S3Store::new(&location, &credentials)?, config::cache_dir(&location)?)?;
                    (Box::new(store), Config::for_s3(location.clone())?, Some((location, credentials)))
                }
                _ => return Err(CmdError::other("Choose where the vault is.")),
            };
            let (vault, key) =
                Vault::enroll_with_code(store, &code, config.device_id, &device_name()).map_err(|e| match e {
                    Error::InvalidCredentials => CmdError {
                        kind: ErrorKind::InvalidPassword,
                        message: "That code doesn't work. Codes last ten minutes and work once.".into(),
                    },
                    e => e.into(),
                })?;
            save_key(config.device_id, &key)?;
            if let Some((location, credentials)) = credentials {
                dotsafe_stores::save_credentials(&location, &credentials)?;
            }
            config.device_unlock = true;
            config.last_password_unlock = Some(now_ms());
            config.save()?;
            s.config = Some(config);
            s.vault = Some(Arc::new(vault));
            Ok(())
        })
        .await
}
