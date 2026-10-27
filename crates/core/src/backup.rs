//! Copies of a vault: dated backup snapshots, and moving a vault to another
//! store. Everything copied is already encrypted, so a backup folder or a new
//! location never sees anything readable.
//!
//! A snapshot is a complete vault folder, so restoring one is just
//! connecting dotsafe to it (and moving it back, if wanted).

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::store::{Cond, FolderStore, Store};

/// How many snapshots a backup folder keeps; older ones are deleted.
pub const KEEP: usize = 10;
const PREFIX: &str = "dotsafe-backup-";

/// Copies every object (`vault.json`, `head.json`, `records/…`) from `src`
/// to `dst`. Returns how many.
pub fn copy_all(src: &dyn Store, dst: &dyn Store) -> Result<usize> {
    let mut copied = 0;
    for dir in ["", "records"] {
        for key in src.list(dir)? {
            if let Some(object) = src.get(&key)? {
                dst.put(&key, &object.bytes, Cond::Any)?;
                copied += 1;
            }
        }
    }
    Ok(copied)
}

/// Moves a vault: copies everything from `src` to `dst`, which must not hold
/// a vault yet. The caller checks the copy opens, then switches to it; `src`
/// is left as it was.
pub fn copy_to_empty(src: &dyn Store, dst: &dyn Store) -> Result<usize> {
    if dst.get("vault.json")?.is_some() {
        return Err(Error::AlreadyExists(dst.location()));
    }
    copy_all(src, dst)
}

/// Writes a dated snapshot of `src` into `dir` and deletes the oldest beyond
/// [`KEEP`]. `stamp` names it, e.g. `2026-10-27-1430`.
pub fn snapshot(src: &dyn Store, dir: &Path, stamp: &str) -> Result<PathBuf> {
    let path = dir.join(format!("{PREFIX}{stamp}"));
    // A second backup in the same minute replaces the first.
    if path.exists() {
        fs::remove_dir_all(&path)?;
    }
    fs::create_dir_all(path.join("records"))?;
    copy_all(src, &FolderStore::new(&path))?;
    let mut snapshots = snapshots(dir)?;
    while snapshots.len() > KEEP {
        let oldest = snapshots.remove(0);
        fs::remove_dir_all(oldest)?;
    }
    Ok(path)
}

/// Snapshots in `dir`, oldest first.
pub fn snapshots(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut found: Vec<PathBuf> = match fs::read_dir(dir) {
        Ok(entries) => entries
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with(PREFIX) && e.path().join("vault.json").is_file())
            .map(|e| e.path())
            .collect(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(e) => return Err(e.into()),
    };
    found.sort();
    Ok(found)
}

/// Unix milliseconds as `YYYY-MM-DD-HHMM` (UTC), for snapshot names.
pub fn stamp(ms: u64) -> String {
    let secs = ms / 1000;
    let (days, rem) = ((secs / 86_400) as i64, secs % 86_400);
    // Days since 1970-01-01 to a civil date (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}-{:02}{:02}", rem / 3600, rem % 3600 / 60)
}
