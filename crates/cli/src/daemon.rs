//! `dotsafe daemon`: the agent broker without the desktop app, for Linux
//! machines, servers reached over SSH, or anyone who'd rather not run the app.
//!
//! It serves the same socket `dotsafe mcp` talks to, and runs `dotsafe run`
//! commands for scripts and cron jobs, so the master password never has to
//! live on the machine. Each new command waits for approval: in the daemon's
//! terminal, or from `dotsafe approve` in any other terminal (which asks for
//! the master password). Grants last until the daemon stops, or for the time
//! chosen, and live only in its memory.
//!
//! With `--share` it serves a share's snapshot instead of a vault, on a
//! machine with no vault or license (a cloud VM running an agent): the same
//! broker, read-only, with only the share's environments. `dotsafe approve
//! --share` answers with the token, and `--unattended` approves everything
//! the refusal list lets through, for when nobody is there to ask.

use std::collections::HashMap;
use std::io::{BufRead, IsTerminal, Read, Write};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use dotsafe_agent::broker::{ApprovalRequest, Broker, Host};
use dotsafe_agent::grants::{Grants, Lifetime, Scope};
use dotsafe_agent::protocol::PendingInfo;
use dotsafe_core::audit::AuditEvent;
use dotsafe_core::share::Snapshot;
use dotsafe_core::{ShareToken, Vault};
use uuid::Uuid;
use zeroize::Zeroizing;

/// How long a caller waits for an answer when it didn't say.
const DEFAULT_WAIT: Duration = Duration::from_secs(5 * 60);
/// How long a run that stopped waiting can still be approved for next time.
const KEEP_PENDING: Duration = Duration::from_secs(24 * 60 * 60);
/// How long a share's snapshot is used for listing before it's fetched
/// again. Runs always fetch it again.
const SHARE_REFETCH: Duration = Duration::from_secs(30);

struct Pending {
    request: ApprovalRequest,
    created: Instant,
    /// The caller still waiting for the answer, if any.
    waiter: Option<Sender<Option<Lifetime>>>,
}

#[derive(Default)]
struct Queue {
    pending: HashMap<Uuid, Pending>,
    /// The request the terminal is asking about.
    asking: Option<Uuid>,
}

/// Where the daemon's names and values come from.
enum Source {
    Vault(Arc<Vault>, Uuid),
    Share(ShareSource),
}

/// `--share`: the share's snapshot, as last fetched.
struct ShareSource {
    token: ShareToken,
    latest: Mutex<Option<(Instant, Arc<Snapshot>)>>,
}

impl ShareSource {
    /// Fetches the snapshot once, so a bad token or an unreachable bucket
    /// stops the daemon from starting.
    fn open(token: ShareToken) -> Result<ShareSource> {
        let snapshot = crate::share::fetch(&token)?
            .ok_or_else(|| anyhow!("this share has no snapshot: it was revoked or has expired"))?;
        Ok(ShareSource { token, latest: Mutex::new(Some((Instant::now(), Arc::new(snapshot)))) })
    }

    /// The snapshot: the last one if it's recent and `fresh` isn't asked
    /// for, else fetched again. If the bucket can't be reached, the last one
    /// is used while it's within its expiry and lease. A missing snapshot
    /// means the share was revoked or expired, and nothing is served after.
    fn snapshot(&self, fresh: bool) -> Result<Arc<Snapshot>, String> {
        let last = lock(&self.latest).clone();
        if let Some((at, snapshot)) = &last
            && !fresh
            && at.elapsed() < SHARE_REFETCH
        {
            return Ok(snapshot.clone());
        }
        // Fetched without the lock, so a slow bucket holds up nothing else.
        match crate::share::fetch(&self.token) {
            Ok(Some(snapshot)) => {
                let snapshot = Arc::new(snapshot);
                *lock(&self.latest) = Some((Instant::now(), snapshot.clone()));
                Ok(snapshot)
            }
            Ok(None) => {
                *lock(&self.latest) = None;
                Err("The share was revoked or has expired, so this machine can't use it anymore.".into())
            }
            Err(e) => {
                let now = crate::share::now_ms();
                match last {
                    Some((at, snapshot)) if snapshot.expires > now && snapshot.lease_until > now => {
                        let age = ago(at.elapsed().as_millis() as u64);
                        eprintln!("dotsafe daemon: couldn't refresh the share ({e}); using the copy fetched {age}.");
                        Ok(snapshot)
                    }
                    _ => Err(format!("Couldn't read the share: {e}.")),
                }
            }
        }
    }
}

struct DaemonHost {
    source: Source,
    grants: Mutex<Grants>,
    queue: Arc<Mutex<Queue>>,
    /// Ask in the terminal too; otherwise only `dotsafe approve` answers.
    interactive: bool,
    /// `--unattended`: nobody is asked, and every request the refusal list
    /// lets through is allowed. Only with a share.
    unattended: bool,
}

impl DaemonHost {
    /// The command that answers this daemon's requests from another terminal.
    fn approve_command(&self) -> &'static str {
        match self.source {
            Source::Vault(..) => "dotsafe approve",
            Source::Share(_) => "dotsafe approve --share",
        }
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Host for DaemonHost {
    fn vault(&self) -> Option<(Arc<Vault>, Uuid)> {
        match &self.source {
            Source::Vault(vault, device) => Some((vault.clone(), *device)),
            Source::Share(_) => None,
        }
    }

    fn share(&self, fresh: bool) -> Option<Result<Arc<Snapshot>, String>> {
        match &self.source {
            Source::Share(share) => Some(share.snapshot(fresh)),
            Source::Vault(..) => None,
        }
    }

    fn check_answer(&self, message: &[u8], proof: &str) -> bool {
        match &self.source {
            Source::Vault(vault, _) => vault.check_approval_tag(message, proof),
            Source::Share(share) => share.token.check_approval_tag(message, proof),
        }
    }

    fn record(&self, event: AuditEvent) {
        match &self.source {
            Source::Vault(vault, device) => {
                let _ = dotsafe_core::audit::record(vault, *device, event);
            }
            // No vault to write to: the daemon's own output (its log when
            // detached), names only.
            Source::Share(_) => {
                let outcome = format!("{:?}", event.outcome).to_lowercase();
                let detail = event.detail.map(|d| format!("\t{d}")).unwrap_or_default();
                eprintln!(
                    "{}\t{}\t{outcome}\t{}\t{}\t{}{detail}",
                    crate::local_time(event.at),
                    event.actor,
                    event.summary,
                    event.environment,
                    event.variables.join(",")
                );
            }
        }
    }

    fn allows(&self, scope: &Scope, now: Instant) -> bool {
        self.unattended || lock(&self.grants).allows(scope, now)
    }

    fn grant(&self, scope: Scope, lifetime: Lifetime, now: Instant) {
        lock(&self.grants).add(scope, lifetime, now);
    }

    fn ask(&self, prompt: ApprovalRequest) -> Option<Lifetime> {
        let id = prompt.id;
        let wait = prompt.wait.unwrap_or(DEFAULT_WAIT);
        let (tx, rx) = mpsc::channel();
        let mut err = std::io::stderr();
        let _ = writeln!(err, "\n{}", describe(&prompt.info(true, Duration::ZERO)));
        {
            let mut queue = lock(&self.queue);
            queue.pending.insert(id, Pending { request: prompt, created: Instant::now(), waiter: Some(tx) });
            if self.interactive {
                prompt_next(&mut queue);
            } else {
                let _ = writeln!(err, "Waiting up to {} for `{}`.", minutes(wait), self.approve_command());
            }
        }
        let answer = rx.recv_timeout(wait);
        let mut queue = lock(&self.queue);
        // An answer that came in just as the wait ran out still counts.
        let answer = answer.or_else(|_| rx.try_recv()).ok();
        if answer.is_none() {
            let keep = queue.pending.get(&id).is_some_and(|p| p.request.scope.is_some());
            if keep {
                // Approving it later still counts for the next run.
                if let Some(p) = queue.pending.get_mut(&id) {
                    p.waiter = None;
                }
                let _ = writeln!(err, "Stopped waiting. It can still be approved with `{}` for next time.", self.approve_command());
            } else {
                queue.pending.remove(&id);
            }
            if queue.asking == Some(id) {
                queue.asking = None;
                let _ = writeln!(err);
                if self.interactive {
                    prompt_next(&mut queue);
                }
            }
        }
        answer.flatten()
    }

    fn runs_for_callers(&self) -> bool {
        true
    }

    fn pending(&self) -> Option<Vec<PendingInfo>> {
        let mut queue = lock(&self.queue);
        queue.pending.retain(|_, p| p.waiter.is_some() || p.created.elapsed() < KEEP_PENDING);
        let mut list: Vec<(Instant, PendingInfo)> = queue
            .pending
            .values()
            .map(|p| (p.created, p.request.info(p.waiter.is_some(), p.created.elapsed())))
            .collect();
        list.sort_by_key(|(created, _)| *created);
        Some(list.into_iter().map(|(_, info)| info).collect())
    }

    fn answer(&self, id: Uuid, lifetime: Option<Lifetime>) -> Result<(), String> {
        let mut queue = lock(&self.queue);
        let Some(pending) = queue.pending.remove(&id) else {
            return Err("Nothing with that id is waiting; it may have been answered already.".into());
        };
        let late = pending.waiter.is_none();
        if late && lifetime == Some(Lifetime::Once) {
            queue.pending.insert(id, pending);
            return Err("That command stopped waiting, so \"once\" would do nothing. Allow it for an hour or until the daemon stops.".into());
        }
        let mut err = std::io::stderr();
        match (&pending.waiter, lifetime) {
            (Some(waiter), _) => {
                let _ = waiter.send(lifetime);
            }
            (None, Some(lifetime)) => {
                if let Some(scope) = pending.request.scope.clone() {
                    self.grant(scope, lifetime, Instant::now());
                }
            }
            (None, None) => {}
        }
        let verdict = if lifetime.is_some() { "Allowed" } else { "Denied" };
        let _ = writeln!(err, "{verdict} with dotsafe approve: {}", summary(&pending.request.info(!late, Duration::ZERO)));
        if queue.asking == Some(id) {
            queue.asking = None;
            if self.interactive {
                prompt_next(&mut queue);
            }
        }
        Ok(())
    }
}

/// Asks in the terminal about the oldest request still waiting, if the
/// terminal isn't asking about one already.
fn prompt_next(queue: &mut Queue) {
    if queue.asking.is_some() {
        return;
    }
    let next = queue.pending.iter().filter(|(_, p)| p.waiter.is_some()).min_by_key(|(_, p)| p.created);
    let Some((id, pending)) = next else { return };
    let (id, repeatable) = (*id, pending.request.kind != "generate");
    queue.asking = Some(id);
    let mut err = std::io::stderr();
    if queue.pending.len() > 1 {
        let _ = writeln!(err, "\n{}", describe(&pending.request.info(true, pending.created.elapsed())));
    }
    let question = if repeatable {
        "Allow? [y] once, [t] 15 minutes, [h] an hour, [a] until the daemon stops, [N] no: "
    } else {
        "Allow? [y] yes, [N] no: "
    };
    let _ = write!(err, "{question}");
    let _ = err.flush();
}

/// Reads answers typed in the daemon's terminal, for the request it's asking about.
fn read_terminal(queue: Arc<Mutex<Queue>>) {
    for line in std::io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        let mut queue = lock(&queue);
        let Some(id) = queue.asking.take() else { continue };
        let Some(pending) = queue.pending.remove(&id) else { continue };
        let answer = parse_answer(&line, pending.request.kind);
        if let Some(waiter) = pending.waiter {
            let _ = waiter.send(answer);
        }
        eprintln!("{}", if answer.is_some() { "Allowed." } else { "Denied." });
        prompt_next(&mut queue);
    }
}

/// The typed answer: anything but a clear yes is a no. Generating a secret
/// is a one-off, so it can't be granted for a while.
fn parse_answer(line: &str, kind: &str) -> Option<Lifetime> {
    let repeatable = kind != "generate";
    match line.trim().to_lowercase().as_str() {
        "y" | "yes" => Some(Lifetime::Once),
        "t" if repeatable => Some(Lifetime::For(Duration::from_secs(15 * 60))),
        "h" if repeatable => Some(Lifetime::For(Duration::from_secs(60 * 60))),
        "a" if repeatable => Some(Lifetime::UntilLock),
        _ => None,
    }
}

/// One line: who wants what.
pub fn summary(p: &PendingInfo) -> String {
    let env = format!("{}/{}", p.project, p.environment);
    match p.kind.as_str() {
        "generate" => format!("{} wants to add {} to {env}", p.client, p.variables.join(", ")),
        _ => format!("{} wants to run {} with {} from {env}", p.client, p.command.join(" "), p.variables.join(", ")),
    }
}

/// What the person is asked about, in a few lines.
pub fn describe(p: &PendingInfo) -> String {
    let env = format!("{}/{}", p.project, p.environment);
    if p.kind == "generate" {
        let what = p.generated.as_deref().unwrap_or("a random value");
        return format!("{} wants to add {} ({what}) to {env}.", p.client, p.variables.join(", "));
    }
    let mut text = format!(
        "{} wants to run: {}\n  in {} ({env}), program {}",
        p.client,
        p.command.join(" "),
        p.cwd,
        p.program_path
    );
    let n = p.variables.len();
    text.push_str(&format!("\n  with {n} {}: {}", if n == 1 { "secret" } else { "secrets" }, p.variables.join(", ")));
    if !p.proxied.is_empty() {
        text.push_str(&format!("\n  through the secret proxy: {}", p.proxied.join(", ")));
    }
    if p.kind == "exec" {
        text.push_str("\n  its output goes to its own terminal or log, not scrubbed");
    }
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64);
    for file in &p.files {
        let changed = file.modified_ms.map_or(String::new(), |ms| format!(", changed {}", ago(now.saturating_sub(ms))));
        text.push_str(&format!("\n  runs {}{changed}", file.path));
    }
    text
}

/// "20 seconds ago", "5 minutes ago", "3 days ago".
pub fn ago(ms: u64) -> String {
    let secs = ms / 1000;
    let (n, unit) = match secs {
        0..60 => (secs, "second"),
        60..3600 => (secs / 60, "minute"),
        3600..86_400 => (secs / 3600, "hour"),
        _ => (secs / 86_400, "day"),
    };
    format!("{n} {unit}{} ago", if n == 1 { "" } else { "s" })
}

fn minutes(d: Duration) -> String {
    match d.as_secs() {
        1 => "1 second".into(),
        s if s < 120 => format!("{s} seconds"),
        s => format!("{} minutes", s / 60),
    }
}

/// How the daemon gets the vault (or the share).
#[derive(Clone, Copy)]
pub enum Start {
    /// Unlock here: session cache, `DOTSAFE_PASSWORD`, or a prompt. With
    /// `--share`, the token in `DOTSAFE_SHARE`.
    Here,
    /// `--detach`: unlock here, then hand the key (or the share token) to a
    /// copy of itself that keeps running after the terminal closes.
    Detach,
    /// The detached copy: the vault key or share token comes in on stdin
    /// from `Detach`.
    KeyFromStdin,
}

pub struct Options {
    pub stop_after: Option<Duration>,
    pub start: Start,
    /// Serve the share in `DOTSAFE_SHARE` instead of a vault.
    pub share: bool,
    /// With `share`: allow every request without asking.
    pub unattended: bool,
}

pub fn run(options: Options) -> Result<()> {
    let Options { stop_after, start, share, unattended } = options;
    let path = dotsafe_agent::socket_path().context("no config directory for the agent socket")?;
    if matches!(start, Start::Detach) {
        return detach(&path, stop_after, share, unattended);
    }
    let source = match (share, start) {
        (true, Start::KeyFromStdin) => {
            let mut token = Zeroizing::new(String::new());
            std::io::stdin().read_to_string(&mut token).context("no share token on stdin")?;
            Source::Share(ShareSource::open(ShareToken::parse(&token)?)?)
        }
        (true, _) => Source::Share(ShareSource::open(crate::share::token_from_env()?)?),
        (false, Start::KeyFromStdin) => {
            let config = crate::load_config()?;
            let mut key = Zeroizing::new([0u8; 32]);
            std::io::stdin().read_exact(key.as_mut()).context("no vault key on stdin")?;
            let store = dotsafe_stores::open(&config).map_err(crate::session::friendly)?;
            let vault = Vault::unlock_with_session_key(store, &key).map_err(crate::session::friendly)?;
            Source::Vault(Arc::new(vault), config.device_id)
        }
        (false, _) => {
            let config = crate::load_config()?;
            Source::Vault(Arc::new(crate::session::open(&config)?), config.device_id)
        }
    };
    let listener = match dotsafe_agent::ipc::bind(&path) {
        Ok(listener) => listener,
        Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
            bail!("another dotsafe broker is serving agents (the desktop app?); quit it first")
        }
        Err(e) => return Err(e).context("couldn't open the agent socket"),
    };
    let interactive = matches!(start, Start::Here) && std::io::stdin().is_terminal() && !unattended;
    let queue = Arc::new(Mutex::new(Queue::default()));
    let host = DaemonHost { source, grants: Mutex::new(Grants::default()), queue: queue.clone(), interactive, unattended };
    match &host.source {
        Source::Vault(vault, _) => eprintln!("dotsafe daemon is serving {}.", vault.location()),
        Source::Share(share) => {
            let labels = share.snapshot(false).map(|s| s.environments.keys().cloned().collect::<Vec<_>>().join(", "));
            eprintln!("dotsafe daemon is serving a share with {}.", labels.unwrap_or_default());
        }
    }
    if unattended {
        eprintln!("Nobody is asked: every command agents ask for runs with the share's values, except what dotsafe always refuses.");
    } else if interactive {
        eprintln!("Approve requests here, or with `{}` in another terminal. Stop with Ctrl-C.", host.approve_command());
        std::thread::spawn(move || read_terminal(queue));
    } else {
        eprintln!("Approve requests with `{}`.", host.approve_command());
    }
    if let Some(after) = stop_after {
        std::thread::spawn(move || {
            std::thread::sleep(after);
            eprintln!("dotsafe daemon: time's up; stopping.");
            std::process::exit(0);
        });
    }
    dotsafe_agent::ipc::serve(listener, Arc::new(Broker::new(host)));
    Ok(())
}

/// Unlocks, starts a copy of the daemon in its own session (so it outlives
/// this terminal and the SSH connection), and passes it the vault key over a
/// pipe. With `share`, it passes the share token instead, and the copy's
/// environment doesn't carry it. Nothing is written to disk but the log,
/// which has names only.
fn detach(socket: &std::path::Path, stop_after: Option<Duration>, share: bool, unattended: bool) -> Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};

    if std::os::unix::net::UnixStream::connect(socket).is_ok() {
        bail!("a dotsafe daemon (or the desktop app) is already running");
    }
    let key: Zeroizing<Vec<u8>> = if share {
        // Checked here, so a typo fails in this terminal, not in the log.
        crate::share::token_from_env()?;
        Zeroizing::new(std::env::var(crate::share::TOKEN_VAR).unwrap_or_default().into_bytes())
    } else {
        let vault = crate::session::open(&crate::load_config()?)?;
        Zeroizing::new(vault.session_key().to_vec())
    };
    let log_path = socket.with_file_name("daemon.log");
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(&log_path)
        .with_context(|| format!("couldn't open {}", log_path.display()))?;
    let mut cmd = Command::new(std::env::current_exe().context("couldn't find the dotsafe binary")?);
    cmd.arg("daemon").arg("--key-from-stdin");
    if share {
        cmd.arg("--share");
    }
    if unattended {
        cmd.arg("--unattended");
    }
    if let Some(after) = stop_after {
        cmd.arg("--for").arg(format!("{}s", after.as_secs()));
    }
    // The copy finds its vault from the same config, never from a password,
    // and gets a share token only on stdin.
    cmd.env_remove("DOTSAFE_PASSWORD").env_remove(crate::share::TOKEN_VAR);
    cmd.stdin(Stdio::piped()).stdout(Stdio::null()).stderr(log);
    // SAFETY: setsid is async-signal-safe and touches no memory.
    unsafe {
        cmd.pre_exec(|| {
            if libc::setsid() < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut child = cmd.spawn().context("couldn't start the daemon")?;
    {
        let mut stdin = child.stdin.take().context("no pipe to the daemon")?;
        stdin.write_all(&key).context("couldn't hand the key to the daemon")?;
    }
    drop(key);
    let started = Instant::now();
    while std::os::unix::net::UnixStream::connect(socket).is_err() {
        if let Ok(Some(status)) = child.try_wait() {
            bail!("the daemon stopped ({status}); see {}", log_path.display());
        }
        if started.elapsed() > Duration::from_secs(30) {
            bail!("the daemon didn't start; see {}", log_path.display());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    eprintln!("dotsafe daemon is running in the background (pid {}).", child.id());
    match (share, unattended) {
        (true, true) => eprintln!("Nobody is asked before commands run. Log: {}", log_path.display()),
        (true, false) => eprintln!("Approve new commands with `dotsafe approve --share`. Log: {}", log_path.display()),
        _ => eprintln!("Approve new commands with `dotsafe approve`. Log: {}", log_path.display()),
    }
    let held = if share { "the share's values" } else { "the vault key" };
    eprintln!("Stop it with `kill {}`; it holds {held} until then.", child.id());
    Ok(())
}

/// `dotsafe run` through the daemon, when this machine can't open the vault
/// by itself (no `DOTSAFE_PASSWORD`, no `dotsafe unlock` key) and a daemon is
/// listening, or when `--daemon` says so. The daemon starts the command on
/// our stdin, stdout and stderr; we only wait for it. Returns `None` to open
/// the vault here instead.
pub fn exec_through(
    env: Option<&str>,
    proxy: bool,
    only: Option<&[String]>,
    forced: bool,
    wait: Duration,
    command: &[String],
) -> Result<Option<std::process::ExitCode>> {
    use std::os::fd::AsFd;

    use dotsafe_agent::protocol::{ErrorCode, ExecRequest, Response, Target};

    let config = crate::load_config()?;
    if !forced && (std::env::var_os("DOTSAFE_PASSWORD").is_some() || crate::session::has_cached_key(&config)) {
        return Ok(None);
    }
    let socket = dotsafe_agent::socket_path().context("no config directory for the agent socket")?;
    if !forced && std::os::unix::net::UnixStream::connect(&socket).is_err() {
        return Ok(None);
    }
    let cwd = std::env::current_dir()?;
    let request = ExecRequest {
        client: "dotsafe run".into(),
        command: command.to_vec(),
        cwd: cwd.clone(),
        target: Target { project: None, environment: env.map(str::to_string), cwd: Some(cwd) },
        variables: only.map(<[String]>::to_vec),
        path: std::env::var("PATH").ok(),
        proxy,
        wait_secs: Some(wait.as_secs()),
    };
    // A terminal's input belongs to the foreground job, which the command
    // isn't (it's the daemon's child), so reading it would stop it.
    let null = std::fs::File::open("/dev/null")?;
    let stdin = std::io::stdin();
    let input = if stdin.is_terminal() { null.as_fd() } else { stdin.as_fd() };
    let (stdout, stderr) = (std::io::stdout(), std::io::stderr());
    let response = dotsafe_agent::ipc::call_exec(&socket, request, [input, stdout.as_fd(), stderr.as_fd()]);
    match response {
        Ok(Response::Exited(exit)) => {
            let code = exit.code.or(exit.signal.map(|s| 128 + s)).unwrap_or(1);
            Ok(Some(std::process::ExitCode::from(code.clamp(0, 255) as u8)))
        }
        // The desktop app opens the vault itself; so do we.
        Ok(Response::Error(e)) if e.code == ErrorCode::Unavailable && !forced => Ok(None),
        Ok(Response::Error(e)) if e.code == ErrorCode::Denied => bail!(
            "not approved: {} Approve it with `dotsafe approve` (it stays open for a day), then run it again",
            e.message
        ),
        Ok(Response::Error(e)) => bail!("{}", e.message),
        Ok(other) => bail!("unexpected answer from dotsafe daemon: {other:?}"),
        // Never fall back here: the command may already have run.
        Err(e) => Err(e).context("couldn't reach dotsafe daemon; start it with `dotsafe daemon --detach`"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_clear_yes_allows() {
        assert!(matches!(parse_answer("y\n", "run"), Some(Lifetime::Once)));
        assert!(matches!(parse_answer(" H ", "run"), Some(Lifetime::For(d)) if d == Duration::from_secs(3600)));
        assert!(matches!(parse_answer("a", "run"), Some(Lifetime::UntilLock)));
        assert!(parse_answer("", "run").is_none());
        assert!(parse_answer("sure", "run").is_none());
        assert!(parse_answer("a", "generate").is_none(), "generating can't be granted for a while");
    }
}
