//! A vault on disk, usually inside a synced folder (iCloud Drive, Google Drive,
//! Dropbox, ...):
//!
//! ```text
//! <root>/vault.json            header and key slots
//! <root>/records/<uuid>.json   one encrypted record per file
//! ```
//!
//! One file per record keeps sync conflicts rare. When a sync client still
//! leaves a conflict copy (`<uuid> (1).json`, `<uuid> 2.json`,
//! `<uuid> (conflicted copy ...).json`), reads merge it in and the next write
//! for that record removes it.

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::crypto::{self, KEY_LEN, KdfParams, Sealed, SecretKey};
use crate::error::{Error, Result};
use crate::header::{RecoveryKey, VaultHeader};
use crate::model::Record;

const HEADER_FILE: &str = "vault.json";
const RECORDS_DIR: &str = "records";
const RECORD_FORMAT: &str = "dotsafe-record";
const RECORD_VERSION: u32 = 1;
const RECORD_AAD: &[u8] = b"dotsafe/v1/record";
const RECORDS_KEY_INFO: &[u8] = b"dotsafe/v1/records";

/// The encrypted form of a record, as stored in `records/<uuid>.json`.
#[derive(Debug, Serialize, Deserialize)]
struct RecordFile {
    format: String,
    version: u32,
    id: Uuid,
    sealed: Sealed,
}

/// An unlocked vault. Holds the vault key in memory until dropped.
pub struct Vault {
    root: PathBuf,
    header: VaultHeader,
    vault_key: SecretKey,
    records_key: SecretKey,
}

impl std::fmt::Debug for Vault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Vault").field("root", &self.root).field("id", &self.id()).finish()
    }
}

impl Vault {
    /// Creates a new, empty vault in `root`, which may exist but must not
    /// already hold a vault.
    pub fn create(root: impl Into<PathBuf>, password: &str, params: KdfParams) -> Result<Vault> {
        let root = root.into();
        if root.join(HEADER_FILE).exists() {
            return Err(Error::AlreadyExists(root));
        }
        fs::create_dir_all(root.join(RECORDS_DIR))?;
        let vault_key = SecretKey::generate()?;
        let header = VaultHeader::new(&vault_key, password.as_bytes(), params)?;
        let vault = Vault::open(root, header, vault_key);
        vault.write_header()?;
        Ok(vault)
    }

    pub fn unlock(root: impl Into<PathBuf>, password: &str) -> Result<Vault> {
        let root = root.into();
        let header = read_header(&root)?;
        let vault_key = header.unlock_with_password(password.as_bytes())?;
        Ok(Vault::open(root, header, vault_key))
    }

    pub fn unlock_with_recovery(root: impl Into<PathBuf>, recovery: &RecoveryKey) -> Result<Vault> {
        let root = root.into();
        let header = read_header(&root)?;
        let vault_key = header.unlock_with_recovery(recovery)?;
        Ok(Vault::open(root, header, vault_key))
    }

    /// Unlocks with a key from [`Vault::session_key`], so a client can cache it
    /// (for example in the OS keychain) instead of asking for the password on
    /// every use. Fails if the vault was replaced since the key was cached.
    pub fn unlock_with_session_key(root: impl Into<PathBuf>, key: &[u8; KEY_LEN]) -> Result<Vault> {
        let root = root.into();
        let header = read_header(&root)?;
        let vault_key = header.unlock_with_key(SecretKey::from_bytes(*key))?;
        Ok(Vault::open(root, header, vault_key))
    }

    /// The raw vault key, for caching in secure OS storage. Anyone holding it
    /// can read the vault, so never write it anywhere else.
    pub fn session_key(&self) -> Zeroizing<[u8; KEY_LEN]> {
        Zeroizing::new(*self.vault_key.as_bytes())
    }

    fn open(root: PathBuf, header: VaultHeader, vault_key: SecretKey) -> Vault {
        let records_key = vault_key.derive(RECORDS_KEY_INFO);
        Vault { root, header, vault_key, records_key }
    }

    pub fn id(&self) -> Uuid {
        self.header.vault_id
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Replaces the password. Records are untouched; only the key slot changes.
    pub fn change_password(&mut self, new_password: &str, params: KdfParams) -> Result<()> {
        let mut header = self.header.clone();
        header.set_password(&self.vault_key, new_password.as_bytes(), params)?;
        self.commit_header(header)
    }

    /// Creates a recovery key, replacing any previous one. Show it to the user
    /// once; it can't be retrieved later.
    pub fn reset_recovery_key(&mut self) -> Result<RecoveryKey> {
        let mut header = self.header.clone();
        let recovery = header.set_recovery(&self.vault_key)?;
        self.commit_header(header)?;
        Ok(recovery)
    }

    fn commit_header(&mut self, header: VaultHeader) -> Result<()> {
        let previous = std::mem::replace(&mut self.header, header);
        if let Err(e) = self.write_header() {
            self.header = previous;
            return Err(e);
        }
        Ok(())
    }

    fn write_header(&self) -> Result<()> {
        write_atomic(&self.root.join(HEADER_FILE), &serde_json::to_vec_pretty(&self.header)?)
    }

    /// Reads one record, merging any sync-conflict copies of it.
    pub fn get(&self, id: Uuid) -> Result<Option<Record>> {
        let files = self.record_files()?.remove(&id).unwrap_or_default();
        self.read_merged(id, &files)
    }

    /// Reads every record, merging any sync-conflict copies.
    pub fn list(&self) -> Result<Vec<Record>> {
        let mut records = Vec::new();
        for (id, files) in self.record_files()? {
            records.extend(self.read_merged(id, &files)?);
        }
        Ok(records)
    }

    /// Writes a record, merged with whatever is already on disk so changes
    /// synced in from another device aren't lost. Returns the merged record.
    pub fn put(&self, record: Record) -> Result<Record> {
        let id = record.id();
        let files = self.record_files()?.remove(&id).unwrap_or_default();
        let merged = match self.read_merged(id, &files)? {
            Some(existing) => existing.merge(record)?,
            None => record,
        };

        let plaintext = Zeroizing::new(serde_json::to_vec(&merged)?);
        let file = RecordFile {
            format: RECORD_FORMAT.into(),
            version: RECORD_VERSION,
            id,
            sealed: crypto::seal(&self.records_key, &plaintext, &self.record_aad(id))?,
        };
        let canonical = self.record_path(id);
        write_atomic(&canonical, &serde_json::to_vec_pretty(&file)?)?;
        for path in files.iter().filter(|p| **p != canonical) {
            // Best effort: a copy that can't be removed is merged again next time.
            let _ = fs::remove_file(path);
        }
        Ok(merged)
    }

    fn read_merged(&self, id: Uuid, files: &[PathBuf]) -> Result<Option<Record>> {
        let mut merged: Option<Record> = None;
        for path in files {
            let record = self.read_record(id, path)?;
            merged = Some(match merged {
                Some(m) => m.merge(record)?,
                None => record,
            });
        }
        Ok(merged)
    }

    fn read_record(&self, id: Uuid, path: &Path) -> Result<Record> {
        let file: RecordFile = serde_json::from_slice(&fs::read(path)?)?;
        if file.format != RECORD_FORMAT {
            return Err(Error::Format(format!("{} is not a dotsafe record", path.display())));
        }
        if file.version != RECORD_VERSION {
            return Err(Error::UnsupportedVersion { kind: "record", version: file.version });
        }
        // The AAD uses the id from the file name, so a record moved to another
        // file name (or another vault) fails to decrypt.
        let plaintext = crypto::open(&self.records_key, &file.sealed, &self.record_aad(id))
            .ok_or(Error::RecordTampered(id))?;
        let record: Record = serde_json::from_slice(&plaintext)?;
        if record.id() != id {
            return Err(Error::RecordTampered(id));
        }
        Ok(record)
    }

    /// Record files grouped by record id, canonical file first.
    fn record_files(&self) -> Result<BTreeMap<Uuid, Vec<PathBuf>>> {
        let mut groups: BTreeMap<Uuid, Vec<PathBuf>> = BTreeMap::new();
        let dir = self.root.join(RECORDS_DIR);
        if !dir.exists() {
            return Ok(groups);
        }
        for entry in fs::read_dir(&dir)? {
            let path = entry?.path();
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else { continue };
            if name.starts_with('.') || !name.ends_with(".json") {
                continue;
            }
            let Some(id) = name.get(..36).and_then(|s| Uuid::parse_str(s).ok()) else { continue };
            groups.entry(id).or_default().push(path);
        }
        for (id, paths) in groups.iter_mut() {
            let canonical = self.record_path(*id);
            paths.sort_by_key(|p| (*p != canonical, p.clone()));
        }
        Ok(groups)
    }

    fn record_path(&self, id: Uuid) -> PathBuf {
        self.root.join(RECORDS_DIR).join(format!("{id}.json"))
    }

    fn record_aad(&self, id: Uuid) -> Vec<u8> {
        [RECORD_AAD, self.header.vault_id.as_bytes(), id.as_bytes()].concat()
    }
}

fn read_header(root: &Path) -> Result<VaultHeader> {
    let path = root.join(HEADER_FILE);
    let bytes = match fs::read(&path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(Error::NotFound(root.to_path_buf()));
        }
        Err(e) => return Err(e.into()),
    };
    let header: VaultHeader = serde_json::from_slice(&bytes)?;
    header.check_version()?;
    Ok(header)
}

/// Writes via a hidden temp file and rename, so sync clients and readers never
/// see a half-written file.
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let dir = path.parent().ok_or_else(|| Error::Format("path has no parent".into()))?;
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
    let tmp = dir.join(format!(".{name}.{}.tmp", Uuid::new_v4()));
    let result = (|| {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    Ok(result?)
}
