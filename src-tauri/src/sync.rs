//! Background sync for vaults in a bucket. Runs off the session lock: the
//! vault is shared, so a slow or unreachable bucket never freezes the app.
//! Folder vaults are kept in sync by the OS sync client and skip all this.

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use dotsafe_core::{Error, Vault};
use tauri::{AppHandle, Emitter, Runtime};

use crate::state::{AppState, SyncStatus};

const INTERVAL: Duration = Duration::from_secs(60);

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

fn shared_vault(state: &AppState) -> Option<Arc<Vault>> {
    let session = state.0.lock().ok()?;
    session.config.as_ref()?.s3.as_ref()?;
    session.vault.clone()
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}
