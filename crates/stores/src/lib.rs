//! Storage backends that need the network, kept out of `dotsafe-core` so the
//! core stays small and easy to audit. Also opens the right store for a
//! machine's [`Config`], so the app and the CLI do it the same way.

mod s3;
mod webdav;

use dotsafe_core::{CachedStore, Config, Error, FolderStore, Result, S3Location, Store, WebDavLocation};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

pub use s3::{S3Credentials, S3Store};
pub use webdav::WebDavStore;

const KEYCHAIN_SERVICE: &str = "app.dotsafe.storage";

/// The store a machine's config points at: the folder itself, or a bucket or
/// WebDAV folder behind a local cache. Opening a remote vault syncs when online.
pub fn open(config: &Config) -> Result<Box<dyn Store>> {
    if let Some(webdav) = &config.webdav {
        let password = webdav_password(webdav)?.ok_or_else(|| {
            Error::Storage(format!("no password saved for {}; connect the vault again", webdav.describe()))
        })?;
        let remote = WebDavStore::new(webdav, &password)?;
        return Ok(Box::new(CachedStore::open(remote, &config.vault_path)?));
    }
    match &config.s3 {
        None => Ok(Box::new(FolderStore::new(&config.vault_path))),
        Some(s3) => {
            let credentials = credentials(s3)?.ok_or_else(|| {
                Error::Storage(format!("no access key saved for {}; connect the vault again", s3.describe()))
            })?;
            let remote = S3Store::new(s3, &credentials)?;
            Ok(Box::new(CachedStore::open(remote, &config.vault_path)?))
        }
    }
}

#[derive(Serialize, Deserialize)]
struct Saved {
    access_key_id: String,
    secret_access_key: Zeroizing<String>,
}

/// The access key for `s3`: from `DOTSAFE_S3_ACCESS_KEY_ID` and
/// `DOTSAFE_S3_SECRET_ACCESS_KEY` if set (CI and scripts), otherwise from the
/// OS keychain.
pub fn credentials(s3: &S3Location) -> Result<Option<S3Credentials>> {
    if let (Ok(id), Ok(secret)) =
        (std::env::var("DOTSAFE_S3_ACCESS_KEY_ID"), std::env::var("DOTSAFE_S3_SECRET_ACCESS_KEY"))
    {
        return Ok(Some(S3Credentials { access_key_id: id, secret_access_key: Zeroizing::new(secret) }));
    }
    let secret = match entry(s3)?.get_secret() {
        Ok(secret) => Zeroizing::new(secret),
        Err(keyring::Error::NoEntry) => return Ok(None),
        Err(e) => return Err(Error::Storage(format!("couldn't read the OS keychain: {e}"))),
    };
    let saved: Saved = serde_json::from_slice(&secret)?;
    Ok(Some(S3Credentials { access_key_id: saved.access_key_id, secret_access_key: saved.secret_access_key }))
}

/// Saves the access key for `s3` in the OS keychain.
pub fn save_credentials(s3: &S3Location, credentials: &S3Credentials) -> Result<()> {
    let saved = Saved {
        access_key_id: credentials.access_key_id.clone(),
        secret_access_key: credentials.secret_access_key.clone(),
    };
    let blob = Zeroizing::new(serde_json::to_vec(&saved)?);
    entry(s3)?.set_secret(&blob).map_err(|e| Error::Storage(format!("couldn't save to the OS keychain: {e}")))
}

fn entry(s3: &S3Location) -> Result<keyring::Entry> {
    keychain(&s3.id())
}

fn keychain(account: &str) -> Result<keyring::Entry> {
    if std::env::var_os("DOTSAFE_NO_KEYCHAIN").is_some() {
        return Err(Error::Storage("the OS keychain is disabled (DOTSAFE_NO_KEYCHAIN is set)".into()));
    }
    keyring::Entry::new(KEYCHAIN_SERVICE, account)
        .map_err(|e| Error::Storage(format!("couldn't open the OS keychain: {e}")))
}

/// The password for a WebDAV folder: from `DOTSAFE_WEBDAV_PASSWORD` if set
/// (CI and scripts), otherwise from the OS keychain.
pub fn webdav_password(webdav: &WebDavLocation) -> Result<Option<Zeroizing<String>>> {
    if let Ok(password) = std::env::var("DOTSAFE_WEBDAV_PASSWORD") {
        return Ok(Some(Zeroizing::new(password)));
    }
    match keychain(&webdav.id())?.get_password() {
        Ok(password) => Ok(Some(Zeroizing::new(password))),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(Error::Storage(format!("couldn't read the OS keychain: {e}"))),
    }
}

/// Saves a WebDAV folder's password in the OS keychain.
pub fn save_webdav_password(webdav: &WebDavLocation, password: &str) -> Result<()> {
    keychain(&webdav.id())?
        .set_password(password)
        .map_err(|e| Error::Storage(format!("couldn't save to the OS keychain: {e}")))
}
