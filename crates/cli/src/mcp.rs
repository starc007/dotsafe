//! `dotsafe mcp`: a Model Context Protocol server on stdio, for AI agents.
//!
//! It holds no keys and can't unlock anything: every tool is forwarded to the
//! broker in the desktop app, which asks the user before any command gets
//! secrets. There is no tool that returns a value, and there never will be.

use std::io::{BufRead, Write};
use std::path::PathBuf;

use anyhow::Result;
use dotsafe_agent::protocol::{ErrorCode, GenerateRequest, Request, Response, RunOutput, RunRequest, Target};
use serde_json::{Value, json};

const PROTOCOL_VERSIONS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];

const INSTRUCTIONS: &str = "Secrets for this project are managed by dotsafe. Use list_variables to see which exist, \
and run_with_secrets to run a command that needs them: the user approves it, and its output comes back with secret \
values replaced by [dotsafe:NAME]. OpenAI and Anthropic keys go through a local proxy, so the command never holds them. \
To add a new random secret (JWT_SECRET, SESSION_KEY…), use generate_secret: the value is created in dotsafe and never \
shown to you. You never see values. Don't read, create or edit .env files.";

pub fn serve() -> Result<()> {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    let mut client = String::from("An AI agent");
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let Ok(message) = serde_json::from_str::<Value>(&line) else {
            write(
                &mut stdout,
                &json!({"jsonrpc": "2.0", "id": null, "error": {"code": -32700, "message": "Parse error"}}),
            )?;
            continue;
        };
        // Notifications (no id) need no answer.
        let Some(id) = message.get("id").cloned() else { continue };
        let method = message["method"].as_str().unwrap_or_default();
        let params = &message["params"];
        let reply = match method {
            "initialize" => {
                if let Some(name) = params["clientInfo"]["name"].as_str() {
                    client = name.to_string();
                }
                let requested = params["protocolVersion"].as_str().unwrap_or_default();
                let version = PROTOCOL_VERSIONS.iter().find(|v| **v == requested).unwrap_or(&PROTOCOL_VERSIONS[0]);
                Ok(json!({
                    "protocolVersion": version,
                    "capabilities": {"tools": {"listChanged": false}},
                    "serverInfo": {"name": "dotsafe", "version": env!("CARGO_PKG_VERSION")},
                    "instructions": INSTRUCTIONS,
                }))
            }
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({"tools": tools()})),
            "tools/call" => Ok(call_tool(&client, params)),
            _ => Err(json!({"code": -32601, "message": format!("Method not found: {method}")})),
        };
        let response = match reply {
            Ok(result) => json!({"jsonrpc": "2.0", "id": id, "result": result}),
            Err(error) => json!({"jsonrpc": "2.0", "id": id, "error": error}),
        };
        write(&mut stdout, &response)?;
    }
    Ok(())
}

fn write(out: &mut impl Write, value: &Value) -> Result<()> {
    serde_json::to_writer(&mut *out, value)?;
    out.write_all(b"\n")?;
    out.flush()?;
    Ok(())
}

fn tools() -> Value {
    let target = json!({
        "project": {"type": "string", "description": "Project name. Defaults to the one linked to cwd by .dotsafe.json."},
        "environment": {"type": "string", "description": "Environment name, such as development. Defaults to the linked one."},
        "cwd": {"type": "string", "description": "Directory to look up the linked project from. Defaults to the server's working directory."},
    });
    let mut run_props = target.clone();
    let run = run_props.as_object_mut().expect("object");
    run.insert(
        "command".into(),
        json!({"type": "array", "items": {"type": "string"}, "minItems": 1,
               "description": "Program and arguments, run directly (no shell). For example [\"bun\", \"test\"]."}),
    );
    run.insert(
        "variables".into(),
        json!({"type": "array", "items": {"type": "string"},
               "description": "Only inject these variables. Defaults to all of the environment's."}),
    );
    run.insert(
        "timeout_seconds".into(),
        json!({"type": "integer", "minimum": 1, "maximum": 900, "description": "Defaults to 120."}),
    );
    run.insert(
        "cwd".into(),
        json!({"type": "string", "description": "Working directory for the command. Defaults to the server's."}),
    );
    run.insert(
        "proxy".into(),
        json!({"type": "boolean", "description": "Send OpenAI and Anthropic keys through dotsafe's local proxy instead of injecting them. Defaults to true; turn off only if the code can't use a custom base URL."}),
    );
    let mut generate_props = target.clone();
    let generate = generate_props.as_object_mut().expect("object");
    generate.insert(
        "name".into(),
        json!({"type": "string", "description": "Variable name, for example JWT_SECRET. Must not exist yet."}),
    );
    generate.insert(
        "kind".into(),
        json!({"type": "string", "enum": ["random", "hex", "base64", "uuid"], "description": "Defaults to random (letters and digits)."}),
    );
    generate.insert(
        "length".into(),
        json!({"type": "integer", "minimum": 8, "maximum": 256, "description": "Characters for random (default 48), bytes for hex and base64 (default 32)."}),
    );
    json!([
        {
            "name": "list_projects",
            "description": "List dotsafe projects and their environments.",
            "inputSchema": {"type": "object", "properties": {}},
            "annotations": {"readOnlyHint": true},
        },
        {
            "name": "list_variables",
            "description": "List the names of the variables in an environment, and which are shared secrets. Never returns values.",
            "inputSchema": {"type": "object", "properties": target},
            "annotations": {"readOnlyHint": true},
        },
        {
            "name": "run_with_secrets",
            "description": "Run a command with the environment's variables set, after the user approves it in dotsafe. Returns the exit code and output, with secret values replaced by [dotsafe:NAME]. Shells with -c, inline code (node -e, python -c) and commands that print the environment are refused.",
            "inputSchema": {"type": "object", "properties": run_props, "required": ["command"]},
            "annotations": {"destructiveHint": true, "openWorldHint": true},
        },
        {
            "name": "generate_secret",
            "description": "Add a variable with a random value, after the user approves it in dotsafe. The value is generated inside dotsafe and never returned. Use it for JWT secrets, session keys and the like.",
            "inputSchema": {"type": "object", "properties": generate_props, "required": ["name"]},
            "annotations": {"destructiveHint": false, "idempotentHint": false},
        },
    ])
}

fn call_tool(client: &str, params: &Value) -> Value {
    let args = &params["arguments"];
    let request = match params["name"].as_str().unwrap_or_default() {
        "list_projects" => Request::ListProjects,
        "list_variables" => Request::ListVariables(target(args)),
        "run_with_secrets" => {
            let command: Vec<String> = args["command"]
                .as_array()
                .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                .unwrap_or_default();
            if command.is_empty() {
                return tool_error("Give the command as an array, for example [\"bun\", \"test\"].");
            }
            let cwd = args["cwd"]
                .as_str()
                .map(PathBuf::from)
                .or_else(|| std::env::current_dir().ok())
                .unwrap_or_else(|| PathBuf::from("."));
            Request::Run(RunRequest {
                client: client.to_string(),
                command,
                cwd: cwd.clone(),
                target: Target { cwd: Some(cwd), ..target(args) },
                variables: args["variables"]
                    .as_array()
                    .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect()),
                timeout_secs: args["timeout_seconds"].as_u64(),
                path: std::env::var("PATH").ok(),
                proxy: args["proxy"].as_bool().unwrap_or(true),
            })
        }
        "generate_secret" => {
            let Some(name) = args["name"].as_str().filter(|n| !n.is_empty()) else {
                return tool_error("Give the variable a name, for example JWT_SECRET.");
            };
            Request::Generate(GenerateRequest {
                client: client.to_string(),
                name: name.to_string(),
                target: target(args),
                kind: args["kind"].as_str().map(String::from),
                length: args["length"].as_u64().and_then(|n| u32::try_from(n).ok()),
            })
        }
        other => return tool_error(&format!("Unknown tool {other}.")),
    };
    match broker(&request) {
        Response::Projects(projects) if projects.is_empty() => text("No projects yet."),
        Response::Projects(projects) => text(
            &projects
                .iter()
                .map(|p| format!("{}: {}", p.name, p.environments.join(", ")))
                .collect::<Vec<_>>()
                .join("\n"),
        ),
        Response::Variables(info) => {
            let mut out = format!("{}/{}:\n", info.project, info.environment);
            for v in &info.variables {
                match &v.shared {
                    Some(secret) => out += &format!("{} (shared secret {secret})\n", v.name),
                    None => out += &format!("{}\n", v.name),
                }
            }
            if info.variables.is_empty() {
                out += "(no variables)\n";
            }
            text(out.trim_end())
        }
        Response::Ran(output) => text(&describe_run(&output)),
        Response::Generated(g) => text(&format!(
            "Added {} to {}/{} ({}). The value was generated in dotsafe and isn't shown to you; commands run with run_with_secrets get it.",
            g.name, g.project, g.environment, g.description
        )),
        Response::Error(error) => tool_error(&error.message),
    }
}

fn target(args: &Value) -> Target {
    let field = |name: &str| args[name].as_str().filter(|s| !s.is_empty()).map(String::from);
    Target {
        project: field("project"),
        environment: field("environment"),
        cwd: field("cwd").map(PathBuf::from).or_else(|| std::env::current_dir().ok()),
    }
}

fn describe_run(output: &RunOutput) -> String {
    let mut out = match (output.timed_out, output.exit_code) {
        (true, _) => "Timed out and was stopped.".to_string(),
        (false, Some(code)) => format!("Exit code {code}."),
        (false, None) => "Ended by a signal.".to_string(),
    };
    if output.redactions > 0 {
        out += &format!(" {} secret value(s) were replaced with [dotsafe:NAME].", output.redactions);
    }
    if !output.proxied.is_empty() {
        out += &format!(" {} went through dotsafe's proxy; the process never had the key.", output.proxied.join(", "));
    }
    if output.truncated {
        out += " Output was long; only the end is shown.";
    }
    if !output.stdout.is_empty() {
        out += &format!("\n\nstdout:\n{}", output.stdout);
    }
    if !output.stderr.is_empty() {
        out += &format!("\n\nstderr:\n{}", output.stderr);
    }
    out
}

fn text(s: &str) -> Value {
    json!({"content": [{"type": "text", "text": s}]})
}

fn tool_error(s: &str) -> Value {
    json!({"content": [{"type": "text", "text": s}], "isError": true})
}

#[cfg(unix)]
fn broker(request: &Request) -> Response {
    let unavailable = || {
        Response::error(
            ErrorCode::Unavailable,
            "dotsafe isn't running. Open the dotsafe app and unlock it, then try again.",
        )
    };
    let Some(path) = dotsafe_agent::socket_path() else { return unavailable() };
    dotsafe_agent::ipc::call(&path, request).unwrap_or_else(|_| unavailable())
}

#[cfg(not(unix))]
fn broker(_request: &Request) -> Response {
    Response::error(ErrorCode::Unavailable, "Agent access isn't available on this platform yet.")
}

/// How to add dotsafe to common agents.
pub fn print_setup() -> Result<()> {
    let exe = std::env::current_exe()?;
    let exe = exe.display();
    println!("Claude Code:\n  claude mcp add dotsafe -- {exe} mcp\n");
    println!("Cursor, Claude Desktop and others (mcpServers in their JSON config):");
    println!(
        "{}",
        serde_json::to_string_pretty(
            &json!({"mcpServers": {"dotsafe": {"command": exe.to_string(), "args": ["mcp"]}}})
        )?
    );
    println!("\nThe dotsafe app must be open and unlocked; it asks before any command gets secrets.");
    Ok(())
}
