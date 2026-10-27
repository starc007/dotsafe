//! `dotsafe daemon`: the agent broker without the desktop app, for Linux
//! machines, servers reached over SSH, or anyone who'd rather not run the app.
//!
//! It serves the same socket `dotsafe mcp` talks to. Approvals are asked in
//! the daemon's terminal; without one (stdin isn't a terminal), every request
//! is refused, because nobody is there to say yes. Grants last until the
//! daemon stops, or for the time chosen.

use std::io::{BufRead, IsTerminal, Write};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use dotsafe_agent::broker::{ApprovalRequest, Broker, Host};
use dotsafe_agent::grants::{Grants, Lifetime, Scope};
use dotsafe_core::Vault;
use uuid::Uuid;

struct TerminalHost {
    vault: Arc<Vault>,
    device: Uuid,
    grants: Mutex<Grants>,
    /// Ask in the terminal; otherwise refuse everything.
    interactive: bool,
    /// One prompt at a time.
    asking: Mutex<()>,
}

impl Host for TerminalHost {
    fn vault(&self) -> Option<(Arc<Vault>, Uuid)> {
        Some((self.vault.clone(), self.device))
    }

    fn allows(&self, scope: &Scope, now: Instant) -> bool {
        self.grants.lock().unwrap_or_else(|e| e.into_inner()).allows(scope, now)
    }

    fn grant(&self, scope: Scope, lifetime: Lifetime, now: Instant) {
        self.grants.lock().unwrap_or_else(|e| e.into_inner()).add(scope, lifetime, now);
    }

    fn ask(&self, prompt: ApprovalRequest) -> Option<Lifetime> {
        let _turn = self.asking.lock().unwrap_or_else(|e| e.into_inner());
        let mut err = std::io::stderr();
        let _ = writeln!(err);
        let _ = writeln!(err, "{}", describe(&prompt));
        if !self.interactive {
            let _ = writeln!(err, "Refused: nobody is at a terminal to approve it.");
            return None;
        }
        let question = if prompt.kind == "generate" {
            "Allow? [y] yes, [N] no: "
        } else {
            "Allow? [y] once, [t] 15 minutes, [h] an hour, [a] until the daemon stops, [N] no: "
        };
        let _ = write!(err, "{question}");
        let _ = err.flush();
        let mut line = String::new();
        std::io::stdin().lock().read_line(&mut line).ok()?;
        let answer = parse_answer(&line, prompt.kind);
        let _ = writeln!(err, "{}", if answer.is_some() { "Allowed." } else { "Denied." });
        answer
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

/// What the person is asked about, in a few lines.
fn describe(p: &ApprovalRequest) -> String {
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
    text
}

pub fn run(stop_after: Option<Duration>) -> Result<()> {
    let config = crate::load_config()?;
    let vault = crate::session::open(&config)?;
    let path = dotsafe_agent::socket_path().context("no config directory for the agent socket")?;
    let listener = match dotsafe_agent::ipc::bind(&path) {
        Ok(listener) => listener,
        Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
            bail!("another dotsafe broker is serving agents (the desktop app?); quit it first")
        }
        Err(e) => return Err(e).context("couldn't open the agent socket"),
    };
    let interactive = std::io::stdin().is_terminal();
    let host = TerminalHost {
        vault: Arc::new(vault),
        device: config.device_id,
        grants: Mutex::new(Grants::default()),
        interactive,
        asking: Mutex::new(()),
    };
    eprintln!("dotsafe daemon is serving agents for {}.", host.vault.location());
    if interactive {
        eprintln!("Approvals are asked here. Stop with Ctrl-C.");
    } else {
        eprintln!("No terminal: every request will be refused. Run it in a terminal to approve requests.");
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
