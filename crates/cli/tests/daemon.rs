//! `dotsafe daemon` end to end: the real binary serving the shared broker on
//! the agent socket, without the desktop app.
#![cfg(unix)]

use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use dotsafe_agent::ipc;
use dotsafe_agent::protocol::{ErrorCode, Request, Response, RunRequest, Target};

const PASSWORD: &str = "correct horse battery";

fn dotsafe(config: &Path, cwd: &Path, args: &[&str]) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_dotsafe"));
    cmd.current_dir(cwd)
        .args(args)
        .env("DOTSAFE_CONFIG_DIR", config)
        .env("DOTSAFE_NO_KEYCHAIN", "1")
        .env("DOTSAFE_PASSWORD", PASSWORD);
    cmd
}

struct Daemon(Child);

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn the_daemon_serves_agents_and_refuses_without_a_terminal() {
    let root = tempfile::tempdir().unwrap();
    let config = root.path().join("config");
    let project = root.path().join("my-api");
    std::fs::create_dir_all(&project).unwrap();
    let vault = root.path().join("vault");
    assert!(dotsafe(&config, root.path(), &["init", "--path", vault.to_str().unwrap()]).output().unwrap().status.success());
    assert!(dotsafe(&config, &project, &["set", "API_KEY=sk_daemon_test"]).output().unwrap().status.success());

    // No terminal on stdin: it serves, but nobody can approve.
    let child = dotsafe(&config, root.path(), &["daemon"]).stdin(Stdio::null()).stderr(Stdio::piped()).spawn().unwrap();
    let _daemon = Daemon(child);
    let socket = config.join("agent.sock");
    let started = Instant::now();
    while ipc::call(&socket, &Request::ListProjects).is_err() {
        assert!(started.elapsed() < Duration::from_secs(20), "the daemon didn't start");
        std::thread::sleep(Duration::from_millis(100));
    }

    let Response::Projects(projects) = ipc::call(&socket, &Request::ListProjects).unwrap() else { panic!() };
    assert_eq!(projects[0].name, "my-api");
    let target = Target { project: None, environment: None, cwd: Some(project.clone()) };
    let Response::Variables(vars) = ipc::call(&socket, &Request::ListVariables(target.clone())).unwrap() else { panic!() };
    assert_eq!(vars.variables.iter().map(|v| v.name.as_str()).collect::<Vec<_>>(), ["API_KEY"]);

    let run = RunRequest {
        client: "Test agent".into(),
        command: vec!["true".into()],
        cwd: project.clone(),
        target,
        variables: None,
        path: std::env::var("PATH").ok(),
        timeout_secs: Some(5),
        proxy: false,
    };
    match ipc::call(&socket, &Request::Run(run)).unwrap() {
        Response::Error(e) => assert_eq!(e.code, ErrorCode::Denied),
        other => panic!("expected a refusal, got {other:?}"),
    }

    // Only one broker at a time.
    let second = dotsafe(&config, root.path(), &["daemon"]).stdin(Stdio::null()).output().unwrap();
    assert!(String::from_utf8_lossy(&second.stderr).contains("another dotsafe broker"), "{}", String::from_utf8_lossy(&second.stderr));
}
