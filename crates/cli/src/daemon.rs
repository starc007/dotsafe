//! `dotsafe daemon`: the agent broker without the desktop app, for Linux
//! machines, servers reached over SSH, or anyone who'd rather not run the app.
//!
//! It serves the same socket `dotsafe mcp` talks to, and runs `dotsafe run`
//! commands for scripts and cron jobs, so the master password never has to
//! live on the machine. Each new command waits for approval: in the daemon's
//! terminal, or from `dotsafe approve` in any other terminal (which asks for
//! the master password). Grants last until the daemon stops, or for the time
//! chosen, and live only in its memory.

use std::collections::HashMap;
use std::io::{BufRead, IsTerminal, Read, Write};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use dotsafe_agent::broker::{ApprovalRequest, Broker, Host};
use dotsafe_agent::grants::{Grants, Lifetime, Scope};
use dotsafe_agent::protocol::PendingInfo;
use dotsafe_core::Vault;
use uuid::Uuid;
use zeroize::Zeroizing;

/// How long a caller waits for an answer when it didn't say.
const DEFAULT_WAIT: Duration = Duration::from_secs(5 * 60);
/// How long a run that stopped waiting can still be approved for next time.
const KEEP_PENDING: Duration = Duration::from_secs(24 * 60 * 60);

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

struct DaemonHost {
    vault: Arc<Vault>,
    device: Uuid,
    grants: Mutex<Grants>,
    queue: Arc<Mutex<Queue>>,
    /// Ask in the terminal too; otherwise only `dotsafe approve` answers.
    interactive: bool,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Host for DaemonHost {
    fn vault(&self) -> Option<(Arc<Vault>, Uuid)> {
        Some((self.vault.clone(), self.device))
    }

    fn allows(&self, scope: &Scope, now: Instant) -> bool {
        lock(&self.grants).allows(scope, now)
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
                let _ = writeln!(err, "Waiting up to {} for `dotsafe approve`.", minutes(wait));
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
                let _ = writeln!(err, "Stopped waiting. It can still be approved with `dotsafe approve` for next time.");
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

/// How the daemon gets the vault.
pub enum Start {
    /// Unlock here: session cache, `DOTSAFE_PASSWORD`, or a prompt.
    Here,
    /// `--detach`: unlock here, then hand the key to a copy of itself that
    /// keeps running after the terminal closes.
    Detach,
    /// The detached copy: the vault key comes in on stdin from `Detach`.
    KeyFromStdin,
}

pub fn run(stop_after: Option<Duration>, start: Start) -> Result<()> {
    let config = crate::load_config()?;
    let path = dotsafe_agent::socket_path().context("no config directory for the agent socket")?;
    if matches!(start, Start::Detach) {
        return detach(&config, &path, stop_after);
    }
    let vault = match start {
        Start::KeyFromStdin => {
            let mut key = Zeroizing::new([0u8; 32]);
            std::io::stdin().read_exact(key.as_mut()).context("no vault key on stdin")?;
            let store = dotsafe_stores::open(&config).map_err(crate::session::friendly)?;
            Vault::unlock_with_session_key(store, &key).map_err(crate::session::friendly)?
        }
        _ => crate::session::open(&config)?,
    };
    let listener = match dotsafe_agent::ipc::bind(&path) {
        Ok(listener) => listener,
        Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
            bail!("another dotsafe broker is serving agents (the desktop app?); quit it first")
        }
        Err(e) => return Err(e).context("couldn't open the agent socket"),
    };
    let interactive = matches!(start, Start::Here) && std::io::stdin().is_terminal();
    let queue = Arc::new(Mutex::new(Queue::default()));
    let host = DaemonHost {
        vault: Arc::new(vault),
        device: config.device_id,
        grants: Mutex::new(Grants::default()),
        queue: queue.clone(),
        interactive,
    };
    eprintln!("dotsafe daemon is serving {}.", host.vault.location());
    if interactive {
        eprintln!("Approve requests here, or with `dotsafe approve` in another terminal. Stop with Ctrl-C.");
        std::thread::spawn(move || read_terminal(queue));
    } else {
        eprintln!("Approve requests with `dotsafe approve`.");
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
/// pipe. Nothing is written to disk but the log, which has names only.
fn detach(config: &dotsafe_core::Config, socket: &std::path::Path, stop_after: Option<Duration>) -> Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};

    if std::os::unix::net::UnixStream::connect(socket).is_ok() {
        bail!("a dotsafe daemon (or the desktop app) is already running");
    }
    let vault = crate::session::open(config)?;
    let key = vault.session_key();
    drop(vault);
    let log_path = socket.with_file_name("daemon.log");
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(&log_path)
        .with_context(|| format!("couldn't open {}", log_path.display()))?;
    let mut cmd = Command::new(std::env::current_exe().context("couldn't find the dotsafe binary")?);
    cmd.arg("daemon").arg("--key-from-stdin");
    if let Some(after) = stop_after {
        cmd.arg("--for").arg(format!("{}s", after.as_secs()));
    }
    // The copy finds its vault from the same config, never from a password.
    cmd.env_remove("DOTSAFE_PASSWORD").stdin(Stdio::piped()).stdout(Stdio::null()).stderr(log);
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
        stdin.write_all(key.as_ref()).context("couldn't hand the vault key to the daemon")?;
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
    eprintln!("Approve new commands with `dotsafe approve`. Log: {}", log_path.display());
    eprintln!("Stop it with `kill {}`; it holds the vault key until then.", child.id());
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
