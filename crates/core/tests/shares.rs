//! Shares: the v6 record, sealed snapshots, tokens, and the checks a machine
//! makes before using a snapshot.

use std::fs;
use std::path::Path;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use dotsafe_core::share::{self, Status};
use dotsafe_core::{
    Environment, Error, FolderStore, KdfParams, Project, Pushed, Record, Secret, Share, ShareEnv, ShareMode, ShareToken,
    Vault,
};
use serde_json::Value;
use uuid::Uuid;

// Cheap Argon2 settings so tests run fast. Never use these for real vaults.
const FAST: KdfParams = KdfParams { m_cost_kib: 64, t_cost: 1, p_cost: 1 };
const DAY: u64 = 24 * 60 * 60 * 1000;
const URL: &str = "https://shares.example.com";

fn record_path(root: &Path, id: Uuid) -> std::path::PathBuf {
    root.join("records").join(format!("{id}.json"))
}

fn record_json(root: &Path, id: Uuid) -> Value {
    serde_json::from_slice(&fs::read(record_path(root, id)).unwrap()).unwrap()
}

fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64
}

struct Setup {
    dir: tempfile::TempDir,
    vault: Vault,
    device: Uuid,
    project: Project,
    production: Environment,
    staging: Environment,
    secret: Secret,
}

/// A project whose staging inherits production, with a linked secret and a
/// reference.
fn setup() -> Setup {
    let dir = tempfile::tempdir().unwrap();
    let vault = Vault::create(FolderStore::new(dir.path()), "pw", FAST).unwrap();
    let device = Uuid::new_v4();
    let project = Project::new("api", vault.stamp(device));
    vault.put(Record::Project(project.clone())).unwrap();
    let secret = Secret::new("STRIPE_KEY", "sk_live_1", vault.stamp(device));
    vault.put(Record::Secret(secret.clone())).unwrap();
    let mut production = Environment::new(project.id, "production", vault.stamp(device));
    production.set("DB_HOST", "db.internal", vault.stamp(device));
    production.set("DB_PASS", "hunter2", vault.stamp(device));
    production.set("DB_URL", "postgres://app:${{ DB_PASS }}@${{ DB_HOST }}/app", vault.stamp(device));
    production.set("PORT", "8080", vault.stamp(device));
    production.link("STRIPE_KEY", secret.id, vault.stamp(device));
    vault.put(Record::Environment(production.clone())).unwrap();
    let mut staging = Environment::new(project.id, "staging", vault.stamp(device));
    staging.parent = Some(production.id);
    staging.set("DB_HOST", "db.staging", vault.stamp(device));
    vault.put(Record::Environment(staging.clone())).unwrap();
    Setup { dir, vault, device, project, production, staging, secret }
}

fn standing(s: &Setup, envs: Vec<ShareEnv>, expires: u64) -> Share {
    Share::new("ci", ShareMode::Standing, envs, expires, 14, s.device, s.vault.stamp(s.device)).unwrap()
}

fn whole(env: &Environment) -> ShareEnv {
    ShareEnv { id: env.id, only: None }
}

#[test]
fn shares_are_v6_records_that_round_trip() {
    let s = setup();
    let share = standing(&s, vec![whole(&s.production)], now() + 30 * DAY);
    s.vault.put(Record::Share(share.clone())).unwrap();
    let json = record_json(s.dir.path(), share.id);
    assert_eq!(json["version"], 6);
    let raw = fs::read_to_string(record_path(s.dir.path(), share.id)).unwrap();
    assert!(!raw.contains("standing") && !raw.contains("\"ci\""), "the record is sealed");

    let other = Vault::unlock(FolderStore::new(s.dir.path()), "pw").unwrap();
    assert_eq!(other.get(share.id).unwrap(), Some(Record::Share(share)));
}

#[test]
fn v6_records_cant_be_relabelled_for_older_clients() {
    let s = setup();
    let share = standing(&s, vec![whole(&s.production)], now() + DAY);
    s.vault.put(Record::Share(share.clone())).unwrap();
    for older in [5, 4, 3, 2] {
        let mut json = record_json(s.dir.path(), share.id);
        json["version"] = older.into();
        fs::write(record_path(s.dir.path(), share.id), serde_json::to_vec(&json).unwrap()).unwrap();
        assert!(matches!(s.vault.get(share.id), Err(Error::RecordTampered(_))), "relabelled as v{older}");
    }
    let mut json = record_json(s.dir.path(), share.id);
    json["version"] = 7.into();
    fs::write(record_path(s.dir.path(), share.id), serde_json::to_vec(&json).unwrap()).unwrap();
    assert!(matches!(s.vault.get(share.id), Err(Error::UnsupportedVersion { version: 7, .. })));
}

#[test]
fn copies_merge_fields_pushes_and_revokes_on_their_own() {
    let s = setup();
    let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
    let share = standing(&s, vec![whole(&s.production)], now() + DAY);
    s.vault.put(Record::Share(share.clone())).unwrap();

    // One device pushes; the other, not having seen it, renames the share.
    let mut pushed = share.clone();
    pushed.pushed = Some(Pushed { sha256: "aa".into(), at: 10 });
    let mut renamed = share.clone();
    renamed.name = "deploy".into();
    renamed.updated = s.vault.stamp(b);
    s.vault.put(Record::Share(pushed)).unwrap();
    s.vault.put(Record::Share(renamed)).unwrap();
    let Some(Record::Share(merged)) = s.vault.get(share.id).unwrap() else { panic!() };
    assert_eq!(merged.name, "deploy");
    assert_eq!(merged.pushed.as_ref().map(|p| p.sha256.as_str()), Some("aa"), "a push doesn't lose to a rename");

    // A revoke sticks, even against a newer edit that hadn't seen it.
    let mut revoked = merged.clone();
    revoked.revoked = Some(20);
    revoked.updated = s.vault.stamp(a);
    let mut stale = merged.clone();
    stale.name = "later".into();
    stale.updated = s.vault.stamp(b);
    stale.pushed = Some(Pushed { sha256: "bb".into(), at: 5 });
    s.vault.put(Record::Share(revoked)).unwrap();
    s.vault.put(Record::Share(stale)).unwrap();
    let Some(Record::Share(merged)) = s.vault.get(share.id).unwrap() else { panic!() };
    assert_eq!((merged.name.as_str(), merged.revoked), ("later", Some(20)));
    assert_eq!(merged.pushed.unwrap().sha256, "aa", "the newer push is kept");
}

#[test]
fn snapshots_hold_final_values_and_open_with_the_token() {
    let s = setup();
    let share = standing(&s, vec![whole(&s.production), whole(&s.staging)], now() + 30 * DAY);
    let records = s.vault.list().unwrap();
    let contents = share.contents(&records).unwrap();
    let sealed = share.seal(s.vault.id(), &contents, s.vault.stamp(s.device), now()).unwrap();
    let text = String::from_utf8(sealed.clone()).unwrap();
    assert!(text.contains("dotsafe-share-snapshot"));
    for value in ["hunter2", "sk_live_1", "db.internal", "DB_URL"] {
        assert!(!text.contains(value), "{value} is sealed");
    }

    let token = share.token(s.vault.id(), &format!("{URL}/")).unwrap();
    assert!(token.starts_with("dss_"));
    let parsed = ShareToken::parse(&token).unwrap();
    assert_eq!((parsed.vault_id, parsed.share_id), (s.vault.id(), share.id));
    assert_eq!(parsed.snapshot_url(), format!("{URL}/{}", share.object_key().unwrap()));
    let snapshot = parsed.open(&sealed, None, now()).unwrap();
    let (name, production) = snapshot.environment(Some("production")).unwrap();
    assert_eq!(name, "production");
    assert_eq!(production["DB_URL"], "postgres://app:hunter2@db.internal/app");
    assert_eq!(production["STRIPE_KEY"], "sk_live_1", "linked secrets are resolved");
    let (_, staging) = snapshot.environment(Some("staging")).unwrap();
    assert_eq!(staging["DB_URL"], "postgres://app:hunter2@db.staging/app", "inherited and re-resolved");
    assert!(snapshot.environment(None).is_err(), "two environments need -e");
    assert!(snapshot.lease_until <= now() + 14 * DAY);
}

#[test]
fn only_narrows_a_snapshot() {
    let s = setup();
    let share = standing(&s, vec![ShareEnv { id: s.production.id, only: Some(vec!["DB_URL".into()]) }], now() + DAY);
    let contents = share.contents(&s.vault.list().unwrap()).unwrap();
    let vars = &contents.0["production"];
    assert_eq!(vars.keys().collect::<Vec<_>>(), ["DB_URL"]);
    assert_eq!(vars["DB_URL"], "postgres://app:hunter2@db.internal/app");
}

#[test]
fn tampered_or_moved_snapshots_are_refused() {
    let s = setup();
    let share = standing(&s, vec![whole(&s.production)], now() + DAY);
    let contents = share.contents(&s.vault.list().unwrap()).unwrap();
    let sealed = share.seal(s.vault.id(), &contents, s.vault.stamp(s.device), now()).unwrap();
    let token = ShareToken::parse(&share.token(s.vault.id(), URL).unwrap()).unwrap();

    let mut flipped: Value = serde_json::from_slice(&sealed).unwrap();
    let ct = flipped["sealed"]["ct"].as_str().unwrap().to_string();
    let mut bytes = STANDARD.decode(ct).unwrap();
    bytes[10] ^= 1;
    flipped["sealed"]["ct"] = STANDARD.encode(&bytes).into();
    assert!(matches!(token.open(&serde_json::to_vec(&flipped).unwrap(), None, now()), Err(Error::SnapshotTampered)));

    // The same key, but a token for another vault: the AAD doesn't match.
    let elsewhere = ShareToken::parse(&share.token(Uuid::new_v4(), URL).unwrap()).unwrap();
    assert!(matches!(elsewhere.open(&sealed, None, now()), Err(Error::SnapshotTampered)));

    // Another share's token can't open it either.
    let other = standing(&s, vec![whole(&s.production)], now() + DAY);
    let other = ShareToken::parse(&other.token(s.vault.id(), URL).unwrap()).unwrap();
    assert!(matches!(other.open(&sealed, None, now()), Err(Error::SnapshotTampered)));

    let mut wrong_format: Value = serde_json::from_slice(&sealed).unwrap();
    wrong_format["version"] = 2.into();
    assert!(matches!(token.open(&serde_json::to_vec(&wrong_format).unwrap(), None, now()), Err(Error::UnsupportedVersion { .. })));
    assert!(token.open(b"<html>not found</html>", None, now()).is_err());
}

#[test]
fn older_snapshots_are_refused_once_a_newer_one_was_seen() {
    let s = setup();
    let share = standing(&s, vec![whole(&s.production)], now() + DAY);
    let contents = share.contents(&s.vault.list().unwrap()).unwrap();
    let token = ShareToken::parse(&share.token(s.vault.id(), URL).unwrap()).unwrap();
    let old_stamp = s.vault.stamp(s.device);
    let old = share.seal(s.vault.id(), &contents, old_stamp, now()).unwrap();
    let new_stamp = s.vault.stamp(s.device);
    let new = share.seal(s.vault.id(), &contents, new_stamp, now()).unwrap();

    assert_eq!(token.open(&new, None, now()).unwrap().stamp, new_stamp);
    assert!(token.open(&new, Some(new_stamp), now()).is_ok(), "the same snapshot again is fine");
    assert!(token.open(&new, Some(old_stamp), now()).is_ok());
    let err = token.open(&old, Some(new_stamp), now()).unwrap_err();
    assert!(err.to_string().contains("older than one this machine already used"), "{err}");
}

#[test]
fn expiry_and_leases_are_checked() {
    let s = setup();
    let created = now();
    let share = standing(&s, vec![whole(&s.production)], created + 30 * DAY);
    let contents = share.contents(&s.vault.list().unwrap()).unwrap();
    let token = ShareToken::parse(&share.token(s.vault.id(), URL).unwrap()).unwrap();
    let sealed = share.seal(s.vault.id(), &contents, s.vault.stamp(s.device), created).unwrap();
    assert!(token.open(&sealed, None, created + 13 * DAY).is_ok());
    let err = token.open(&sealed, None, created + 14 * DAY).unwrap_err();
    assert!(err.to_string().contains("lease ran out"), "{err}");

    // The lease never outlasts the share.
    let short = standing(&s, vec![whole(&s.production)], created + DAY);
    assert_eq!(short.lease_until(created), created + DAY);
    let token = ShareToken::parse(&short.token(s.vault.id(), URL).unwrap()).unwrap();
    let sealed = short.seal(s.vault.id(), &contents, s.vault.stamp(s.device), created).unwrap();
    let err = token.open(&sealed, None, created + DAY).unwrap_err();
    assert!(err.to_string().contains("expired"), "{err}");
}

#[test]
fn status_follows_the_vault() {
    let s = setup();
    let mut share = standing(&s, vec![whole(&s.staging)], now() + DAY);
    let records = s.vault.list().unwrap();
    assert_eq!(share.status(&records, now()).unwrap(), Status::NeverPushed);
    share.pushed = Some(Pushed { sha256: share.digest(&share.contents(&records).unwrap()), at: now() });
    assert_eq!(share.status(&records, now()).unwrap(), Status::Current);

    // An edit elsewhere doesn't matter; one up the parent chain does.
    let other = Environment::new(s.project.id, "development", s.vault.stamp(s.device));
    s.vault.put(Record::Environment(other)).unwrap();
    assert_eq!(share.status(&s.vault.list().unwrap(), now()).unwrap(), Status::Current);
    let mut secret = s.secret.clone();
    secret.rotate("sk_live_2", None, s.vault.stamp(s.device)).unwrap();
    s.vault.put(Record::Secret(secret)).unwrap();
    assert_eq!(share.status(&s.vault.list().unwrap(), now()).unwrap(), Status::OutOfDate);

    assert_eq!(share.status(&records, share.expires).unwrap(), Status::Expired);
    share.revoked = Some(now());
    assert_eq!(share.status(&records, now()).unwrap(), Status::Revoked);
    assert!(!share.is_live(now()));
}

#[test]
fn a_revoked_share_exposes_what_it_could_read() {
    let s = setup();
    let narrow = ShareEnv { id: s.staging.id, only: Some(vec!["DB_URL".into()]) };
    let share = standing(&s, vec![narrow], now() + DAY);
    let records = s.vault.list().unwrap();
    assert!(!share.holds_secret(&records, s.secret.id).unwrap());
    std::thread::sleep(std::time::Duration::from_millis(5));
    let revoked_at = now();

    // DB_URL, and the values it refers to, wherever they're set.
    let exposed = share.exposed(&records, revoked_at);
    assert!(exposed.secrets.is_empty());
    let mut vars: Vec<(Uuid, &str)> = exposed.variables.iter().map(|(e, k)| (*e, k.as_str())).collect();
    vars.sort_by_key(|(_, k)| *k);
    assert_eq!(vars, [(s.staging.id, "DB_HOST"), (s.production.id, "DB_PASS"), (s.production.id, "DB_URL")]);

    let whole_share = standing(&s, vec![whole(&s.production)], now() + DAY);
    assert!(whole_share.holds_secret(&records, s.secret.id).unwrap());
    let exposed = whole_share.exposed(&records, revoked_at);
    assert_eq!(exposed.secrets, [s.secret.id]);
    assert_eq!(exposed.variables.len(), 4, "linked variables are covered by their secret");
    assert!(whole_share.exposed(&records, 0).variables.is_empty(), "nothing older than the start of time");
}

#[test]
fn tokens_are_checked() {
    let s = setup();
    let share = standing(&s, vec![whole(&s.production)], now() + DAY);
    let token = share.token(s.vault.id(), URL).unwrap();
    assert!(ShareToken::parse(&token.to_lowercase()).is_ok(), "case doesn't matter");
    for bad in ["", "dss_", "sk_live_123", "dss_!!!", &token[..40]] {
        assert!(ShareToken::parse(bad).is_err(), "{bad:?}");
    }
    assert!(share.token(s.vault.id(), "http://shares.example.com").is_err(), "HTTPS only");
    assert!(!format!("{:?}", ShareToken::parse(&token).unwrap()).contains(&token[4..20]), "Debug hides the key");
}

#[test]
fn environments_with_the_same_name_are_labelled_by_project() {
    let s = setup();
    let web = Project::new("web", s.vault.stamp(s.device));
    s.vault.put(Record::Project(web.clone())).unwrap();
    let mut web_production = Environment::new(web.id, "production", s.vault.stamp(s.device));
    web_production.set("PORT", "3000", s.vault.stamp(s.device));
    s.vault.put(Record::Environment(web_production.clone())).unwrap();
    let share = standing(&s, vec![whole(&s.production), whole(&web_production), whole(&s.staging)], now() + DAY);
    let records = s.vault.list().unwrap();
    assert_eq!(share.labels(&records).unwrap(), ["api/production", "web/production", "staging"]);

    let contents = share.contents(&records).unwrap();
    let sealed = share.seal(s.vault.id(), &contents, s.vault.stamp(s.device), now()).unwrap();
    let snapshot = ShareToken::parse(&share.token(s.vault.id(), URL).unwrap()).unwrap().open(&sealed, None, now()).unwrap();
    assert!(snapshot.environment(Some("production")).is_err(), "ambiguous");
    assert_eq!(snapshot.environment(Some("web/production")).unwrap().1["PORT"], "3000");
    assert_eq!(snapshot.environment(Some("staging")).unwrap().0, "staging");
}

#[test]
fn live_shares_are_counted_and_get_expiry_rules() {
    let s = setup();
    let t = now();
    let live = standing(&s, vec![whole(&s.production)], t + DAY);
    let expired = standing(&s, vec![whole(&s.production)], t - 1);
    let mut revoked = standing(&s, vec![whole(&s.production)], t + DAY);
    revoked.revoked = Some(t);
    let mut revoked_not_cleaned = revoked.clone();
    revoked_not_cleaned.id = Uuid::new_v4();
    revoked_not_cleaned.pushed = Some(Pushed { sha256: "aa".into(), at: t });
    let records: Vec<Record> =
        [&live, &expired, &revoked, &revoked_not_cleaned].into_iter().map(|s| Record::Share(s.clone())).collect();
    assert_eq!(share::live_count(&records, t), 1);
    let rules = share::expiry_rules(&records, t).unwrap();
    let ids: Vec<Uuid> = rules.iter().map(|r| r.share).collect();
    assert!(ids.contains(&live.id) && ids.contains(&revoked_not_cleaned.id) && ids.len() == 2, "{ids:?}");
    assert!(rules.iter().all(|r| r.prefix.starts_with("s/") && r.prefix.len() == 34));
}
