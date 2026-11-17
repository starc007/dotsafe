//! `dotsafe approve`: answers commands waiting in `dotsafe daemon`, from any
//! terminal on the machine (another SSH session, say).
//!
//! Any process running as this user can reach the daemon's socket, including
//! the cron jobs it serves. So an answer carries a tag made with the vault
//! key, and this command only gets that key from the master password: never
//! from the `dotsafe unlock` cache or the daemon itself.
//!
//! `dotsafe daemon --share` has no vault key. With `--share`, answers are
//! tagged with a key from the share token in `DOTSAFE_SHARE` instead: it
//! already reads every value in the share, and the commands the daemon runs
//! never get it.

use std::io::{BufRead, IsTerminal, Write};

use anyhow::{Context, Result, bail};
use dotsafe_agent::protocol::{AnswerRequest, PendingInfo, Request, Response, answer_message};
use dotsafe_core::{ShareToken, Vault};

use crate::daemon::{describe, summary};

pub fn run(id: Option<String>, lifetime: Option<String>, deny: bool, list: bool, share: bool) -> Result<()> {
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

    let signer = if share { Signer::Share(crate::share::token_from_env()?) } else { Signer::Vault(Box::new(crate::session::open_with_password()?)) };
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
        let proof = signer.tag(&answer_message(p.id, answer.as_deref()));
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

/// What answers are tagged with: the vault key, or a share's token.
enum Signer {
    Vault(Box<Vault>),
    Share(ShareToken),
}

impl Signer {
    fn tag(&self, message: &[u8]) -> String {
        match self {
            Signer::Vault(vault) => vault.approval_tag(message),
            Signer::Share(token) => token.approval_tag(message),
        }
    }
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
