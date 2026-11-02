//! The app's side of licensing: the shared client (`dotsafe_integrations::licensing`)
//! plus recording checks and rechecking in the background. dotsafe is paid,
//! so the UI asks for a license before setup; the license is saved in its own
//! file, outside the session, so checks never take the session lock.

use std::time::Duration;

use dotsafe_core::License;
use dotsafe_core::license::unix_now;
pub use dotsafe_integrations::licensing::{ApiError, OFFLINE_MESSAGE, activate, deactivate, device_name, normalize_key, validate};
use tauri::{AppHandle, Emitter, Runtime};

use crate::state::{CmdError, ErrorKind};

impl From<ApiError> for CmdError {
    fn from(e: ApiError) -> Self {
        match e {
            ApiError::Offline => CmdError { kind: ErrorKind::Offline, message: OFFLINE_MESSAGE.into() },
            ApiError::Rejected(message) => CmdError::other(message),
        }
    }
}

/// Records the server's answer for the license with `instance_id`, if it's
/// still the one saved (it may have been removed while the check ran).
/// Returns whether anything visible changed.
pub fn record_check(instance_id: &str, valid: bool) -> Result<bool, CmdError> {
    let Some(mut license) = License::load()?.filter(|l| l.instance_id == instance_id) else {
        return Ok(false);
    };
    let now = unix_now();
    let before = license.status(now);
    license.checked_at = now;
    license.revoked = !valid;
    license.save()?;
    Ok(license.status(now) != before)
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
                && let Ok(valid) = validate(&license)
                && let Ok(true) = record_check(&license.instance_id, valid)
            {
                let _ = app.emit("license-changed", ());
            }
            std::thread::sleep(Duration::from_secs(60 * 60));
        }
    });
}
