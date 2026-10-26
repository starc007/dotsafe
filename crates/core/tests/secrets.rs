use std::fs;
use std::path::Path;

use dotsafe_core::{Entry, Environment, Error, FolderStore, KdfParams, Record, Secret, Stamp, Vault};
use serde_json::Value;
use uuid::Uuid;

// Cheap Argon2 settings so tests run fast. Never use these for real vaults.
const FAST: KdfParams = KdfParams { m_cost_kib: 64, t_cost: 1, p_cost: 1 };

fn record_json(root: &Path, id: Uuid) -> Value {
    serde_json::from_slice(&fs::read(root.join("records").join(format!("{id}.json"))).unwrap()).unwrap()
}

fn setup() -> (tempfile::TempDir, Vault, Secret, Environment) {
    let dir = tempfile::tempdir().unwrap();
    let vault = Vault::create(FolderStore::new(dir.path()), "pw", FAST).unwrap();
    let device = Uuid::new_v4();
    let secret = Secret::new("STRIPE_KEY", "sk_live_old", vault.stamp(device));
    vault.put(Record::Secret(secret.clone())).unwrap();
    let mut env = Environment::new(Uuid::new_v4(), "production", vault.stamp(device));
    env.set("PLAIN", "x", vault.stamp(device));
    env.link("STRIPE_KEY", secret.id, vault.stamp(device));
    vault.put(Record::Environment(env.clone())).unwrap();
    (dir, vault, secret, env)
}

#[test]
fn linked_variables_resolve_to_the_current_value() {
    let (_dir, vault, secret, env) = setup();
    assert_eq!(env.entry("STRIPE_KEY"), Some(Entry::Secret(secret.id)));
    assert_eq!(env.value("STRIPE_KEY"), None, "a link has no value of its own");

    let secrets = vault.secrets().unwrap();
    let resolved: Vec<_> = secrets.resolve(&env).collect();
    assert_eq!(resolved, [("PLAIN", "x"), ("STRIPE_KEY", "sk_live_old")]);

    // Rotating the secret changes every linked variable at once.
    let Some(Record::Secret(mut stored)) = vault.get(secret.id).unwrap() else { panic!() };
    stored.rotate("sk_live_new", None, vault.stamp(Uuid::new_v4())).unwrap();
    vault.put(Record::Secret(stored)).unwrap();
    let secrets = vault.secrets().unwrap();
    assert_eq!(secrets.value(Entry::Secret(secret.id)), Some("sk_live_new"));
}

#[test]
fn records_using_secrets_are_v3_and_the_rest_stay_v2() {
    let (dir, vault, secret, env) = setup();
    assert_eq!(record_json(dir.path(), secret.id)["version"], 3);
    assert_eq!(record_json(dir.path(), env.id)["version"], 3);

    let mut unlinked = env.clone();
    unlinked.remove("STRIPE_KEY", vault.stamp(Uuid::new_v4()));
    vault.put(Record::Environment(unlinked)).unwrap();
    assert_eq!(record_json(dir.path(), env.id)["version"], 2, "no links left");
}

#[test]
fn v3_records_cant_be_relabelled_for_older_clients() {
    let (dir, vault, _secret, env) = setup();
    let path = dir.path().join("records").join(format!("{}.json", env.id));
    let mut json = record_json(dir.path(), env.id);
    json["version"] = 2.into();
    fs::write(&path, json.to_string()).unwrap();
    assert!(matches!(vault.get(env.id), Err(Error::RecordTampered(_))));
}

#[test]
fn deleted_and_missing_secrets_resolve_to_nothing() {
    let (_dir, vault, secret, env) = setup();
    let Some(Record::Secret(mut stored)) = vault.get(secret.id).unwrap() else { panic!() };
    stored.delete(vault.stamp(Uuid::new_v4()));
    vault.put(Record::Secret(stored)).unwrap();
    let secrets = vault.secrets().unwrap();
    assert!(secrets.get(secret.id).is_none());
    assert_eq!(secrets.resolve(&env).collect::<Vec<_>>(), [("PLAIN", "x")]);
}

#[test]
fn rotations_on_two_devices_merge() {
    let (dir, vault, secret, _env) = setup();
    let other = Vault::unlock(FolderStore::new(dir.path()), "pw").unwrap();

    let Some(Record::Secret(mut ours)) = vault.get(secret.id).unwrap() else { panic!() };
    let Some(Record::Secret(mut theirs)) = other.get(secret.id).unwrap() else { panic!() };
    ours.rotate("from-a", None, Stamp::new(ours.max_stamp_for_tests() + 1, Uuid::from_u128(1))).unwrap();
    theirs.rotate("from-b", None, Stamp::new(theirs.max_stamp_for_tests() + 2, Uuid::from_u128(2))).unwrap();
    vault.put(Record::Secret(ours)).unwrap();
    other.put(Record::Secret(theirs)).unwrap();

    let Some(Record::Secret(merged)) = vault.get(secret.id).unwrap() else { panic!() };
    assert_eq!(merged.current_value(), Some("from-b"));
    assert_eq!(merged.versions().len(), 3, "both rotations are in the history");
}

trait MaxStamp {
    fn max_stamp_for_tests(&self) -> u64;
}

impl MaxStamp for Secret {
    fn max_stamp_for_tests(&self) -> u64 {
        Record::Secret(self.clone()).max_stamp().at_ms
    }
}
