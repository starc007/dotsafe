//! Activates, checks and deactivates licenses through the dotsafe API (see
//! api/ in the repo). Requests block, so call them off the UI thread and never
//! while holding the session lock: a slow network must not freeze the vault.

use std::time::Duration;

use dotsafe_core::License;
use dotsafe_core::license::unix_now;
use serde::Deserialize;
use serde_json::json;
use tauri::{AppHandle, Emitter};

use crate::state::{AppState, CmdError, ErrorKind};

/// Set `DOTSAFE_API_URL` at build time to point at another deployment.
const API_URL: &str = match option_env!("DOTSAFE_API_URL") {
    Some(url) => url,
    None if cfg!(debug_assertions) => "http://localhost:8787",
    None => "https://api.dotsafe.app",
};

const OFFLINE_MESSAGE: &str = "Couldn't reach the license server. Check your connection and try again.";

#[derive(Debug, PartialEq)]
pub enum ApiError {
    /// No answer, or the server or Dodo failed. Never held against the user.
    Offline,
    /// The server refused the request, with a message to show.
    Rejected(String),
}

impl From<ApiError> for CmdError {
    fn from(e: ApiError) -> Self {
        match e {
            ApiError::Offline => CmdError { kind: ErrorKind::Offline, message: OFFLINE_MESSAGE.into() },
            ApiError::Rejected(message) => CmdError::other(message),
        }
    }
}

/// Activates `key` for this device and returns the activation's instance id.
pub fn activate(key: &str, device_name: &str) -> Result<String, ApiError> {
    #[derive(Deserialize)]
    struct Activated {
        instance_id: String,
    }
    let activated: Activated =
        post("/v1/licenses/activate", json!({ "license_key": key, "device_name": device_name }))?;
    Ok(activated.instance_id)
}

/// Whether the activation is still good. A key the server no longer
/// recognizes counts as not valid.
pub fn validate(license: &License) -> Result<bool, ApiError> {
    #[derive(Deserialize)]
    struct Validity {
        valid: bool,
    }
    let body = json!({ "license_key": license.key, "instance_id": license.instance_id });
    match post::<Validity>("/v1/licenses/validate", body) {
        Ok(v) => Ok(v.valid),
        Err(ApiError::Rejected(_)) => Ok(false),
        Err(e) => Err(e),
    }
}

/// Frees this device's slot so the key can be used elsewhere.
pub fn deactivate(license: &License) -> Result<(), ApiError> {
    let body = json!({ "license_key": license.key, "instance_id": license.instance_id });
    post::<serde_json::Value>("/v1/licenses/deactivate", body).map(|_| ())
}

/// How this machine is listed among the key's activations.
pub fn device_name() -> String {
    let name = whoami::devicename().unwrap_or_default();
    let name = name.trim();
    if name.is_empty() { "dotsafe device".into() } else { name.chars().take(100).collect() }
}

fn post<T: serde::de::DeserializeOwned>(path: &str, body: serde_json::Value) -> Result<T, ApiError> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_global(Some(Duration::from_secs(15)))
        .build()
        .into();
    let mut res = agent.post(format!("{API_URL}{path}")).send_json(body).map_err(|_| ApiError::Offline)?;
    let status = res.status().as_u16();
    let text = res.body_mut().read_to_string().map_err(|_| ApiError::Offline)?;
    parse(status, &text)
}

#[derive(Deserialize)]
struct ErrorBody {
    error: ErrorDetail,
}

#[derive(Deserialize)]
struct ErrorDetail {
    message: String,
}

/// Maps an API response to a value or an error the user can act on.
fn parse<T: serde::de::DeserializeOwned>(status: u16, text: &str) -> Result<T, ApiError> {
    match status {
        200..=299 => serde_json::from_str(text).map_err(|_| ApiError::Offline),
        400..=499 => {
            let message = serde_json::from_str::<ErrorBody>(text)
                .map(|b| b.error.message)
                .unwrap_or_else(|_| "The license server refused the request.".into());
            Err(ApiError::Rejected(message))
        }
        _ => Err(ApiError::Offline),
    }
}

/// Records the server's answer for the license with `instance_id`, if it's
/// still the one saved (it may have been removed while the check ran).
/// Returns whether anything visible changed.
pub fn record_check(state: &AppState, instance_id: &str, valid: bool) -> Result<bool, CmdError> {
    let mut session = state.0.lock().map_err(|_| CmdError::other("Internal error."))?;
    let Some(mut config) = session.config.clone() else {
        return Ok(false);
    };
    let Some(license) = config.license.as_mut().filter(|l| l.instance_id == instance_id) else {
        return Ok(false);
    };
    let now = unix_now();
    let before = license.status(now);
    license.checked_at = now;
    license.revoked = !valid;
    let changed = license.status(now) != before;
    config.save()?;
    session.config = Some(config);
    Ok(changed)
}

/// Rechecks the license in the background about twice a day, and soon after
/// launch if it's due. Emits `license-changed` when its status changes.
/// Failures are silent: being offline is never held against the user.
pub fn spawn_checker(state: AppState, app: AppHandle) {
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(10));
        loop {
            let due = state
                .0
                .lock()
                .ok()
                .and_then(|s| s.config.as_ref()?.license.clone())
                .filter(|l| l.needs_check(unix_now()));
            if let Some(license) = due
                && let Ok(valid) = validate(&license)
                && let Ok(true) = record_check(&state, &license.instance_id, valid)
            {
                let _ = app.emit("license-changed", ());
            }
            std::thread::sleep(Duration::from_secs(60 * 60));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Deserialize, PartialEq)]
    struct Valid {
        valid: bool,
    }

    #[test]
    fn success_bodies_are_parsed() {
        assert_eq!(parse::<Valid>(200, r#"{"valid":true}"#), Ok(Valid { valid: true }));
    }

    #[test]
    fn refusals_carry_the_servers_message() {
        let body = r#"{"error":{"code":"limit_reached","message":"Deactivate it on another device first."}}"#;
        assert_eq!(parse::<Valid>(409, body), Err(ApiError::Rejected("Deactivate it on another device first.".into())));
        assert!(matches!(parse::<Valid>(404, "<html>"), Err(ApiError::Rejected(_))));
    }

    #[test]
    fn server_failures_count_as_offline() {
        assert_eq!(parse::<Valid>(502, r#"{"error":{"code":"upstream_error","message":"x"}}"#), Err(ApiError::Offline));
        assert_eq!(parse::<Valid>(500, ""), Err(ApiError::Offline));
        assert_eq!(parse::<Valid>(200, "not json"), Err(ApiError::Offline));
    }
}
