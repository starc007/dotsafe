//! Opening the vault: from the keychain cache left by `dotsafe unlock`, from
//! `DOTSAFE_PASSWORD`, or by prompting.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow, bail};
use dotsafe_core::{Config, Error, Vault};
use zeroize::Zeroizing;

const KEYCHAIN_SERVICE: &str = "app.dotsafe.session";

pub fn open(config: &Config) -> Result<Vault> {
    if let Some(vault) = from_cache(config)? {
        return Ok(vault);
    }
    let password = password("Master password: ")?;
    let vault = Vault::unlock(dotsafe_stores::open(config).map_err(friendly)?, &password).map_err(friendly)?;
    if std::env::var_os("DOTSAFE_PASSWORD").is_none() {
        eprintln!("tip: run `dotsafe unlock` to skip the password for a while");
    }
    Ok(vault)
}

/// Caches the vault key in the OS keychain until `ttl` passes.
pub fn cache(vault: &Vault, config: &Config, ttl: Duration) -> Result<()> {
    let expires = now_secs() + ttl.as_secs();
    let mut blob = Zeroizing::new(Vec::with_capacity(40));
    blob.extend_from_slice(&expires.to_be_bytes());
    blob.extend_from_slice(vault.session_key().as_ref());
    entry(config)?
        .set_secret(&blob)
        .context("couldn't save to the OS keychain")
}

/// Removes the cached key. Returns whether there was one.
pub fn clear(config: &Config) -> Result<bool> {
    match entry(config)?.delete_credential() {
        Ok(()) => Ok(true),
        Err(keyring::Error::NoEntry) => Ok(false),
        Err(e) => Err(anyhow!(e).context("couldn't remove the key from the OS keychain")),
    }
}

fn from_cache(config: &Config) -> Result<Option<Vault>> {
    let Some(key) = cached_key(config) else { return Ok(None) };
    let store = dotsafe_stores::open(config).map_err(friendly)?;
    // A stale key (vault replaced, or the folder changed) just falls through
    // to the password prompt.
    Ok(Vault::unlock_with_session_key(store, &key).ok())
}

fn cached_key(config: &Config) -> Option<Zeroizing<[u8; 32]>> {
    let entry = entry(config).ok()?;
    let blob = Zeroizing::new(entry.get_secret().ok()?);
    let (expires, key) = blob.split_at_checked(8)?;
    let expires = u64::from_be_bytes(expires.try_into().ok()?);
    if expires <= now_secs() {
        let _ = entry.delete_credential();
        return None;
    }
    Some(Zeroizing::new(key.try_into().ok()?))
}

fn entry(config: &Config) -> Result<keyring::Entry> {
    if std::env::var_os("DOTSAFE_NO_KEYCHAIN").is_some() {
        bail!("the OS keychain is disabled (DOTSAFE_NO_KEYCHAIN is set)");
    }
    let account = match config.remote_id() {
        Some(id) => id,
        None => {
            let path = &config.vault_path;
            path.canonicalize().unwrap_or_else(|_| path.to_path_buf()).to_string_lossy().into_owned()
        }
    };
    keyring::Entry::new(KEYCHAIN_SERVICE, &account).context("couldn't open the OS keychain")
}

/// Reads a password from `DOTSAFE_PASSWORD` (for CI and scripts) or the terminal.
pub fn password(prompt: &str) -> Result<Zeroizing<String>> {
    if let Ok(pw) = std::env::var("DOTSAFE_PASSWORD") {
        return Ok(Zeroizing::new(pw));
    }
    let pw = rpassword::prompt_password(prompt).context(
        "couldn't ask for the password (no terminal?); run `dotsafe unlock` first, or set DOTSAFE_PASSWORD",
    )?;
    Ok(Zeroizing::new(pw))
}

pub fn friendly(e: Error) -> anyhow::Error {
    match e {
        Error::NotFound(location) => {
            anyhow!("no vault at {location}; is the synced folder available? (run `dotsafe init` to set one up)")
        }
        Error::Unreachable(location) => {
            anyhow!("couldn't reach {location}, and nothing is cached on this machine yet; check your connection")
        }
        e => anyhow!(e),
    }
}

/// Parses durations like `90s`, `30m`, `8h`, `7d`.
pub fn parse_duration(s: &str) -> Result<Duration, String> {
    let (num, unit) = s.split_at(s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len()));
    let n: u64 = num.parse().map_err(|_| format!("invalid duration {s:?}"))?;
    if n == 0 {
        return Err("duration must be more than zero".into());
    }
    let secs = match unit {
        "s" => n,
        "m" | "" => n * 60,
        "h" => n * 3600,
        "d" => n * 86400,
        _ => return Err(format!("invalid duration {s:?}; use s, m, h or d")),
    };
    Ok(Duration::from_secs(secs))
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}
