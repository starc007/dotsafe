//! Session state shared by all commands: the config, the unlocked vault (if
//! any), and background housekeeping for auto-lock and clipboard clearing.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use dotsafe_agent::grants::Grants;
use dotsafe_core::{Config, Environment, Project, Record, Stamp, Vault};
use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, Runtime};
use tauri_plugin_clipboard_manager::ClipboardExt;
use uuid::Uuid;
use zeroize::Zeroizing;

/// Lock the vault after this long without any command from the UI.
pub const IDLE_LOCK: Duration = Duration::from_secs(15 * 60);
/// Clear a copied secret from the clipboard after this long.
pub const CLIPBOARD_CLEAR: Duration = Duration::from_secs(30);

#[derive(Clone, Default)]
pub struct AppState(pub Arc<Mutex<Session>>);

pub struct Session {
    pub config: Option<Config>,
    /// Shared so background sync can use it without holding the session lock.
    pub vault: Option<Arc<Vault>>,
    /// Last background or manual sync with the bucket, for vaults in S3.
    pub sync: SyncStatus,
    pub last_activity: Instant,
    /// Hash of a secret we put on the clipboard, so we only clear it if the
    /// user hasn't copied something else since.
    pub clipboard: Option<([u8; 32], Instant)>,
    /// Files chosen in the picker or dropped on the window, waiting to be
    /// previewed and imported. Paths never come from the webview.
    pub import_files: Vec<PathBuf>,
    /// Set after unlocking with the recovery key: the user has forgotten the
    /// password, so they must choose a new one before using the vault.
    pub must_change_password: bool,
    /// Commands agents may run without asking again. Ends with the unlock.
    pub agent_grants: Grants,
    /// The last enrollment code made here, so it can be copied in Rust.
    pub enrollment_code: Option<Zeroizing<String>>,
}

impl Default for Session {
    fn default() -> Self {
        Session {
            config: Config::load().ok().flatten(),
            vault: None,
            sync: SyncStatus::default(),
            last_activity: Instant::now(),
            clipboard: None,
            import_files: Vec::new(),
            must_change_password: false,
            agent_grants: Grants::default(),
            enrollment_code: None,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct SyncStatus {
    /// Unix milliseconds of the last successful sync.
    pub last_synced_at: Option<u64>,
    /// Why the last attempt failed, if it did.
    pub error: Option<String>,
}

/// Errors as the UI sees them: a kind it can branch on, and a message to show.
#[derive(Debug, Serialize)]
pub struct CmdError {
    pub kind: ErrorKind,
    pub message: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    Locked,
    InvalidPassword,
    /// The license server or the vault's bucket couldn't be reached.
    Offline,
    Other,
}

impl CmdError {
    pub fn other(message: impl Into<String>) -> Self {
        CmdError { kind: ErrorKind::Other, message: message.into() }
    }
}

impl From<dotsafe_core::Error> for CmdError {
    fn from(e: dotsafe_core::Error) -> Self {
        match e {
            dotsafe_core::Error::InvalidCredentials => {
                CmdError { kind: ErrorKind::InvalidPassword, message: "Wrong password.".into() }
            }
            dotsafe_core::Error::NotFound(location) => {
                CmdError::other(format!("No vault found at {location}. Is the synced folder available?"))
            }
            dotsafe_core::Error::Unreachable(location) => CmdError {
                kind: ErrorKind::Offline,
                message: format!("Couldn't reach {location}. Check your connection and try again."),
            },
            dotsafe_core::Error::Storage(message) => CmdError::other(capitalize(&message)),
            e => CmdError::other(e.to_string()),
        }
    }
}

pub type CmdResult<T> = Result<T, CmdError>;

impl Session {
    /// Forgets the vault key and everything tied to this unlock.
    pub fn lock(&mut self) {
        self.vault = None;
        self.import_files.clear();
        self.agent_grants.clear();
        self.enrollment_code = None;
        self.must_change_password = false;
    }

    pub fn vault(&self) -> CmdResult<&Vault> {
        self.vault.as_deref().ok_or(CmdError { kind: ErrorKind::Locked, message: "The vault is locked.".into() })
    }

    /// The vault, for changes that need it to themselves (password, recovery
    /// key). Fails briefly while a background sync is using it.
    pub fn vault_mut(&mut self) -> CmdResult<&mut Vault> {
        let vault =
            self.vault.as_mut().ok_or(CmdError { kind: ErrorKind::Locked, message: "The vault is locked.".into() })?;
        Arc::get_mut(vault).ok_or_else(|| CmdError::other("dotsafe is syncing. Try again in a moment."))
    }

    pub fn config(&self) -> CmdResult<&Config> {
        self.config.as_ref().ok_or_else(|| CmdError::other("No vault is set up yet."))
    }

    pub fn stamp(&self) -> CmdResult<Stamp> {
        Ok(self.vault()?.stamp(self.config()?.device_id))
    }

    pub fn project(&self, id: Uuid) -> CmdResult<Project> {
        match self.vault()?.get(id)? {
            Some(Record::Project(p)) if !p.deleted => Ok(p),
            _ => Err(CmdError::other("That project no longer exists.")),
        }
    }

    pub fn environment(&self, id: Uuid) -> CmdResult<Environment> {
        match self.vault()?.get(id)? {
            Some(Record::Environment(e)) if !e.deleted => Ok(e),
            _ => Err(CmdError::other("That environment no longer exists.")),
        }
    }
}

impl AppState {
    /// Runs `f` on a worker thread with the session locked. Argon2 and file
    /// IO must not block the UI thread.
    pub async fn run<T, F>(&self, f: F) -> CmdResult<T>
    where
        T: Send + 'static,
        F: FnOnce(&mut Session) -> CmdResult<T> + Send + 'static,
    {
        let inner = self.0.clone();
        tauri::async_runtime::spawn_blocking(move || {
            let mut session = inner.lock().map_err(|_| CmdError::other("Internal error."))?;
            session.last_activity = Instant::now();
            f(&mut session)
        })
        .await
        .map_err(|e| CmdError::other(e.to_string()))?
    }

    /// Locks the vault when idle and clears copied secrets from the
    /// clipboard. Emits `vault-locked` so the UI can return to the lock screen.
    pub fn spawn_housekeeping(&self, app: AppHandle) {
        let inner = self.0.clone();
        std::thread::spawn(move || {
            loop {
                std::thread::sleep(Duration::from_secs(5));
                let Ok(mut session) = inner.lock() else { return };

                if session.vault.is_some() && session.last_activity.elapsed() >= IDLE_LOCK {
                    session.lock();
                    let _ = app.emit("vault-locked", ());
                }

                if let Some((hash, at)) = session.clipboard
                    && at.elapsed() >= CLIPBOARD_CLEAR
                {
                    session.clipboard = None;
                    let still_ours = app.clipboard().read_text().is_ok_and(|text| hash_text(&text) == hash);
                    if still_ours {
                        let _ = app.clipboard().clear();
                    }
                }
            }
        });
    }
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    chars.next().map(|c| c.to_uppercase().chain(chars).collect()).unwrap_or_default()
}

/// Puts a secret on the clipboard and remembers its hash, so housekeeping
/// clears it after [`CLIPBOARD_CLEAR`] unless something else was copied.
/// Test builds skip the system clipboard; they check the remembered hash.
pub fn copy_to_clipboard<R: Runtime>(app: &AppHandle<R>, s: &mut Session, text: &str) -> CmdResult<()> {
    #[cfg(not(test))]
    app.clipboard().write_text(text).map_err(|e| CmdError::other(e.to_string()))?;
    #[cfg(test)]
    let _ = app;
    s.clipboard = Some((hash_text(text), Instant::now()));
    Ok(())
}

pub fn hash_text(text: &str) -> [u8; 32] {
    Sha256::digest(text.as_bytes()).into()
}
