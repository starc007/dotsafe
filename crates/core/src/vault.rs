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
use crate::header::{DeviceInfo, DeviceKey, EnrollmentCode, HEADER_VERSION, RecoveryKey, VaultHeader};
use crate::model::{Clock, Record, Secrets, Stamp};
use crate::store::{Cond, ETag, Put, Store};

const HEADER_KEY: &str = "vault.json";
const HEAD_KEY: &str = "head.json";
const RECORDS_DIR: &str = "records";

const RECORD_FORMAT: &str = "dotsafe-record";
/// Records are written with the oldest version that can hold them
/// ([`Record::min_version`]): v3 for shared secrets and linked variables, v4
/// for integrations, v5 for environments with a parent or references, v2 for
/// the rest. Clients that predate a version refuse
/// those records instead of rewriting them without what they don't know.
const RECORD_VERSION_MAX: u32 = 5;
const RECORD_AAD_V1: &[u8] = b"dotsafe/v1/record";
const RECORD_AAD: &[u8] = b"dotsafe/v2/record";
const RECORD_AAD_V3: &[u8] = b"dotsafe/v3/record";
const RECORD_AAD_V4: &[u8] = b"dotsafe/v4/record";
const RECORD_AAD_V5: &[u8] = b"dotsafe/v5/record";
// A domain label, unchanged from v1 so the records key stays the same.
const RECORDS_KEY_INFO: &[u8] = b"dotsafe/v1/records";
/// Subkey for `dotsafe approve` answers. Never stored; only tags messages.
const APPROVAL_KEY_INFO: &[u8] = b"dotsafe/v1/approve";

fn decode_hex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) || !s.is_ascii() {
        return None;
    }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok()).collect()
}

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
    /// The key epoch it's sealed in. Absent (epoch 1) before re-keying existed.
    #[serde(default, skip_serializing_if = "is_zero")]
    epoch: u32,
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
    /// The records key of the epoch before, while a re-key is re-sealing.
    previous: Option<(u32, SecretKey)>,
    clock: Clock,
    /// The newest stamp seen for each record, from heads and from records.
    known: Mutex<Head>,
    /// Records last read older than `known`: rolled back by storage, or not
    /// finished syncing yet.
    stale: Mutex<BTreeSet<Uuid>>,
    /// Records sealed with a key this vault no longer has (written by a
    /// device that hadn't seen a re-key, or by a removed one). Skipped.
    unreadable: Mutex<BTreeSet<Uuid>>,
    /// Records that failed authentication or aren't valid record files, as
    /// of the last [`Vault::list`]. Listing skips them; reading or writing
    /// one still fails, so nothing is written over it.
    damaged: Mutex<BTreeSet<Uuid>>,
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
        let (header, vault_key) = read_unlocking(&*store, |h| h.unlock_with_password(password.as_bytes()))?;
        Vault::unlocked(store, header, vault_key)
    }

    pub fn unlock_with_recovery(store: impl Store + 'static, recovery: &RecoveryKey) -> Result<Vault> {
        let store: Box<dyn Store> = Box::new(store);
        let (header, vault_key) = read_unlocking(&*store, |h| h.unlock_with_recovery(recovery))?;
        Vault::unlocked(store, header, vault_key)
    }

    /// Unlocks with a key from [`Vault::session_key`], so a client can cache it
    /// (for example in the OS keychain) instead of asking for the password on
    /// every use. Fails if the vault was replaced since the key was cached.
    pub fn unlock_with_session_key(store: impl Store + 'static, key: &[u8; KEY_LEN]) -> Result<Vault> {
        let store: Box<dyn Store> = Box::new(store);
        let (header, vault_key) = read_unlocking(&*store, |h| h.unlock_with_key(SecretKey::from_bytes(*key)))?;
        Vault::unlocked(store, header, vault_key)
    }

    /// Unlocks with this device's own key (see [`Vault::enroll_device`]).
    /// Fails with [`Error::DeviceStale`] after a re-key: unlock with the
    /// password, then enroll again.
    pub fn unlock_with_device(store: impl Store + 'static, device_id: Uuid, key: &DeviceKey) -> Result<Vault> {
        let store: Box<dyn Store> = Box::new(store);
        let (header, vault_key) = read_unlocking(&*store, |h| h.unlock_with_device(device_id, key))?;
        Vault::unlocked(store, header, vault_key)
    }

    /// Joins with a code from [`Vault::create_enrollment_code`] on another
    /// device: enrolls this device and uses up the code. Returns the vault
    /// and this device's key, for its keychain.
    pub fn enroll_with_code(
        store: impl Store + 'static,
        code: &EnrollmentCode,
        device_id: Uuid,
        name: &str,
    ) -> Result<(Vault, DeviceKey)> {
        let store: Box<dyn Store> = Box::new(store);
        let (header, (vault_key, slot)) = read_unlocking(&*store, |h| h.unlock_with_enrollment(code, now_ms()))?;
        let mut vault = Vault::unlocked(store, header, vault_key)?;
        let key = vault.update_header(|header, key| {
            header.remove_slot(key, slot)?;
            header.set_device(key, device_id, name, now_ms())
        })?;
        Ok((vault, key))
    }

    /// The raw vault key, for caching in secure OS storage. Anyone holding it
    /// can read the vault, so never write it anywhere else.
    pub fn session_key(&self) -> Zeroizing<[u8; KEY_LEN]> {
        Zeroizing::new(*self.vault_key.as_bytes())
    }

    /// Tags `message` with a subkey of the vault key, in hex. `dotsafe
    /// approve` uses it to prove to the daemon that the person answering
    /// knows the master password; any process can reach the socket, but only
    /// someone who can open the vault can make the tag.
    pub fn approval_tag(&self, message: &[u8]) -> String {
        let key = self.vault_key.derive(APPROVAL_KEY_INFO);
        crypto::mac(&key, message).iter().map(|b| format!("{b:02x}")).collect()
    }

    /// Whether `tag` is [`Vault::approval_tag`] of `message`, in constant time.
    pub fn check_approval_tag(&self, message: &[u8], tag: &str) -> bool {
        let Some(bytes) = decode_hex(tag) else { return false };
        crypto::verify_mac(&self.vault_key.derive(APPROVAL_KEY_INFO), message, &bytes)
    }

    fn open(store: Box<dyn Store>, header: VaultHeader, vault_key: SecretKey) -> Vault {
        let records_key = vault_key.derive(RECORDS_KEY_INFO);
        let previous = header.previous_keys.first().and_then(|p| {
            header.previous_key(&vault_key, p.epoch).map(|key| (p.epoch, key.derive(RECORDS_KEY_INFO)))
        });
        Vault {
            store,
            header,
            vault_key,
            records_key,
            previous,
            clock: Clock::default(),
            known: Mutex::default(),
            stale: Mutex::default(),
            unreadable: Mutex::default(),
            damaged: Mutex::default(),
        }
    }

    /// Opens an unlocked vault, upgrading an older format and finishing an
    /// interrupted re-key first.
    fn unlocked(store: Box<dyn Store>, header: VaultHeader, vault_key: SecretKey) -> Result<Vault> {
        let mut vault = Vault::open(store, header, vault_key);
        vault.merge_header_copies()?;
        if vault.header.version < HEADER_VERSION {
            vault.upgrade()?;
        }
        if !vault.header.previous_keys.is_empty() {
            vault.finish_rekey()?;
        }
        Ok(vault)
    }

    /// Upgrades an older vault in place. v1 records are re-sealed first and
    /// the header goes last, so an interrupted upgrade just runs again.
    fn upgrade(&mut self) -> Result<()> {
        if self.header.version == 1 {
            for record in self.list()? {
                self.put(record)?;
            }
        }
        self.update_header(|header, key| header.upgrade(key))
    }

    /// Merges sync-conflict copies of `vault.json` into it and deletes them.
    /// Copies that don't verify with this vault's key are left alone, except
    /// ones from an earlier epoch, which a re-key made obsolete.
    fn merge_header_copies(&mut self) -> Result<()> {
        // Only write when there's something to merge or clean up.
        if self.mergeable_copies(&self.header)?.1.is_empty() {
            return Ok(());
        }
        // `update_header` folds the copies in on every write.
        self.update_header(|_, _| Ok(()))
    }

    /// The conflict copies of `vault.json` that can be merged into `header`
    /// (same vault and epoch, verified with this vault's key), and every copy
    /// that can be deleted once it's written: those, plus copies from an
    /// earlier epoch.
    fn mergeable_copies(&self, header: &VaultHeader) -> Result<(Vec<VaultHeader>, Vec<String>)> {
        let (mut merge, mut done) = (Vec::new(), Vec::new());
        for key in header_copies(&*self.store)? {
            let Some(object) = self.store.get(&key)? else { continue };
            let Ok(copy) = serde_json::from_slice::<VaultHeader>(&object.bytes) else { continue };
            if copy.check_version().is_err() || copy.vault_id != header.vault_id {
                continue;
            }
            match copy.current_epoch().cmp(&header.current_epoch()) {
                std::cmp::Ordering::Less => done.push(key),
                // Another device re-keyed; this key can't check it.
                std::cmp::Ordering::Greater => {}
                std::cmp::Ordering::Equal => {
                    if copy.unlock_with_key(self.vault_key.clone()).is_ok() {
                        merge.push(copy);
                        done.push(key);
                    }
                }
            }
        }
        Ok((merge, done))
    }

    pub fn id(&self) -> Uuid {
        self.header.vault_id
    }

    /// Where the vault is stored, for messages.
    pub fn location(&self) -> String {
        self.store.location()
    }

    /// The store underneath, holding only encrypted objects. For copying the
    /// vault whole (backups, moving it) without opening the store twice.
    pub fn store(&self) -> &dyn Store {
        &*self.store
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
        self.update_header(|header, key| header.set_password(key, new_password.as_bytes(), params))
    }

    /// Creates a recovery key, replacing any previous one. Show it to the user
    /// once; it can't be retrieved later.
    pub fn reset_recovery_key(&mut self) -> Result<RecoveryKey> {
        self.update_header(|header, key| header.set_recovery(key))
    }

    /// Enrolled devices.
    pub fn devices(&self) -> Vec<DeviceInfo> {
        self.header.devices()
    }

    /// Lets this device unlock with its own key (behind Touch ID), replacing
    /// any slot it had. Keep the returned key in the device's keychain only.
    pub fn enroll_device(&mut self, device_id: Uuid, name: &str) -> Result<DeviceKey> {
        self.update_header(|header, key| header.set_device(key, device_id, name, now_ms()))
    }

    /// Removes a device's slot without re-keying: for turning off Touch ID
    /// unlock on this device. To shut out a lost device, use
    /// [`Vault::revoke_device`]. Returns whether it had a slot.
    pub fn forget_device(&mut self, device_id: Uuid) -> Result<bool> {
        self.update_header(|header, key| header.remove_device(key, device_id))
    }

    /// A one-time code another device can use, for ten minutes, to join
    /// without the password.
    pub fn create_enrollment_code(&mut self) -> Result<EnrollmentCode> {
        self.update_header(|header, key| header.add_enrollment(key, now_ms()))
    }

    /// Shuts a device out: removes its slot and switches the vault to a new
    /// key, re-sealing every record, so anything it still holds can't read
    /// what's written from now on. Needs the password, which is set again for
    /// the new key; the recovery key is replaced, and the new one returned to
    /// show once. Other devices' Touch ID unlock needs the password once.
    pub fn revoke_device(&mut self, device_id: Uuid, password: &str, params: KdfParams) -> Result<RecoveryKey> {
        self.check_password(password)?;
        let new_key = SecretKey::generate()?;
        let old_key = self.vault_key.clone();
        let old_epoch = self.epoch();
        let recovery = self.update_header(|header, key| {
            header.remove_device(key, device_id)?;
            header.rekey(key, &new_key, password.as_bytes(), params)
        })?;
        self.previous = Some((old_epoch, old_key.derive(RECORDS_KEY_INFO)));
        self.records_key = new_key.derive(RECORDS_KEY_INFO);
        self.vault_key = new_key;
        self.finish_rekey()?;
        Ok(recovery)
    }

    /// Re-seals every record (and the head) under the current key, then drops
    /// the previous key from the header. Safe to run again if interrupted.
    fn finish_rekey(&mut self) -> Result<()> {
        for (id, keys) in self.record_keys()? {
            if let Some(record) = self.read_record_group(id, &keys)?.value {
                self.put(record)?;
            }
        }
        self.compact_head()?;
        self.update_header(|header, key| header.drop_previous_keys(key))?;
        self.previous = None;
        Ok(())
    }

    /// Records skipped because they're sealed with a key this vault no longer
    /// has.
    pub fn unreadable_records(&self) -> Vec<Uuid> {
        lock(&self.unreadable).iter().copied().collect()
    }

    /// Records the last [`Vault::list`] skipped because they failed
    /// authentication or weren't valid: damaged in sync, or changed outside
    /// dotsafe.
    pub fn damaged_records(&self) -> Vec<Uuid> {
        lock(&self.damaged).iter().copied().collect()
    }

    /// Applies `change` to the newest header and writes it only if nobody
    /// else changed it meanwhile; otherwise re-reads and applies it again, so
    /// two devices changing the header at once both keep their change. (On
    /// synced folders the check can't be atomic; see docs/vault-format.md.)
    fn update_header<T>(&mut self, mut change: impl FnMut(&mut VaultHeader, &SecretKey) -> Result<T>) -> Result<T> {
        for _ in 0..MAX_ATTEMPTS {
            let object = self.store.get(HEADER_KEY)?.ok_or_else(|| Error::NotFound(self.location()))?;
            let mut header: VaultHeader = serde_json::from_slice(&object.bytes)?;
            header.check_version()?;
            if header.vault_id != self.header.vault_id {
                return Err(Error::Format("the vault at this location was replaced".into()));
            }
            // Another device may have re-keyed: then our key no longer fits.
            header
                .unlock_with_key(self.vault_key.clone())
                .map_err(|_| Error::Invalid("the vault's key changed on another device; lock and unlock again".into()))?;
            header.prune_enrollments(now_ms());
            let (copies, done) = self.mergeable_copies(&header)?;
            if !copies.is_empty() {
                header.merge_copies(&self.vault_key, &copies)?;
            }
            let out = change(&mut header, &self.vault_key)?;
            let bytes = serde_json::to_vec_pretty(&header)?;
            if let Put::Written(_) = self.store.put(HEADER_KEY, &bytes, Cond::IfMatch(object.etag))? {
                self.header = header;
                for key in &done {
                    let _ = self.store.delete(key);
                }
                return Ok(out);
            }
        }
        Err(Error::Busy(self.header.vault_id))
    }

    /// Reads one record, merging any sync-conflict copies of it.
    pub fn get(&self, id: Uuid) -> Result<Option<Record>> {
        self.refresh_head()?;
        let keys = self.record_keys()?.remove(&id).unwrap_or_default();
        Ok(self.read_record_group(id, &keys)?.value)
    }

    /// The vault's live shared secrets, for resolving linked variables.
    pub fn secrets(&self) -> Result<Secrets> {
        Ok(Secrets::from_records(&self.list()?))
    }

    /// Reads every record, merging any sync-conflict copies. A damaged
    /// record is skipped and noted ([`Vault::damaged_records`]), so one bad
    /// file doesn't hide the rest of the vault.
    pub fn list(&self) -> Result<Vec<Record>> {
        self.refresh_head()?;
        let mut records = Vec::new();
        let mut damaged = BTreeSet::new();
        for (id, keys) in self.record_keys()? {
            match self.read_record_group(id, &keys) {
                Ok(group) => records.extend(group.value),
                Err(Error::RecordTampered(_) | Error::Json(_) | Error::Format(_)) => {
                    damaged.insert(id);
                }
                Err(e) => return Err(e),
            }
        }
        *lock(&self.damaged) = damaged;
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
        let version = record.min_version();
        let plaintext = padded(serde_json::to_vec(record)?);
        let epoch = self.epoch();
        let file = RecordFile {
            format: RECORD_FORMAT.into(),
            version,
            id,
            epoch,
            sealed: crypto::seal(&self.records_key, &plaintext, &self.record_aad(version, epoch, id))?,
        };
        Ok(serde_json::to_vec_pretty(&file)?)
    }

    fn read_record_group(&self, id: Uuid, keys: &[String]) -> Result<Group<Record>> {
        let mut group = Group { value: None::<Record>, etag: None, copies: Vec::new() };
        for key in keys {
            let Some(object) = self.store.get(key)? else { continue };
            let record = match self.open_record(id, key, &object.bytes) {
                Ok(record) => record,
                Err(Error::UnknownEpoch(_)) => {
                    lock(&self.unreadable).insert(id);
                    continue;
                }
                Err(e) => return Err(e),
            };
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
        // v1 records have no epoch; they were sealed in epoch 1.
        let epoch = file.epoch.max(1);
        let records_key = self.records_key_for(epoch).ok_or(Error::UnknownEpoch(id))?;
        let aad = match file.version {
            1 => [RECORD_AAD_V1, self.header.vault_id.as_bytes(), id.as_bytes()].concat(),
            2..=RECORD_VERSION_MAX => self.record_aad(file.version, epoch, id),
            version => return Err(Error::UnsupportedVersion { kind: "record", version }),
        };
        let plaintext = crypto::open(records_key, &file.sealed, &aad).ok_or(Error::RecordTampered(id))?;
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
            epoch: self.epoch(),
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
            if key == HEAD_KEY {
                group.etag = Some(object.etag);
            } else {
                group.copies.push(key.clone());
            }
            // A head sealed with a key we no longer have is simply replaced
            // on the next write.
            let Some(head_key) = self.records_key_for(file.epoch.max(1)) else { continue };
            let plaintext = crypto::open(head_key, &file.sealed, &self.head_aad()).ok_or(Error::HeadTampered)?;
            let head: Head = serde_json::from_slice(&plaintext)?;
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
        self.header.current_epoch()
    }

    /// The records key for `epoch`: the current one, or the previous one
    /// while a re-key is re-sealing.
    fn records_key_for(&self, epoch: u32) -> Option<&SecretKey> {
        match &self.previous {
            _ if epoch == self.epoch() => Some(&self.records_key),
            Some((previous, key)) if *previous == epoch => Some(key),
            _ => None,
        }
    }

    /// Binds a record to its vault, key epoch, id and format version, so it
    /// can't be moved, or relabelled as a version older clients would read.
    fn record_aad(&self, version: u32, epoch: u32, id: Uuid) -> Vec<u8> {
        let label = match version {
            3 => RECORD_AAD_V3,
            4 => RECORD_AAD_V4,
            5 => RECORD_AAD_V5,
            _ => RECORD_AAD,
        };
        [label, self.header.vault_id.as_bytes(), &epoch.to_be_bytes(), id.as_bytes()].concat()
    }

    fn head_aad(&self) -> Vec<u8> {
        [HEAD_AAD, self.header.vault_id.as_bytes()].concat()
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

fn record_key(id: Uuid) -> String {
    format!("{RECORDS_DIR}/{id}.json")
}

/// Sync-conflict copies of `vault.json` (`vault 2.json`, `vault (1).json`,
/// `vault (conflicted copy …).json`).
fn header_copies(store: &dyn Store) -> Result<Vec<String>> {
    Ok(store
        .list("")?
        .into_iter()
        .filter(|k| k != HEADER_KEY && k.starts_with("vault") && k.ends_with(".json") && !k.contains('/'))
        .collect())
}

/// Reads the header and unlocks it with `unlock`. If the credential doesn't
/// fit, tries the header's conflict copies too: a password changed on
/// another device may so far exist only in a copy.
fn read_unlocking<T>(store: &dyn Store, unlock: impl Fn(&VaultHeader) -> Result<T>) -> Result<(VaultHeader, T)> {
    let header = read_header(store)?;
    let first = match unlock(&header) {
        Ok(found) => return Ok((header, found)),
        Err(e) => e,
    };
    if !matches!(first, Error::InvalidCredentials | Error::DeviceStale) {
        return Err(first);
    }
    for key in header_copies(store)? {
        let Some(object) = store.get(&key)? else { continue };
        let Ok(copy) = serde_json::from_slice::<VaultHeader>(&object.bytes) else { continue };
        if copy.check_version().is_err() || copy.vault_id != header.vault_id {
            continue;
        }
        if let Ok(found) = unlock(&copy) {
            return Ok((copy, found));
        }
    }
    Err(first)
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
