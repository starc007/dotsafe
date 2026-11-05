//! `dotsafe daemon` end to end: the real binary serving the shared broker on
//! the agent socket, without the desktop app, and running `dotsafe run`
//! commands approved with `dotsafe approve`.
#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

use dotsafe_agent::ipc;
use dotsafe_agent::protocol::{AnswerRequest, ErrorCode, PendingInfo, Request, Response, RunRequest, Target};

const PASSWORD: &str = "correct horse battery";

/// A command with the test's config, and the password (for unlocking and approving).
fn dotsafe(config: &Path, cwd: &Path, args: &[&str]) -> Command {
    let mut cmd = without_password(config, cwd, args);
    cmd.env("DOTSAFE_PASSWORD", PASSWORD);
    cmd
}

/// As a cron job on the server runs: no password anywhere.
fn without_password(config: &Path, cwd: &Path, args: &[&str]) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_dotsafe"));
    cmd.current_dir(cwd)
        .args(args)
        .env("DOTSAFE_CONFIG_DIR", config)
        .env("DOTSAFE_NO_KEYCHAIN", "1")
        .env_remove("DOTSAFE_PASSWORD")
        .stdin(Stdio::null());
    cmd
}

struct Daemon(Child);

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

struct Server {
    _root: tempfile::TempDir,
    config: PathBuf,
    project: PathBuf,
    socket: PathBuf,
    _daemon: Daemon,
}

/// A licensed machine with a vault, a linked project with two variables, and
/// a daemon with no terminal, as on a server.
fn server() -> Server {
    let root = tempfile::tempdir().unwrap();
    let config = root.path().join("config");
    let project = root.path().join("my-api");
    std::fs::create_dir_all(&project).unwrap();
    let vault = root.path().join("vault");
    std::fs::create_dir_all(&config).unwrap();
    // A licensed machine, as after `dotsafe license activate`.
    let license = r#"{"key":"DOTSAFE-TEST-KEY","instance_id":"lki_test","device_name":"Test","activated_at":1,"checked_at":1}"#;
    std::fs::write(config.join("license.json"), license).unwrap();
    assert!(dotsafe(&config, root.path(), &["init", "--path", vault.to_str().unwrap()]).output().unwrap().status.success());
    assert!(dotsafe(&config, &project, &["set", "API_KEY=sk_daemon_test"]).output().unwrap().status.success());
    assert!(dotsafe(&config, &project, &["set", "OTHER=other_value_1"]).output().unwrap().status.success());

    let child = dotsafe(&config, root.path(), &["daemon"]).stderr(Stdio::null()).spawn().unwrap();
    let daemon = Daemon(child);
    let socket = config.join("agent.sock");
    let started = Instant::now();
    while ipc::call(&socket, &Request::ListProjects).is_err() {
        assert!(started.elapsed() < Duration::from_secs(20), "the daemon didn't start");
        std::thread::sleep(Duration::from_millis(100));
    }
    Server { _root: root, config, project, socket, _daemon: daemon }
}

fn pending(socket: &Path) -> Vec<PendingInfo> {
    match ipc::call(socket, &Request::Pending).unwrap() {
        Response::Pending(p) => p,
        other => panic!("{other:?}"),
    }
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn run_job(s: &Server, args: &[&str]) -> Output {
    let mut all = vec!["run", "--wait", "1s"];
    all.extend_from_slice(args);
    without_password(&s.config, &s.project, &all).output().unwrap()
}

#[test]
fn the_daemon_serves_agents_and_asks_dotsafe_approve() {
    let s = server();
    let Response::Projects(projects) = ipc::call(&s.socket, &Request::ListProjects).unwrap() else { panic!() };
    assert_eq!(projects[0].name, "my-api");
    let target = Target { project: None, environment: None, cwd: Some(s.project.clone()) };
    let Response::Variables(vars) = ipc::call(&s.socket, &Request::ListVariables(target.clone())).unwrap() else {
        panic!()
    };
    assert_eq!(vars.variables.iter().map(|v| v.name.as_str()).collect::<Vec<_>>(), ["API_KEY", "OTHER"]);

    // An agent's run waits for an answer; `dotsafe approve --deny` gives one.
    let run = RunRequest {
        client: "Test agent".into(),
        command: vec!["true".into()],
        cwd: s.project.clone(),
        target,
        variables: None,
        path: std::env::var("PATH").ok(),
        timeout_secs: Some(5),
        proxy: false,
    };
    let socket = s.socket.clone();
    let agent = std::thread::spawn(move || ipc::call(&socket, &Request::Run(run)).unwrap());
    let started = Instant::now();
    let id = loop {
        if let Some(p) = pending(&s.socket).first() {
            assert_eq!((p.kind.as_str(), p.client.as_str(), p.waiting), ("run", "Test agent", true));
            break p.id.to_string();
        }
        assert!(started.elapsed() < Duration::from_secs(20), "the run never showed up as waiting");
        std::thread::sleep(Duration::from_millis(50));
    };
    let out = dotsafe(&s.config, &s.project, &["approve", &id[..8], "--deny"]).output().unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    match agent.join().unwrap() {
        Response::Error(e) => assert_eq!(e.code, ErrorCode::Denied),
        other => panic!("expected a denial, got {other:?}"),
    }

    // Only one broker at a time.
    let second = dotsafe(&s.config, &s.project, &["daemon"]).output().unwrap();
    assert!(text(&second.stderr).contains("another dotsafe broker"), "{}", text(&second.stderr));
}

#[test]
fn cron_jobs_run_through_the_daemon_once_approved() {
    let s = server();
    let job = s.project.join("job.sh");
    std::fs::write(&job, "#!/bin/sh\necho \"key=$API_KEY other=${OTHER:-none}\"\nexit 3\n").unwrap();
    Command::new("chmod").args(["+x", job.to_str().unwrap()]).status().unwrap();

    // First run: nobody has approved it yet.
    let out = run_job(&s, &["--", "./job.sh"]);
    assert!(!out.status.success());
    assert!(text(&out.stderr).contains("dotsafe approve"), "{}", text(&out.stderr));
    assert!(out.stdout.is_empty(), "it didn't run");

    // It stays open, no longer waiting.
    let list = dotsafe(&s.config, &s.project, &["approve", "--list"]).output().unwrap();
    let list = text(&list.stdout);
    assert!(list.contains("stopped waiting") && list.contains("./job.sh"), "{list}");
    let id = list.split('\t').next().unwrap().to_string();

    // "Once" means nothing for a job that already gave up.
    let once = dotsafe(&s.config, &s.project, &["approve", &id, "--for", "once"]).output().unwrap();
    assert!(!once.status.success());
    assert!(text(&once.stderr).contains("stopped waiting"), "{}", text(&once.stderr));

    // Approving needs the master password.
    let wrong = dotsafe(&s.config, &s.project, &["approve", &id, "--for", "stop"])
        .env("DOTSAFE_PASSWORD", "not it")
        .output()
        .unwrap();
    assert!(!wrong.status.success());
    // A forged answer from any process on the socket is refused.
    let forged = AnswerRequest { id: id.parse().unwrap(), lifetime: Some("stop".into()), proof: "00".repeat(32) };
    match ipc::call(&s.socket, &Request::Answer(forged)).unwrap() {
        Response::Error(e) => assert_eq!(e.code, ErrorCode::Denied),
        other => panic!("{other:?}"),
    }
    assert_eq!(pending(&s.socket).len(), 1, "still waiting for a real answer");

    let ok = dotsafe(&s.config, &s.project, &["approve", &id, "--for", "stop"]).output().unwrap();
    assert!(ok.status.success(), "{}", text(&ok.stderr));
    assert!(pending(&s.socket).is_empty());

    // Now it runs without asking, on our own stdout, with its exit code.
    let out = run_job(&s, &["--", "./job.sh"]);
    assert_eq!(out.status.code(), Some(3), "{}", text(&out.stderr));
    assert_eq!(text(&out.stdout), "key=sk_daemon_test other=other_value_1\n");
    let again = run_job(&s, &["--", "./job.sh"]);
    assert_eq!(again.status.code(), Some(3), "and again: {}", text(&again.stderr));

    // Anything different asks again: fewer keys, other arguments, an edited script.
    for args in [&["--only", "API_KEY", "--", "./job.sh"][..], &["--", "./job.sh", "--all"][..]] {
        let out = run_job(&s, args);
        assert!(!out.status.success() && out.stdout.is_empty(), "{args:?} ran without approval");
    }
    std::fs::write(&job, "#!/bin/sh\necho \"stolen=$API_KEY\"\n").unwrap();
    let edited = run_job(&s, &["--", "./job.sh"]);
    assert!(!edited.status.success() && edited.stdout.is_empty(), "an edited script needs a new approval");

    // Commands that dump the environment are never run.
    let env = run_job(&s, &["--", "env"]);
    assert!(!env.status.success());
    assert!(!text(&env.stdout).contains("sk_daemon_test"));

    // With the password, `dotsafe run` opens the vault itself, as before.
    let local = dotsafe(&s.config, &s.project, &["run", "--only", "OTHER", "--", "sh", "./job.sh"]).output().unwrap();
    assert!(local.status.success(), "{}", text(&local.stderr));
    assert_eq!(text(&local.stdout), "stolen=\n");
}
