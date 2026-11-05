//! `dotsafe approve`: answers commands waiting in `dotsafe daemon`, from any
//! terminal on the machine (another SSH session, say).
//!
//! Any process running as this user can reach the daemon's socket, including
//! the cron jobs it serves. So an answer carries a tag made with the vault
//! key, and this command only gets that key from the master password: never
//! from the `dotsafe unlock` cache or the daemon itself.

use std::io::{BufRead, IsTerminal, Write};

use anyhow::{Context, Result, bail};
use dotsafe_agent::protocol::{AnswerRequest, PendingInfo, Request, Response, answer_message};
use dotsafe_core::Vault;

use crate::daemon::{describe, summary};

pub fn run(id: Option<String>, lifetime: Option<String>, deny: bool, list: bool) -> Result<()> {
    let socket = dotsafe_agent::socket_path().context("no config directory for the agent socket")?;
    let call = |request: &Request| {
        dotsafe_agent::ipc::call(&socket, request)
            .context("couldn't reach dotsafe daemon; start it with `dotsafe daemon --detach`")
    };
    let pending = match call(&Request::Pending)? {
        Response::Pending(pending) => pending,
        Response::Error(e) => bail!("{}", e.message),
        other => bail!("unexpected answer from dotsafe daemon: {other:?}"),
    };
    if list {
        for p in &pending {
            let state = if p.waiting { "waiting" } else { "stopped waiting" };
            println!("{}\t{state}\t{}", p.id, summary(p));
        }
        if pending.is_empty() {
            eprintln!("Nothing is waiting for approval.");
        }
        return Ok(());
    }
    let chosen: Vec<&PendingInfo> = match &id {
        Some(prefix) => {
            let matches: Vec<&PendingInfo> = pending.iter().filter(|p| p.id.to_string().starts_with(prefix)).collect();
            match matches.len() {
                0 => bail!("nothing waiting matches {prefix}; see `dotsafe approve --list`"),
                1 => matches,
                _ => bail!("more than one request starts with {prefix}; give more of the id"),
            }
        }
        None if pending.is_empty() => {
            eprintln!("Nothing is waiting for approval.");
            return Ok(());
        }
        None => pending.iter().collect(),
    };
    let decided = lifetime.is_some() || deny;
    if !decided && !std::io::stdin().is_terminal() {
        bail!("say what to do with --for once|1h|stop or --deny, or run it in a terminal");
    }

    let vault = open_with_password()?;
    let mut stdin = std::io::stdin().lock();
    let mut failed = 0;
    for p in chosen {
        let answer = if decided {
            lifetime.clone()
        } else {
            eprintln!("\n{}", describe(p));
            let question = match (p.kind.as_str(), p.waiting) {
                ("generate", _) => "Allow? [y] yes, [N] no: ",
                (_, true) => "Allow? [y] once, [h] an hour, [a] until the daemon stops, [N] no: ",
                // It gave up waiting: only a standing grant helps its next run.
                (_, false) => "It stopped waiting. Allow its next runs? [h] an hour, [a] until the daemon stops, [N] no: ",
            };
            eprint!("{question}");
            std::io::stderr().flush()?;
            let mut line = String::new();
            stdin.read_line(&mut line)?;
            parse(&line, p)
        };
        let proof = vault.approval_tag(&answer_message(p.id, answer.as_deref()));
        let request = Request::Answer(AnswerRequest { id: p.id, lifetime: answer.clone(), proof });
        match call(&request)? {
            Response::Answered => eprintln!("{}", if answer.is_some() { "Allowed." } else { "Denied." }),
            Response::Error(e) => {
                eprintln!("error: {}", e.message);
                failed += 1;
            }
            other => bail!("unexpected answer from dotsafe daemon: {other:?}"),
        }
    }
    if failed > 0 {
        bail!("{failed} {} not answered", if failed == 1 { "request was" } else { "requests were" });
    }
    Ok(())
}

/// The typed answer: anything but a clear yes is a no.
fn parse(line: &str, p: &PendingInfo) -> Option<String> {
    let repeatable = p.kind != "generate";
    match line.trim().to_lowercase().as_str() {
        "y" | "yes" if p.waiting || !repeatable => Some("once".into()),
        "h" if repeatable => Some("1h".into()),
        "a" if repeatable => Some("stop".into()),
        _ => None,
    }
}

/// Opens the vault with the master password (`DOTSAFE_PASSWORD` or a prompt),
/// to make answer tags. Never the `dotsafe unlock` cache: a cron job could
/// use that as easily as this command.
fn open_with_password() -> Result<Vault> {
    let config = crate::load_config()?;
    let password = crate::session::password("Master password: ")?;
    let store = dotsafe_stores::open(&config).map_err(crate::session::friendly)?;
    Vault::unlock(store, &password).map_err(crate::session::friendly)
}
