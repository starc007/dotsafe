//! Activates, checks and deactivates licenses through the dotsafe API (see
//! apps/api/ in the repo), for the app and the CLI. Requests block, so the app
//! calls them off the UI thread and never while holding its session lock: a
//! slow network must not freeze the vault.

use std::time::Duration;

use dotsafe_core::License;
use dotsafe_core::license::unix_now;
use serde::Deserialize;
use serde_json::json;

/// Set `DOTSAFE_API_URL` at build time to point at another deployment.
const API_URL: &str = match option_env!("DOTSAFE_API_URL") {
    Some(url) => url,
    None if cfg!(debug_assertions) => "http://localhost:8787",
    None => "https://api.dotsafe.dev",
};

pub const OFFLINE_MESSAGE: &str = "Couldn't reach the license server. Check your connection and try again.";

#[derive(Debug, PartialEq)]
pub enum ApiError {
    /// No answer, or the server or Dodo failed. Never held against the user.
    Offline,
    /// The server refused the request, with a message to show.
    Rejected(String),
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApiError::Offline => f.write_str(OFFLINE_MESSAGE),
            ApiError::Rejected(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for ApiError {}

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

/// Activates `key` on this machine and returns the license to save. One
/// place for the app and the CLI, so both record an activation the same way.
pub fn activate_here(key: &str) -> Result<License, ApiError> {
    let key = normalize_key(key).map_err(|message| ApiError::Rejected(message.into()))?;
    let device_name = device_name();
    let instance_id = activate(&key, &device_name)?;
    let now = unix_now();
    Ok(License { key, instance_id, device_name, activated_at: now, checked_at: now, revoked: false })
}

/// Frees this machine's slot before the license is forgotten here. With
/// `force`, an unreachable server counts as done. Returns whether the server
/// freed the slot (false: forced while offline, so it stays taken).
pub fn release(license: &License, force: bool) -> Result<bool, ApiError> {
    match deactivate(license) {
        Ok(()) => Ok(true),
        Err(ApiError::Offline) if force => Ok(false),
        Err(e) => Err(e),
    }
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

/// Trims a pasted key and rejects anything that can't be a license key.
pub fn normalize_key(input: &str) -> Result<String, &'static str> {
    let key = input.trim();
    if key.is_empty() {
        return Err("Enter your license key.");
    }
    if key.len() < 8 || key.len() > 128 || key.chars().any(char::is_whitespace) {
        return Err("That doesn't look like a license key. Copy it again from your purchase email.");
    }
    Ok(key.to_string())
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
        // Rate limited (often a shared IP): a passing state, never a refusal,
        // so a background check can't mark a good license invalid.
        429 => Err(ApiError::Offline),
        400..=499 => {
            let message = serde_json::from_str::<ErrorBody>(text)
                .map(|b| b.error.message)
                .unwrap_or_else(|_| "The license server refused the request.".into());
            Err(ApiError::Rejected(message))
        }
        _ => Err(ApiError::Offline),
    }
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

    #[test]
    fn rate_limits_count_as_offline_not_invalid() {
        let body = r#"{"error":{"code":"rate_limited","message":"Too many requests. Try again in a minute."}}"#;
        assert_eq!(parse::<Valid>(429, body), Err(ApiError::Offline));
    }

    #[test]
    fn license_keys_are_trimmed_and_shape_checked() {
        assert_eq!(normalize_key("  DOTSAFE-ABCD-1234-EFGH \n"), Ok("DOTSAFE-ABCD-1234-EFGH".into()));
        assert!(normalize_key("").is_err());
        assert!(normalize_key("short").is_err());
        assert!(normalize_key("has a space in it").is_err());
        assert!(normalize_key(&"x".repeat(129)).is_err());
    }
}
