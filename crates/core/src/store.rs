//! Where a vault's objects live. A store holds opaque, already-encrypted bytes
//! under keys like `vault.json` and `records/<uuid>.json`; it's never trusted
//! with anything readable. Merging, integrity and access control live in
//! [`crate::Vault`].
//!
//! Writes are conditional, so two devices writing the same object can't
//! silently overwrite each other: the loser gets [`Put::Conflict`], re-reads,
//! merges and tries again.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::error::Result;

/// Identifies one version of an object. Opaque to callers.
pub type ETag = String;

pub struct Object {
    pub bytes: Vec<u8>,
    pub etag: ETag,
}

/// An object's key and current version, from a listing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjectMeta {
    pub key: String,
    pub etag: ETag,
}

/// The condition a write must meet to go ahead.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Cond {
    Any,
    /// Only if the object doesn't exist yet.
    IfAbsent,
    /// Only if the object is still at this version.
    IfMatch(ETag),
}

#[derive(Debug, PartialEq, Eq)]
pub enum Put {
    Written(ETag),
    /// Someone else wrote first; re-read and try again.
    Conflict,
}

pub trait Store: Send + Sync {
    /// Where the store points, for messages ("/Users/me/iCloud/dotsafe").
    fn location(&self) -> String;
    /// Keys of the objects directly inside `dir` (`""` for the top level),
    /// sorted. Hidden names (starting with `.`) are skipped.
    fn list(&self, dir: &str) -> Result<Vec<String>>;
    fn get(&self, key: &str) -> Result<Option<Object>>;
    fn put(&self, key: &str, bytes: &[u8], cond: Cond) -> Result<Put>;
    /// Removes an object if it exists.
    fn delete(&self, key: &str) -> Result<()>;

    /// Like [`Store::list`], with each object's version. Stores that can list
    /// versions cheaply (S3) override this; the default reads every object.
    fn list_meta(&self, dir: &str) -> Result<Vec<ObjectMeta>> {
        let mut metas = Vec::new();
        for key in self.list(dir)? {
            if let Some(object) = self.get(&key)? {
                metas.push(ObjectMeta { key, etag: object.etag });
            }
        }
        Ok(metas)
    }

    /// Exchanges changes with the remote, for stores that keep a local copy
    /// ([`crate::CachedStore`]), and returns whether anything came in. Fails
    /// with [`crate::Error::Unreachable`] when offline. A no-op for everything
    /// else.
    fn sync(&self) -> Result<bool> {
        Ok(false)
    }
}

impl Store for Box<dyn Store> {
    fn location(&self) -> String {
        (**self).location()
    }
    fn list(&self, dir: &str) -> Result<Vec<String>> {
        (**self).list(dir)
    }
    fn get(&self, key: &str) -> Result<Option<Object>> {
        (**self).get(key)
    }
    fn put(&self, key: &str, bytes: &[u8], cond: Cond) -> Result<Put> {
        (**self).put(key, bytes, cond)
    }
    fn delete(&self, key: &str) -> Result<()> {
        (**self).delete(key)
    }
    fn list_meta(&self, dir: &str) -> Result<Vec<ObjectMeta>> {
        (**self).list_meta(dir)
    }
    fn sync(&self) -> Result<bool> {
        (**self).sync()
    }
}

/// A vault in a local folder, usually one a sync client mirrors (iCloud Drive,
/// Google Drive, Dropbox, ...).
///
/// Conditional writes are best effort here: the check and the rename aren't
/// atomic, and a sync client can bring in another device's version at any
/// time. That's fine, because the vault merges whatever it finds, including
/// the conflict copies sync clients leave behind.
#[derive(Clone, Debug)]
pub struct FolderStore {
    root: PathBuf,
}

impl FolderStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        FolderStore { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn path(&self, key: &str) -> PathBuf {
        key.split('/').fold(self.root.clone(), |p, part| p.join(part))
    }
}

impl Store for FolderStore {
    fn location(&self) -> String {
        self.root.display().to_string()
    }

    fn list(&self, dir: &str) -> Result<Vec<String>> {
        let path = if dir.is_empty() { self.root.clone() } else { self.path(dir) };
        let entries = match fs::read_dir(&path) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e.into()),
        };
        let mut keys = Vec::new();
        for entry in entries {
            let entry = entry?;
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else { continue };
            if name.starts_with('.') || !entry.file_type()?.is_file() {
                continue;
            }
            keys.push(if dir.is_empty() { name } else { format!("{dir}/{name}") });
        }
        keys.sort();
        Ok(keys)
    }

    fn get(&self, key: &str) -> Result<Option<Object>> {
        match fs::read(self.path(key)) {
            Ok(bytes) => Ok(Some(Object { etag: etag(&bytes), bytes })),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    fn put(&self, key: &str, bytes: &[u8], cond: Cond) -> Result<Put> {
        let current = match cond {
            Cond::Any => None,
            _ => self.get(key)?.map(|o| o.etag),
        };
        let allowed = match &cond {
            Cond::Any => true,
            Cond::IfAbsent => current.is_none(),
            Cond::IfMatch(expected) => current.as_ref() == Some(expected),
        };
        if !allowed {
            return Ok(Put::Conflict);
        }
        let path = self.path(key);
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        write_atomic(&path, bytes)?;
        Ok(Put::Written(etag(bytes)))
    }

    fn delete(&self, key: &str) -> Result<()> {
        match fs::remove_file(self.path(key)) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
            _ => Ok(()),
        }
    }
}

pub fn etag(bytes: &[u8]) -> ETag {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

/// Writes via a hidden temp file and rename, so sync clients and readers never
/// see a half-written file.
pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let dir = path.parent().unwrap_or(Path::new("."));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conditional_writes() {
        let dir = tempfile::tempdir().unwrap();
        let store = FolderStore::new(dir.path());
        let Put::Written(v1) = store.put("records/a.json", b"one", Cond::IfAbsent).unwrap() else { panic!() };
        assert_eq!(store.put("records/a.json", b"two", Cond::IfAbsent).unwrap(), Put::Conflict);
        assert_eq!(store.put("records/a.json", b"two", Cond::IfMatch("stale".into())).unwrap(), Put::Conflict);
        assert!(matches!(store.put("records/a.json", b"two", Cond::IfMatch(v1)).unwrap(), Put::Written(_)));
        assert_eq!(store.get("records/a.json").unwrap().unwrap().bytes, b"two");
    }

    #[test]
    fn lists_visible_files_only() {
        let dir = tempfile::tempdir().unwrap();
        let store = FolderStore::new(dir.path());
        store.put("records/b.json", b"", Cond::Any).unwrap();
        store.put("records/a.json", b"", Cond::Any).unwrap();
        fs::write(dir.path().join("records/.DS_Store"), b"").unwrap();
        fs::create_dir(dir.path().join("records/sub")).unwrap();
        assert_eq!(store.list("records").unwrap(), ["records/a.json", "records/b.json"]);
        assert!(store.list("missing").unwrap().is_empty());
        assert_eq!(store.list("").unwrap(), Vec::<String>::new());
    }

    #[test]
    fn missing_objects() {
        let dir = tempfile::tempdir().unwrap();
        let store = FolderStore::new(dir.path());
        assert!(store.get("vault.json").unwrap().is_none());
        store.delete("vault.json").unwrap();
    }
}
