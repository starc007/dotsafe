//! The app's license commands and background check, on the client shared
//! with the CLI (`dotsafe_integrations::licensing`). dotsafe is paid, so the
//! UI asks for a license before setup; the license is saved in its own file,
//! outside the session, so nothing here takes the session lock.

use std::time::Duration;

use dotsafe_core::license::unix_now;
use dotsafe_core::{License, LicenseStatus};
use dotsafe_integrations::licensing::{self, ApiError, OFFLINE_MESSAGE};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Runtime};

use crate::state::{CmdError, CmdResult, ErrorKind};

impl From<ApiError> for CmdError {
    fn from(e: ApiError) -> Self {
        match e {
            ApiError::Offline => CmdError { kind: ErrorKind::Offline, message: OFFLINE_MESSAGE.into() },
            ApiError::Rejected(message) => CmdError::other(message),
        }
    }
}

/// The saved license as the webview sees it.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LicenseView {
    key: String,
    device_name: String,
    /// Milliseconds, for JavaScript dates.
    activated_at: u64,
    checked_at: u64,
    status: LicenseStatus,
}

impl From<&License> for LicenseView {
    fn from(l: &License) -> Self {
        LicenseView {
            key: l.key.clone(),
            device_name: l.device_name.clone(),
            activated_at: l.activated_at * 1000,
            checked_at: l.checked_at * 1000,
            status: l.status(unix_now()),
        }
    }
}

/// The saved license, for Settings.
pub fn saved() -> CmdResult<Option<LicenseView>> {
    Ok(License::load()?.as_ref().map(LicenseView::from))
}

/// Activates a license key for this machine with the license server and
/// saves the activation.
#[tauri::command]
pub async fn activate_license(key: String) -> CmdResult<LicenseView> {
    let license = off_thread(move || licensing::activate_here(&key)).await?;
    license.save()?;
    Ok(LicenseView::from(&license))
}

/// Asks the license server about the saved license now. Errors with kind
/// `offline` if it can't be reached, leaving the license as it was.
#[tauri::command]
pub async fn check_license() -> CmdResult<Option<LicenseView>> {
    let Some(license) = License::load()? else { return Ok(None) };
    let valid = {
        let license = license.clone();
        off_thread(move || licensing::validate(&license)).await?
    };
    record_check(&license.instance_id, valid)?;
    saved()
}

/// Deactivates the license on the server, freeing this device's slot, then
/// forgets it here. With `force`, forgets it even when the server can't be
/// reached (the slot then stays taken).
#[tauri::command]
pub async fn remove_license(force: bool) -> CmdResult<()> {
    let Some(license) = License::load()? else { return Ok(()) };
    off_thread(move || licensing::release(&license, force)).await?;
    License::remove()?;
    Ok(())
}

/// Saves the server's answer for the license with `instance_id`, if it's
/// still the one saved (it may have been removed while the check ran).
/// Returns whether anything visible changed.
fn record_check(instance_id: &str, valid: bool) -> CmdResult<bool> {
    let Some(mut license) = License::load()?.filter(|l| l.instance_id == instance_id) else {
        return Ok(false);
    };
    let changed = license.record_check(valid, unix_now());
    license.save()?;
    Ok(changed)
}

/// Rechecks the license in the background about twice a day, and soon after
/// launch if it's due. Emits `license-changed` when its status changes.
/// Failures are silent: being offline is never held against the user.
pub fn spawn_checker<R: Runtime>(app: AppHandle<R>) {
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(10));
        loop {
            let due = License::load().ok().flatten().filter(|l| l.needs_check(unix_now()));
            if let Some(license) = due
                && let Ok(valid) = licensing::validate(&license)
                && let Ok(true) = record_check(&license.instance_id, valid)
            {
                let _ = app.emit("license-changed", ());
            }
            std::thread::sleep(Duration::from_secs(60 * 60));
        }
    });
}

/// Runs a license server request on a worker thread: they block, and a slow
/// network must not hold up the command's thread.
async fn off_thread<T: Send + 'static>(f: impl FnOnce() -> Result<T, ApiError> + Send + 'static) -> CmdResult<T> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| CmdError::other(e.to_string()))?.map_err(CmdError::from)
}
