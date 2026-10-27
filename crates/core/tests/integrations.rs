use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use dotsafe_core::{
    Binding, Connection, Error, FolderStore, KdfParams, ProviderLink, Pushed, Record, Secret, Target, Vault,
};
use serde_json::Value;
use uuid::Uuid;

// Cheap Argon2 settings so tests run fast. Never use these for real vaults.
const FAST: KdfParams = KdfParams { m_cost_kib: 64, t_cost: 1, p_cost: 1 };

fn record_path(root: &Path, id: Uuid) -> std::path::PathBuf {
    root.join("records").join(format!("{id}.json"))
}

fn record_json(root: &Path, id: Uuid) -> Value {
    serde_json::from_slice(&fs::read(record_path(root, id)).unwrap()).unwrap()
}

fn setup() -> (tempfile::TempDir, Vault, Connection, Binding) {
    let dir = tempfile::tempdir().unwrap();
    let vault = Vault::create(FolderStore::new(dir.path()), "pw", FAST).unwrap();
    let device = Uuid::new_v4();
    let token = Secret::new("OpenAI admin key", "sk-admin-x", vault.stamp(device));
    vault.put(Record::Secret(token.clone())).unwrap();
    let settings = BTreeMap::from([("project_id".to_string(), "proj_1".to_string())]);
    let connection = Connection::new("openai", "Acme", token.id, settings, vault.stamp(device));
    vault.put(Record::Connection(connection.clone())).unwrap();
    let secret = Secret::new("OPENAI_API_KEY", "sk-proj-1", vault.stamp(device));
    vault.put(Record::Secret(secret.clone())).unwrap();
    let mut binding = Binding::new(secret.id, vault.stamp(device));
    binding.provider = Some(ProviderLink { connection: connection.id, settings: BTreeMap::new() });
    binding.keys.insert(secret.current().unwrap().0, "svc_acct_1".into());
    vault.put(Record::Binding(binding.clone())).unwrap();
    (dir, vault, connection, binding)
}

#[test]
fn integrations_are_v4_records_that_round_trip() {
    let (dir, vault, connection, binding) = setup();
    assert_eq!(record_json(dir.path(), connection.id)["version"], 4);
    assert_eq!(record_json(dir.path(), binding.id)["version"], 4);
    assert_eq!(binding.id, Binding::id_for(binding.secret));

    let other = Vault::unlock(FolderStore::new(dir.path()), "pw").unwrap();
    assert_eq!(other.get(connection.id).unwrap(), Some(Record::Connection(connection)));
    assert_eq!(other.get(binding.id).unwrap(), Some(Record::Binding(binding)));
    drop(vault);
}

#[test]
fn v4_records_cant_be_relabelled_for_older_clients() {
    let (dir, vault, connection, _) = setup();
    for older in [3, 2] {
        let mut json = record_json(dir.path(), connection.id);
        json["version"] = older.into();
        fs::write(record_path(dir.path(), connection.id), serde_json::to_vec(&json).unwrap()).unwrap();
        assert!(matches!(vault.get(connection.id), Err(Error::RecordTampered(_))), "relabelled as v{older}");
    }
}

#[test]
fn targets_and_keys_from_two_devices_merge() {
    let (dir, a, connection, binding) = setup();
    let b = Vault::unlock(FolderStore::new(dir.path()), "pw").unwrap();
    let (da, db) = (Uuid::new_v4(), Uuid::new_v4());
    let target = |name: &str, stamp| Target {
        connection: connection.id,
        name: name.into(),
        settings: BTreeMap::new(),
        pushed: None,
        deleted: false,
        changed: stamp,
    };

    // Each device adds a target and a key id on its own copy.
    let mut on_a = binding.clone();
    on_a.targets.insert(Uuid::new_v4(), target("VERCEL", a.stamp(da)));
    on_a.keys.insert(Uuid::new_v4(), "svc_acct_a".into());
    let mut on_b = binding.clone();
    let shared = Uuid::new_v4();
    on_b.targets.insert(shared, target("WORKER", b.stamp(db)));
    on_b.keys.insert(Uuid::new_v4(), "svc_acct_b".into());
    a.put(Record::Binding(on_a)).unwrap();
    b.put(Record::Binding(on_b)).unwrap();

    let Some(Record::Binding(merged)) = a.get(binding.id).unwrap() else { panic!() };
    assert_eq!(merged.targets().count(), 2);
    assert_eq!(merged.keys.len(), 3);

    // A later push record on one target wins over the older copy.
    let mut pushed = merged.clone();
    let t = pushed.targets.get_mut(&shared).unwrap();
    t.pushed = Some(Pushed { sha256: "ab".into(), at: 1 });
    t.changed = a.stamp(da);
    a.put(Record::Binding(pushed)).unwrap();
    b.put(Record::Binding(merged)).unwrap();
    let Some(Record::Binding(latest)) = b.get(binding.id).unwrap() else { panic!() };
    assert_eq!(latest.targets[&shared].pushed.as_ref().map(|p| p.sha256.as_str()), Some("ab"));
}

#[test]
fn values_older_than_a_removal_are_exposed_until_rotated() {
    use dotsafe_core::integration::exposed_since;
    use dotsafe_core::{Environment, Project};
    let dir = tempfile::tempdir().unwrap();
    let vault = Vault::create(FolderStore::new(dir.path()), "pw", FAST).unwrap();
    let device = Uuid::new_v4();
    let project = Project::new("api", vault.stamp(device));
    vault.put(Record::Project(project.clone())).unwrap();
    let secret = Secret::new("STRIPE", "sk_old", vault.stamp(device));
    vault.put(Record::Secret(secret.clone())).unwrap();
    let mut env = Environment::new(project.id, "production", vault.stamp(device));
    env.set("DB_URL", "postgres://old", vault.stamp(device));
    env.set("PORT", "3000", vault.stamp(device));
    env.link("STRIPE", secret.id, vault.stamp(device));
    vault.put(Record::Environment(env.clone())).unwrap();

    std::thread::sleep(std::time::Duration::from_millis(5));
    let removed_at = vault.stamp(device).at_ms;
    std::thread::sleep(std::time::Duration::from_millis(5));

    let exposed = exposed_since(&vault.list().unwrap(), removed_at);
    assert_eq!(exposed.secrets, [secret.id]);
    let mut vars: Vec<&str> = exposed.variables.iter().map(|(_, k)| k.as_str()).collect();
    vars.sort();
    assert_eq!(vars, ["DB_URL", "PORT"], "linked variables are covered by their secret");

    // Rotating takes things off the list.
    let Some(Record::Secret(mut stored)) = vault.get(secret.id).unwrap() else { panic!() };
    stored.rotate("sk_new", None, vault.stamp(device)).unwrap();
    vault.put(Record::Secret(stored)).unwrap();
    let Some(Record::Environment(mut stored)) = vault.get(env.id).unwrap() else { panic!() };
    stored.set("DB_URL", "postgres://new", vault.stamp(device));
    vault.put(Record::Environment(stored)).unwrap();
    let exposed = exposed_since(&vault.list().unwrap(), removed_at);
    assert!(exposed.secrets.is_empty());
    assert_eq!(exposed.variables.iter().map(|(_, k)| k.as_str()).collect::<Vec<_>>(), ["PORT"]);
}
