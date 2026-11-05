//! `dotsafe mcp` end to end: the real binary speaking MCP on stdio, against
//! a stand-in broker on the socket the desktop app would serve.
#![cfg(unix)]

use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

use dotsafe_agent::ipc::{self, Handler};
use dotsafe_agent::protocol::{Generated, ProjectInfo, Request, Response, RunOutput};
use serde_json::{Value, json};

#[derive(Default)]
struct FakeBroker {
    seen: Mutex<Vec<Request>>,
}

impl Handler for FakeBroker {
    fn handle(&self, request: Request) -> Response {
        self.seen.lock().unwrap().push(request.clone());
        match request {
            Request::ListProjects => Response::Projects(vec![ProjectInfo {
                name: "my-api".into(),
                environments: vec!["development".into()],
            }]),
            Request::Run(_) => Response::Ran(RunOutput {
                exit_code: Some(0),
                timed_out: false,
                stdout: "connected to [dotsafe:DATABASE_URL]\n".into(),
                stderr: String::new(),
                redactions: 1,
                truncated: false,
                proxied: vec!["OPENAI_API_KEY".into()],
            }),
            Request::Generate(g) => Response::Generated(Generated {
                project: "my-api".into(),
                environment: "development".into(),
                name: g.name,
                description: "48 random characters".into(),
            }),
            Request::ListVariables(_) => {
                Response::error(dotsafe_agent::protocol::ErrorCode::Locked, "dotsafe is locked.")
            }
            Request::Exec(_) | Request::Pending | Request::Answer(_) => {
                Response::error(dotsafe_agent::protocol::ErrorCode::Invalid, "unsupported")
            }
        }
    }
}

/// Sends each message to a fresh `dotsafe mcp` and returns its replies.
fn session(config: &std::path::Path, messages: &[Value]) -> Vec<Value> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_dotsafe"))
        .arg("mcp")
        .env("DOTSAFE_CONFIG_DIR", config)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    {
        let stdin = child.stdin.as_mut().unwrap();
        for m in messages {
            writeln!(stdin, "{m}").unwrap();
        }
    }
    drop(child.stdin.take());
    let out = child.wait_with_output().unwrap();
    String::from_utf8(out.stdout).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect()
}

fn init() -> Value {
    json!({"jsonrpc": "2.0", "id": 1, "method": "initialize",
           "params": {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "Claude Code", "version": "1"}}})
}

fn call(id: u64, name: &str, arguments: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "method": "tools/call", "params": {"name": name, "arguments": arguments}})
}

#[test]
fn handshake_and_tools() {
    let config = tempfile::tempdir().unwrap();
    let replies = session(
        config.path(),
        &[
            init(),
            json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
            json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
            json!({"jsonrpc": "2.0", "id": 3, "method": "nope"}),
        ],
    );
    assert_eq!(replies.len(), 3, "notifications get no reply");
    assert_eq!(replies[0]["result"]["protocolVersion"], "2025-06-18");
    assert_eq!(replies[0]["result"]["serverInfo"]["name"], "dotsafe");
    let names: Vec<&str> =
        replies[1]["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["list_projects", "list_variables", "run_with_secrets", "generate_secret"]);
    assert!(!names.iter().any(|n| n.contains("reveal") || n.contains("get")), "no tool returns values");
    assert_eq!(replies[2]["error"]["code"], -32601);
}

#[test]
fn says_when_dotsafe_isnt_running() {
    let config = tempfile::tempdir().unwrap();
    let replies = session(config.path(), &[init(), call(2, "list_projects", json!({}))]);
    assert_eq!(replies[1]["result"]["isError"], true);
    assert!(replies[1]["result"]["content"][0]["text"].as_str().unwrap().contains("isn't running"));
}

#[test]
fn tools_are_forwarded_to_the_broker() {
    let config = tempfile::tempdir().unwrap();
    let broker = Arc::new(FakeBroker::default());
    let listener = ipc::bind(&config.path().join("agent.sock")).unwrap();
    let handler = broker.clone();
    std::thread::spawn(move || ipc::serve(listener, handler));

    let work = tempfile::tempdir().unwrap();
    let replies = session(
        config.path(),
        &[
            init(),
            call(2, "list_projects", json!({})),
            call(
                3,
                "run_with_secrets",
                json!({"command": ["bun", "test"], "cwd": work.path(), "variables": ["DATABASE_URL"]}),
            ),
            call(4, "list_variables", json!({"project": "my-api"})),
            call(5, "run_with_secrets", json!({"command": []})),
            call(6, "generate_secret", json!({"name": "JWT_SECRET", "kind": "hex", "length": 32})),
        ],
    );
    let text = |i: usize| replies[i]["result"]["content"][0]["text"].as_str().unwrap().to_string();
    assert_eq!(text(1), "my-api: development");
    assert!(text(2).starts_with("Exit code 0. 1 secret value was replaced"), "{}", text(2));
    assert!(text(2).contains("connected to [dotsafe:DATABASE_URL]"));
    assert!(text(2).contains("OPENAI_API_KEY went through dotsafe's proxy"), "{}", text(2));
    assert!(text(5).starts_with("Added JWT_SECRET to my-api/development (48 random characters)."), "{}", text(5));
    assert_eq!(replies[3]["result"]["isError"], true, "broker errors become tool errors");
    assert_eq!(replies[4]["result"]["isError"], true, "empty commands never reach the broker");

    let seen = broker.seen.lock().unwrap();
    let Request::Run(run) = &seen[1] else { panic!("expected a run, got {:?}", seen[1]) };
    assert_eq!(run.client, "Claude Code", "the client's name comes from initialize");
    assert_eq!(run.command, ["bun", "test"]);
    assert_eq!(run.cwd, work.path());
    assert_eq!(run.variables.as_deref(), Some(&["DATABASE_URL".to_string()][..]));
    assert!(run.path.is_some(), "the agent's PATH is passed along");
    assert!(run.proxy, "the proxy is on unless the agent turns it off");
    let Request::Generate(generate) = &seen[3] else { panic!("expected a generate, got {:?}", seen[3]) };
    assert_eq!(
        (generate.name.as_str(), generate.kind.as_deref(), generate.length),
        ("JWT_SECRET", Some("hex"), Some(32))
    );
    assert_eq!(generate.client, "Claude Code");
    assert_eq!(seen.len(), 4);
}
