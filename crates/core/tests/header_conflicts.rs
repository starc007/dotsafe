//! Sync-conflict copies of `vault.json`: two devices change the header at
//! once on a synced folder, and the sync client keeps one version as
//! `vault.json` and the other as `vault 2.json`.

use std::fs;
use std::path::Path;

use dotsafe_core::{Error, FolderStore, KdfParams, Vault};
use uuid::Uuid;

// Cheap Argon2 settings so tests run fast. Never use these for real vaults.
const FAST: KdfParams = KdfParams { m_cost_kib: 64, t_cost: 1, p_cost: 1 };

fn store(dir: &Path) -> FolderStore {
    FolderStore::new(dir)
}

/// Copies a vault folder: a second device's view of the same synced folder,
/// before the sync client catches up.
fn clone_vault(from: &Path) -> tempfile::TempDir {
    let to = tempfile::tempdir().unwrap();
    fs::create_dir_all(to.path().join("records")).unwrap();
    for entry in walk(from) {
        let rel = entry.strip_prefix(from).unwrap();
        fs::copy(&entry, to.path().join(rel)).unwrap();
    }
    to
}

fn walk(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() { out.extend(walk(&path)) } else { out.push(path) }
    }
    out
}

/// Puts the other device's header next to this one, as a conflict copy.
fn conflict_copy(from: &Path, into: &Path, name: &str) {
    fs::copy(from.join("vault.json"), into.join(name)).unwrap();
}

#[test]
fn a_password_change_and_a_new_device_both_survive() {
    let dir = tempfile::tempdir().unwrap();
    Vault::create(store(dir.path()), "pw-original", FAST).unwrap();
    let other = clone_vault(dir.path());

    // A changes the password; B enrolls a device, each on its own copy.
    Vault::unlock(store(dir.path()), "pw-original").unwrap().change_password("pw-new", FAST).unwrap();
    let laptop = Uuid::new_v4();
    let key = Vault::unlock(store(other.path()), "pw-original").unwrap().enroll_device(laptop, "Laptop").unwrap();
    conflict_copy(other.path(), dir.path(), "vault 2.json");

    let merged = Vault::unlock(store(dir.path()), "pw-new").unwrap();
    assert!(!dir.path().join("vault 2.json").exists(), "the copy is merged and removed");
    assert!(merged.devices().iter().any(|d| d.device_id == laptop));
    assert!(Vault::unlock_with_device(store(dir.path()), laptop, &key).is_ok(), "B's device works");
    // The replaced password doesn't come back from B's copy.
    assert!(matches!(Vault::unlock(store(dir.path()), "pw-original"), Err(Error::InvalidCredentials)));
}

#[test]
fn a_password_only_in_a_copy_still_unlocks_and_both_new_ones_work() {
    let dir = tempfile::tempdir().unwrap();
    Vault::create(store(dir.path()), "pw-original", FAST).unwrap();
    let other = clone_vault(dir.path());
    Vault::unlock(store(dir.path()), "pw-original").unwrap().change_password("pw-a", FAST).unwrap();
    Vault::unlock(store(other.path()), "pw-original").unwrap().change_password("pw-b", FAST).unwrap();
    conflict_copy(other.path(), dir.path(), "vault (conflicted copy 2026-10-27).json");

    // B's password is only in the copy so far.
    Vault::unlock(store(dir.path()), "pw-b").unwrap();
    assert!(Vault::unlock(store(dir.path()), "pw-a").is_ok(), "both changes were made by the user; both work");
    assert!(Vault::unlock(store(dir.path()), "pw-b").is_ok());
    assert!(Vault::unlock(store(dir.path()), "pw-original").is_err());

    // The next change replaces both.
    Vault::unlock(store(dir.path()), "pw-a").unwrap().change_password("pw-final", FAST).unwrap();
    assert!(Vault::unlock(store(dir.path()), "pw-a").is_err());
    assert!(Vault::unlock(store(dir.path()), "pw-b").is_err());
    assert!(Vault::unlock(store(dir.path()), "pw-final").is_ok());
}

#[test]
fn copies_that_dont_verify_are_left_alone() {
    let dir = tempfile::tempdir().unwrap();
    Vault::create(store(dir.path()), "pw", FAST).unwrap();
    // A copy someone without the vault key edited.
    let mut forged: serde_json::Value = serde_json::from_slice(&fs::read(dir.path().join("vault.json")).unwrap()).unwrap();
    forged["key_slots"] = serde_json::json!([]);
    fs::write(dir.path().join("vault 2.json"), serde_json::to_vec(&forged).unwrap()).unwrap();

    let vault = Vault::unlock(store(dir.path()), "pw").unwrap();
    assert!(dir.path().join("vault 2.json").exists(), "not merged, not deleted");
    drop(vault);
    assert!(Vault::unlock(store(dir.path()), "pw").is_ok(), "the forged copy changed nothing");
}

#[test]
fn copies_from_before_a_re_key_are_dropped() {
    let dir = tempfile::tempdir().unwrap();
    let mut vault = Vault::create(store(dir.path()), "pw", FAST).unwrap();
    let laptop = Uuid::new_v4();
    vault.enroll_device(laptop, "Laptop").unwrap();
    let before = clone_vault(dir.path());
    vault.revoke_device(laptop, "pw", FAST).unwrap();
    conflict_copy(before.path(), dir.path(), "vault 2.json");

    let reopened = Vault::unlock(store(dir.path()), "pw").unwrap();
    assert!(!dir.path().join("vault 2.json").exists(), "an old epoch's copy is obsolete");
    assert!(reopened.devices().is_empty(), "the removed device doesn't come back");
}
