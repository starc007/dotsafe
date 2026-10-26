//! The agent broker: answers `dotsafe mcp` over a local socket, asks the user
//! before any command gets secrets, then runs it and returns scrubbed output.
//!
//! Network-free, but a run can take minutes, so nothing here holds the session
//! lock while waiting for the user or the command. See dotsafe_agent and
//! docs/architecture/agents.md.

// Agent access needs the Unix socket for now; elsewhere the broker is built
// but never started.
#![cfg_attr(not(unix), allow(dead_code, unused_imports))]

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use dotsafe_agent::grants::{Lifetime, Scope};
use dotsafe_agent::policy;
use dotsafe_agent::protocol::{
    ErrorCode, ProjectInfo, Request, Response, RunRequest, Target, VariableInfo, VariablesInfo,
};
use dotsafe_agent::run::{self, DEFAULT_TIMEOUT, MAX_TIMEOUT, RunPlan};
use dotsafe_core::{Entry, Environment, Project, Record};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, UserAttentionType};
use uuid::Uuid;

use crate::state::{AgentEvent, AppState, Session};

/// How long an approval prompt waits before counting as a no.
const ANSWER_TIMEOUT: Duration = Duration::from_secs(120);

/// Approvals waiting for the user, by request id.
#[derive(Clone, Default)]
pub struct Approvals(pub Arc<Mutex<HashMap<Uuid, Sender<Option<Lifetime>>>>>);

/// What the approval dialog shows.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalRequest {
    id: Uuid,
    client: String,
    /// The command as typed: program and arguments.
    command: Vec<String>,
    /// Where the program was found, so a lookalike on another path is visible.
    program_path: String,
    cwd: String,
    project: String,
    environment: String,
    variables: Vec<String>,
}

pub struct Broker {
    state: AppState,
    approvals: Approvals,
    app: AppHandle,
}

impl Broker {
    pub fn new(state: AppState, approvals: Approvals, app: AppHandle) -> Broker {
        Broker { state, approvals, app }
    }

    fn session(&self) -> std::sync::MutexGuard<'_, Session> {
        self.state.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn list_projects(&self) -> Response {
        let session = self.session();
        let Ok(vault) = session.vault() else { return locked() };
        let records = match vault.list() {
            Ok(records) => records,
            Err(e) => return Response::error(ErrorCode::Internal, e.to_string()),
        };
        let mut projects: Vec<ProjectInfo> = records
            .iter()
            .filter_map(|r| match r {
                Record::Project(p) if !p.deleted => Some(ProjectInfo {
                    name: p.name.clone(),
                    environments: environments_of(&records, p.id).iter().map(|e| e.name.clone()).collect(),
                }),
                _ => None,
            })
            .collect();
        projects.sort_by_key(|p| p.name.to_lowercase());
        Response::Projects(projects)
    }

    fn list_variables(&self, target: &Target) -> Response {
        let session = self.session();
        let Ok(vault) = session.vault() else { return locked() };
        let records = match vault.list() {
            Ok(records) => records,
            Err(e) => return Response::error(ErrorCode::Internal, e.to_string()),
        };
        let (project, env) = match resolve(&records, target) {
            Ok(found) => found,
            Err(response) => return response,
        };
        let secrets = dotsafe_core::Secrets::from_records(&records);
        let variables = env
            .entries()
            .map(|(name, entry)| VariableInfo {
                name: name.to_string(),
                shared: match entry {
                    Entry::Secret(id) => Some(secrets.get(id).map_or_else(|| "deleted".into(), |s| s.name.clone())),
                    Entry::Value(_) => None,
                },
            })
            .collect();
        Response::Variables(VariablesInfo { project: project.name, environment: env.name, variables })
    }

    fn run(&self, request: RunRequest) -> Response {
        let label = request.command.join(" ");
        if let Some(reason) = policy::refusal(&request.command) {
            self.log(&request.client, &label, "", "refused", Some(reason.clone()));
            return Response::error(ErrorCode::Refused, reason);
        }
        let cwd = match request.cwd.canonicalize() {
            Ok(cwd) if cwd.is_dir() => cwd,
            _ => return Response::error(ErrorCode::NotFound, format!("{} isn't a directory.", request.cwd.display())),
        };
        let Some(program) = policy::resolve_program(&request.command[0], request.path.as_deref(), &cwd) else {
            return Response::error(ErrorCode::NotFound, format!("Couldn't find {} on PATH.", request.command[0]));
        };

        // Work out what would run, under the lock, without any values.
        let (scope, project, environment, approved) = {
            let mut session = self.session();
            let Ok(vault) = session.vault() else { return locked() };
            let records = match vault.list() {
                Ok(records) => records,
                Err(e) => return Response::error(ErrorCode::Internal, e.to_string()),
            };
            let target = Target { cwd: request.target.cwd.clone().or(Some(cwd.clone())), ..request.target.clone() };
            let (project, env) = match resolve(&records, &target) {
                Ok(found) => found,
                Err(response) => return response,
            };
            let mut variables: Vec<String> = env.keys().map(String::from).collect();
            if let Some(wanted) = &request.variables {
                let missing: Vec<&String> = wanted.iter().filter(|w| !variables.contains(w)).collect();
                if !missing.is_empty() {
                    let missing: Vec<&str> = missing.iter().map(|s| s.as_str()).collect();
                    return Response::error(
                        ErrorCode::NotFound,
                        format!("{}/{} has no {}.", project.name, env.name, missing.join(", ")),
                    );
                }
                variables.retain(|v| wanted.contains(v));
            }
            variables.sort();
            let scope = Scope {
                program: program.clone(),
                args: request.command[1..].to_vec(),
                cwd: cwd.clone(),
                environment: env.id.to_string(),
                variables,
            };
            let approved = session.agent_grants.allows(&scope, Instant::now());
            (scope, project.name, env.name, approved)
        };
        let env_label = format!("{project}/{environment}");

        if !approved {
            match self.ask(&request, &scope, &project, &environment) {
                Some(lifetime) => self.session().agent_grants.add(scope.clone(), lifetime, Instant::now()),
                None => {
                    self.log(&request.client, &label, &env_label, "denied", None);
                    return Response::error(ErrorCode::Denied, "The user didn't allow this command.");
                }
            }
        }

        // Fetch the values only now, and only if the vault is still unlocked.
        let secrets = {
            let session = self.session();
            let Ok(vault) = session.vault() else { return locked() };
            let records = match vault.list() {
                Ok(records) => records,
                Err(e) => return Response::error(ErrorCode::Internal, e.to_string()),
            };
            let all = dotsafe_core::Secrets::from_records(&records);
            let Some(env) = records.iter().find_map(|r| match r {
                Record::Environment(e) if e.id.to_string() == scope.environment && !e.deleted => Some(e.clone()),
                _ => None,
            }) else {
                return Response::error(ErrorCode::NotFound, "That environment no longer exists.");
            };
            all.resolve(&env)
                .filter(|(k, _)| scope.variables.iter().any(|v| v == k))
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect::<Vec<_>>()
        };

        let timeout = request
            .timeout_secs
            .map(Duration::from_secs)
            .unwrap_or(DEFAULT_TIMEOUT)
            .clamp(Duration::from_secs(1), MAX_TIMEOUT);
        let plan = RunPlan { program, args: scope.args.clone(), cwd, path: request.path.clone(), secrets, timeout };
        match run::run(plan) {
            Ok(output) => {
                let outcome = match (output.timed_out, output.exit_code) {
                    (true, _) => "timed out".to_string(),
                    (false, Some(code)) => format!("exit {code}"),
                    (false, None) => "ended by a signal".to_string(),
                };
                self.log(&request.client, &label, &env_label, "ran", Some(outcome));
                Response::Ran(output)
            }
            Err(e) => {
                self.log(&request.client, &label, &env_label, "failed", Some(e.to_string()));
                Response::error(ErrorCode::Internal, format!("Couldn't start {}: {e}", request.command[0]))
            }
        }
    }

    /// Shows the approval dialog and waits for the answer.
    fn ask(&self, request: &RunRequest, scope: &Scope, project: &str, environment: &str) -> Option<Lifetime> {
        let id = Uuid::new_v4();
        let (tx, rx) = mpsc::channel();
        self.approvals.0.lock().ok()?.insert(id, tx);
        let prompt = ApprovalRequest {
            id,
            client: request.client.clone(),
            command: request.command.clone(),
            program_path: scope.program.display().to_string(),
            cwd: scope.cwd.display().to_string(),
            project: project.to_string(),
            environment: environment.to_string(),
            variables: scope.variables.clone(),
        };
        if let Some(window) = self.app.get_webview_window("main") {
            let _ = window.unminimize();
            let _ = window.show();
            let _ = window.set_focus();
            let _ = window.request_user_attention(Some(UserAttentionType::Critical));
        }
        let _ = self.app.emit("agent-request", prompt);
        let answer = rx.recv_timeout(ANSWER_TIMEOUT).ok().flatten();
        if let Ok(mut pending) = self.approvals.0.lock() {
            pending.remove(&id);
        }
        let _ = self.app.emit("agent-request-closed", id);
        answer
    }

    fn log(&self, client: &str, command: &str, environment: &str, outcome: &str, detail: Option<String>) {
        let at = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0);
        self.session().log_agent(AgentEvent {
            at,
            client: client.to_string(),
            command: command.to_string(),
            environment: environment.to_string(),
            outcome: outcome.to_string(),
            detail,
        });
        let _ = self.app.emit("agent-activity", ());
    }
}

#[cfg(unix)]
impl dotsafe_agent::ipc::Handler for Broker {
    fn handle(&self, request: Request) -> Response {
        match request {
            Request::ListProjects => self.list_projects(),
            Request::ListVariables(target) => self.list_variables(&target),
            Request::Run(run) => self.run(run),
        }
    }
}

/// Starts listening for agents. Unix only for now; elsewhere `dotsafe mcp`
/// says agent access isn't available yet.
pub fn spawn(state: AppState, approvals: Approvals, app: AppHandle) {
    #[cfg(unix)]
    {
        let Some(path) = dotsafe_agent::socket_path() else { return };
        match dotsafe_agent::ipc::bind(&path) {
            Ok(listener) => {
                let broker = Arc::new(Broker::new(state, approvals, app));
                std::thread::spawn(move || dotsafe_agent::ipc::serve(listener, broker));
            }
            Err(e) => eprintln!("dotsafe: agent access is off: {e}"),
        }
    }
    #[cfg(not(unix))]
    let _ = (state, approvals, app);
}

fn locked() -> Response {
    Response::error(ErrorCode::Locked, "dotsafe is locked. Ask the user to unlock it, then try again.")
}

fn environments_of(records: &[Record], project: Uuid) -> Vec<&Environment> {
    let mut envs: Vec<&Environment> = records
        .iter()
        .filter_map(|r| match r {
            Record::Environment(e) if e.project_id == project && !e.deleted => Some(e),
            _ => None,
        })
        .collect();
    envs.sort_by_key(|e| e.name.clone());
    envs
}

/// The project and environment a request is about: by name, or from the
/// nearest `.dotsafe.json` link above `cwd` (defaulting the environment to the
/// link's).
fn resolve(records: &[Record], target: &Target) -> Result<(Project, Environment), Response> {
    let projects: Vec<&Project> = records
        .iter()
        .filter_map(|r| match r {
            Record::Project(p) if !p.deleted => Some(p),
            _ => None,
        })
        .collect();
    let link = target.cwd.as_deref().and_then(find_link);
    let project = match (&target.project, &link) {
        (Some(name), _) => {
            let matches: Vec<&&Project> = projects.iter().filter(|p| &p.name == name).collect();
            match matches.as_slice() {
                [one] => (**one).clone(),
                [] => return Err(Response::error(ErrorCode::NotFound, format!("No project named {name}."))),
                _ => {
                    return Err(Response::error(ErrorCode::Invalid, format!("More than one project is named {name}.")));
                }
            }
        }
        (None, Some((project_id, _))) => match projects.iter().find(|p| p.id == *project_id) {
            Some(p) => (*p).clone(),
            None => {
                return Err(Response::error(
                    ErrorCode::NotFound,
                    "The project linked by .dotsafe.json isn't in this vault.",
                ));
            }
        },
        (None, None) => {
            return Err(Response::error(
                ErrorCode::Invalid,
                "Say which project to use: there's no .dotsafe.json link in this directory.",
            ));
        }
    };
    let env_name = target
        .environment
        .clone()
        .or_else(|| link.filter(|(id, _)| *id == project.id).map(|(_, env)| env))
        .unwrap_or_else(|| "development".into());
    let env =
        environments_of(records, project.id).into_iter().find(|e| e.name == env_name).cloned().ok_or_else(|| {
            Response::error(ErrorCode::NotFound, format!("{} has no environment named {env_name}.", project.name))
        })?;
    Ok((project, env))
}

/// The nearest `.dotsafe.json` in `dir` or above it: (project id, environment).
fn find_link(dir: &Path) -> Option<(Uuid, String)> {
    #[derive(serde::Deserialize)]
    struct Link {
        project_id: Uuid,
        environment: String,
    }
    dir.ancestors().find_map(|d| {
        let path: PathBuf = d.join(".dotsafe.json");
        let link: Link = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
        Some((link.project_id, link.environment))
    })
}

/// The user's answer to an approval prompt. `lifetime` is `once`, `15m`,
/// `1h` or `lock`; `None` denies.
#[tauri::command]
pub async fn answer_agent_request(
    approvals: tauri::State<'_, Approvals>,
    id: Uuid,
    lifetime: Option<String>,
) -> crate::state::CmdResult<()> {
    let lifetime = match lifetime.as_deref() {
        None => None,
        Some("once") => Some(Lifetime::Once),
        Some("15m") => Some(Lifetime::For(Duration::from_secs(15 * 60))),
        Some("1h") => Some(Lifetime::For(Duration::from_secs(60 * 60))),
        Some("lock") => Some(Lifetime::UntilLock),
        Some(other) => return Err(crate::state::CmdError::other(format!("Unknown duration {other}."))),
    };
    let sender = approvals.0.lock().ok().and_then(|mut p| p.remove(&id));
    match sender {
        Some(tx) => {
            let _ = tx.send(lifetime);
            Ok(())
        }
        None => Err(crate::state::CmdError::other("That request already expired.")),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentActivity {
    grants: Vec<GrantView>,
    recent: Vec<AgentEvent>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GrantView {
    command: String,
    cwd: String,
    variables: usize,
    /// Seconds left; `None` until the vault locks.
    expires_in: Option<u64>,
}

#[tauri::command]
pub async fn agent_activity(state: tauri::State<'_, AppState>) -> crate::state::CmdResult<AgentActivity> {
    state
        .run(|s| {
            let now = Instant::now();
            let grants = s
                .agent_grants
                .entries(now)
                .into_iter()
                .map(|(scope, expires)| GrantView {
                    command: std::iter::once(policy::program_name(&scope.program.display().to_string()))
                        .chain(scope.args.iter().cloned())
                        .collect::<Vec<_>>()
                        .join(" "),
                    cwd: scope.cwd.display().to_string(),
                    variables: scope.variables.len(),
                    expires_in: expires.map(|e| e.saturating_duration_since(now).as_secs()),
                })
                .collect();
            Ok(AgentActivity { grants, recent: s.agent_log.iter().rev().cloned().collect() })
        })
        .await
}

#[tauri::command]
pub async fn revoke_agent_grants(state: tauri::State<'_, AppState>) -> crate::state::CmdResult<()> {
    state
        .run(|s| {
            s.agent_grants.clear();
            Ok(())
        })
        .await
}
