use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use dotsafe_core::store::{Cond, Object, Put, Store};
use dotsafe_core::{CachedStore, Environment, Error, KdfParams, Record, Stamp, Vault};
use uuid::Uuid;

// Cheap Argon2 settings so tests run fast. Never use these for real vaults.
const FAST: KdfParams = KdfParams { m_cost_kib: 64, t_cost: 1, p_cost: 1 };

/// An in-memory remote with real conditional writes, that can go offline.
#[derive(Default)]
struct Remote {
    objects: Mutex<BTreeMap<String, (Vec<u8>, String)>>,
    offline: AtomicBool,
    versions: AtomicU64,
}

#[derive(Clone)]
struct RemoteHandle(Arc<Remote>);

impl Remote {
    fn reachable(&self) -> dotsafe_core::Result<()> {
        if self.offline.load(Ordering::SeqCst) { Err(Error::Unreachable("test remote".into())) } else { Ok(()) }
    }
    fn snapshot(&self) -> BTreeMap<String, (Vec<u8>, String)> {
        self.objects.lock().unwrap().clone()
    }
    fn restore(&self, snapshot: BTreeMap<String, (Vec<u8>, String)>) {
        *self.objects.lock().unwrap() = snapshot;
    }
}

impl Store for RemoteHandle {
    fn location(&self) -> String {
        "test remote".into()
    }
    fn list(&self, dir: &str) -> dotsafe_core::Result<Vec<String>> {
        self.0.reachable()?;
        let prefix = if dir.is_empty() { String::new() } else { format!("{dir}/") };
        Ok(self
            .0
            .objects
            .lock()
            .unwrap()
            .keys()
            .filter(|k| k.strip_prefix(&prefix).is_some_and(|rest| !rest.contains('/')))
            .cloned()
            .collect())
    }
    fn get(&self, key: &str) -> dotsafe_core::Result<Option<Object>> {
        self.0.reachable()?;
        Ok(self.0.objects.lock().unwrap().get(key).map(|(b, e)| Object { bytes: b.clone(), etag: e.clone() }))
    }
    fn put(&self, key: &str, bytes: &[u8], cond: Cond) -> dotsafe_core::Result<Put> {
        self.0.reachable()?;
        let mut objects = self.0.objects.lock().unwrap();
        let current = objects.get(key).map(|(_, e)| e.clone());
        let allowed = match cond {
            Cond::Any => true,
            Cond::IfAbsent => current.is_none(),
            Cond::IfMatch(e) => current == Some(e),
        };
        if !allowed {
            return Ok(Put::Conflict);
        }
        let etag = format!("v{}", self.0.versions.fetch_add(1, Ordering::SeqCst));
        objects.insert(key.into(), (bytes.to_vec(), etag.clone()));
        Ok(Put::Written(etag))
    }
    fn delete(&self, key: &str) -> dotsafe_core::Result<()> {
        self.0.reachable()?;
        self.0.objects.lock().unwrap().remove(key);
        Ok(())
    }
}

struct Device {
    cache: tempfile::TempDir,
    id: Uuid,
}

impl Device {
    fn new() -> Self {
        Device { cache: tempfile::tempdir().unwrap(), id: Uuid::new_v4() }
    }
    fn store(&self, remote: &RemoteHandle) -> dotsafe_core::Result<CachedStore> {
        CachedStore::open(remote.clone(), self.cache.path())
    }
    fn unlock(&self, remote: &RemoteHandle) -> Vault {
        Vault::unlock(self.store(remote).unwrap(), "pw").unwrap()
    }
}

fn get_env(vault: &Vault, id: Uuid) -> Environment {
    match vault.get(id).unwrap() {
        Some(Record::Environment(e)) => e,
        other => panic!("expected an environment, got {other:?}"),
    }
}

fn setup() -> (RemoteHandle, Device, Environment) {
    let remote = RemoteHandle(Arc::default());
    let a = Device::new();
    let vault = Vault::create(a.store(&remote).unwrap(), "pw", FAST).unwrap();
    let mut env = Environment::new(Uuid::new_v4(), "development", vault.stamp(a.id));
    env.set("A", "1", vault.stamp(a.id));
    vault.put(Record::Environment(env.clone())).unwrap();
    (remote, a, env)
}

#[test]
fn other_devices_see_changes() {
    let (remote, _a, env) = setup();
    let b = Device::new();
    assert_eq!(get_env(&b.unlock(&remote), env.id).get("A"), Some("1"));
}

#[test]
fn works_offline_from_the_cache() {
    let (remote, a, env) = setup();
    remote.0.offline.store(true, Ordering::SeqCst);

    let vault = a.unlock(&remote);
    assert_eq!(get_env(&vault, env.id).get("A"), Some("1"));
    assert!(matches!(vault.sync(), Err(Error::Unreachable(_))));

    // A device with nothing cached can't open the vault offline.
    assert!(matches!(Device::new().store(&remote), Err(Error::Unreachable(_))));
}

#[test]
fn offline_changes_are_pushed_on_sync() {
    let (remote, a, env) = setup();
    remote.0.offline.store(true, Ordering::SeqCst);
    let store = a.store(&remote).unwrap();
    let vault = Vault::unlock(store, "pw").unwrap();
    let mut edited = get_env(&vault, env.id);
    edited.set("B", "offline", vault.stamp(a.id));
    vault.put(Record::Environment(edited)).unwrap();
    assert_eq!(get_env(&vault, env.id).get("B"), Some("offline"));

    let b = Device::new();
    remote.0.offline.store(false, Ordering::SeqCst);
    assert_eq!(get_env(&b.unlock(&remote), env.id).get("B"), None, "not pushed yet");

    vault.sync().unwrap();
    assert_eq!(get_env(&b.unlock(&remote), env.id).get("B"), Some("offline"));
}

#[test]
fn changes_on_both_sides_are_merged() {
    let (remote, a, env) = setup();
    let b = Device::new();
    let vault_b = b.unlock(&remote);

    // A edits offline.
    remote.0.offline.store(true, Ordering::SeqCst);
    let vault_a = a.unlock(&remote);
    let mut ours = get_env(&vault_a, env.id);
    ours.set("FROM_A", "a", vault_a.stamp(a.id));
    vault_a.put(Record::Environment(ours)).unwrap();

    // Meanwhile B edits the same environment online.
    remote.0.offline.store(false, Ordering::SeqCst);
    let mut theirs = get_env(&vault_b, env.id);
    theirs.set("FROM_B", "b", vault_b.stamp(b.id));
    vault_b.put(Record::Environment(theirs)).unwrap();

    vault_a.sync().unwrap();
    let merged = get_env(&vault_a, env.id);
    assert_eq!((merged.get("FROM_A"), merged.get("FROM_B")), (Some("a"), Some("b")));

    vault_b.sync().unwrap();
    let seen_by_b = get_env(&vault_b, env.id);
    assert_eq!((seen_by_b.get("FROM_A"), seen_by_b.get("FROM_B")), (Some("a"), Some("b")));
}

#[test]
fn online_writes_that_lose_a_race_merge_and_retry() {
    let (remote, a, env) = setup();
    let b = Device::new();
    let vault_a = a.unlock(&remote);
    let vault_b = b.unlock(&remote);

    let mut theirs = get_env(&vault_b, env.id);
    theirs.set("FROM_B", "b", vault_b.stamp(b.id));
    vault_b.put(Record::Environment(theirs)).unwrap();

    // A's cache is behind; its write conflicts, picks up B's change and retries.
    let mut ours = get_env(&vault_a, env.id);
    ours.set("FROM_A", "a", vault_a.stamp(a.id));
    vault_a.put(Record::Environment(ours)).unwrap();

    let c = Device::new();
    let merged = get_env(&c.unlock(&remote), env.id);
    assert_eq!((merged.get("FROM_A"), merged.get("FROM_B")), (Some("a"), Some("b")));
}

#[test]
fn a_rolled_back_remote_is_noticed_across_sessions() {
    let (remote, a, env) = setup();
    let old = remote.0.snapshot();

    let vault = a.unlock(&remote);
    let mut newer = get_env(&vault, env.id);
    newer.set("A", "2", vault.stamp(a.id));
    vault.put(Record::Environment(newer)).unwrap();
    drop(vault);

    // The bucket is restored to an older state, head.json included.
    remote.0.restore(old);
    let vault = a.unlock(&remote);
    vault.sync().unwrap();
    vault.list().unwrap();
    assert_eq!(vault.stale_records(), [env.id]);
}

#[test]
fn a_wiped_remote_is_restored_from_the_cache() {
    let (remote, a, env) = setup();
    remote.0.restore(BTreeMap::new());

    let vault = a.unlock(&remote);
    assert_eq!(get_env(&vault, env.id).get("A"), Some("1"));
    vault.sync().unwrap();

    let b = Device::new();
    assert_eq!(get_env(&b.unlock(&remote), env.id).get("A"), Some("1"), "the bucket was refilled");
}

#[test]
fn stamps_from_the_cache_keep_ordering() {
    let (remote, a, env) = setup();
    let vault = a.unlock(&remote);
    vault.list().unwrap();
    let stamp = vault.stamp(a.id);
    let stored = Record::Environment(get_env(&vault, env.id)).max_stamp();
    assert!(stamp > stored);
    let _ = Stamp::new(0, a.id);
}
