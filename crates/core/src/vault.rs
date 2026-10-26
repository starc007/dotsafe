//! A vault in a [`Store`], usually a synced folder (iCloud Drive, Google
//! Drive, Dropbox, ...):
//!
//! ```text
//! vault.json            header and key slots
//! head.json             sealed list of each record's newest stamp, for rollback detection
//! records/<uuid>.json   one encrypted record per object
//! ```
//!
//! One object per record keeps sync conflicts rare. When a sync client still
//! leaves a conflict copy (`<uuid> (1).json`, `<uuid> 2.json`,
//! `<uuid> (conflicted copy ...).json`), reads merge it in and the next write
//! for that record removes it. `head.json` gets the same treatment.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::crypto::{self, KEY_LEN, KdfParams, Sealed, SecretKey};
use crate::error::{Error, Result};
use crate::header::{HEADER_VERSION, RecoveryKey, VaultHeader};
use crate::model::{Clock, Record, Stamp};
use crate::store::{Cond, ETag, Put, Store};

const HEADER_KEY: &str = "vault.json";
const HEAD_KEY: &str = "head.json";
const RECORDS_DIR: &str = "records";

const RECORD_FORMAT: &str = "dotsafe-record";
const RECORD_VERSION: u32 = 2;
const RECORD_AAD_V1: &[u8] = b"dotsafe/v1/record";
const RECORD_AAD: &[u8] = b"dotsafe/v2/record";
// A domain label, unchanged from v1 so the records key stays the same.
const RECORDS_KEY_INFO: &[u8] = b"dotsafe/v1/records";

const HEAD_FORMAT: &str = "dotsafe-head";
const HEAD_VERSION: u32 = 2;
const HEAD_AAD: &[u8] = b"dotsafe/v2/head";

/// How many times a write re-reads and merges after losing a race.
const MAX_ATTEMPTS: usize = 16;

/// The encrypted form of a record, as stored in `records/<uuid>.json`.
#[derive(Debug, Serialize, Deserialize)]
struct RecordFile {
    format: String,
    version: u32,
    id: Uuid,
    /// The key epoch the record is sealed in. Absent in v1 records.
    #[serde(default, skip_serializing_if = "is_zero")]
    epoch: u32,
    sealed: Sealed,
}

fn is_zero(n: &u32) -> bool {
    *n == 0
}

#[derive(Debug, Serialize, Deserialize)]
struct HeadFile {
    format: String,
    version: u32,
    sealed: Sealed,
}

/// The newest stamp of every record. Merges by taking the newer stamp per
/// record, so concurrent writers converge.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
struct Head {
    records: BTreeMap<Uuid, Stamp>,
}

impl Head {
    /// Records `stamp` for `id` if it's newer. Returns whether it was.
    fn note(&mut self, id: Uuid, stamp: Stamp) -> bool {
        match self.records.get(&id) {
            Some(known) if *known >= stamp => false,
            _ => {
                self.records.insert(id, stamp);
                true
            }
        }
    }

    fn merge(&mut self, other: &Head) {
        for (id, stamp) in &other.records {
            self.note(*id, *stamp);
        }
    }
}

/// One object and its sync-conflict copies, merged.
struct Group<T> {
    value: Option<T>,
    /// Version of the canonical object, for the next conditional write.
    etag: Option<ETag>,
    copies: Vec<String>,
}

/// An unlocked vault. Holds the vault key in memory until dropped.
pub struct Vault {
    store: Box<dyn Store>,
    header: VaultHeader,
    vault_key: SecretKey,
    records_key: SecretKey,
    clock: Clock,
    /// The newest stamp seen for each record, from heads and from records.
    known: Mutex<Head>,
    /// Records last read older than `known`: rolled back by storage, or not
    /// finished syncing yet.
    stale: Mutex<BTreeSet<Uuid>>,
}

impl std::fmt::Debug for Vault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Vault").field("location", &self.location()).field("id", &self.id()).finish()
    }
}

impl Vault {
    /// Creates a new, empty vault in `store`, which must not already hold one.
    pub fn create(store: impl Store + 'static, password: &str, params: KdfParams) -> Result<Vault> {
        let store: Box<dyn Store> = Box::new(store);
        if store.get(HEADER_KEY)?.is_some() {
            return Err(Error::AlreadyExists(store.location()));
        }
        let vault_key = SecretKey::generate()?;
        let header = VaultHeader::new(&vault_key, password.as_bytes(), params)?;
        let vault = Vault::open(store, header, vault_key);
        match vault.store.put(HEADER_KEY, &serde_json::to_vec_pretty(&vault.header)?, Cond::IfAbsent)? {
            Put::Written(_) => Ok(vault),
            Put::Conflict => Err(Error::AlreadyExists(vault.location())),
        }
    }

    pub fn unlock(store: impl Store + 'static, password: &str) -> Result<Vault> {
        let store: Box<dyn Store> = Box::new(store);
        let header = read_header(&*store)?;
        let vault_key = header.unlock_with_password(password.as_bytes())?;
        Vault::unlocked(store, header, vault_key)
    }

    pub fn unlock_with_recovery(store: impl Store + 'static, recovery: &RecoveryKey) -> Result<Vault> {
        let store: Box<dyn Store> = Box::new(store);
        let header = read_header(&*store)?;
        let vault_key = header.unlock_with_recovery(recovery)?;
        Vault::unlocked(store, header, vault_key)
    }

    /// Unlocks with a key from [`Vault::session_key`], so a client can cache it
    /// (for example in the OS keychain) instead of asking for the password on
    /// every use. Fails if the vault was replaced since the key was cached.
    pub fn unlock_with_session_key(store: impl Store + 'static, key: &[u8; KEY_LEN]) -> Result<Vault> {
        let store: Box<dyn Store> = Box::new(store);
        let header = read_header(&*store)?;
        let vault_key = header.unlock_with_key(SecretKey::from_bytes(*key))?;
        Vault::unlocked(store, header, vault_key)
    }

    /// The raw vault key, for caching in secure OS storage. Anyone holding it
    /// can read the vault, so never write it anywhere else.
    pub fn session_key(&self) -> Zeroizing<[u8; KEY_LEN]> {
        Zeroizing::new(*self.vault_key.as_bytes())
    }

    fn open(store: Box<dyn Store>, header: VaultHeader, vault_key: SecretKey) -> Vault {
        let records_key = vault_key.derive(RECORDS_KEY_INFO);
        Vault {
            store,
            header,
            vault_key,
            records_key,
            clock: Clock::default(),
            known: Mutex::default(),
            stale: Mutex::default(),
        }
    }

    /// Opens an unlocked vault, upgrading an older format first.
    fn unlocked(store: Box<dyn Store>, header: VaultHeader, vault_key: SecretKey) -> Result<Vault> {
        let mut vault = Vault::open(store, header, vault_key);
        if vault.header.version < HEADER_VERSION {
            vault.upgrade()?;
        }
        Ok(vault)
    }

    /// Upgrades a v1 vault in place. Records are re-sealed first and the
    /// header goes last, so an interrupted upgrade just runs again next time.
    fn upgrade(&mut self) -> Result<()> {
        for record in self.list()? {
            self.put(record)?;
        }
        let mut header = self.header.clone();
        header.upgrade(&self.vault_key)?;
        self.commit_header(header)
    }

    pub fn id(&self) -> Uuid {
        self.header.vault_id
    }

    /// Where the vault is stored, for messages.
    pub fn location(&self) -> String {
        self.store.location()
    }

    /// A stamp for a new change, ordered after every change this vault has seen.
    pub fn stamp(&self, device: Uuid) -> Stamp {
        self.clock.stamp(device)
    }

    /// Records whose last read copy was older than one seen before, in a
    /// record or in `head.json`. Storage may have served an old copy, or a
    /// sync client may not have delivered the newest one yet. On synced
    /// folders the two look the same.
    pub fn stale_records(&self) -> Vec<Uuid> {
        lock(&self.stale).iter().copied().collect()
    }

    /// Exchanges changes with the store's remote, if it has one (a
    /// [`crate::CachedStore`]), then merges anything that changed on both
    /// sides. Returns whether anything came in. Fails with
    /// [`Error::Unreachable`] when offline; local changes are kept and pushed
    /// next time.
    pub fn sync(&self) -> Result<bool> {
        let changed = self.store.sync()?;
        self.refresh_head()?;
        for (id, keys) in self.record_keys()? {
            if keys.len() > 1
                && let Some(record) = self.read_record_group(id, &keys)?.value
            {
                self.put(record)?;
            }
        }
        if !self.read_head()?.copies.is_empty() {
            self.compact_head()?;
        }
        // Push the merges.
        self.store.sync()?;
        Ok(changed)
    }

    /// Checks `password` against this vault's password slot without touching
    /// storage. For confirming the password before sensitive changes.
    pub fn check_password(&self, password: &str) -> Result<()> {
        let key = self.header.unlock_with_password(password.as_bytes())?;
        if key.as_bytes() == self.vault_key.as_bytes() { Ok(()) } else { Err(Error::InvalidCredentials) }
    }

    /// Rewrites `head.json` with its copies merged in.
    fn compact_head(&self) -> Result<()> {
        for _ in 0..MAX_ATTEMPTS {
            let group = self.read_head()?;
            let head = group.value.unwrap_or_default();
            let cond = group.etag.map_or(Cond::IfAbsent, Cond::IfMatch);
            if let Put::Written(_) = self.store.put(HEAD_KEY, &self.seal_head(&head)?, cond)? {
                for copy in &group.copies {
                    let _ = self.store.delete(copy);
                }
                return Ok(());
            }
        }
        Ok(())
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

    // Header writes aren't conditional yet: two devices changing the password
    // at once still lose one change (see docs/vault-format.md).
    fn commit_header(&mut self, header: VaultHeader) -> Result<()> {
        self.store.put(HEADER_KEY, &serde_json::to_vec_pretty(&header)?, Cond::Any)?;
        self.header = header;
        Ok(())
    }

    /// Reads one record, merging any sync-conflict copies of it.
    pub fn get(&self, id: Uuid) -> Result<Option<Record>> {
        self.refresh_head()?;
        let keys = self.record_keys()?.remove(&id).unwrap_or_default();
        Ok(self.read_record_group(id, &keys)?.value)
    }

    /// Reads every record, merging any sync-conflict copies.
    pub fn list(&self) -> Result<Vec<Record>> {
        self.refresh_head()?;
        let mut records = Vec::new();
        for (id, keys) in self.record_keys()? {
            records.extend(self.read_record_group(id, &keys)?.value);
        }
        Ok(records)
    }

    /// Writes a record, merged with whatever is already stored so changes
    /// synced in from another device aren't lost. Returns the merged record.
    pub fn put(&self, record: Record) -> Result<Record> {
        let id = record.id();
        for _ in 0..MAX_ATTEMPTS {
            let keys = self.record_keys()?.remove(&id).unwrap_or_default();
            let group = self.read_record_group(id, &keys)?;
            let merged = match group.value {
                Some(existing) => existing.merge(record.clone())?,
                None => record.clone(),
            };
            let cond = group.etag.map_or(Cond::IfAbsent, Cond::IfMatch);
            if let Put::Written(_) = self.store.put(&record_key(id), &self.seal_record(&merged)?, cond)? {
                for copy in &group.copies {
                    // Best effort: a copy that can't be removed is merged again next time.
                    let _ = self.store.delete(copy);
                }
                self.observe(id, merged.max_stamp());
                self.write_head(id, merged.max_stamp())?;
                return Ok(merged);
            }
        }
        Err(Error::Busy(id))
    }

    fn seal_record(&self, record: &Record) -> Result<Vec<u8>> {
        let id = record.id();
        let plaintext = padded(serde_json::to_vec(record)?);
        let file = RecordFile {
            format: RECORD_FORMAT.into(),
            version: RECORD_VERSION,
            id,
            epoch: self.epoch(),
            sealed: crypto::seal(&self.records_key, &plaintext, &self.record_aad(id))?,
        };
        Ok(serde_json::to_vec_pretty(&file)?)
    }

    fn read_record_group(&self, id: Uuid, keys: &[String]) -> Result<Group<Record>> {
        let mut group = Group { value: None::<Record>, etag: None, copies: Vec::new() };
        for key in keys {
            let Some(object) = self.store.get(key)? else { continue };
            let record = self.open_record(id, key, &object.bytes)?;
            if *key == record_key(id) {
                group.etag = Some(object.etag);
            } else {
                group.copies.push(key.clone());
            }
            group.value = Some(match group.value {
                Some(merged) => merged.merge(record)?,
                None => record,
            });
        }
        if let Some(record) = &group.value {
            self.observe(id, record.max_stamp());
        }
        Ok(group)
    }

    fn open_record(&self, id: Uuid, key: &str, bytes: &[u8]) -> Result<Record> {
        let file: RecordFile = serde_json::from_slice(bytes)?;
        if file.format != RECORD_FORMAT {
            return Err(Error::Format(format!("{key} is not a dotsafe record")));
        }
        // The AAD uses the id from the object's name, so a record moved to
        // another name (or another vault) fails to decrypt.
        let aad = match file.version {
            1 => [RECORD_AAD_V1, self.header.vault_id.as_bytes(), id.as_bytes()].concat(),
            2 if file.epoch == self.epoch() => self.record_aad(id),
            2 => {
                return Err(Error::Format(format!(
                    "record {id} is sealed in key epoch {}, which this vault doesn't have",
                    file.epoch
                )));
            }
            version => return Err(Error::UnsupportedVersion { kind: "record", version }),
        };
        let plaintext = crypto::open(&self.records_key, &file.sealed, &aad).ok_or(Error::RecordTampered(id))?;
        let record: Record = serde_json::from_slice(&plaintext)?;
        if record.id() != id {
            return Err(Error::RecordTampered(id));
        }
        Ok(record)
    }

    /// Notes a record version just read or written: advances the clock, and
    /// flags it as stale if a newer version was seen before.
    fn observe(&self, id: Uuid, stamp: Stamp) {
        self.clock.observe(stamp);
        let mut known = lock(&self.known);
        let mut stale = lock(&self.stale);
        if known.records.get(&id).is_some_and(|k| *k > stamp) {
            stale.insert(id);
        } else {
            known.note(id, stamp);
            stale.remove(&id);
        }
    }

    fn refresh_head(&self) -> Result<()> {
        let head = self.read_head()?.value.unwrap_or_default();
        lock(&self.known).merge(&head);
        Ok(())
    }

    /// Records a record's newest stamp in `head.json`. The record is already
    /// saved, so losing every race here just leaves the head a step behind,
    /// which the next write fixes.
    fn write_head(&self, id: Uuid, stamp: Stamp) -> Result<()> {
        for _ in 0..MAX_ATTEMPTS {
            let group = self.read_head()?;
            let mut head = group.value.unwrap_or_default();
            if !head.note(id, stamp) && group.copies.is_empty() {
                return Ok(());
            }
            let cond = group.etag.map_or(Cond::IfAbsent, Cond::IfMatch);
            if let Put::Written(_) = self.store.put(HEAD_KEY, &self.seal_head(&head)?, cond)? {
                for copy in &group.copies {
                    let _ = self.store.delete(copy);
                }
                lock(&self.known).merge(&head);
                return Ok(());
            }
        }
        Ok(())
    }

    fn seal_head(&self, head: &Head) -> Result<Vec<u8>> {
        let plaintext = padded(serde_json::to_vec(head)?);
        let file = HeadFile {
            format: HEAD_FORMAT.into(),
            version: HEAD_VERSION,
            sealed: crypto::seal(&self.records_key, &plaintext, &self.head_aad())?,
        };
        Ok(serde_json::to_vec_pretty(&file)?)
    }

    /// Reads `head.json` and any sync-conflict copies of it, merged.
    fn read_head(&self) -> Result<Group<Head>> {
        let mut group = Group { value: None::<Head>, etag: None, copies: Vec::new() };
        let keys = self.store.list("")?;
        for key in keys.iter().filter(|k| k.starts_with("head") && k.ends_with(".json")) {
            let Some(object) = self.store.get(key)? else { continue };
            let file: HeadFile = serde_json::from_slice(&object.bytes)?;
            if file.format != HEAD_FORMAT || file.version != HEAD_VERSION {
                return Err(Error::UnsupportedVersion { kind: "head", version: file.version });
            }
            let plaintext =
                crypto::open(&self.records_key, &file.sealed, &self.head_aad()).ok_or(Error::HeadTampered)?;
            let head: Head = serde_json::from_slice(&plaintext)?;
            if key == HEAD_KEY {
                group.etag = Some(object.etag);
            } else {
                group.copies.push(key.clone());
            }
            group.value.get_or_insert_with(Head::default).merge(&head);
        }
        Ok(group)
    }

    /// Record keys grouped by record id, canonical key first.
    fn record_keys(&self) -> Result<BTreeMap<Uuid, Vec<String>>> {
        let mut groups: BTreeMap<Uuid, Vec<String>> = BTreeMap::new();
        for key in self.store.list(RECORDS_DIR)? {
            let name = &key[RECORDS_DIR.len() + 1..];
            if !name.ends_with(".json") {
                continue;
            }
            let Some(id) = name.get(..36).and_then(|s| Uuid::parse_str(s).ok()) else { continue };
            groups.entry(id).or_default().push(key);
        }
        for (id, keys) in groups.iter_mut() {
            let canonical = record_key(*id);
            keys.sort_by_key(|k| (*k != canonical, k.clone()));
        }
        Ok(groups)
    }

    /// v1 headers have no epoch; their records are moved into epoch 1.
    fn epoch(&self) -> u32 {
        self.header.epoch.max(1)
    }

    fn record_aad(&self, id: Uuid) -> Vec<u8> {
        [RECORD_AAD, self.header.vault_id.as_bytes(), &self.epoch().to_be_bytes(), id.as_bytes()].concat()
    }

    fn head_aad(&self) -> Vec<u8> {
        [HEAD_AAD, self.header.vault_id.as_bytes()].concat()
    }
}

fn record_key(id: Uuid) -> String {
    format!("{RECORDS_DIR}/{id}.json")
}

fn read_header(store: &dyn Store) -> Result<VaultHeader> {
    let object = store.get(HEADER_KEY)?.ok_or_else(|| Error::NotFound(store.location()))?;
    let header: VaultHeader = serde_json::from_slice(&object.bytes)?;
    header.check_version()?;
    Ok(header)
}

/// Pads JSON with trailing spaces (still valid JSON) so ciphertext sizes only
/// reveal a size bucket: 1 KiB steps up to 16 KiB, then powers of two.
fn padded(mut json: Vec<u8>) -> Zeroizing<Vec<u8>> {
    let len = json.len();
    let target = if len <= 16 * 1024 { len.div_ceil(1024).max(1) * 1024 } else { len.next_power_of_two() };
    json.resize(target, b' ');
    Zeroizing::new(json)
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn padding_buckets() {
        assert_eq!(padded(vec![b'x'; 10]).len(), 1024);
        assert_eq!(padded(vec![b'x'; 1024]).len(), 1024);
        assert_eq!(padded(vec![b'x'; 1025]).len(), 2048);
        assert_eq!(padded(vec![b'x'; 16 * 1024]).len(), 16 * 1024);
        assert_eq!(padded(vec![b'x'; 16 * 1024 + 1]).len(), 32 * 1024);
    }

    #[test]
    fn heads_merge_to_the_newest_stamp() {
        let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
        let mut one = Head::default();
        one.note(a, Stamp::new(5, Uuid::nil()));
        one.note(b, Stamp::new(1, Uuid::nil()));
        let mut two = Head::default();
        two.note(a, Stamp::new(3, Uuid::nil()));
        two.note(b, Stamp::new(9, Uuid::nil()));
        one.merge(&two);
        assert_eq!(one.records[&a].at_ms, 5);
        assert_eq!(one.records[&b].at_ms, 9);
    }
}
