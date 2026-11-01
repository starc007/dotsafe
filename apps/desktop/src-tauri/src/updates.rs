//! App updates, from the GitHub releases of the dotsafe repo. Release builds
//! check quietly at launch and every few hours and tell the UI; nothing is
//! downloaded until the user asks. The updater plugin checks each download's
//! signature against the public key in tauri.conf.json before installing it,
//! so a tampered release or a hijacked download is refused.
//!
//! All of it runs in Rust; the webview has no updater permission.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tauri_plugin_updater::{Update, UpdaterExt};

use crate::state::{CmdError, CmdResult, ErrorKind};

/// How often release builds look for a new version.
const EVERY: Duration = Duration::from_secs(6 * 60 * 60);
/// The first look waits a little, so it doesn't compete with unlocking.
const FIRST_AFTER: Duration = Duration::from_secs(30);

/// The update the last check found, kept until it's installed.
#[derive(Clone, Default)]
pub struct PendingUpdate(Arc<Mutex<Option<Update>>>);

/// What the UI shows about an available update.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateView {
    version: String,
    current_version: String,
    /// The release notes, as written on the GitHub release.
    notes: Option<String>,
}

/// Download progress, in bytes. `total` is `None` when the server doesn't say.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Progress {
    downloaded: u64,
    total: Option<u64>,
}

fn offline(e: impl std::fmt::Display) -> CmdError {
    CmdError {
        kind: ErrorKind::Offline,
        message: format!("Couldn't check for updates ({e}). Check your connection and try again."),
    }
}

async fn check<R: Runtime>(app: &AppHandle<R>) -> CmdResult<Option<UpdateView>> {
    let updater = app.updater().map_err(|e| CmdError::other(format!("Updates aren't set up: {e}")))?;
    let update = updater.check().await.map_err(offline)?;
    let view = update.as_ref().map(|u| UpdateView {
        version: u.version.clone(),
        current_version: u.current_version.clone(),
        notes: u.body.clone().filter(|n| !n.trim().is_empty()),
    });
    *app.state::<PendingUpdate>().0.lock().unwrap_or_else(|e| e.into_inner()) = update;
    Ok(view)
}

/// Looks for a newer version now. `None` when this is the latest.
#[tauri::command]
pub async fn check_for_update<R: Runtime>(app: AppHandle<R>) -> CmdResult<Option<UpdateView>> {
    check(&app).await
}

/// Downloads the update the last check found, installs it and restarts.
/// Emits `update-progress` while downloading.
#[tauri::command]
pub async fn install_update<R: Runtime>(app: AppHandle<R>) -> CmdResult<()> {
    let update = app
        .state::<PendingUpdate>()
        .0
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .ok_or_else(|| CmdError::other("Check for updates first."))?;
    let mut downloaded = 0u64;
    let progress = app.clone();
    update
        .download_and_install(
            move |chunk, total| {
                downloaded += chunk as u64;
                let _ = progress.emit("update-progress", Progress { downloaded, total });
            },
            || {},
        )
        .await
        .map_err(|e| CmdError::other(format!("Couldn't install the update: {e}")))?;
    app.restart()
}

/// Release builds look for updates in the background and emit
/// `update-available`. Debug builds only check when asked.
pub fn spawn_checker(app: AppHandle) {
    if cfg!(debug_assertions) {
        return;
    }
    std::thread::spawn(move || {
        std::thread::sleep(FIRST_AFTER);
        loop {
            if let Ok(Some(update)) = tauri::async_runtime::block_on(check(&app)) {
                let _ = app.emit("update-available", update);
            }
            std::thread::sleep(EVERY);
        }
    });
}
