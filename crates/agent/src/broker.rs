//! The agent broker: answers `dotsafe mcp` over a local socket, asks the
//! person before any command gets secrets, then runs it and returns scrubbed
//! output. Shared by the desktop app and `dotsafe daemon`, which differ only
//! in how they ask (a dialog with Touch ID, or a terminal prompt); see
//! [`Host`].
//!
//! Nothing here holds a lock while waiting for the person or the command.

#![cfg_attr(not(unix), allow(dead_code, unused_imports))]

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use dotsafe_core::audit::{self, AuditAction, AuditEvent, AuditOutcome};
use dotsafe_core::resolve::Raw;
use dotsafe_core::{Charset, Environment, Generator, Project, Record, Vault, is_valid_var_name};
use serde::Serialize;
use uuid::Uuid;

use crate::files;
use crate::grants::{Lifetime, Scope};
use crate::policy;
use crate::protocol::{
    ErrorCode, GenerateRequest, Generated, ProjectInfo, Request, Response, RunRequest, Target, VariableInfo,
    VariablesInfo,
};
use crate::proxy::{self, ProxyServer};
use crate::run::{self, DEFAULT_TIMEOUT, MAX_TIMEOUT, RunPlan};

/// What the approval dialog shows.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalRequest {
    pub id: Uuid,
    /// `run` or `generate`.
    pub kind: &'static str,
    pub client: String,
    pub project: String,
    pub environment: String,
    /// Run: the command as typed, program and arguments.
    pub command: Vec<String>,
    /// Run: where the program was found, so a lookalike on another path is visible.
    pub program_path: String,
    pub cwd: String,
    /// Run: the variables it gets. Generate: the one to create.
    pub variables: Vec<String>,
    /// Run: variables that go through the secret proxy instead.
    pub proxied: Vec<String>,
    /// Run: scripts it runs, with when each last changed. An agent may have
    /// just written one.
    pub files: Vec<ApprovalFile>,
    /// Generate: what kind of value, for example "48 random characters".
    pub generated: Option<String>,
}

/// A script in an approval.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalFile {
    pub path: String,
    /// When it last changed, in Unix milliseconds.
    pub modified_ms: Option<u64>,
}

/// Where the broker runs: the desktop app, or `dotsafe daemon` in a
/// terminal. It provides the vault, remembers grants and asks the person.
pub trait Host: Send + Sync + 'static {
    /// The unlocked vault and this device's id, or `None` while locked.
    fn vault(&self) -> Option<(Arc<Vault>, Uuid)>;
    /// Whether an earlier approval covers `scope`.
    fn allows(&self, scope: &Scope, now: Instant) -> bool;
    /// Remembers an approval for `lifetime`.
    fn grant(&self, scope: Scope, lifetime: Lifetime, now: Instant);
    /// Asks the person and waits. `None` is a no (or no answer in time).
    fn ask(&self, prompt: ApprovalRequest) -> Option<Lifetime>;
    /// Something was written to the audit log.
    fn activity(&self) {}
    /// The vault changed (a generated secret was added).
    fn vault_changed(&self) {}
}

pub struct Broker<H: Host> {
    host: H,
    /// Started on first use and kept for the broker's lifetime.
    proxy: OnceLock<Option<ProxyServer>>,
}

impl<H: Host> Broker<H> {
    pub fn new(host: H) -> Broker<H> {
        Broker { host, proxy: OnceLock::new() }
    }

    pub fn host(&self) -> &H {
        &self.host
    }

    fn proxy(&self) -> Option<&ProxyServer> {
        self.proxy.get_or_init(|| ProxyServer::start().ok()).as_ref()
    }

    fn list_projects(&self) -> Response {
        let Some((vault, _)) = self.host.vault() else { return locked() };
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
        let Some((vault, _)) = self.host.vault() else { return locked() };
        let records = match vault.list() {
            Ok(records) => records,
            Err(e) => return Response::error(ErrorCode::Internal, e.to_string()),
        };
        let (project, env) = match resolve(&records, target) {
            Ok(found) => found,
            Err(response) => return response,
        };
        let resolver = dotsafe_core::Resolver::from_records(&records);
        let entries = match resolver.entries(env.id) {
            Ok(entries) => entries,
            Err(e) => return Response::error(ErrorCode::Internal, e.to_string()),
        };
        let variables = entries
            .into_iter()
            .map(|e| VariableInfo {
                shared: match e.entry {
                    Raw::Secret(id) => {
                        Some(resolver.secrets().get(id).map_or_else(|| "deleted".into(), |s| s.name.clone()))
                    }
                    Raw::Value(_) => None,
                },
                name: e.key,
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
        // Read before taking the vault, so a big file never holds it up.
        let run_files = files::files_run(&program, &request.command[1..], &cwd);

        // Work out what would run, under the lock, without any values.
        let (scope, project, environment, approved, profiles) = {
            let Some((vault, _)) = self.host.vault() else { return locked() };
            let records = match vault.list() {
                Ok(records) => records,
                Err(e) => return Response::error(ErrorCode::Internal, e.to_string()),
            };
            let target = Target { cwd: request.target.cwd.clone().or(Some(cwd.clone())), ..request.target.clone() };
            let (project, env) = match resolve(&records, &target) {
                Ok(found) => found,
                Err(response) => return response,
            };
            // Own and inherited variables.
            let mut variables: Vec<String> = match dotsafe_core::Resolver::from_records(&records).entries(env.id) {
                Ok(entries) => entries.into_iter().map(|e| e.key).collect(),
                Err(e) => return Response::error(ErrorCode::Internal, e.to_string()),
            };
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
                files: run_files.iter().map(|f| (f.path.clone(), f.fingerprint.clone())).collect(),
            };
            let approved = self.host.allows(&scope, Instant::now());
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
                files: run_files
                    .iter()
                    .map(|f| ApprovalFile { path: f.path.display().to_string(), modified_ms: f.modified_ms })
                    .collect(),
                generated: None,
            };
            match self.ask(prompt) {
                Some(lifetime) => self.host.grant(scope.clone(), lifetime, Instant::now()),
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
            let Some((vault, _)) = self.host.vault() else { return locked() };
            let records = match vault.list() {
                Ok(records) => records,
                Err(e) => return Response::error(ErrorCode::Internal, e.to_string()),
            };
            let Some(env) = records.iter().find_map(|r| match r {
                Record::Environment(e) if e.id.to_string() == scope.environment && !e.deleted => Some(e.clone()),
                _ => None,
            }) else {
                return Response::error(ErrorCode::NotFound, "That environment no longer exists.");
            };
            // Inherited variables, links and references, worked out in one place.
            match dotsafe_core::Resolver::from_records(&records).resolve(env.id) {
                Ok(all) => all.into_iter().filter(|(k, _)| scope.variables.iter().any(|v| v == k)).collect::<Vec<_>>(),
                Err(e) => return Response::error(ErrorCode::Internal, e.to_string()),
            }
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
            let Some((vault, _)) = self.host.vault() else { return locked() };
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
            files: Vec::new(),
            generated: Some(description.clone()),
        };
        if self.ask(prompt).is_none() {
            self.audit(&client, AuditAction::Generate, AuditOutcome::Denied, &name, &env_label, &[], None);
            return Response::error(ErrorCode::Denied, "The user didn't allow it.");
        }

        let saved = (|| -> Result<(), String> {
            let value = generator.generate().map_err(|e| e.to_string())?;
            let (vault, device) = self.host.vault().ok_or("dotsafe locked before it could save.")?;
            let stamp = vault.stamp(device);
            let mut env = match vault.get(env_id).map_err(|e| e.to_string())? {
                Some(Record::Environment(e)) if !e.deleted => e,
                _ => return Err("That environment no longer exists.".into()),
            };
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
                self.host.vault_changed();
                Response::Generated(Generated { project, environment, name, description })
            }
            Err(message) => {
                let detail = Some(message.clone());
                self.audit(&client, AuditAction::Generate, AuditOutcome::Failed, &name, &env_label, &[], detail);
                Response::error(ErrorCode::Internal, message)
            }
        }
    }

    fn ask(&self, prompt: ApprovalRequest) -> Option<Lifetime> {
        self.host.ask(prompt)
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
        if let Some((vault, device)) = self.host.vault() {
            let _ = audit::record(&vault, device, event);
        }
        self.host.activity();
    }
}

#[cfg(unix)]
impl<H: Host> crate::ipc::Handler for Broker<H> {
    fn handle(&self, request: Request) -> Response {
        match request {
            Request::ListProjects => self.list_projects(),
            Request::ListVariables(target) => self.list_variables(&target),
            Request::Run(run) => self.run(run),
            Request::Generate(generate) => self.generate(generate),
        }
    }
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

