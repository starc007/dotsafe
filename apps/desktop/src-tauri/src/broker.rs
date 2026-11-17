//! The desktop side of the agent broker: the broker itself is
//! `dotsafe_agent::broker`, shared with `dotsafe daemon`. Here it gets the
//! app's session for the vault and grants, and asks the user with a dialog
//! (confirmed with Touch ID in `answer_agent_request`).
//!
//! It also answers machines with approve shares: while unlocked, it watches
//! each approve share's doorbell, and shows what a machine asks for in the
//! same dialog, then writes the answer back through the share bucket.

// Agent access needs the Unix socket for now; elsewhere the broker is built
// but never started.
#![cfg_attr(not(unix), allow(dead_code, unused_imports))]

use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use dotsafe_agent::broker::{ApprovalRequest, Broker, Host};
use dotsafe_agent::grants::{Lifetime, Scope};
use dotsafe_agent::policy;
use dotsafe_core::audit::{self, AuditEvent};
use dotsafe_core::Vault;
use dotsafe_integrations::approvals::{self, Decision, Waiting};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, UserAttentionType};
use uuid::Uuid;

use crate::state::AppState;

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

/// The broker's host in the app: the session's vault and grants, and the
/// approval dialog.
pub struct DesktopHost {
    state: AppState,
    approvals: Approvals,
    app: AppHandle,
}

impl Host for DesktopHost {
    fn vault(&self) -> Option<(Arc<Vault>, Uuid)> {
        let session = self.state.0.lock().unwrap_or_else(|e| e.into_inner());
        Some((session.vault.clone()?, session.config.as_ref()?.device_id))
    }

    fn allows(&self, scope: &Scope, now: Instant) -> bool {
        self.state.0.lock().unwrap_or_else(|e| e.into_inner()).agent_grants.allows(scope, now)
    }

    fn grant(&self, scope: Scope, lifetime: Lifetime, now: Instant) {
        self.state.0.lock().unwrap_or_else(|e| e.into_inner()).agent_grants.add(scope, lifetime, now);
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

    fn activity(&self) {
        let _ = self.app.emit("agent-activity", ());
    }

    fn vault_changed(&self) {
        let _ = self.app.emit("vault-synced", ());
    }
}

/// How often an unlocked app looks at approve shares' doorbells. One cheap
/// HEAD request per approve share each time.
const DOORBELL_CHECK: Duration = Duration::from_secs(2);

/// A machine's request as the approval dialog shows it: an agent request's
/// fields, plus where it comes from.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct MachineRequest {
    id: Uuid,
    kind: &'static str,
    client: String,
    project: String,
    environment: String,
    command: Vec<String>,
    program_path: String,
    cwd: String,
    variables: Vec<String>,
    proxied: Vec<String>,
    files: Vec<MachineFile>,
    generated: Option<String>,
    machine: MachineInfo,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct MachineFile {
    path: String,
    modified_ms: Option<u64>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct MachineInfo {
    share: String,
    host: String,
    /// The command's output goes back to an agent, scrubbed. Otherwise it
    /// goes to the machine's own terminal or log.
    scrubbed: bool,
}

impl MachineRequest {
    fn new(w: &Waiting) -> MachineRequest {
        let r = &w.request;
        MachineRequest {
            id: r.id,
            kind: "run",
            client: r.client.clone(),
            project: w.share_name.clone(),
            environment: r.environment.clone(),
            command: r.command.clone(),
            program_path: r.program_path.clone(),
            cwd: r.cwd.clone(),
            variables: r.variables.clone(),
            proxied: r.proxied.clone(),
            files: r.files.iter().map(|f| MachineFile { path: f.path.clone(), modified_ms: f.modified_ms }).collect(),
            generated: None,
            machine: MachineInfo { share: w.share_name.clone(), host: r.host.clone(), scrubbed: r.kind == "run" },
        }
    }
}

/// Watches approve shares' doorbells while the vault is unlocked, and asks
/// the user about each new request a machine writes.
pub fn spawn_share_watcher(state: AppState, approvals: Approvals, app: AppHandle) {
    std::thread::spawn(move || {
        // Each share's doorbell as last seen. The first look reads the
        // slots too, for requests made while the app was locked.
        let mut rung: HashMap<Uuid, Option<String>> = HashMap::new();
        let asking: Arc<Mutex<HashSet<Uuid>>> = Arc::default();
        loop {
            std::thread::sleep(DOORBELL_CHECK);
            let unlocked = state.0.lock().ok().and_then(|s| Some((s.vault.clone()?, s.config.clone()?)));
            let Some((vault, config)) = unlocked.filter(|(_, c)| c.share_bucket.is_some()) else {
                rung.clear();
                continue;
            };
            let Ok(records) = vault.list() else { continue };
            let shares = approvals::approve_shares(&records, now_ms());
            rung.retain(|id, _| shares.iter().any(|s| s.id == *id));
            if shares.is_empty() {
                continue;
            }
            let Ok(Some(bucket)) = crate::shares::bucket_of(&config) else { continue };
            let changed: Vec<_> = shares
                .into_iter()
                .filter(|s| match approvals::doorbell(&bucket, s) {
                    Ok(tag) => rung.insert(s.id, tag.clone()) != Some(tag),
                    Err(_) => false,
                })
                .collect();
            if changed.is_empty() {
                continue;
            }
            let Ok(waiting) = approvals::waiting(&vault, &bucket, &changed, now_ms()) else { continue };
            for w in waiting {
                let new = asking.lock().map(|mut a| a.insert(w.request.id)).unwrap_or(false);
                if new {
                    let (state, approvals, app, asking) = (state.clone(), approvals.clone(), app.clone(), asking.clone());
                    std::thread::spawn(move || {
                        ask_machine(&state, &approvals, &app, &w);
                        if let Ok(mut a) = asking.lock() {
                            a.remove(&w.request.id);
                        }
                    });
                }
            }
        }
    });
}

/// Shows a machine's request in the approval dialog, waits for the user
/// (until the machine stops waiting), and writes the answer. Touch ID is
/// checked in `answer_agent_request`, as for agents.
fn ask_machine(state: &AppState, approvals: &Approvals, app: &AppHandle, w: &Waiting) {
    let r = &w.request;
    let (tx, rx) = mpsc::channel();
    let n = r.variables.len();
    let reason = format!(
        "let {} on {} run “{}” with {n} {}",
        r.client,
        r.host,
        r.command.join(" "),
        if n == 1 { "secret" } else { "secrets" }
    );
    if let Ok(mut pending) = approvals.0.lock() {
        pending.insert(r.id, Pending { answer: tx, reason });
    }
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        let _ = window.request_user_attention(Some(UserAttentionType::Critical));
    }
    let _ = app.emit("agent-request", MachineRequest::new(w));
    let answer = rx.recv_timeout(Duration::from_millis(r.expires.saturating_sub(now_ms())));
    if let Ok(mut pending) = approvals.0.lock() {
        pending.remove(&r.id);
    }
    let _ = app.emit("agent-request-closed", r.id);
    let decision = match answer {
        Ok(Some(Lifetime::Once)) => Decision::Once,
        Ok(Some(Lifetime::For(d))) => Decision::For(d.as_millis() as u64),
        // The dialog doesn't offer "until lock" for machines.
        Ok(Some(Lifetime::UntilLock)) => Decision::For(60 * 60 * 1000),
        Ok(None) => Decision::Deny,
        // The machine stopped waiting; there's nothing to answer.
        Err(_) => return,
    };
    let unlocked = state.0.lock().ok().and_then(|s| Some((s.vault.clone()?, s.config.clone()?)));
    let outcome = match unlocked {
        None => Err("dotsafe locked before the answer was sent".to_string()),
        Some((vault, config)) => match crate::shares::bucket_of(&config) {
            Ok(Some(bucket)) => approvals::answer(&vault, &bucket, config.device_id, w, decision).map(|_| ()).map_err(|e| e.to_string()),
            Ok(None) => Err("this Mac has no share bucket".into()),
            Err(e) => Err(e.message),
        },
    };
    match outcome {
        Ok(()) => {
            let _ = app.emit("agent-activity", ());
        }
        Err(why) => {
            let _ = app.emit("share-approval-failed", format!("The answer to {} wasn't sent: {why}.", r.host));
        }
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// Starts listening for agents. Unix only for now; elsewhere `dotsafe mcp`
/// says agent access isn't available yet.
pub fn spawn(state: AppState, approvals: Approvals, app: AppHandle) {
    #[cfg(unix)]
    {
        let Some(path) = dotsafe_agent::socket_path() else { return };
        match dotsafe_agent::ipc::bind(&path) {
            Ok(listener) => {
                let broker = Arc::new(Broker::new(DesktopHost { state, approvals, app }));
                std::thread::spawn(move || dotsafe_agent::ipc::serve(listener, broker));
            }
            Err(e) => eprintln!("dotsafe: agent access is off: {e}"),
        }
    }
    #[cfg(not(unix))]
    let _ = (state, approvals, app);
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
