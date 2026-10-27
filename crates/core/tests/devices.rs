use std::fs;
use std::path::Path;

use dotsafe_core::{
    DeviceKey, EnrollmentCode, Environment, Error, FolderStore, KdfParams, Project, Record, RecoveryKey, Vault,
};
use serde_json::Value;
use uuid::Uuid;

// Cheap Argon2 settings so tests run fast. Never use these for real vaults.
const FAST: KdfParams = KdfParams { m_cost_kib: 64, t_cost: 1, p_cost: 1 };

fn store(dir: &Path) -> FolderStore {
    FolderStore::new(dir)
}

fn header(dir: &Path) -> Value {
    serde_json::from_slice(&fs::read(dir.join("vault.json")).unwrap()).unwrap()
}

fn record_epoch(dir: &Path, id: Uuid) -> Value {
    let json: Value =
        serde_json::from_slice(&fs::read(dir.join("records").join(format!("{id}.json"))).unwrap()).unwrap();
    json["epoch"].clone()
}

/// A vault with a project and an environment, and its recovery key.
fn setup() -> (tempfile::TempDir, Vault, RecoveryKey, Environment) {
    let dir = tempfile::tempdir().unwrap();
    let mut vault = Vault::create(store(dir.path()), "pw-original", FAST).unwrap();
    let recovery = vault.reset_recovery_key().unwrap();
    let device = Uuid::new_v4();
    let project = Project::new("api", vault.stamp(device));
    let mut env = Environment::new(project.id, "production", vault.stamp(device));
    env.set("STRIPE_KEY", "sk_live_x", vault.stamp(device));
    vault.put(Record::Project(project)).unwrap();
    vault.put(Record::Environment(env.clone())).unwrap();
    (dir, vault, recovery, env)
}

#[test]
fn enrolled_devices_unlock_with_their_own_key() {
    let (dir, mut vault, _, env) = setup();
    let mac = Uuid::new_v4();
    let key = vault.enroll_device(mac, "Studio Mac").unwrap();

    let unlocked = Vault::unlock_with_device(store(dir.path()), mac, &key).unwrap();
    assert!(unlocked.get(env.id).unwrap().is_some());
    assert_eq!(header(dir.path())["version"], 4);
    let devices = vault.devices();
    assert_eq!((devices[0].name.as_str(), devices[0].stale), ("Studio Mac", false));

    // Another device's id, or a wrong key, gets nothing.
    assert!(matches!(
        Vault::unlock_with_device(store(dir.path()), Uuid::new_v4(), &key),
        Err(Error::InvalidCredentials)
    ));
    let wrong = DeviceKey::from_bytes([7; 32]);
    assert!(matches!(Vault::unlock_with_device(store(dir.path()), mac, &wrong), Err(Error::InvalidCredentials)));

    // Turning Touch ID off on this Mac removes its slot.
    assert!(vault.forget_device(mac).unwrap());
    assert!(Vault::unlock_with_device(store(dir.path()), mac, &key).is_err());
}

#[test]
fn enrollment_codes_add_a_device_once() {
    let (dir, mut vault, _, env) = setup();
    let code = vault.create_enrollment_code().unwrap();
    let typed = EnrollmentCode::parse(&code.to_display_string().to_lowercase()).unwrap();

    let laptop = Uuid::new_v4();
    let (joined, key) = Vault::enroll_with_code(store(dir.path()), &typed, laptop, "Laptop").unwrap();
    assert!(joined.get(env.id).unwrap().is_some());
    assert!(Vault::unlock_with_device(store(dir.path()), laptop, &key).is_ok(), "the new device can unlock on its own");

    // The code is used up.
    assert!(matches!(
        Vault::enroll_with_code(store(dir.path()), &typed, Uuid::new_v4(), "Thief"),
        Err(Error::InvalidCredentials)
    ));
    assert!(EnrollmentCode::parse("not-a-code").is_err());
}

#[test]
fn revoking_a_device_re_keys_the_vault() {
    let (dir, mut vault, old_recovery, env) = setup();
    let (this_mac, lost_laptop) = (Uuid::new_v4(), Uuid::new_v4());
    let this_key = vault.enroll_device(this_mac, "This Mac").unwrap();
    let lost_key = vault.enroll_device(lost_laptop, "Lost laptop").unwrap();
    let old_session = vault.session_key();
    // What the lost laptop still holds: an unlocked vault with the old key.
    let lost = Vault::unlock_with_device(store(dir.path()), lost_laptop, &lost_key).unwrap();

    assert!(matches!(vault.revoke_device(lost_laptop, "wrong", FAST), Err(Error::InvalidCredentials)));
    let new_recovery = vault.revoke_device(lost_laptop, "pw-original", FAST).unwrap();

    assert_eq!(header(dir.path())["epoch"], 2);
    assert_eq!(record_epoch(dir.path(), env.id), 2, "every record is re-sealed");
    assert!(header(dir.path()).get("previous_keys").is_none(), "the old key is dropped once re-sealed");

    // The lost laptop is out, whatever it kept.
    assert!(Vault::unlock_with_device(store(dir.path()), lost_laptop, &lost_key).is_err());
    assert!(Vault::unlock_with_session_key(store(dir.path()), &old_session).is_err());
    assert!(Vault::unlock_with_recovery(store(dir.path()), &old_recovery).is_err());

    // The password and the new recovery key work; this Mac needs the password once.
    let fresh = Vault::unlock(store(dir.path()), "pw-original").unwrap();
    assert!(fresh.get(env.id).unwrap().is_some());
    assert!(Vault::unlock_with_recovery(store(dir.path()), &new_recovery).is_ok());
    assert!(matches!(Vault::unlock_with_device(store(dir.path()), this_mac, &this_key), Err(Error::DeviceStale)));
    assert!(fresh.devices().iter().any(|d| d.device_id == this_mac && d.stale));

    // Writes the lost laptop makes with the old key are skipped, not trusted.
    let mut planted = Environment::new(Uuid::new_v4(), "planted", lost.stamp(lost_laptop));
    planted.set("EVIL", "1", lost.stamp(lost_laptop));
    lost.put(Record::Environment(planted.clone())).unwrap();
    let reader = Vault::unlock(store(dir.path()), "pw-original").unwrap();
    assert!(reader.list().unwrap().iter().all(|r| r.id() != planted.id));
    assert_eq!(reader.unreadable_records(), [planted.id]);

    // Enrolling again refreshes this Mac.
    let mut fresh = fresh;
    let new_key = fresh.enroll_device(this_mac, "This Mac").unwrap();
    assert!(Vault::unlock_with_device(store(dir.path()), this_mac, &new_key).is_ok());
}

#[test]
fn concurrent_header_changes_both_survive() {
    let (dir, mut a, _, _) = setup();
    let mut b = Vault::unlock(store(dir.path()), "pw-original").unwrap();

    a.change_password("pw-from-a", FAST).unwrap();
    // B's copy of the header is out of date; its change is applied to the newest one.
    let laptop = Uuid::new_v4();
    let key = b.enroll_device(laptop, "Laptop").unwrap();

    assert!(Vault::unlock(store(dir.path()), "pw-from-a").is_ok(), "A's password change wasn't lost");
    assert!(Vault::unlock_with_device(store(dir.path()), laptop, &key).is_ok(), "B's device slot wasn't lost");
}

#[test]
fn a_re_key_elsewhere_makes_old_handles_stop_writing_the_header() {
    let (dir, mut a, _, _) = setup();
    let mut b = Vault::unlock(store(dir.path()), "pw-original").unwrap();
    let laptop = Uuid::new_v4();
    a.enroll_device(laptop, "Laptop").unwrap();
    a.revoke_device(laptop, "pw-original", FAST).unwrap();
    assert!(matches!(b.change_password("pw-b", FAST), Err(Error::Invalid(_))));
}
