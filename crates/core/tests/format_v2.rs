use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use dotsafe_core::store::{Cond, Object, Put, Store};
use dotsafe_core::{Environment, Error, FolderStore, KdfParams, Project, Record, RecoveryKey, Stamp, Vault};
use serde_json::Value;
use uuid::Uuid;

// Cheap Argon2 settings so tests run fast. Never use these for real vaults.
const FAST: KdfParams = KdfParams { m_cost_kib: 64, t_cost: 1, p_cost: 1 };
const DEVICE: Uuid = Uuid::nil();

fn stamp(at_ms: u64) -> Stamp {
    Stamp::new(at_ms, DEVICE)
}

fn json(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}

fn record_path(root: &Path, id: Uuid) -> PathBuf {
    root.join("records").join(format!("{id}.json"))
}

fn env(vars: &[(&str, &str)], at: u64) -> Environment {
    let mut env = Environment::new(Uuid::new_v4(), "development", stamp(at));
    for (k, v) in vars {
        env.set(*k, *v, stamp(at));
    }
    env
}

fn vars(record: Option<Record>) -> Vec<(String, String)> {
    let Some(Record::Environment(env)) = record else { panic!("expected an environment") };
    env.keys().filter_map(|k| Some((k.into(), env.value(k)?.into()))).collect()
}

#[test]
fn new_vaults_are_written_in_v2() {
    let dir = tempfile::tempdir().unwrap();
    let vault = Vault::create(FolderStore::new(dir.path()), "pw", FAST).unwrap();
    let e = env(&[("A", "1")], 1);
    vault.put(Record::Environment(e.clone())).unwrap();

    let header = json(&dir.path().join("vault.json"));
    assert_eq!(header["version"], 3);
    assert_eq!(header["epoch"], 1);
    let record = json(&record_path(dir.path(), e.id));
    assert_eq!(record["version"], 2);
    assert_eq!(record["epoch"], 1);
    let head = json(&dir.path().join("head.json"));
    assert_eq!(head["format"], "dotsafe-head");
}

#[test]
fn record_sizes_only_reveal_a_bucket() {
    let dir = tempfile::tempdir().unwrap();
    let vault = Vault::create(FolderStore::new(dir.path()), "pw", FAST).unwrap();
    let one = env(&[("A", "1")], 1);
    let five = env(&[("A", "1"), ("B", "22"), ("C", "333"), ("D", "4444"), ("E", "55555")], 1);
    vault.put(Record::Environment(one.clone())).unwrap();
    vault.put(Record::Environment(five.clone())).unwrap();
    let ct_len = |id| json(&record_path(dir.path(), id))["sealed"]["ct"].as_str().unwrap().len();
    assert_eq!(ct_len(one.id), ct_len(five.id));
}

#[test]
fn records_are_bound_to_their_epoch() {
    let dir = tempfile::tempdir().unwrap();
    let vault = Vault::create(FolderStore::new(dir.path()), "pw", FAST).unwrap();
    let e = env(&[("A", "1")], 1);
    vault.put(Record::Environment(e.clone())).unwrap();
    let path = record_path(dir.path(), e.id);
    let original = json(&path);

    let mut other_epoch = original.clone();
    other_epoch["epoch"] = 2.into();
    fs::write(&path, other_epoch.to_string()).unwrap();
    // A key this vault doesn't have: skipped and reported, not trusted.
    assert!(vault.get(e.id).unwrap().is_none());
    assert_eq!(vault.unreadable_records(), [e.id]);

    // Claiming to be a v1 record changes the AAD, so it fails authentication.
    let mut downgraded = original;
    downgraded["version"] = 1.into();
    downgraded.as_object_mut().unwrap().remove("epoch");
    fs::write(&path, downgraded.to_string()).unwrap();
    assert!(matches!(vault.get(e.id), Err(Error::RecordTampered(_))));
}

#[test]
fn new_stamps_order_after_everything_seen() {
    let dir = tempfile::tempdir().unwrap();
    let vault = Vault::create(FolderStore::new(dir.path()), "pw", FAST).unwrap();
    // Another device's clock is a year ahead.
    let future = 4_000_000_000_000;
    let mut e = env(&[], 1);
    e.set("A", "from the future", Stamp::new(future, Uuid::new_v4()));
    vault.put(Record::Environment(e.clone())).unwrap();

    let fresh = Vault::unlock(FolderStore::new(dir.path()), "pw").unwrap();
    fresh.list().unwrap();
    let first = fresh.stamp(DEVICE);
    let second = fresh.stamp(DEVICE);
    assert!(first.at_ms >= future && first > Record::Environment(e.clone()).max_stamp());
    assert!(second > first);

    // So an edit made after reading the future value wins.
    let mut edited = e.clone();
    edited.set("A", "fixed", first);
    fresh.put(Record::Environment(edited)).unwrap();
    assert_eq!(vars(fresh.get(e.id).unwrap()), [("A".into(), "fixed".into())]);
}

#[test]
fn rolled_back_records_are_flagged() {
    let dir = tempfile::tempdir().unwrap();
    let vault = Vault::create(FolderStore::new(dir.path()), "pw", FAST).unwrap();
    let e = env(&[("A", "1")], 1);
    vault.put(Record::Environment(e.clone())).unwrap();
    let path = record_path(dir.path(), e.id);
    let old = fs::read(&path).unwrap();

    let mut newer = e.clone();
    newer.set("A", "2", stamp(5));
    vault.put(Record::Environment(newer)).unwrap();
    assert!(vault.stale_records().is_empty());

    // Storage serves the old, validly encrypted copy again.
    fs::write(&path, old).unwrap();
    vault.list().unwrap();
    assert_eq!(vault.stale_records(), [e.id]);

    // A device opening the vault fresh still notices, from head.json.
    let fresh = Vault::unlock(FolderStore::new(dir.path()), "pw").unwrap();
    fresh.get(e.id).unwrap();
    assert_eq!(fresh.stale_records(), [e.id]);
}

#[test]
fn tampered_head_is_detected() {
    let dir = tempfile::tempdir().unwrap();
    let vault = Vault::create(FolderStore::new(dir.path()), "pw", FAST).unwrap();
    vault.put(Record::Project(Project::new("api", stamp(1)))).unwrap();
    let path = dir.path().join("head.json");
    let mut head = json(&path);
    let ct = head["sealed"]["ct"].as_str().unwrap().to_string();
    let flipped = if ct.starts_with('A') { ct.replacen('A', "B", 1) } else { format!("A{}", &ct[1..]) };
    head["sealed"]["ct"] = flipped.into();
    fs::write(&path, head.to_string()).unwrap();
    assert!(matches!(vault.list(), Err(Error::HeadTampered)));
}

#[test]
fn head_conflict_copies_are_merged_and_cleaned_up() {
    let dir = tempfile::tempdir().unwrap();
    let vault = Vault::create(FolderStore::new(dir.path()), "pw", FAST).unwrap();
    let first = env(&[("A", "1")], 1);
    vault.put(Record::Environment(first.clone())).unwrap();
    // Park this head, as if another device's version of it arrived as a copy.
    let copy = dir.path().join("head (1).json");
    fs::rename(dir.path().join("head.json"), &copy).unwrap();

    let second = env(&[("B", "2")], 2);
    vault.put(Record::Environment(second)).unwrap();
    assert!(!copy.exists(), "head copy merged and removed");

    // The merged head still knows the first record's stamp.
    let old = fs::read(record_path(dir.path(), first.id)).unwrap();
    let mut newer = first.clone();
    newer.set("A", "2", stamp(9));
    vault.put(Record::Environment(newer)).unwrap();
    fs::write(record_path(dir.path(), first.id), old).unwrap();
    let fresh = Vault::unlock(FolderStore::new(dir.path()), "pw").unwrap();
    fresh.list().unwrap();
    assert_eq!(fresh.stale_records(), [first.id]);
}

/// A folder store that lets another writer in just before one write, to
/// simulate two devices saving the same record at the same moment.
struct RacingStore {
    inner: FolderStore,
    racer: Mutex<Option<Box<dyn FnOnce() + Send>>>,
}

impl Store for RacingStore {
    fn location(&self) -> String {
        self.inner.location()
    }
    fn list(&self, dir: &str) -> dotsafe_core::Result<Vec<String>> {
        self.inner.list(dir)
    }
    fn get(&self, key: &str) -> dotsafe_core::Result<Option<Object>> {
        self.inner.get(key)
    }
    fn put(&self, key: &str, bytes: &[u8], cond: Cond) -> dotsafe_core::Result<Put> {
        if key.starts_with("records/")
            && let Some(race) = self.racer.lock().unwrap().take()
        {
            race();
        }
        self.inner.put(key, bytes, cond)
    }
    fn delete(&self, key: &str) -> dotsafe_core::Result<()> {
        self.inner.delete(key)
    }
}

#[test]
fn concurrent_writes_retry_and_merge() {
    let dir = tempfile::tempdir().unwrap();
    let setup = Vault::create(FolderStore::new(dir.path()), "pw", FAST).unwrap();
    let base = env(&[("A", "1")], 1);
    setup.put(Record::Environment(base.clone())).unwrap();

    let other_device = Vault::unlock(FolderStore::new(dir.path()), "pw").unwrap();
    let mut theirs = base.clone();
    theirs.set("B", "2", stamp(2));
    let store = RacingStore {
        inner: FolderStore::new(dir.path()),
        racer: Mutex::new(Some(Box::new(move || {
            other_device.put(Record::Environment(theirs)).unwrap();
        }))),
    };
    let vault = Vault::unlock(store, "pw").unwrap();
    let mut ours = base.clone();
    ours.set("C", "3", stamp(3));
    vault.put(Record::Environment(ours)).unwrap();

    let fresh = Vault::unlock(FolderStore::new(dir.path()), "pw").unwrap();
    let got = vars(fresh.get(base.id).unwrap());
    assert_eq!(got, [("A".into(), "1".into()), ("B".into(), "2".into()), ("C".into(), "3".into())]);
}

fn v1_fixture() -> tempfile::TempDir {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/v1-vault");
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("records")).unwrap();
    for entry in walk(&src) {
        let rel = entry.strip_prefix(&src).unwrap();
        fs::copy(&entry, dir.path().join(rel)).unwrap();
    }
    dir
}

fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() { files.extend(walk(&path)) } else { files.push(path) }
    }
    files
}

fn assert_fixture_data(vault: &Vault) {
    let records = vault.list().unwrap();
    assert_eq!(records.len(), 2);
    let env = records
        .into_iter()
        .find_map(|r| match r {
            Record::Environment(e) => Some(e),
            _ => None,
        })
        .unwrap();
    assert_eq!(env.value("DATABASE_URL"), Some("postgres://localhost/app"));
    assert_eq!(env.value("API_KEY"), Some("sk_test_123"));
    assert_eq!(env.value("OLD"), None);
}

#[test]
fn v1_vaults_are_upgraded_on_unlock() {
    let dir = v1_fixture();
    assert_eq!(json(&dir.path().join("vault.json"))["version"], 1);

    let vault = Vault::unlock(FolderStore::new(dir.path()), "fixture-password").unwrap();
    assert_fixture_data(&vault);

    let header = json(&dir.path().join("vault.json"));
    assert_eq!((header["version"].clone(), header["epoch"].clone()), (3.into(), 1.into()));
    for path in walk(&dir.path().join("records")) {
        assert_eq!(json(&path)["version"], 2);
    }
    assert!(dir.path().join("head.json").exists());

    // Every way in still works afterwards.
    let recovery =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/v1-vault-recovery.txt"));
    let recovery = RecoveryKey::parse(&recovery.unwrap()).unwrap();
    assert_fixture_data(&Vault::unlock_with_recovery(FolderStore::new(dir.path()), &recovery).unwrap());
    let key = vault.session_key();
    assert_fixture_data(&Vault::unlock_with_session_key(FolderStore::new(dir.path()), &key).unwrap());
}

#[test]
fn interrupted_upgrades_finish_next_time() {
    let dir = v1_fixture();
    let v1_header = fs::read(dir.path().join("vault.json")).unwrap();
    Vault::unlock(FolderStore::new(dir.path()), "fixture-password").unwrap();
    // Records are v2 but the header write never happened.
    fs::write(dir.path().join("vault.json"), v1_header).unwrap();

    let vault = Vault::unlock(FolderStore::new(dir.path()), "fixture-password").unwrap();
    assert_fixture_data(&vault);
    assert_eq!(json(&dir.path().join("vault.json"))["version"], 3);
}

#[test]
fn upgrading_needs_the_right_key() {
    let dir = v1_fixture();
    assert!(matches!(Vault::unlock(FolderStore::new(dir.path()), "wrong"), Err(Error::InvalidCredentials)));
    assert_eq!(json(&dir.path().join("vault.json"))["version"], 1, "nothing changed");
}
