use std::fs;
use std::path::Path;

use dotsafe_core::{Environment, Error, KdfParams, Project, Record, RecoveryKey, Stamp, Vault};
use uuid::Uuid;

// Cheap Argon2 settings so tests run fast. Never use these for real vaults.
const FAST: KdfParams = KdfParams { m_cost_kib: 64, t_cost: 1, p_cost: 1 };

fn stamp(at_ms: u64) -> Stamp {
    Stamp { at_ms, device: Uuid::nil() }
}

fn env_with(project: Uuid, vars: &[(&str, &str)], at: u64) -> Environment {
    let mut env = Environment::new(project, "development", stamp(at));
    for (k, v) in vars {
        env.set(*k, *v, stamp(at));
    }
    env
}

fn record_path(root: &Path, id: Uuid) -> std::path::PathBuf {
    root.join("records").join(format!("{id}.json"))
}

#[test]
fn create_unlock_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let vault = Vault::create(dir.path(), "correct horse", FAST).unwrap();
    let project = Project::new("api", stamp(1));
    let env = env_with(project.id, &[("DATABASE_URL", "postgres://x"), ("API_KEY", "sk-1")], 1);
    vault.put(Record::Project(project.clone())).unwrap();
    vault.put(Record::Environment(env.clone())).unwrap();
    let id = vault.id();
    drop(vault);

    let vault = Vault::unlock(dir.path(), "correct horse").unwrap();
    assert_eq!(vault.id(), id);
    assert_eq!(vault.get(env.id).unwrap(), Some(Record::Environment(env)));
    assert_eq!(vault.list().unwrap().len(), 2);
}

#[test]
fn secrets_are_not_stored_in_plaintext() {
    let dir = tempfile::tempdir().unwrap();
    let vault = Vault::create(dir.path(), "pw", FAST).unwrap();
    let env = env_with(Uuid::new_v4(), &[("STRIPE_SECRET", "sk_live_supersecret")], 1);
    vault.put(Record::Environment(env.clone())).unwrap();
    let raw = fs::read_to_string(record_path(dir.path(), env.id)).unwrap();
    assert!(!raw.contains("sk_live_supersecret"));
    assert!(!raw.contains("STRIPE_SECRET"));
}

#[test]
fn wrong_password_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    Vault::create(dir.path(), "right", FAST).unwrap();
    assert!(matches!(Vault::unlock(dir.path(), "wrong"), Err(Error::InvalidCredentials)));
}

#[test]
fn creating_over_existing_vault_fails() {
    let dir = tempfile::tempdir().unwrap();
    Vault::create(dir.path(), "pw", FAST).unwrap();
    assert!(matches!(Vault::create(dir.path(), "pw", FAST), Err(Error::AlreadyExists(_))));
}

#[test]
fn change_password_keeps_data() {
    let dir = tempfile::tempdir().unwrap();
    let mut vault = Vault::create(dir.path(), "old", FAST).unwrap();
    let env = env_with(Uuid::new_v4(), &[("A", "1")], 1);
    vault.put(Record::Environment(env.clone())).unwrap();
    vault.change_password("new", FAST).unwrap();

    assert!(matches!(Vault::unlock(dir.path(), "old"), Err(Error::InvalidCredentials)));
    let vault = Vault::unlock(dir.path(), "new").unwrap();
    assert_eq!(vault.get(env.id).unwrap(), Some(Record::Environment(env)));
}

#[test]
fn recovery_key_unlocks_and_survives_formatting() {
    let dir = tempfile::tempdir().unwrap();
    let mut vault = Vault::create(dir.path(), "pw", FAST).unwrap();
    let shown = vault.reset_recovery_key().unwrap().to_display_string();
    assert_eq!(shown.len(), 52 + 12, "13 groups of 4 with dashes");

    // Users may retype it lowercase, with spaces, and confuse O/0.
    let typed = shown.to_lowercase().replace('-', " ").replace('0', "o");
    let vault = Vault::unlock_with_recovery(dir.path(), &RecoveryKey::parse(&typed).unwrap()).unwrap();
    assert_eq!(vault.id(), Vault::unlock(dir.path(), "pw").unwrap().id());
}

#[test]
fn resetting_recovery_key_revokes_the_old_one() {
    let dir = tempfile::tempdir().unwrap();
    let mut vault = Vault::create(dir.path(), "pw", FAST).unwrap();
    let old = vault.reset_recovery_key().unwrap();
    vault.reset_recovery_key().unwrap();
    assert!(matches!(Vault::unlock_with_recovery(dir.path(), &old), Err(Error::InvalidCredentials)));
}

#[test]
fn malformed_recovery_key_is_rejected() {
    assert!(RecoveryKey::parse("ABCD-EFGH").is_err());
    assert!(RecoveryKey::parse(&"U".repeat(52)).is_err());
}

#[test]
fn tampered_header_is_detected() {
    let dir = tempfile::tempdir().unwrap();
    Vault::create(dir.path(), "pw", FAST).unwrap();
    let path = dir.path().join("vault.json");
    let mut header: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    header["vault_id"] = serde_json::json!(Uuid::new_v4());
    fs::write(&path, serde_json::to_vec(&header).unwrap()).unwrap();
    // Changing the vault id also breaks the slot's AAD, so the key won't unwrap.
    assert!(Vault::unlock(dir.path(), "pw").is_err());

    // A change that leaves the slot intact is caught by the header MAC.
    let dir = tempfile::tempdir().unwrap();
    Vault::create(dir.path(), "pw", FAST).unwrap();
    let path = dir.path().join("vault.json");
    let mut header: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    header["key_slots"].as_array_mut().unwrap().push(serde_json::json!({
        "id": Uuid::new_v4(),
        "unlock": { "kind": "recovery", "salt": "AAAAAAAAAAAAAAAAAAAAAA==" },
        "wrapped_key": { "nonce": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA", "ct": "AAAA" }
    }));
    fs::write(&path, serde_json::to_vec(&header).unwrap()).unwrap();
    assert!(matches!(Vault::unlock(dir.path(), "pw"), Err(Error::HeaderTampered)));
}

#[test]
fn absurd_kdf_params_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    Vault::create(dir.path(), "pw", FAST).unwrap();
    let path = dir.path().join("vault.json");
    let mut header: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    header["key_slots"][0]["unlock"]["kdf"]["m_cost_kib"] = serde_json::json!(u32::MAX);
    fs::write(&path, serde_json::to_vec(&header).unwrap()).unwrap();
    assert!(matches!(Vault::unlock(dir.path(), "pw"), Err(Error::Format(_))));
}

#[test]
fn tampered_record_is_detected() {
    let dir = tempfile::tempdir().unwrap();
    let vault = Vault::create(dir.path(), "pw", FAST).unwrap();
    let env = env_with(Uuid::new_v4(), &[("A", "1")], 1);
    vault.put(Record::Environment(env.clone())).unwrap();

    let path = record_path(dir.path(), env.id);
    let mut file: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    let ct = file["sealed"]["ct"].as_str().unwrap().to_string();
    let flipped = if ct.starts_with('A') { ct.replacen('A', "B", 1) } else { format!("A{}", &ct[1..]) };
    file["sealed"]["ct"] = serde_json::json!(flipped);
    fs::write(&path, serde_json::to_vec(&file).unwrap()).unwrap();

    assert!(matches!(vault.get(env.id), Err(Error::RecordTampered(id)) if id == env.id));
}

#[test]
fn record_swapped_to_another_id_is_detected() {
    let dir = tempfile::tempdir().unwrap();
    let vault = Vault::create(dir.path(), "pw", FAST).unwrap();
    let a = env_with(Uuid::new_v4(), &[("A", "1")], 1);
    let b = env_with(Uuid::new_v4(), &[("B", "2")], 1);
    vault.put(Record::Environment(a.clone())).unwrap();
    vault.put(Record::Environment(b.clone())).unwrap();

    fs::copy(record_path(dir.path(), a.id), record_path(dir.path(), b.id)).unwrap();
    assert!(matches!(vault.get(b.id), Err(Error::RecordTampered(_))));
}

#[test]
fn record_from_another_vault_is_rejected() {
    let a_dir = tempfile::tempdir().unwrap();
    let b_dir = tempfile::tempdir().unwrap();
    let a = Vault::create(a_dir.path(), "pw", FAST).unwrap();
    let b = Vault::create(b_dir.path(), "pw", FAST).unwrap();
    let env = env_with(Uuid::new_v4(), &[("A", "1")], 1);
    a.put(Record::Environment(env.clone())).unwrap();

    fs::copy(record_path(a_dir.path(), env.id), record_path(b_dir.path(), env.id)).unwrap();
    assert!(matches!(b.get(env.id), Err(Error::RecordTampered(_))));
}

#[test]
fn put_merges_with_changes_from_another_device() {
    let dir = tempfile::tempdir().unwrap();
    let vault = Vault::create(dir.path(), "pw", FAST).unwrap();
    let base = env_with(Uuid::new_v4(), &[("SHARED", "v1")], 1);
    vault.put(Record::Environment(base.clone())).unwrap();

    // Device A adds a var; device B, starting from the same base, edits another.
    let mut on_a = base.clone();
    on_a.set("FROM_A", "a", stamp(2));
    vault.put(Record::Environment(on_a)).unwrap();

    let mut on_b = base;
    on_b.set("SHARED", "v2", stamp(3));
    let merged = vault.put(Record::Environment(on_b)).unwrap();

    let Record::Environment(env) = merged else { panic!() };
    assert_eq!(env.get("FROM_A"), Some("a"));
    assert_eq!(env.get("SHARED"), Some("v2"));
}

#[test]
fn deletes_win_over_older_edits() {
    let dir = tempfile::tempdir().unwrap();
    let vault = Vault::create(dir.path(), "pw", FAST).unwrap();
    let base = env_with(Uuid::new_v4(), &[("OLD_KEY", "x")], 1);
    vault.put(Record::Environment(base.clone())).unwrap();

    let mut deleted = base.clone();
    deleted.remove("OLD_KEY", stamp(5));
    vault.put(Record::Environment(deleted)).unwrap();

    // A stale copy from before the delete must not bring it back.
    let merged = vault.put(Record::Environment(base.clone())).unwrap();
    let Record::Environment(env) = merged else { panic!() };
    assert_eq!(env.get("OLD_KEY"), None);
    assert_eq!(env.vars().count(), 0);
}

#[test]
fn sync_conflict_copies_are_merged_and_cleaned_up() {
    let dir = tempfile::tempdir().unwrap();
    let vault = Vault::create(dir.path(), "pw", FAST).unwrap();
    let base = env_with(Uuid::new_v4(), &[("A", "1")], 1);
    let canonical = record_path(dir.path(), base.id);
    let copy = dir.path().join("records").join(format!("{} (conflicted copy).json", base.id));

    // Simulate a sync client that couldn't reconcile two devices: the other
    // device's version ends up as a conflict copy next to ours.
    let mut theirs = base.clone();
    theirs.set("B", "2", stamp(2));
    vault.put(Record::Environment(theirs)).unwrap();
    let parked = dir.path().join("theirs.json");
    fs::rename(&canonical, &parked).unwrap();

    let mut ours = base.clone();
    ours.set("C", "3", stamp(3));
    vault.put(Record::Environment(ours)).unwrap();
    fs::rename(&parked, &copy).unwrap();

    let Some(Record::Environment(read)) = vault.get(base.id).unwrap() else { panic!() };
    assert_eq!(read.get("A"), Some("1"));
    assert_eq!(read.get("B"), Some("2"));
    assert_eq!(read.get("C"), Some("3"));

    vault.put(Record::Environment(read)).unwrap();
    assert!(!copy.exists(), "conflict copy removed after write");
    assert_eq!(vault.list().unwrap().len(), 1);
}

#[test]
fn temp_and_foreign_files_are_ignored() {
    let dir = tempfile::tempdir().unwrap();
    let vault = Vault::create(dir.path(), "pw", FAST).unwrap();
    fs::write(dir.path().join("records").join(".DS_Store"), b"junk").unwrap();
    fs::write(dir.path().join("records").join("notes.txt"), b"junk").unwrap();
    fs::write(dir.path().join("records").join(format!(".{}.json.x.tmp", Uuid::new_v4())), b"junk").unwrap();
    assert!(vault.list().unwrap().is_empty());
}

#[test]
fn session_key_unlocks_until_vault_is_replaced() {
    let dir = tempfile::tempdir().unwrap();
    let vault = Vault::create(dir.path(), "pw", FAST).unwrap();
    let key = vault.session_key();
    assert_eq!(Vault::unlock_with_session_key(dir.path(), &key).unwrap().id(), vault.id());

    assert!(matches!(
        Vault::unlock_with_session_key(dir.path(), &[7u8; 32]),
        Err(Error::InvalidCredentials)
    ));

    // A different vault at the same path must not accept the old cached key.
    fs::remove_dir_all(dir.path()).unwrap();
    Vault::create(dir.path(), "pw", FAST).unwrap();
    assert!(matches!(Vault::unlock_with_session_key(dir.path(), &key), Err(Error::InvalidCredentials)));
}

#[test]
fn default_kdf_params_are_strong() {
    let p = KdfParams::default();
    assert!(p.m_cost_kib >= 64 * 1024 && p.t_cost >= 3);
}
