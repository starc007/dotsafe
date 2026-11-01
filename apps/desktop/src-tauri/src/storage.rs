//! Where the vault lives, beyond the first setup: WebDAV vaults, backup
//! snapshots, and moving the vault somewhere else. Everything copied is
//! already encrypted.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use dotsafe_core::{CachedStore, Config, FolderStore, KdfParams, Vault, WebDavLocation, backup, config};
use dotsafe_stores::WebDavStore;
use serde::Deserialize;
use tauri::{AppHandle, Emitter, Runtime, State};
use zeroize::Zeroizing;

use crate::state::{AppState, CmdError, CmdResult};

/// A daily backup runs when the last one is older than this.
const BACKUP_EVERY: Duration = Duration::from_secs(24 * 60 * 60);
const CHECK_EVERY: Duration = Duration::from_secs(60 * 60);

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// A WebDAV folder as the setup form describes it, with its password.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebDavInput {
    url: String,
    username: String,
    password: String,
}

impl WebDavInput {
    fn parse(self) -> CmdResult<(WebDavLocation, Zeroizing<String>, WebDavStore)> {
        let username = self.username.trim().to_string();
        let password = Zeroizing::new(self.password);
        if username.is_empty() || password.is_empty() {
            return Err(CmdError::other("Enter the username and password."));
        }
        let location = WebDavLocation { url: self.url.trim().to_string(), username };
        let store = WebDavStore::new(&location, &password)?;
        Ok((location, password, store))
    }
}

/// Whether the WebDAV folder already holds a vault. Also checks the password.
#[tauri::command]
pub async fn webdav_has_vault(webdav: WebDavInput) -> CmdResult<bool> {
    tauri::async_runtime::spawn_blocking(move || -> CmdResult<bool> {
        let (_, _, store) = webdav.parse()?;
        use dotsafe_core::Store;
        Ok(store.get("vault.json")?.is_some())
    })
    .await
    .map_err(|e| CmdError::other(e.to_string()))?
}

/// Creates a vault in a WebDAV folder and returns its recovery key, shown once.
#[tauri::command]
pub async fn create_webdav_vault(state: State<'_, AppState>, webdav: WebDavInput, password: String) -> CmdResult<String> {
    let password = Zeroizing::new(password);
    crate::commands::check_new_password(&password).map_err(CmdError::other)?;
    state
        .run(move |s| {
            let (location, webdav_password, remote) = webdav.parse()?;
            let store = CachedStore::open(remote, config::cache_dir_for(&location.id())?)?;
            let mut vault = Vault::create(store, &password, KdfParams::default())?;
            let recovery = vault.reset_recovery_key()?.to_display_string();
            dotsafe_stores::save_webdav_password(&location, &webdav_password)?;
            let config = Config::for_webdav(location)?;
            config.save()?;
            s.config = Some(config);
            s.vault = Some(Arc::new(vault));
            Ok(recovery)
        })
        .await
}

/// Connects this machine to a vault that's already in a WebDAV folder.
#[tauri::command]
pub async fn connect_webdav_vault(state: State<'_, AppState>, webdav: WebDavInput, password: String) -> CmdResult<()> {
    let password = Zeroizing::new(password);
    state
        .run(move |s| {
            let (location, webdav_password, remote) = webdav.parse()?;
            let store = CachedStore::open(remote, config::cache_dir_for(&location.id())?)?;
            let vault = Vault::unlock(store, &password)?;
            dotsafe_stores::save_webdav_password(&location, &webdav_password)?;
            let config = Config::for_webdav(location)?;
            config.save()?;
            s.config = Some(config);
            s.vault = Some(Arc::new(vault));
            Ok(())
        })
        .await
}

/// Sets (or, with `null`, clears) the folder backups go to.
#[tauri::command]
pub async fn set_backup_dir(state: State<'_, AppState>, path: Option<String>) -> CmdResult<()> {
    let path = match path.map(|p| PathBuf::from(p.trim())) {
        Some(p) if !p.is_absolute() => return Err(CmdError::other("Choose a folder.")),
        other => other,
    };
    state
        .run(move |s| {
            let mut config = s.config()?.clone();
            config.backup_dir = path;
            config.save()?;
            s.config = Some(config);
            Ok(())
        })
        .await
}

/// Takes a backup snapshot now. Returns when (Unix ms).
#[tauri::command]
pub async fn back_up_now(state: State<'_, AppState>) -> CmdResult<u64> {
    let inner = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || back_up(&inner)).await.map_err(|e| CmdError::other(e.to_string()))?
}

/// Snapshots the unlocked vault into the backup folder, off the session lock.
fn back_up(state: &AppState) -> CmdResult<u64> {
    let (vault, dir) = {
        let s = state.0.lock().map_err(|_| CmdError::other("Internal error."))?;
        s.vault()?;
        let dir = s.config()?.backup_dir.clone().ok_or_else(|| CmdError::other("Choose a backup folder first."))?;
        (s.vault.clone().expect("checked above"), dir)
    };
    let now = now_ms();
    backup::snapshot(vault.store(), &dir, &backup::stamp(now))?;
    if let Ok(mut s) = state.0.lock()
        && let Some(config) = s.config.as_mut()
    {
        config.last_backup = Some(now);
        let _ = config.save();
    }
    Ok(now)
}

/// Backs up once a day while the vault is unlocked, if a folder is set.
pub fn spawn_backups<R: Runtime>(state: AppState, app: AppHandle<R>) {
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(CHECK_EVERY);
            let due = state.0.lock().ok().is_some_and(|s| {
                s.vault.is_some()
                    && s.config.as_ref().is_some_and(|c| {
                        c.backup_dir.is_some()
                            && c.last_backup.is_none_or(|at| now_ms().saturating_sub(at) >= BACKUP_EVERY.as_millis() as u64)
                    })
            });
            if due && back_up(&state).is_ok() {
                let _ = app.emit("backup-done", ());
            }
        }
    });
}

/// Moves the vault to an empty folder: copies it, checks the copy opens,
/// then switches this Mac to it. The old copy is left where it was.
#[tauri::command]
pub async fn move_vault_to_folder(state: State<'_, AppState>, path: String) -> CmdResult<String> {
    let path = PathBuf::from(path.trim());
    if !path.is_absolute() {
        return Err(CmdError::other("Choose a folder."));
    }
    let inner = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || -> CmdResult<String> {
        let (vault, old) = {
            let s = inner.0.lock().map_err(|_| CmdError::other("Internal error."))?;
            s.vault()?;
            (s.vault.clone().expect("checked above"), s.config()?.clone())
        };
        if old.is_remote() {
            vault.sync().map_err(|e| CmdError::other(format!("Sync first so nothing is left behind: {e}")))?;
        }
        std::fs::create_dir_all(&path).map_err(|e| CmdError::other(e.to_string()))?;
        backup::copy_to_empty(vault.store(), &FolderStore::new(&path)).map_err(|e| match e {
            dotsafe_core::Error::AlreadyExists(_) => CmdError::other("That folder already holds a vault. Pick an empty one."),
            e => e.into(),
        })?;
        let moved = Vault::unlock_with_session_key(FolderStore::new(&path), &vault.session_key())
            .map_err(|e| CmdError::other(format!("The copy doesn't open ({e}); nothing was switched.")))?;
        moved.list()?;
        let config = Config::for_vault(path);
        config.save()?;
        let mut s = inner.0.lock().map_err(|_| CmdError::other("Internal error."))?;
        s.config = Some(config);
        s.vault = Some(Arc::new(moved));
        s.sync = Default::default();
        Ok(old.describe_location())
    })
    .await
    .map_err(|e| CmdError::other(e.to_string()))?
}
