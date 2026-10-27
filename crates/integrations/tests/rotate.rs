//! Provider rotation and pushes against a local fake of each service's API.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use dotsafe_core::{
    Binding, Connection, FolderStore, KdfParams, ProviderLink, Record, Secret, Target, VersionState, Vault,
};
use dotsafe_integrations::{Client, check_settings, rotate, service};
use serde_json::{Value, json};
use uuid::Uuid;

// Cheap Argon2 settings so tests run fast. Never use these for real vaults.
const FAST: KdfParams = KdfParams { m_cost_kib: 64, t_cost: 1, p_cost: 1 };

/// A request the fake received.
#[derive(Debug, Clone)]
struct Seen {
    method: String,
    path: String,
    auth: String,
    body: Value,
}

/// Answers by method and path; records every request.
struct Fake {
    url: String,
    seen: Arc<Mutex<Vec<Seen>>>,
}

impl Fake {
    fn start(answer: impl Fn(&str, &str, &str) -> (u16, Value) + Send + 'static) -> Fake {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let url = format!("http://{}", server.server_addr().to_ip().unwrap());
        let seen = Arc::new(Mutex::new(Vec::new()));
        let log = seen.clone();
        std::thread::spawn(move || {
            for mut request in server.incoming_requests() {
                let mut body = String::new();
                request.as_reader().read_to_string(&mut body).unwrap();
                let auth = request
                    .headers()
                    .iter()
                    .find(|h| h.field.equiv("Authorization"))
                    .map(|h| h.value.to_string())
                    .unwrap_or_default();
                let method = request.method().to_string();
                let path = request.url().to_string();
                let (status, reply) = answer(&method, &path, &auth);
                log.lock().unwrap().push(Seen {
                    method,
                    path,
                    auth,
                    body: serde_json::from_str(&body).unwrap_or(Value::Null),
                });
                let response = tiny_http::Response::from_string(reply.to_string()).with_status_code(status);
                let _ = request.respond(response);
            }
        });
        Fake { url, seen }
    }

    fn seen(&self) -> Vec<Seen> {
        self.seen.lock().unwrap().clone()
    }
}

struct Setup {
    _dir: tempfile::TempDir,
    vault: Vault,
    device: Uuid,
    secret: Uuid,
    vercel: Uuid,
}

/// A vault with a secret issued by `service`, its first key id known, and a
/// Vercel target.
fn setup(service: &str, connection_settings: &[(&str, &str)], link_settings: &[(&str, &str)]) -> Setup {
    let dir = tempfile::tempdir().unwrap();
    let vault = Vault::create(FolderStore::new(dir.path()), "pw", FAST).unwrap();
    let device = Uuid::new_v4();
    let map = |pairs: &[(&str, &str)]| -> BTreeMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    };
    let connect = |service: &str, token: &str, settings| {
        let credential = Secret::new(format!("{service} admin"), token, vault.stamp(device));
        vault.put(Record::Secret(credential.clone())).unwrap();
        let c = Connection::new(service, &format!("Acme {service}"), credential.id, settings, vault.stamp(device));
        vault.put(Record::Connection(c.clone())).unwrap();
        c.id
    };
    let provider = connect(service, "admin-token", map(connection_settings));
    let vercel = connect("vercel", "vercel-token", map(&[("team_id", "team_1")]));

    let secret = Secret::new("API_KEY", "old-key", vault.stamp(device));
    vault.put(Record::Secret(secret.clone())).unwrap();
    let mut binding = Binding::new(secret.id, vault.stamp(device));
    binding.provider = Some(ProviderLink { connection: provider, settings: map(link_settings) });
    binding.keys.insert(secret.current().unwrap().0, "old-key-id".into());
    binding.targets.insert(
        Uuid::new_v4(),
        Target {
            connection: vercel,
            name: "API_KEY".into(),
            settings: map(&[("project", "web app"), ("environments", "production")]),
            pushed: None,
            deleted: false,
            changed: vault.stamp(device),
        },
    );
    vault.put(Record::Binding(binding)).unwrap();
    Setup { _dir: dir, vault, device, secret: secret.id, vercel }
}

fn secret(vault: &Vault, id: Uuid) -> Secret {
    match vault.get(id).unwrap() {
        Some(Record::Secret(s)) => s,
        other => panic!("{other:?}"),
    }
}

fn binding(vault: &Vault, id: Uuid) -> Binding {
    match vault.get(Binding::id_for(id)).unwrap() {
        Some(Record::Binding(b)) => b,
        other => panic!("{other:?}"),
    }
}

fn states(s: &Secret) -> Vec<(String, VersionState)> {
    s.versions().into_iter().map(|(_, v)| (v.value.clone().unwrap_or_default(), v.state)).collect()
}

/// OpenAI and Vercel, both answering yes.
fn openai_fake(models_status: u16) -> Fake {
    Fake::start(move |method, path, _| match (method, path) {
        ("POST", "/organization/projects/proj_1/service_accounts") => {
            (200, json!({ "id": "svc_new", "api_key": { "id": "key_x", "value": "sk-new" } }))
        }
        ("GET", "/models") => (models_status, json!({ "error": { "message": "Incorrect API key" } })),
        ("DELETE", p) if p.starts_with("/organization/projects/proj_1/service_accounts/") => (200, json!({})),
        ("POST", p) if p.starts_with("/v10/projects/") => (200, json!({ "created": {} })),
        _ => (404, json!({ "error": { "message": "no such route" } })),
    })
}

#[test]
fn openai_rotation_issues_verifies_promotes_and_pushes() {
    let s = setup("openai", &[("project_id", "proj_1")], &[]);
    let fake = openai_fake(200);
    let client = Client::with_base_for_tests(&fake.url);

    let out = rotate::rotate(&s.vault, &client, s.device, s.secret, false).unwrap();
    assert!(out.promoted && out.staged.is_none() && !out.revoked_old);
    assert_eq!(out.pushes.len(), 1);
    assert!(out.pushes[0].error.is_none(), "{:?}", out.pushes[0].error);

    let stored = secret(&s.vault, s.secret);
    assert_eq!(states(&stored), [("sk-new".into(), VersionState::Current), ("old-key".into(), VersionState::Previous)]);
    let b = binding(&s.vault, s.secret);
    assert_eq!(b.keys[&stored.current().unwrap().0], "svc_new");
    let pushed = b.targets().next().unwrap().1.pushed.clone().expect("push recorded");
    assert_eq!(pushed.sha256, dotsafe_core::crypto::fingerprint("sk-new"));

    let seen = fake.seen();
    let create = &seen[0];
    assert_eq!((create.method.as_str(), create.auth.as_str()), ("POST", "Bearer admin-token"));
    assert!(create.body["name"].as_str().unwrap().starts_with("dotsafe API_KEY"));
    assert_eq!(seen[1].auth, "Bearer sk-new", "verified with the new key");
    // Settings are path segments, so a space (or a slash) can't change the path.
    let push = seen.iter().find(|r| r.path.starts_with("/v10/")).unwrap();
    assert_eq!(push.path, "/v10/projects/web%20app/env?upsert=true&teamId=team_1");
    assert_eq!(push.auth, "Bearer vercel-token");
    assert_eq!(push.body, json!({ "key": "API_KEY", "value": "sk-new", "type": "encrypted", "target": ["production"] }));

    // Each step is in the audit log, as format v4 days.
    let events = dotsafe_core::audit::recent(&s.vault, 10).unwrap();
    let logged: Vec<String> = events.iter().rev().map(|e| format!("{:?} {}", e.action, e.summary)).collect();
    assert_eq!(logged, ["Rotate API_KEY: new key from Acme openai", "Push API_KEY: pushed to Acme vercel as API_KEY"]);
    let day = s.vault.list().unwrap().into_iter().find(|r| matches!(r, Record::Audit(_))).unwrap();
    let path = s._dir.path().join("records").join(format!("{}.json", day.id()));
    let file: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(file["version"], 4);

    // Retiring revokes the old service account, then marks it retired.
    assert_eq!(rotate::retire(&s.vault, &client, s.device, s.secret).unwrap(), 1);
    let revoke = fake.seen().into_iter().find(|r| r.method == "DELETE").unwrap();
    assert_eq!(revoke.path, "/organization/projects/proj_1/service_accounts/old-key-id");
    assert_eq!(states(&secret(&s.vault, s.secret))[1].1, VersionState::Retired);
    let latest = &dotsafe_core::audit::recent(&s.vault, 1).unwrap()[0];
    assert_eq!(latest.summary, "API_KEY: revoked the old key at Acme openai");
}

#[test]
fn a_key_that_fails_its_check_is_saved_but_left_staged() {
    let s = setup("openai", &[("project_id", "proj_1")], &[]);
    let fake = openai_fake(401);
    let client = Client::with_base_for_tests(&fake.url);

    let out = rotate::rotate(&s.vault, &client, s.device, s.secret, false).unwrap();
    assert!(!out.promoted);
    assert!(out.staged.as_deref().unwrap().contains("Incorrect API key"), "{:?}", out.staged);
    let stored = secret(&s.vault, s.secret);
    assert_eq!(stored.current_value(), Some("old-key"), "nothing changed for the apps");
    let (pending, v) = stored.pending().unwrap();
    assert_eq!(v.value.as_deref(), Some("sk-new"), "the new key isn't lost");
    assert_eq!(binding(&s.vault, s.secret).keys[&pending], "svc_new");
    assert!(fake.seen().iter().all(|r| !r.path.starts_with("/v10/")), "nothing was pushed");

    // A second rotation waits until the staged value is dealt with.
    assert!(rotate::rotate(&s.vault, &client, s.device, s.secret, false).is_err());
}

#[test]
fn leaked_keys_are_revoked_straight_away() {
    let s = setup("openai", &[("project_id", "proj_1")], &[]);
    let fake = openai_fake(200);
    let client = Client::with_base_for_tests(&fake.url);
    let out = rotate::rotate(&s.vault, &client, s.device, s.secret, true).unwrap();
    assert!(out.revoked_old);
    assert!(fake.seen().iter().any(|r| r.method == "DELETE" && r.path.ends_with("/old-key-id")));
    assert_eq!(states(&secret(&s.vault, s.secret))[1].1, VersionState::Retired);
}

#[test]
fn cloudflare_rolls_go_live_at_once_because_the_old_token_is_dead() {
    let s = setup("cloudflare_token", &[], &[("token_id", "tok_1")]);
    let fake = Fake::start(|method, path, _| match (method, path) {
        ("PUT", "/user/tokens/tok_1/value") => (200, json!({ "success": true, "result": "cf-new" })),
        // Even if the check fails, the new token is the only one left.
        ("GET", "/user/tokens/verify") => (200, json!({ "success": true, "result": { "status": "disabled" } })),
        ("POST", _) => (200, json!({})),
        _ => (404, json!({ "success": false, "errors": [{ "message": "no route" }] })),
    });
    let client = Client::with_base_for_tests(&fake.url);

    let out = rotate::rotate(&s.vault, &client, s.device, s.secret, false).unwrap();
    assert!(out.staged.as_deref().unwrap().contains("disabled"));
    assert!(out.promoted && out.revoked_old);
    let stored = secret(&s.vault, s.secret);
    assert_eq!(states(&stored), [("cf-new".into(), VersionState::Current), ("old-key".into(), VersionState::Retired)]);
    assert!(fake.seen().iter().all(|r| r.method != "DELETE"), "rolling already ended the old token");
}

#[test]
fn push_failures_are_reported_per_target() {
    let s = setup("openai", &[("project_id", "proj_1")], &[]);
    // A second target, on a Worker, that refuses.
    let mut b = binding(&s.vault, s.secret);
    let credential = Secret::new("cf", "cf-token", s.vault.stamp(s.device));
    s.vault.put(Record::Secret(credential.clone())).unwrap();
    let settings = BTreeMap::from([("account_id".to_string(), "acc_1".to_string())]);
    let workers = Connection::new("cloudflare_workers", "Workers", credential.id, settings, s.vault.stamp(s.device));
    s.vault.put(Record::Connection(workers.clone())).unwrap();
    b.targets.insert(
        Uuid::new_v4(),
        Target {
            connection: workers.id,
            name: "API_KEY".into(),
            settings: BTreeMap::from([("script".to_string(), "api".to_string())]),
            pushed: None,
            deleted: false,
            changed: s.vault.stamp(s.device),
        },
    );
    s.vault.put(Record::Binding(b)).unwrap();
    let fake = Fake::start(|method, path, _| match (method, path) {
        ("POST", _) => (200, json!({})),
        ("PUT", "/accounts/acc_1/workers/scripts/api/secrets") => {
            (403, json!({ "success": false, "errors": [{ "message": "Authentication error" }] }))
        }
        _ => (404, json!({})),
    });

    let results = rotate::push(&s.vault, &Client::with_base_for_tests(&fake.url), s.device, s.secret).unwrap();
    let mut outcomes: Vec<_> = results.iter().map(|r| r.error.clone()).collect();
    outcomes.sort();
    assert_eq!(outcomes, [None, Some("Cloudflare said no (403): Authentication error".into())]);
    let b = binding(&s.vault, s.secret);
    let pushed: Vec<bool> = b.targets().map(|(_, t)| t.pushed.is_some()).collect();
    assert_eq!(pushed.iter().filter(|p| **p).count(), 1, "only the successful push is recorded");
    let worker_call = fake.seen().into_iter().find(|r| r.method == "PUT").unwrap();
    assert_eq!(worker_call.body, json!({ "name": "API_KEY", "text": "old-key", "type": "secret_text" }));
    let _ = s.vercel;
}

#[test]
fn settings_are_checked_against_each_service() {
    let openai = service("openai").unwrap();
    let ok = BTreeMap::from([("project_id".to_string(), "proj_1".to_string())]);
    assert!(check_settings(openai.connection_fields, &ok).is_ok());
    assert!(check_settings(openai.connection_fields, &BTreeMap::new()).is_err(), "project_id is required");
    let extra = BTreeMap::from([("project_id".to_string(), "p".to_string()), ("evil".to_string(), "x".to_string())]);
    assert!(check_settings(openai.connection_fields, &extra).is_err());
    assert!(service("nope").is_err());
}
