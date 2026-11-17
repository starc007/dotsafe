//! `dotsafe share` and `dotsafe run --share` end to end: the real binary,
//! a temp vault, and a stand-in S3 bucket as the share bucket. The keychain
//! is off; the bucket's write key comes from the environment.
//!
//! A machine fetches its snapshot over HTTPS only, which a local stand-in
//! can't serve, so the machine side is covered here up to the fetch, and
//! from the fetch on by the core and stores tests.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

const PASSWORD: &str = "correct horse battery";
/// Nothing listens here, so a machine's fetch fails fast instead of reaching
/// anything real.
const PUBLIC_URL: &str = "https://127.0.0.1:9";

#[derive(Default)]
struct Bucket {
    objects: Mutex<BTreeMap<String, (Vec<u8>, String)>>,
    lifecycle: Mutex<Option<String>>,
    down: AtomicBool,
}

impl Bucket {
    fn snapshots(&self) -> Vec<String> {
        self.objects.lock().unwrap().keys().cloned().collect()
    }
}

fn serve(bucket: Arc<Bucket>) -> String {
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let port = server.server_addr().to_ip().unwrap().port();
    std::thread::spawn(move || {
        for mut req in server.incoming_requests() {
            let mut body = Vec::new();
            req.as_reader().read_to_end(&mut body).unwrap();
            if bucket.down.load(Ordering::SeqCst) {
                let _ = req.respond(tiny_http::Response::from_string("").with_status_code(503));
                continue;
            }
            let url = url::Url::parse(&format!("http://local{}", req.url())).unwrap();
            let key = url.path().trim_start_matches("/shares").trim_start_matches('/').to_string();
            let lifecycle = url.query_pairs().any(|(k, _)| k == "lifecycle");
            let cache = req.headers().iter().find(|h| h.field.equiv("Cache-Control")).map(|h| h.value.to_string());
            let response = match (req.method().as_str(), lifecycle) {
                ("GET", true) => match bucket.lifecycle.lock().unwrap().clone() {
                    Some(xml) => tiny_http::Response::from_string(xml),
                    None => tiny_http::Response::from_string("<Code>NoSuchLifecycleConfiguration</Code>").with_status_code(404),
                },
                ("PUT", true) => {
                    *bucket.lifecycle.lock().unwrap() = Some(String::from_utf8(body).unwrap());
                    tiny_http::Response::from_string("")
                }
                ("DELETE", true) => {
                    *bucket.lifecycle.lock().unwrap() = None;
                    tiny_http::Response::from_string("").with_status_code(204)
                }
                ("PUT", false) => {
                    bucket.objects.lock().unwrap().insert(key, (body, cache.unwrap_or_default()));
                    tiny_http::Response::from_string("")
                }
                ("DELETE", false) => {
                    bucket.objects.lock().unwrap().remove(&key);
                    tiny_http::Response::from_string("").with_status_code(204)
                }
                _ => tiny_http::Response::from_string("").with_status_code(405),
            };
            let _ = req.respond(response);
        }
    });
    format!("http://127.0.0.1:{port}")
}

struct Machine {
    config: tempfile::TempDir,
    vault: PathBuf,
    licensed: bool,
}

impl Machine {
    fn new(vault: &Path) -> Machine {
        let m = Machine { config: tempfile::tempdir().unwrap(), vault: vault.to_path_buf(), licensed: true };
        let license = r#"{"key":"DOTSAFE-TEST-KEY","instance_id":"lki_test","device_name":"Test","activated_at":1,"checked_at":1}"#;
        fs::write(m.config.path().join("license.json"), license).unwrap();
        m
    }

    /// A CI runner or server: no vault, no license.
    fn bare() -> Machine {
        Machine { config: tempfile::tempdir().unwrap(), vault: PathBuf::new(), licensed: false }
    }

    fn cmd(&self, cwd: &Path, args: &[&str]) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_dotsafe"));
        cmd.current_dir(cwd)
            .args(args)
            .env("DOTSAFE_CONFIG_DIR", self.config.path())
            .env("DOTSAFE_NO_KEYCHAIN", "1")
            .env_remove("DOTSAFE_SHARE");
        if self.licensed {
            cmd.env("DOTSAFE_PASSWORD", PASSWORD)
                .env("DOTSAFE_SHARE_BUCKET_ACCESS_KEY_ID", "AKIATEST")
                .env("DOTSAFE_SHARE_BUCKET_SECRET_ACCESS_KEY", "secret");
        }
        cmd
    }

    fn run(&self, cwd: &Path, args: &[&str]) -> Output {
        self.cmd(cwd, args).output().unwrap()
    }

    fn ok(&self, cwd: &Path, args: &[&str]) -> Output {
        let out = self.run(cwd, args);
        assert!(out.status.success(), "dotsafe {args:?} failed: {}", stderr(&out));
        out
    }
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// A licensed machine with a vault, a project and a share bucket.
fn setup() -> (tempfile::TempDir, Machine, PathBuf, Arc<Bucket>) {
    let root = tempfile::tempdir().unwrap();
    let m = Machine::new(&root.path().join("vault"));
    let project = root.path().join("my-api");
    fs::create_dir(&project).unwrap();
    m.ok(m.config.path(), &["init", "--path", m.vault.to_str().unwrap()]);
    m.ok(&project, &["set", "DATABASE_URL=postgres://prod", "PORT=8080"]);
    m.ok(&project, &["set", "-e", "production", "DATABASE_URL=postgres://prod", "STRIPE_KEY=sk_live_1"]);
    let bucket = Arc::new(Bucket::default());
    let endpoint = serve(bucket.clone());
    m.ok(
        &project,
        &["share", "setup", "--bucket", "shares", "--endpoint", &endpoint, "--public-url", PUBLIC_URL, "--no-check"],
    );
    (root, m, project, bucket)
}

#[test]
fn creating_listing_and_pushing_a_share() {
    let (_root, m, project, bucket) = setup();
    let out = m.ok(&project, &["share", "create", "ci", "-e", "production", "--for", "30d"]);
    let token = stdout(&out).trim().to_string();
    assert!(token.starts_with("dss_") && !token.contains('\n'), "only the token is on stdout: {token:?}");
    assert!(stderr(&out).contains("Created share ci for production"), "{}", stderr(&out));
    assert!(stderr(&out).contains("shown once"));

    // The snapshot is sealed, uncached, and the bucket deletes it at expiry.
    let objects = bucket.objects.lock().unwrap().clone();
    assert_eq!(objects.len(), 1);
    let (key, (bytes, cache)) = objects.into_iter().next().unwrap();
    assert!(key.starts_with("s/") && key.len() == 34, "{key}");
    assert_eq!(cache, "no-store");
    let text = String::from_utf8(bytes.clone()).unwrap();
    assert!(text.contains("dotsafe-share-snapshot") && !text.contains("sk_live_1") && !text.contains("STRIPE_KEY"));
    let rules = bucket.lifecycle.lock().unwrap().clone().unwrap();
    assert!(rules.contains(&format!("<Prefix>{key}</Prefix>")), "{rules}");

    // The token is never written to disk.
    for entry in walk(m.config.path()).into_iter().chain(walk(&m.vault)) {
        assert!(!fs::read_to_string(&entry).unwrap_or_default().contains(&token[4..40]), "{} holds the token", entry.display());
    }

    let list = stdout(&m.ok(&project, &["share", "list"]));
    assert!(list.starts_with("ci\tproduction\tup to date\tuntil "), "{list}");

    // A write elsewhere doesn't push; a write to production does.
    let out = m.ok(&project, &["set", "PORT=9090"]);
    assert!(!stderr(&out).contains("pushed share"), "{}", stderr(&out));
    assert_eq!(bucket.objects.lock().unwrap()[&key].0, bytes);
    let out = m.ok(&project, &["set", "-e", "production", "STRIPE_KEY=sk_live_2"]);
    assert!(stderr(&out).contains("dotsafe: pushed share ci."), "{}", stderr(&out));
    assert_ne!(bucket.objects.lock().unwrap()[&key].0, bytes);
    assert!(stdout(&m.ok(&project, &["share", "list"])).contains("up to date"));

    // A bucket that's down leaves the share out of date, and the write still works.
    bucket.down.store(true, Ordering::SeqCst);
    let out = m.ok(&project, &["set", "-e", "production", "STRIPE_KEY=sk_live_3"]);
    assert!(stderr(&out).contains("couldn't update share ci"), "{}", stderr(&out));
    assert!(stdout(&m.ok(&project, &["share", "list"])).contains("out of date"));
    assert!(!m.run(&project, &["share", "push"]).status.success());
    bucket.down.store(false, Ordering::SeqCst);
    let out = m.ok(&project, &["share", "push"]);
    assert!(stderr(&out).contains("pushed share ci"), "{}", stderr(&out));
    assert!(stderr(&m.ok(&project, &["share", "push"])).contains("Every share is up to date"));
}

#[test]
fn only_and_bad_environments_are_checked() {
    let (_root, m, project, _bucket) = setup();
    let out = m.run(&project, &["share", "create", "x", "-e", "production", "--only", "NOPE"]);
    assert!(stderr(&out).contains("production has no NOPE"), "{}", stderr(&out));
    let out = m.run(&project, &["share", "create", "x", "-e", "staging"]);
    assert!(!out.status.success());
    let out = m.ok(&project, &["share", "create", "agent", "-e", "my-api/production", "--only", "STRIPE_KEY", "--for", "24h"]);
    assert!(stdout(&out).starts_with("dss_"));
    assert!(stderr(&m.run(&project, &["share", "create", "agent", "-e", "production"])).contains("already a live share named agent"));
}

#[test]
fn there_is_no_cap_and_revoking_deletes_the_snapshot() {
    let (_root, m, project, bucket) = setup();
    for n in 0..6 {
        m.ok(&project, &["share", "create", &format!("ci-{n}"), "-e", "development"]);
    }
    assert_eq!(bucket.snapshots().len(), 6);

    let out = m.ok(&project, &["share", "revoke", "ci-0"]);
    assert!(stderr(&out).contains("Revoked ci-0. Its snapshot is deleted"), "{}", stderr(&out));
    assert_eq!(bucket.snapshots().len(), 5);
    assert!(stdout(&m.ok(&project, &["share", "list"])).contains("ci-0\tdevelopment\trevoked\trevoked "));
    m.ok(&project, &["share", "create", "ci-0", "-e", "development"]);
    assert!(stderr(&m.run(&project, &["share", "revoke", "nope"])).contains("no share named"));
}

#[test]
fn a_revoked_share_goes_on_the_rotation_queue() {
    let (_root, m, project, bucket) = setup();
    m.ok(&project, &["secret", "share", "STRIPE_KEY", "-e", "production"]);
    m.ok(&project, &["share", "create", "ci", "-e", "production"]);
    std::thread::sleep(std::time::Duration::from_millis(20));

    // Unreachable at revoke time: revoked anyway, deleted on the next write.
    bucket.down.store(true, Ordering::SeqCst);
    let out = m.ok(&project, &["share", "revoke", "ci"]);
    assert!(stderr(&out).contains("couldn't be deleted yet"), "{}", stderr(&out));
    bucket.down.store(false, Ordering::SeqCst);
    assert_eq!(bucket.snapshots().len(), 1);
    let out = m.ok(&project, &["set", "PORT=1"]);
    assert!(stderr(&out).contains("deleted the snapshot of share ci"), "{}", stderr(&out));
    assert!(bucket.snapshots().is_empty());

    let out = m.ok(&project, &["secret", "queue"]);
    assert_eq!(stdout(&out), "secret\tSTRIPE_KEY\trotate by hand\nvariable\tmy-api/production/DATABASE_URL\n");
    assert!(stderr(&out).contains("2 values to rotate since revoking share ci"), "{}", stderr(&out));
    m.ok(&project, &["set", "-e", "production", "DATABASE_URL=postgres://new"]);
    assert_eq!(stdout(&m.ok(&project, &["secret", "queue"])), "secret\tSTRIPE_KEY\trotate by hand\n");
    m.ok(&project, &["secret", "queue", "--dismiss"]);
    assert!(stderr(&m.run(&project, &["secret", "queue"])).contains("Nothing to rotate"));
}

#[test]
fn retiring_waits_for_shares_that_hold_the_old_value() {
    let (_root, m, project, bucket) = setup();
    m.ok(&project, &["secret", "share", "STRIPE_KEY", "-e", "production"]);
    m.ok(&project, &["share", "create", "ci", "-e", "production"]);

    bucket.down.store(true, Ordering::SeqCst);
    let mut rotate = m.cmd(&project, &["secret", "rotate", "STRIPE_KEY", "--generate"]);
    let out = rotate.output().unwrap();
    assert!(out.status.success(), "{}", stderr(&out));
    let out = m.run(&project, &["secret", "retire", "STRIPE_KEY"]);
    assert!(!out.status.success());
    assert!(stderr(&out).contains("share ci holds STRIPE_KEY"), "{}", stderr(&out));
    assert!(stderr(&out).contains("--force"));

    bucket.down.store(false, Ordering::SeqCst);
    m.ok(&project, &["share", "push"]);
    assert!(stderr(&m.ok(&project, &["secret", "retire", "STRIPE_KEY"])).contains("Retired the previous value"));

    // Forced, it goes ahead even while the share is behind.
    bucket.down.store(true, Ordering::SeqCst);
    m.ok(&project, &["secret", "rotate", "STRIPE_KEY", "--generate"]);
    assert!(!m.run(&project, &["secret", "retire", "STRIPE_KEY"]).status.success());
    m.ok(&project, &["secret", "retire", "STRIPE_KEY", "--force"]);
}

#[test]
fn run_share_needs_no_vault_or_license_on_the_machine() {
    let (root, m, project, _bucket) = setup();
    let token = stdout(&m.ok(&project, &["share", "create", "ci", "-e", "production"])).trim().to_string();
    let ci = Machine::bare();

    // Everything else on the machine still needs a license.
    let out = ci.run(root.path(), &["run", "--", "true"]);
    assert!(stderr(&out).contains("dotsafe license activate"), "{}", stderr(&out));

    let out = ci.run(root.path(), &["run", "--share", "--", "true"]);
    assert!(stderr(&out).contains("set DOTSAFE_SHARE"), "{}", stderr(&out));
    let out = ci.cmd(root.path(), &["run", "--share", "--", "true"]).env("DOTSAFE_SHARE", "sk_live_nope").output().unwrap();
    assert!(stderr(&out).contains("isn't a dotsafe share token"), "{}", stderr(&out));

    // With the token, it goes straight to the share bucket's public URL:
    // past the license check, and without looking for a vault.
    let out = ci.cmd(root.path(), &["run", "--share", "--", "true"]).env("DOTSAFE_SHARE", &token).output().unwrap();
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(err.contains("couldn't reach 127.0.0.1"), "{err}");
    assert!(!err.contains("license") && !err.contains("dotsafe init"), "{err}");
    assert!(!ci.config.path().join("shares").exists(), "nothing remembered from a failed fetch");
    let out = ci.cmd(root.path(), &["run", "--share", "--daemon", "--", "true"]).env("DOTSAFE_SHARE", &token).output().unwrap();
    assert!(stderr(&out).contains("cannot be used with"), "{}", stderr(&out));
}

#[cfg(unix)]
#[test]
fn daemon_and_approve_share_need_only_the_token() {
    let (root, m, project, _bucket) = setup();
    let token = stdout(&m.ok(&project, &["share", "create", "agent", "-e", "production"])).trim().to_string();
    let vm = Machine::bare();
    let with_token = |args: &[&str]| vm.cmd(root.path(), args).env("DOTSAFE_SHARE", &token).output().unwrap();

    let out = vm.run(root.path(), &["daemon", "--share"]);
    assert!(stderr(&out).contains("set DOTSAFE_SHARE"), "{}", stderr(&out));
    let out = vm.run(root.path(), &["daemon", "--unattended"]);
    assert!(stderr(&out).contains("--share"), "--unattended needs --share: {}", stderr(&out));
    let out = vm.cmd(root.path(), &["daemon", "--share", "--detach"]).env("DOTSAFE_SHARE", "sk_live_nope").output().unwrap();
    assert!(stderr(&out).contains("isn't a dotsafe share token"), "checked before detaching: {}", stderr(&out));

    // Past the license check and without a vault, it fetches the snapshot
    // before serving anything, and stops when it can't.
    let out = with_token(&["daemon", "--share", "--unattended"]);
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(err.contains("couldn't reach 127.0.0.1"), "{err}");
    assert!(!err.contains("license") && !err.contains("dotsafe init"), "{err}");
    assert!(!vm.config.path().join("agent.sock").exists(), "nothing served");

    // `approve --share` answers with the token, so it needs no license either.
    let out = with_token(&["approve", "--share", "--list"]);
    assert!(stderr(&out).contains("couldn't reach dotsafe daemon"), "{}", stderr(&out));
    let out = vm.run(root.path(), &["approve", "--list"]);
    assert!(stderr(&out).contains("dotsafe license activate"), "{}", stderr(&out));
}

#[test]
fn shares_need_a_bucket_and_https() {
    let root = tempfile::tempdir().unwrap();
    let m = Machine::new(&root.path().join("vault"));
    m.ok(m.config.path(), &["init", "--path", m.vault.to_str().unwrap()]);
    let project = root.path().join("app");
    fs::create_dir(&project).unwrap();
    m.ok(&project, &["set", "A=1"]);
    let out = m.run(&project, &["share", "create", "ci", "-e", "development"]);
    assert!(stderr(&out).contains("dotsafe share setup"), "{}", stderr(&out));
    let out = m.run(
        &project,
        &["share", "setup", "--bucket", "b", "--endpoint", "http://127.0.0.1:9", "--public-url", "http://shares.example.com", "--no-check"],
    );
    assert!(stderr(&out).contains("must start with https://"), "{}", stderr(&out));
}

fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            files.extend(walk(&path));
        } else {
            files.push(path);
        }
    }
    files
}
