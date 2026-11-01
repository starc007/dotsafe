//! A local mirror of a remote store (S3, R2, ...), so a vault opens fast and
//! keeps working offline. It holds the same ciphertext the remote does, never
//! anything decrypted.
//!
//! Reads always come from the mirror. Writes go to the remote first, with the
//! remote's version as the condition; when the remote can't be reached they're
//! kept locally as unpushed changes and pushed by [`Store::sync`].
//!
//! When the remote changed under an unpushed local change, the remote version
//! is kept next to it as a copy (`<uuid> (remote).json`), the same shape sync
//! clients give conflict copies. The vault already merges those, so both
//! sides' changes survive.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::store::{Cond, ETag, FolderStore, Object, ObjectMeta, Put, Store, etag, write_atomic};

const INDEX_FILE: &str = "index.json";
const OBJECTS_DIR: &str = "objects";
/// Directories of the vault layout to mirror.
const DIRS: [&str; 2] = ["", "records"];
const HEAD_KEY: &str = "head.json";

#[derive(Debug, Default, Serialize, Deserialize)]
struct Index {
    entries: BTreeMap<String, Entry>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Entry {
    /// What `get` reports: the remote version when in step with it, a local
    /// one when there are unpushed changes.
    etag: ETag,
    /// The remote version the local bytes are based on. `None` if the object
    /// isn't on the remote yet.
    base: Option<ETag>,
    /// Changed locally and not pushed yet.
    dirty: bool,
    /// The last write didn't care about the remote's version (`Cond::Any`),
    /// so neither does its push.
    #[serde(default)]
    overwrite: bool,
    /// A copy kept only here, for the vault to merge and then delete.
    #[serde(default)]
    local_only: bool,
}

pub struct CachedStore {
    remote: Box<dyn Store>,
    dir: PathBuf,
    local: FolderStore,
    index: Mutex<Index>,
}

impl CachedStore {
    /// Opens the mirror in `dir` and brings it up to date if the remote can be
    /// reached. Offline, it carries on with what's cached, unless nothing is.
    pub fn open(remote: impl Store + 'static, dir: impl Into<PathBuf>) -> Result<CachedStore> {
        let dir = dir.into();
        let index = match fs::read(dir.join(INDEX_FILE)) {
            Ok(bytes) => serde_json::from_slice(&bytes)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Index::default(),
            Err(e) => return Err(e.into()),
        };
        let store = CachedStore {
            remote: Box::new(remote),
            local: FolderStore::new(dir.join(OBJECTS_DIR)),
            dir,
            index: Mutex::new(index),
        };
        let synced = store.sync();
        let cached = !store.lock().entries.is_empty();
        match synced {
            Err(Error::Unreachable(_)) if cached => Ok(store),
            Err(e) => Err(e),
            Ok(_) => Ok(store),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Index> {
        self.index.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn save(&self, index: &Index) -> Result<()> {
        fs::create_dir_all(&self.dir)?;
        write_atomic(&self.dir.join(INDEX_FILE), &serde_json::to_vec(index)?)
    }

    fn store_clean(&self, index: &mut Index, key: &str, object: Object) -> Result<()> {
        self.local.put(key, &object.bytes, Cond::Any)?;
        let entry = Entry {
            etag: object.etag.clone(),
            base: Some(object.etag),
            dirty: false,
            overwrite: false,
            local_only: false,
        };
        index.entries.insert(key.to_owned(), entry);
        Ok(())
    }

    /// Keeps `object` as a local-only copy of `key` for the vault to merge.
    fn store_copy(&self, index: &mut Index, key: &str, suffix: &str, object: Object) -> Result<()> {
        let copy = copy_key(key, suffix);
        self.local.put(&copy, &object.bytes, Cond::Any)?;
        let entry = Entry { etag: object.etag, base: None, dirty: false, overwrite: false, local_only: true };
        index.entries.insert(copy, entry);
        Ok(())
    }

    /// The remote has a newer version of `key` than the one ours is based on.
    /// A clean entry is simply replaced; unpushed local bytes are kept, with
    /// the remote version as a copy, and the next push targets that version.
    fn apply_remote(&self, index: &mut Index, key: &str, remote: Option<Object>) -> Result<()> {
        let dirty = index.entries.get(key).is_some_and(|e| e.dirty);
        match remote {
            Some(object) if dirty => {
                let remote_etag = object.etag.clone();
                self.store_copy(index, key, "remote", object)?;
                if let Some(entry) = index.entries.get_mut(key) {
                    entry.base = Some(remote_etag);
                }
            }
            Some(object) => self.store_clean(index, key, object)?,
            None => {
                if let Some(entry) = index.entries.get_mut(key) {
                    entry.base = None;
                }
            }
        }
        Ok(())
    }

    // Push and pull talk to the remote without holding the index lock, so a
    // slow or unreachable remote never blocks reads. Results are applied only
    // if the entry didn't change in the meantime.

    fn push(&self) -> Result<()> {
        let pending: Vec<(String, Entry, Vec<u8>)> = {
            let index = self.lock();
            let mut pending = Vec::new();
            for (key, entry) in index.entries.iter().filter(|(_, e)| e.dirty && !e.local_only) {
                // An unmerged remote copy means the remote moved on: pushing
                // now would overwrite its changes. The vault merges first.
                if index.entries.contains_key(&copy_key(key, "remote")) {
                    continue;
                }
                if let Some(object) = self.local.get(key)? {
                    pending.push((key.clone(), entry.clone(), object.bytes));
                }
            }
            pending
        };
        for (key, entry, bytes) in pending {
            let cond = match (&entry.base, entry.overwrite) {
                (_, true) => Cond::Any,
                (Some(base), false) => Cond::IfMatch(base.clone()),
                (None, false) => Cond::IfAbsent,
            };
            let outcome = self.remote.put(&key, &bytes, cond)?;
            let latest = match outcome {
                Put::Conflict => self.remote.get(&key)?,
                Put::Written(_) => None,
            };
            let mut index = self.lock();
            if index.entries.get(&key).map(|e| &e.etag) != Some(&entry.etag) {
                continue;
            }
            match outcome {
                Put::Written(remote_etag) => {
                    let clean = Entry { etag: remote_etag.clone(), base: Some(remote_etag), dirty: false, ..entry };
                    index.entries.insert(key, clean);
                }
                Put::Conflict => self.apply_remote(&mut index, &key, latest)?,
            }
            self.save(&index)?;
        }
        Ok(())
    }

    /// Returns whether anything came in.
    fn pull(&self) -> Result<bool> {
        let mut changed_any = false;
        for dir in DIRS {
            let remote: BTreeMap<String, ETag> =
                self.remote.list_meta(dir)?.into_iter().map(|ObjectMeta { key, etag }| (key, etag)).collect();
            let changed: Vec<String> = {
                let index = self.lock();
                remote
                    .iter()
                    .filter(|(key, etag)| index.entries.get(*key).and_then(|e| e.base.as_ref()) != Some(*etag))
                    .map(|(key, _)| key.clone())
                    .collect()
            };
            let mut fetched = Vec::new();
            for key in changed {
                if let Some(object) = self.remote.get(&key)? {
                    fetched.push((key, object));
                }
            }

            let mut index = self.lock();
            changed_any |= !fetched.is_empty();
            for (key, object) in fetched {
                let current = index.entries.get(&key).cloned();
                if current.as_ref().and_then(|e| e.base.as_ref()) == Some(&object.etag) {
                    continue;
                }
                // head.json only ever grows. Keep the version this device saw
                // next to the new one, so a rolled-back head can't erase what
                // the vault already knew.
                if key == HEAD_KEY
                    && let Some(entry) = current.filter(|e| !e.dirty)
                    && let Some(old) = self.local.get(&key)?
                {
                    self.store_copy(&mut index, &key, "cached", Object { bytes: old.bytes, etag: entry.etag })?;
                }
                self.apply_remote(&mut index, &key, Some(object))?;
            }
            // Missing from the remote. The vault never deletes these objects
            // (deletes are tombstones inside records), so the bucket was
            // wiped or rolled back: upload the cached copy again rather than
            // losing it here too.
            for (key, entry) in index.entries.iter_mut() {
                if in_dir(key, dir) && !entry.local_only && !remote.contains_key(key) {
                    entry.dirty = true;
                    entry.base = None;
                }
            }
            self.save(&index)?;
        }
        Ok(changed_any)
    }
}

impl Store for CachedStore {
    fn location(&self) -> String {
        self.remote.location()
    }

    fn list(&self, dir: &str) -> Result<Vec<String>> {
        Ok(self.lock().entries.keys().filter(|k| in_dir(k, dir)).cloned().collect())
    }

    fn get(&self, key: &str) -> Result<Option<Object>> {
        let index = self.lock();
        let Some(entry) = index.entries.get(key) else { return Ok(None) };
        Ok(self.local.get(key)?.map(|object| Object { bytes: object.bytes, etag: entry.etag.clone() }))
    }

    fn put(&self, key: &str, bytes: &[u8], cond: Cond) -> Result<Put> {
        let mut index = self.lock();
        let current = index.entries.get(key).cloned();
        let allowed = match &cond {
            Cond::Any => true,
            Cond::IfAbsent => current.is_none(),
            Cond::IfMatch(expected) => current.as_ref().map(|e| &e.etag) == Some(expected),
        };
        if !allowed {
            return Ok(Put::Conflict);
        }
        let overwrite = cond == Cond::Any;
        let base = current.as_ref().and_then(|e| e.base.clone());
        let remote_cond = match (&base, overwrite) {
            (_, true) => Cond::Any,
            (Some(base), false) => Cond::IfMatch(base.clone()),
            (None, false) => Cond::IfAbsent,
        };
        match self.remote.put(key, bytes, remote_cond) {
            Ok(Put::Written(remote_etag)) => {
                self.local.put(key, bytes, Cond::Any)?;
                let entry = Entry {
                    etag: remote_etag.clone(),
                    base: Some(remote_etag.clone()),
                    dirty: false,
                    overwrite: false,
                    local_only: false,
                };
                index.entries.insert(key.to_owned(), entry);
                self.save(&index)?;
                Ok(Put::Written(remote_etag))
            }
            Ok(Put::Conflict) => {
                let latest = self.remote.get(key)?;
                self.apply_remote(&mut index, key, latest)?;
                self.save(&index)?;
                Ok(Put::Conflict)
            }
            Err(Error::Unreachable(_)) => {
                self.local.put(key, bytes, Cond::Any)?;
                let local_etag = format!("local:{}", etag(bytes));
                let entry = Entry { etag: local_etag.clone(), base, dirty: true, overwrite, local_only: false };
                index.entries.insert(key.to_owned(), entry);
                self.save(&index)?;
                Ok(Put::Written(local_etag))
            }
            Err(e) => Err(e),
        }
    }

    fn delete(&self, key: &str) -> Result<()> {
        let mut index = self.lock();
        let local_only = index.entries.get(key).is_some_and(|e| e.local_only);
        if !local_only {
            self.remote.delete(key)?;
        }
        self.local.delete(key)?;
        index.entries.remove(key);
        self.save(&index)
    }

    /// Pushes unpushed changes, then pulls whatever changed remotely, then
    /// re-uploads anything the pull found missing.
    fn has_unpushed(&self) -> bool {
        self.lock().entries.values().any(|e| e.dirty)
    }

    fn sync(&self) -> Result<bool> {
        self.push()?;
        let changed = self.pull()?;
        let missing = self.lock().entries.values().any(|e| e.dirty && e.base.is_none());
        if missing {
            self.push()?;
        }
        Ok(changed)
    }
}

/// Whether `key` sits directly inside `dir`.
fn in_dir(key: &str, dir: &str) -> bool {
    match dir {
        "" => !key.contains('/'),
        dir => key.strip_prefix(dir).and_then(|rest| rest.strip_prefix('/')).is_some_and(|name| !name.contains('/')),
    }
}

/// `records/<uuid>.json` → `records/<uuid> (remote).json`.
fn copy_key(key: &str, suffix: &str) -> String {
    match key.strip_suffix(".json") {
        Some(stem) => format!("{stem} ({suffix}).json"),
        None => format!("{key} ({suffix})"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_and_dirs() {
        assert!(in_dir("vault.json", ""));
        assert!(!in_dir("records/a.json", ""));
        assert!(in_dir("records/a.json", "records"));
        assert!(!in_dir("recordsx/a.json", "records"));
        assert_eq!(copy_key("records/a.json", "remote"), "records/a (remote).json");
        assert_eq!(copy_key("head.json", "cached"), "head (cached).json");
    }
}
