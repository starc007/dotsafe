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
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use dotsafe_agent::grants::{Lifetime, Scope};
use dotsafe_agent::policy;
use dotsafe_agent::protocol::{
    ErrorCode, GenerateRequest, Generated, ProjectInfo, Request, Response, RunRequest, Target, VariableInfo,
    VariablesInfo,
};
use dotsafe_agent::proxy::{self, ProxyServer};
use dotsafe_agent::run::{self, DEFAULT_TIMEOUT, MAX_TIMEOUT, RunPlan};
use dotsafe_core::audit::{self, AuditAction, AuditEvent, AuditOutcome};
use dotsafe_core::{Charset, Entry, Environment, Generator, Project, Record, is_valid_var_name};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, UserAttentionType};
use uuid::Uuid;

use crate::state::{AppState, Session};

/// How long an approval prompt waits before counting as a no.
const ANSWER_TIMEOUT: Duration = Duration::from_secs(120);

/// Approvals waiting for the user, by request id.
#[derive(Clone, Default)]
pub struct Approvals(pub Arc<Mutex<HashMap<Uuid, Pending>>>);

pub struct Pending {
    answer: Sender<Option<Lifetime>>,
    /// What the Touch ID prompt says: "dotsafe is trying to <reason>".
    reason: String,
}

/// What the approval dialog shows.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalRequest {
    id: Uuid,
    /// `run` or `generate`.
    kind: &'static str,
    client: String,
    project: String,
    environment: String,
    /// Run: the command as typed, program and arguments.
    command: Vec<String>,
    /// Run: where the program was found, so a lookalike on another path is visible.
    program_path: String,
    cwd: String,
    /// Run: the variables it gets. Generate: the one to create.
    variables: Vec<String>,
    /// Run: variables that go through the secret proxy instead.
    proxied: Vec<String>,
    /// Generate: what kind of value, for example "48 random characters".
    generated: Option<String>,
}

pub struct Broker {
    state: AppState,
    approvals: Approvals,
    app: AppHandle,
    /// Started on first use and kept for the app's lifetime.
    proxy: OnceLock<Option<ProxyServer>>,
}

impl Broker {
    pub fn new(state: AppState, approvals: Approvals, app: AppHandle) -> Broker {
        Broker { state, approvals, app, proxy: OnceLock::new() }
    }

    fn proxy(&self) -> Option<&ProxyServer> {
        self.proxy.get_or_init(|| ProxyServer::start().ok()).as_ref()
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
            self.audit(&request.client, AuditAction::Run, AuditOutcome::Refused, &label, "", &[], Some(reason.clone()));
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
        let (scope, project, environment, approved, profiles) = {
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
            // Built-in and custom profiles; custom ones come from the vault,
            // which agents can't edit.
            let profiles = proxy::profiles(&records);
            (scope, project.name, env.name, approved, profiles)
        };
        let env_label = format!("{project}/{environment}");

        let proxied_names: Vec<String> = if request.proxy && self.proxy().is_some() {
            proxy::proxied_names(scope.variables.iter().map(String::as_str), &profiles)
        } else {
            Vec::new()
        };
        if !approved {
            let prompt = ApprovalRequest {
                id: Uuid::new_v4(),
                kind: "run",
                client: request.client.clone(),
                project: project.clone(),
                environment: environment.clone(),
                command: request.command.clone(),
                program_path: scope.program.display().to_string(),
                cwd: scope.cwd.display().to_string(),
                variables: scope.variables.clone(),
                proxied: proxied_names.clone(),
                generated: None,
            };
            match self.ask(prompt) {
                Some(lifetime) => self.session().agent_grants.add(scope.clone(), lifetime, Instant::now()),
                None => {
                    self.audit(
                        &request.client,
                        AuditAction::Run,
                        AuditOutcome::Denied,
                        &label,
                        &env_label,
                        &scope.variables,
                        None,
                    );
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
        // Known API keys go through the proxy: the process gets a placeholder
        // and a local URL. The routes live until the run ends.
        let proxied = match (request.proxy, self.proxy()) {
            (true, Some(server)) => Some(proxy::apply(server, secrets.clone(), &profiles)),
            _ => None,
        };
        let (env, proxied_names) = match &proxied {
            Some(p) => (p.env.clone(), p.names.clone()),
            None => (secrets.clone(), Vec::new()),
        };
        let plan = RunPlan {
            program,
            args: scope.args.clone(),
            cwd,
            path: request.path.clone(),
            env,
            redact: secrets,
            proxied: proxied_names,
            timeout,
        };
        let result = run::run(plan);
        drop(proxied);
        let (vars, client) = (&scope.variables, &request.client);
        match result {
            Ok(output) => {
                let outcome = match (output.timed_out, output.exit_code) {
                    (true, _) => "timed out".to_string(),
                    (false, Some(code)) => format!("exit {code}"),
                    (false, None) => "ended by a signal".to_string(),
                };
                self.audit(client, AuditAction::Run, AuditOutcome::Allowed, &label, &env_label, vars, Some(outcome));
                Response::Ran(output)
            }
            Err(e) => {
                let detail = Some(e.to_string());
                self.audit(client, AuditAction::Run, AuditOutcome::Failed, &label, &env_label, vars, detail);
                Response::error(ErrorCode::Internal, format!("Couldn't start {}: {e}", request.command[0]))
            }
        }
    }

    fn generate(&self, request: GenerateRequest) -> Response {
        let name = request.name.trim().to_string();
        if !is_valid_var_name(&name) {
            return Response::error(
                ErrorCode::Invalid,
                "Use letters, numbers and underscores, not starting with a number.",
            );
        }
        let (generator, description) = match generator(request.kind.as_deref(), request.length) {
            Ok(found) => found,
            Err(message) => return Response::error(ErrorCode::Invalid, message),
        };
        let (project, environment, env_id) = {
            let session = self.session();
            let Ok(vault) = session.vault() else { return locked() };
            let records = match vault.list() {
                Ok(records) => records,
                Err(e) => return Response::error(ErrorCode::Internal, e.to_string()),
            };
            let (project, env) = match resolve(&records, &request.target) {
                Ok(found) => found,
                Err(response) => return response,
            };
            if env.entry(&name).is_some() {
                return Response::error(
                    ErrorCode::Invalid,
                    format!("{}/{} already has {name}.", project.name, env.name),
                );
            }
            (project.name, env.name, env.id)
        };
        let env_label = format!("{project}/{environment}");
        let client = request.client.clone();
        let prompt = ApprovalRequest {
            id: Uuid::new_v4(),
            kind: "generate",
            client: client.clone(),
            project: project.clone(),
            environment: environment.clone(),
            command: Vec::new(),
            program_path: String::new(),
            cwd: String::new(),
            variables: vec![name.clone()],
            proxied: Vec::new(),
            generated: Some(description.clone()),
        };
        if self.ask(prompt).is_none() {
            self.audit(&client, AuditAction::Generate, AuditOutcome::Denied, &name, &env_label, &[], None);
            return Response::error(ErrorCode::Denied, "The user didn't allow it.");
        }

        let saved = (|| -> Result<(), String> {
            let value = generator.generate().map_err(|e| e.to_string())?;
            let session = self.session();
            let vault = session.vault().map_err(|_| "dotsafe locked before it could save.".to_string())?;
            let stamp = session.stamp().map_err(|e| e.message)?;
            let mut env = session.environment(env_id).map_err(|e| e.message)?;
            if env.entry(&name).is_some() {
                return Err(format!("{env_label} already has {name}."));
            }
            env.set(&name, value.as_str(), stamp);
            vault.put(Record::Environment(env)).map_err(|e| e.to_string())?;
            Ok(())
        })();
        match saved {
            Ok(()) => {
                let created = std::slice::from_ref(&name);
                let detail = Some(description.clone());
                self.audit(&client, AuditAction::Generate, AuditOutcome::Allowed, &name, &env_label, created, detail);
                let _ = self.app.emit("vault-synced", ());
                Response::Generated(Generated { project, environment, name, description })
            }
            Err(message) => {
                let detail = Some(message.clone());
                self.audit(&client, AuditAction::Generate, AuditOutcome::Failed, &name, &env_label, &[], detail);
                Response::error(ErrorCode::Internal, message)
            }
        }
    }

    /// Shows the approval dialog and waits for the answer.
    fn ask(&self, prompt: ApprovalRequest) -> Option<Lifetime> {
        let id = prompt.id;
        let (tx, rx) = mpsc::channel();
        let reason = match prompt.kind {
            "generate" => format!("let {} add {} to {}", prompt.client, prompt.variables.join(""), prompt.project),
            _ => {
                let n = prompt.variables.len();
                format!(
                    "let {} run “{}” with {n} {}",
                    prompt.client,
                    prompt.command.join(" "),
                    if n == 1 { "secret" } else { "secrets" }
                )
            }
        };
        self.approvals.0.lock().ok()?.insert(id, Pending { answer: tx, reason });
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

    /// Records an event in the vault's audit log (encrypted, synced). A
    /// locked vault means nothing to record it in; that's not an error.
    #[allow(clippy::too_many_arguments)]
    fn audit(
        &self,
        client: &str,
        action: AuditAction,
        outcome: AuditOutcome,
        summary: &str,
        environment: &str,
        variables: &[String],
        detail: Option<String>,
    ) {
        let at = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0);
        let event = AuditEvent {
            at,
            actor: client.to_string(),
            action,
            outcome,
            summary: summary.to_string(),
            environment: environment.to_string(),
            variables: variables.to_vec(),
            detail,
        };
        let session = self.session();
        if let (Ok(vault), Some(config)) = (session.vault(), session.config.as_ref()) {
            let _ = audit::record(vault, config.device_id, event);
        }
        drop(session);
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
            Request::Generate(generate) => self.generate(generate),
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

/// The generator for an agent's `kind` and `length`, and how to describe it.
fn generator(kind: Option<&str>, length: Option<u32>) -> Result<(Generator, String), String> {
    match kind.unwrap_or("random") {
        "random" => {
            let length = length.unwrap_or(48);
            Ok((Generator::Random { length, charset: Charset::Alphanumeric }, format!("{length} random characters")))
        }
        "hex" => {
            let bytes = length.unwrap_or(32);
            Ok((Generator::Hex { bytes }, format!("{bytes} random bytes, hex")))
        }
        "base64" => {
            let bytes = length.unwrap_or(32);
            Ok((Generator::Base64 { bytes }, format!("{bytes} random bytes, base64")))
        }
        "uuid" => Ok((Generator::Uuid, "a random UUID".into())),
        other => Err(format!("Unknown kind {other}; use random, hex, base64 or uuid.")),
    }
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
/// `1h` or `lock`; `None` denies. Approving first asks for Touch ID (or the
/// login password) unless the user turned that off; if they cancel, the
/// request stays open so they can try again or deny it.
#[tauri::command]
pub async fn answer_agent_request(
    state: tauri::State<'_, AppState>,
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
    let expired = || crate::state::CmdError::other("That request already expired.");
    if lifetime.is_some() {
        let without_auth = state.run(|s| Ok(s.config()?.agent_approvals_without_auth)).await?;
        // A Mac that can't ask (no login password) falls back to the click.
        if !without_auth && crate::presence::available() {
            let reason = {
                let pending = approvals.0.lock().map_err(|_| expired())?;
                pending.get(&id).ok_or_else(expired)?.reason.clone()
            };
            tauri::async_runtime::spawn_blocking(move || crate::presence::confirm(&reason))
                .await
                .map_err(|e| crate::state::CmdError::other(e.to_string()))?
                .map_err(|why| crate::state::CmdError::other(format!("Not approved: {why}")))?;
        }
    }
    let pending = approvals.0.lock().ok().and_then(|mut p| p.remove(&id)).ok_or_else(expired)?;
    let _ = pending.answer.send(lifetime);
    Ok(())
}

/// Turns the Touch ID check on agent approvals on or off. Turning it off
/// needs Touch ID itself, or it would protect nothing.
#[tauri::command]
pub async fn set_agent_approval_auth(state: tauri::State<'_, AppState>, required: bool) -> crate::state::CmdResult<()> {
    if !required && crate::presence::available() {
        tauri::async_runtime::spawn_blocking(|| {
            crate::presence::confirm("stop asking for Touch ID when agents need approval")
        })
        .await
        .map_err(|e| crate::state::CmdError::other(e.to_string()))?
        .map_err(|why| crate::state::CmdError::other(format!("Not changed: {why}")))?;
    }
    state
        .run(move |s| {
            let mut config = s.config()?.clone();
            config.agent_approvals_without_auth = !required;
            config.save()?;
            s.config = Some(config);
            Ok(())
        })
        .await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentActivity {
    /// Approvals ask for Touch ID or the login password.
    approval_auth: bool,
    /// This Mac can ask for it.
    approval_auth_available: bool,
    grants: Vec<GrantView>,
    /// From the vault's audit log, every device, newest first.
    recent: Vec<AuditEvent>,
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
            let recent = audit::recent(s.vault()?, 50)?;
            let available = crate::presence::available();
            let approval_auth = available && !s.config()?.agent_approvals_without_auth;
            Ok(AgentActivity { approval_auth, approval_auth_available: available, grants, recent })
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
