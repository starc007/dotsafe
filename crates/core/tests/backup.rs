use dotsafe_core::backup::{self, KEEP};
use dotsafe_core::{Environment, Error, FolderStore, KdfParams, Record, Vault};
use uuid::Uuid;

// Cheap Argon2 settings so tests run fast. Never use these for real vaults.
const FAST: KdfParams = KdfParams { m_cost_kib: 64, t_cost: 1, p_cost: 1 };

fn vault_with_a_variable(dir: &std::path::Path) -> (Vault, Uuid) {
    let vault = Vault::create(FolderStore::new(dir), "pw", FAST).unwrap();
    let device = Uuid::new_v4();
    let mut env = Environment::new(Uuid::new_v4(), "production", vault.stamp(device));
    env.set("API_KEY", "sk_backup", vault.stamp(device));
    vault.put(Record::Environment(env.clone())).unwrap();
    (vault, env.id)
}

#[test]
fn snapshots_are_whole_vaults_and_old_ones_go() {
    let source = tempfile::tempdir().unwrap();
    let (_vault, env) = vault_with_a_variable(source.path());
    let backups = tempfile::tempdir().unwrap();

    let first = backup::snapshot(&FolderStore::new(source.path()), backups.path(), "2026-10-01-0900").unwrap();
    // A snapshot opens like any vault folder.
    let restored = Vault::unlock(FolderStore::new(&first), "pw").unwrap();
    let Some(Record::Environment(e)) = restored.get(env).unwrap() else { panic!() };
    assert_eq!(e.value("API_KEY"), Some("sk_backup"));
    assert!(std::fs::read_to_string(first.join("vault.json")).unwrap().contains("wrapped_key"), "still encrypted");
    assert!(!walk_contains(&first, "sk_backup"), "no plaintext in the snapshot");

    for day in 2..=12 {
        backup::snapshot(&FolderStore::new(source.path()), backups.path(), &format!("2026-10-{day:02}-0900")).unwrap();
    }
    let kept = backup::snapshots(backups.path()).unwrap();
    assert_eq!(kept.len(), KEEP);
    assert!(kept[0].ends_with("dotsafe-backup-2026-10-03-0900"), "the oldest went first: {:?}", kept[0]);
    assert_eq!(backup::stamp(1_793_015_254_719), "2026-10-26-1147");
}

#[test]
fn moving_copies_into_an_empty_place_only() {
    let source = tempfile::tempdir().unwrap();
    let (vault, env) = vault_with_a_variable(source.path());
    let target = tempfile::tempdir().unwrap();
    let copied = backup::copy_to_empty(&FolderStore::new(source.path()), &FolderStore::new(target.path())).unwrap();
    assert!(copied >= 3, "header, head and records: {copied}");
    let moved = Vault::unlock_with_session_key(FolderStore::new(target.path()), &vault.session_key()).unwrap();
    assert!(moved.get(env).unwrap().is_some());
    // Never over another vault.
    let again = backup::copy_to_empty(&FolderStore::new(source.path()), &FolderStore::new(target.path()));
    assert!(matches!(again, Err(Error::AlreadyExists(_))));
}

fn walk_contains(dir: &std::path::Path, needle: &str) -> bool {
    std::fs::read_dir(dir).unwrap().filter_map(|e| e.ok()).any(|e| {
        let path = e.path();
        if path.is_dir() { walk_contains(&path, needle) } else { std::fs::read_to_string(&path).unwrap_or_default().contains(needle) }
    })
}
