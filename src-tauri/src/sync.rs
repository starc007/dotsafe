//! Background sync for vaults in a bucket. Runs off the session lock: the
//! vault is shared, so a slow or unreachable bucket never freezes the app.
//! Folder vaults are kept in sync by the OS sync client; for those the app
//! only watches for changes made outside it (the CLI, or a synced edit from
//! another Mac) and tells the UI to reload.

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use dotsafe_core::{Error, ObjectMeta, Vault};
use tauri::{AppHandle, Emitter, Runtime};

use crate::state::{AppState, SyncStatus};

const INTERVAL: Duration = Duration::from_secs(60);
/// How often a folder vault is checked for changes made outside the app.
const WATCH_INTERVAL: Duration = Duration::from_secs(2);

/// Syncs the unlocked vault, if it lives in a bucket, and records the result.
/// Returns whether anything came in. Emits `vault-synced` when it did.
pub fn sync_once<R: Runtime>(state: &AppState, app: Option<&AppHandle<R>>) -> Result<bool, Error> {
    let Some(vault) = shared_vault(state) else { return Ok(false) };
    let result = vault.sync();
    if let Ok(mut session) = state.0.lock() {
        // Only record it if the same vault is still unlocked.
        if session.vault.as_ref().is_some_and(|v| Arc::ptr_eq(v, &vault)) {
            session.sync = match &result {
                Ok(_) => SyncStatus { last_synced_at: Some(now_ms()), error: None },
                Err(Error::Unreachable(_)) => SyncStatus {
                    error: Some("Offline. Changes are saved on this Mac and sync when you're back.".into()),
                    ..session.sync.clone()
                },
                Err(e) => SyncStatus { error: Some(e.to_string()), ..session.sync.clone() },
            };
        }
    }
    if let (Ok(true), Some(app)) = (&result, app) {
        let _ = app.emit("vault-synced", ());
    }
    result
}

pub fn spawn_syncer(state: AppState, app: AppHandle) {
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(INTERVAL);
            let _ = sync_once(&state, Some(&app));
        }
    });
}

/// Emits `vault-synced` when a folder vault changes outside the app.
pub fn spawn_watcher(state: AppState, app: AppHandle) {
    std::thread::spawn(move || {
        let mut watch = FolderWatch::default();
        loop {
            std::thread::sleep(WATCH_INTERVAL);
            let Some(vault) = folder_vault(&state) else {
                watch = FolderWatch::default();
                continue;
            };
            // Reads outside the session lock; the vault is shared.
            if watch.poll(&vault) {
                let _ = app.emit("vault-synced", ());
            }
        }
    });
}

/// Notices changes to a folder vault. Every write rewrites `head.json`, so
/// the top-level files' versions are enough, without decrypting anything.
/// The app's own writes count too, which only reloads lists that are already
/// up to date.
#[derive(Default)]
pub struct FolderWatch {
    last: Option<(Arc<Vault>, Vec<ObjectMeta>)>,
}

impl FolderWatch {
    /// Whether the vault changed since the last poll. The first poll of a
    /// vault (just unlocked) only takes note.
    pub fn poll(&mut self, vault: &Arc<Vault>) -> bool {
        let Ok(mut now) = vault.store().list_meta("") else { return false };
        now.sort_by(|a, b| a.key.cmp(&b.key));
        let changed = matches!(&self.last, Some((seen, before)) if Arc::ptr_eq(seen, vault) && *before != now);
        self.last = Some((vault.clone(), now));
        changed
    }
}

fn shared_vault(state: &AppState) -> Option<Arc<Vault>> {
    let session = state.0.lock().ok()?;
    if !session.config.as_ref()?.is_remote() {
        return None;
    }
    session.vault.clone()
}

fn folder_vault(state: &AppState) -> Option<Arc<Vault>> {
    let session = state.0.lock().ok()?;
    if session.config.as_ref()?.is_remote() {
        return None;
    }
    session.vault.clone()
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}
