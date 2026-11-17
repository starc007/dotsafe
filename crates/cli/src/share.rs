//! `dotsafe share ...`: give a machine (CI, a server, a cloud agent) some
//! environments through a sealed snapshot in the share bucket, and
//! `dotsafe run --share`, which uses one on that machine.

use std::io::{BufRead, IsTerminal, Write};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use clap::Subcommand;
use dotsafe_core::share::{self, Answer, DEFAULT_LEASE_DAYS, Mailbox, Request, SLOTS, Status};
use dotsafe_core::{Record, Share, ShareBucket, ShareBucketLocation, ShareEnv, ShareMode, ShareToken, Stamp};
use dotsafe_integrations::approvals::{self, Decision, Waiting};
use dotsafe_integrations::shares::{self as flow, Action, ShareResult};
use dotsafe_stores::{Fetcher, S3Credentials, S3ShareBucket};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::{Ctx, load_config, local_time, session};

/// The variable `dotsafe run --share` reads the token from.
pub const TOKEN_VAR: &str = "DOTSAFE_SHARE";

#[derive(Subcommand)]
pub enum ShareCmd {
    /// Set up the share bucket: an S3-compatible bucket, public-read with listing off. The write key is read from DOTSAFE_SHARE_BUCKET_ACCESS_KEY_ID and DOTSAFE_SHARE_BUCKET_SECRET_ACCESS_KEY, or asked for, and kept in the OS keychain
    Setup {
        /// Bucket name; use one only for shares, not the vault's
        #[arg(long)]
        bucket: String,
        /// S3 endpoint devices write through, e.g. https://<account>.r2.cloudflarestorage.com
        #[arg(long)]
        endpoint: String,
        /// Region (`auto` for R2)
        #[arg(long, default_value = "auto")]
        region: String,
        /// Where machines read it, https:// only: an R2 custom domain, or the bucket's public S3 address
        #[arg(long)]
        public_url: String,
        /// Save it without checking it's public, unlisted and accepts expiry rules
        #[arg(long)]
        no_check: bool,
    },
    /// Give a machine some environments. Prints its token, once
    Create {
        /// What it's for, e.g. ci or staging-vm
        name: String,
        /// Environments to share: NAME in the linked project, or project/NAME. Repeat for more
        #[arg(short, long = "env", required = true, value_name = "ENV")]
        envs: Vec<String>,
        /// Only these variables, comma-separated (and anything they refer to)
        #[arg(long, value_delimiter = ',', value_name = "NAMES")]
        only: Option<Vec<String>>,
        /// How long it lasts, e.g. 24h, 30d
        #[arg(long = "for", value_parser = session::parse_duration, default_value = "30d")]
        ttl: Duration,
        /// Days a snapshot stays valid without a refresh from one of your devices
        #[arg(long, default_value_t = DEFAULT_LEASE_DAYS)]
        lease_days: u32,
        /// No standing snapshot: each run on the machine waits for you to approve it, in the app or with `dotsafe share approve`
        #[arg(long)]
        approve: bool,
    },
    /// List shares, and whether each one has the vault's current values
    List,
    /// Answer runs waiting for approval on approve shares (asks for the master password)
    Approve {
        /// Allow each one: once, or 1h (this run, and the same command for an hour)
        #[arg(long = "for", value_parser = ["once", "1h"], conflicts_with = "deny")]
        lifetime: Option<String>,
        /// Deny each one
        #[arg(long)]
        deny: bool,
        /// Only list what's waiting
        #[arg(long, conflicts_with_all = ["lifetime", "deny"])]
        list: bool,
    },
    /// Push shares that are out of date now
    Push,
    /// Revoke a share: its snapshot is deleted, and what it could read goes on the rotation queue
    Revoke { name: String },
}

pub fn run(cmd: ShareCmd) -> Result<()> {
    match cmd {
        ShareCmd::Setup { bucket, endpoint, region, public_url, no_check } => setup(bucket, endpoint, region, public_url, no_check),
        ShareCmd::Create { name, envs, only, ttl, lease_days, approve: approved } => {
            let mode = if approved { ShareMode::Approve } else { ShareMode::Standing };
            create(&name, &envs, only, ttl, lease_days, mode)
        }
        ShareCmd::List => list(),
        ShareCmd::Approve { lifetime, deny, list } => approve(lifetime, deny, list),
        ShareCmd::Push => push(),
        ShareCmd::Revoke { name } => revoke(&name),
    }
}

fn setup(bucket: String, endpoint: String, region: String, public_url: String, no_check: bool) -> Result<()> {
    let public_url = share::public_base(&public_url)?;
    let location = ShareBucketLocation { endpoint, region, bucket, public_url };
    let from_env = std::env::var_os("DOTSAFE_SHARE_BUCKET_ACCESS_KEY_ID").is_some();
    let credentials = match dotsafe_stores::share_credentials(&location).ok().flatten() {
        Some(credentials) => credentials,
        None => S3Credentials {
            access_key_id: crate::prompt_line("Access key ID: ")?,
            secret_access_key: Zeroizing::new(rpassword::prompt_password("Secret access key: ")?),
        },
    };
    let store = S3ShareBucket::new(&location, &credentials)?;
    if !no_check {
        let ctx = Ctx::open()?;
        eprintln!("Checking {}...", location.describe());
        for warning in dotsafe_stores::share::probe(&store, &Fetcher::new())? {
            eprintln!("warning: {warning}");
        }
        // Also shows the key may set lifecycle rules.
        store.set_expiry_rules(&share::expiry_rules(&ctx.vault.list()?, now_ms())?)?;
    }
    if !from_env {
        dotsafe_stores::save_share_credentials(&location, &credentials)?;
    }
    let mut config = load_config()?;
    config.share_bucket = Some(location.clone());
    config.save()?;
    eprintln!("Shares go to {} now. Create one with `dotsafe share create <name> -e <env>`.", location.describe());
    Ok(())
}

fn create(name: &str, envs: &[String], only: Option<Vec<String>>, ttl: Duration, lease_days: u32, mode: ShareMode) -> Result<()> {
    let ctx = Ctx::open()?;
    let config = load_config()?;
    let bucket = dotsafe_stores::share_bucket(&config)?
        .ok_or_else(|| anyhow!("set up a share bucket first with `dotsafe share setup`"))?;
    let records = ctx.vault.list()?;
    let mut environments = Vec::new();
    for env in envs {
        let id = find_env(&ctx, &records, env)?;
        if environments.iter().any(|e: &ShareEnv| e.id == id) {
            bail!("{env} is listed twice");
        }
        environments.push(ShareEnv { id, only: only.clone() });
    }
    let expires = now_ms().saturating_add(ttl.as_millis() as u64);
    let (share, token) = flow::create(&ctx.vault, &bucket, ctx.device_id, name, mode, environments, expires, lease_days)?;
    let labels = share.labels(&ctx.vault.list()?)?.join(", ");
    eprintln!("Created share {} for {labels}, until {}.", share.name, local_time(share.expires));
    eprintln!("Give the machine this token as {TOKEN_VAR}. It's shown once and never saved:");
    println!("{}", token.as_str());
    eprintln!("On the machine: dotsafe run --share -- <command>, or dotsafe daemon --share for agents");
    if mode == ShareMode::Approve {
        eprintln!("Each run waits for you: approve it in the dotsafe app, or with `dotsafe share approve`.");
    }
    Ok(())
}

/// Answers what machines with approve shares are waiting for. Opens the
/// vault with the master password, never the `dotsafe unlock` cache: an
/// agent on this machine could use that to approve its own requests.
fn approve(lifetime: Option<String>, deny: bool, list: bool) -> Result<()> {
    let decided = lifetime.is_some() || deny;
    if !list && !decided && !std::io::stdin().is_terminal() {
        bail!("say what to do with --for once|1h or --deny, or run it in a terminal");
    }
    let config = load_config()?;
    let bucket = dotsafe_stores::share_bucket(&config)?
        .ok_or_else(|| anyhow!("this machine has no share bucket; set it up with `dotsafe share setup`"))?;
    let vault = session::open_with_password()?;
    let now = now_ms();
    let shares = approvals::approve_shares(&vault.list()?, now);
    if shares.is_empty() {
        eprintln!("No live share asks for approval. Create one with `dotsafe share create <name> -e <env> --approve`.");
        return Ok(());
    }
    let waiting = approvals::waiting(&vault, &bucket, &shares, now)?;
    if waiting.is_empty() {
        eprintln!("Nothing is waiting for approval.");
        return Ok(());
    }
    if list {
        for w in &waiting {
            println!("{}\t{}\t{}", w.share_name, w.request.host, summary(w));
        }
        return Ok(());
    }
    let mut stdin = std::io::stdin().lock();
    let mut failed = 0;
    for w in &waiting {
        let decision = if decided {
            match lifetime.as_deref() {
                Some("1h") => Decision::For(60 * 60 * 1000),
                Some(_) => Decision::Once,
                None => Decision::Deny,
            }
        } else {
            eprintln!("\n{}", describe(w));
            eprint!("Allow? [y] once, [h] this and the same command for an hour, [N] no: ");
            std::io::stderr().flush()?;
            let mut line = String::new();
            stdin.read_line(&mut line)?;
            match line.trim().to_lowercase().as_str() {
                "y" | "yes" => Decision::Once,
                "h" => Decision::For(60 * 60 * 1000),
                _ => Decision::Deny,
            }
        };
        match approvals::answer(&vault, &bucket, config.device_id, w, decision) {
            Ok(_) if decision == Decision::Deny => eprintln!("Denied."),
            Ok(given) => eprintln!("Allowed, with {}.", if given.is_empty() { "no variables".into() } else { given.join(", ") }),
            Err(e) => {
                failed += 1;
                eprintln!("error: {e}");
            }
        }
    }
    if failed > 0 {
        bail!("{failed} {} not answered", if failed == 1 { "request was" } else { "requests were" });
    }
    Ok(())
}

/// One line: which machine wants what.
fn summary(w: &Waiting) -> String {
    let r = &w.request;
    format!("{} wants to run {} with {} from {}", r.client, r.command.join(" "), r.variables.join(", "), r.environment)
}

/// What the person is asked about.
fn describe(w: &Waiting) -> String {
    let r = &w.request;
    let mut text = format!(
        "Share {} on {}: {} wants to run {}\n  in {}, program {}\n  with {}: {}",
        w.share_name,
        r.host,
        r.client,
        r.command.join(" "),
        r.cwd,
        r.program_path,
        r.environment,
        r.variables.join(", ")
    );
    if !r.proxied.is_empty() {
        text.push_str(&format!("\n  through the secret proxy: {}", r.proxied.join(", ")));
    }
    if r.kind == "exec" {
        text.push_str("\n  its output isn't scrubbed");
    }
    for file in &r.files {
        text.push_str(&format!("\n  runs {}", file.path));
    }
    text
}

/// The environment `name` refers to: `project/env`, or an environment of the
/// project this directory is linked to.
fn find_env(ctx: &Ctx, records: &[Record], name: &str) -> Result<Uuid> {
    let envs = records.iter().filter_map(|r| match r {
        Record::Environment(e) if !e.deleted => Some(e),
        _ => None,
    });
    if let Some((project, env)) = name.split_once('/') {
        let projects: Vec<Uuid> = records
            .iter()
            .filter_map(|r| match r {
                Record::Project(p) if !p.deleted && p.name == project => Some(p.id),
                _ => None,
            })
            .collect();
        let found: Vec<Uuid> = envs.filter(|e| projects.contains(&e.project_id) && e.name == env).map(|e| e.id).collect();
        return match found.as_slice() {
            [one] => Ok(*one),
            [] => bail!("no environment {name}"),
            _ => bail!("more than one environment is called {name}"),
        };
    }
    Ok(ctx.target(Some(name.to_string()), false)?.env.id)
}

fn list() -> Result<()> {
    let ctx = Ctx::open()?;
    let records = ctx.vault.list()?;
    let shares = share::shares(&records);
    if shares.is_empty() {
        eprintln!("No shares yet. Give a machine an environment with `dotsafe share create <name> -e <env>`.");
    }
    let now = now_ms();
    for share in shares {
        let mut labels = share.labels(&records).map(|l| l.join(",")).unwrap_or_else(|_| "deleted environment".into());
        if share.mode == ShareMode::Approve {
            labels.push_str(" (approve)");
        }
        let status = match share.status(&records, now) {
            Ok(Status::Current) => "up to date".to_string(),
            Ok(Status::OutOfDate) => "out of date".to_string(),
            Ok(Status::NeverPushed) => "never pushed".to_string(),
            Ok(Status::Expired) => "expired".to_string(),
            Ok(Status::Revoked) => "revoked".to_string(),
            Err(e) => format!("can't be pushed: {e}"),
        };
        let when = match (share.revoked, flow::lease_days_left(&share, now)) {
            (Some(at), _) => format!("revoked {}", local_time(at)),
            _ if share.expires <= now => format!("expired {}", local_time(share.expires)),
            (None, Some(days)) if days < 3 => format!("until {}, lease ends in {days}d", local_time(share.expires)),
            _ => format!("until {}", local_time(share.expires)),
        };
        println!("{}\t{labels}\t{status}\t{when}", share.name);
    }
    Ok(())
}

fn push() -> Result<()> {
    let ctx = Ctx::open()?;
    let config = load_config()?;
    let bucket = dotsafe_stores::share_bucket(&config)?
        .ok_or_else(|| anyhow!("this machine has no share bucket; set it up with `dotsafe share setup`"))?;
    let results = flow::sync(&ctx.vault, &bucket, ctx.device_id)?;
    if results.is_empty() {
        eprintln!("Every share is up to date.");
    }
    if report(&results) > 0 {
        bail!("some shares couldn't be pushed");
    }
    Ok(())
}

fn revoke(name: &str) -> Result<()> {
    let ctx = Ctx::open()?;
    let records = ctx.vault.list()?;
    let named: Vec<Share> = share::shares(&records).into_iter().filter(|s| s.name == name).collect();
    let share = match named.iter().find(|s| s.revoked.is_none()).or(named.first()) {
        Some(share) => share.clone(),
        None => bail!("no share named {name:?}; see `dotsafe share list`"),
    };
    let mut config = load_config()?;
    let bucket = match dotsafe_stores::share_bucket(&config) {
        Ok(bucket) => bucket,
        Err(e) => {
            eprintln!("warning: {e}");
            None
        }
    };
    let failed = flow::revoke(&ctx.vault, bucket.as_ref().map(|b| b as &dyn ShareBucket), ctx.device_id, share.id)?;
    if !config.share_queue.contains(&share.id) {
        config.share_queue.push(share.id);
        config.save()?;
    }
    match failed {
        None => eprintln!("Revoked {name}. Its snapshot is deleted, so the token no longer works."),
        Some(why) => eprintln!(
            "Revoked {name}, but its snapshot couldn't be deleted yet ({why}). The next write or `dotsafe share push` that reaches the bucket deletes it."
        ),
    }
    eprintln!("The machine can still use what it already read. See what to rotate with `dotsafe secret queue`.");
    Ok(())
}

/// Pushes shares after a write, so machines get what changed. Never fails
/// the write: problems are printed, and the next write or sync tries again.
pub fn after_write(ctx: &Ctx) {
    if let Err(e) = try_after_write(ctx) {
        eprintln!("dotsafe: shares weren't updated: {e}");
    }
}

fn try_after_write(ctx: &Ctx) -> Result<()> {
    let records = ctx.vault.list()?;
    let now = now_ms();
    let shares = share::shares(&records);
    if !shares.iter().any(|s| s.is_live(now) || s.in_bucket()) {
        return Ok(());
    }
    match dotsafe_stores::share_bucket(&load_config()?)? {
        Some(bucket) => {
            report(&flow::sync(&ctx.vault, &bucket, ctx.device_id)?);
        }
        None => {
            for share in shares.iter().filter(|s| !matches!(s.status(&records, now), Ok(Status::Current | Status::Expired | Status::Revoked))) {
                eprintln!(
                    "dotsafe: share {} is out of date, and this machine has no share bucket to push it to (`dotsafe share setup`).",
                    share.name
                );
            }
        }
    }
    Ok(())
}

/// The share bucket, for flows that push in passing (rotation). Problems
/// are printed and leave it out.
pub fn bucket() -> Option<S3ShareBucket> {
    let config = load_config().ok()?;
    match dotsafe_stores::share_bucket(&config) {
        Ok(bucket) => bucket,
        Err(e) => {
            eprintln!("dotsafe: shares won't be pushed: {e}");
            None
        }
    }
}

/// Prints what happened to each share. Returns how many failed.
pub fn report(results: &[ShareResult]) -> usize {
    let mut failed = 0;
    for r in results {
        match (&r.error, r.action) {
            (None, Action::Pushed) => eprintln!("dotsafe: pushed share {}.", r.name),
            (None, Action::Renewed) => {}
            (None, Action::Deleted) => eprintln!("dotsafe: deleted the snapshot of share {} (revoked or expired).", r.name),
            (Some(e), _) => {
                failed += 1;
                eprintln!(
                    "dotsafe: couldn't update share {}: {e}. It stays out of date until the next write or `dotsafe share push`.",
                    r.name
                );
            }
        }
    }
    failed
}

/// What a machine remembers about a share: the newest snapshot it read.
#[derive(Serialize, Deserialize)]
struct Seen {
    stamp: Stamp,
}

fn seen_path(share: Uuid) -> Result<PathBuf> {
    Ok(dotsafe_core::config::config_dir()?.join("shares").join(format!("{share}.json")))
}

fn seen(share: Uuid) -> Option<Stamp> {
    let bytes = std::fs::read(seen_path(share).ok()?).ok()?;
    serde_json::from_slice::<Seen>(&bytes).ok().map(|s| s.stamp)
}

fn remember(share: Uuid, stamp: Stamp) -> Result<()> {
    let path = seen_path(share)?;
    std::fs::create_dir_all(path.parent().expect("has a parent"))?;
    std::fs::write(&path, serde_json::to_vec(&Seen { stamp })?)?;
    Ok(())
}

/// `dotsafe run --share`, on the machine: reads the token from
/// `DOTSAFE_SHARE`, fetches and checks the snapshot, and returns the
/// variables to run with. Opens no vault and needs no license: it's
/// licensed through the device that created the share.
pub fn machine_vars(env: Option<&str>, only: Option<&[String]>, command: &[String], wait: Duration) -> Result<Vec<(String, String)>> {
    let token = token_from_env()?;
    let snapshot = match fetch_any(&token)?.ok_or_else(|| anyhow!("this share was revoked or has expired"))? {
        Fetched::Snapshot(snapshot) => snapshot,
        Fetched::Mailbox(mailbox) => {
            let (label, names) = mailbox.environment(env)?;
            let variables = match only {
                Some(only) => {
                    let missing: Vec<&str> = only.iter().filter(|n| !names.contains(n)).map(String::as_str).collect();
                    if !missing.is_empty() {
                        bail!("{label} has no {} in this share", missing.join(", "));
                    }
                    only.to_vec()
                }
                None => names.clone(),
            };
            let cwd = std::env::current_dir()?;
            let program = dotsafe_agent::policy::resolve_program(&command[0], std::env::var("PATH").ok().as_deref(), &cwd);
            let request = new_request("dotsafe run", "exec", command, program, &cwd, label, variables, Vec::new(), wait);
            eprintln!("dotsafe: waiting up to {} for approval, in the dotsafe app or with `dotsafe share approve`...", wait_text(wait));
            let answer = ask(&token, &mailbox, &request, wait)?;
            if !answer.allowed {
                bail!("not approved");
            }
            return Ok(answer.values.into_iter().collect());
        }
    };
    let (name, vars) = snapshot.environment(env)?;
    let mut vars: Vec<(String, String)> = vars.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    if let Some(only) = only {
        let missing: Vec<&str> = only.iter().filter(|n| !vars.iter().any(|(k, _)| k == *n)).map(String::as_str).collect();
        if !missing.is_empty() {
            bail!("{name} has no {} in this share", missing.join(", "));
        }
        vars.retain(|(k, _)| only.contains(k));
    }
    Ok(vars)
}

/// The share token in `DOTSAFE_SHARE`.
pub fn token_from_env() -> Result<ShareToken> {
    let token = Zeroizing::new(std::env::var(TOKEN_VAR).map_err(|_| {
        anyhow!("set {TOKEN_VAR} to the share's token (`dotsafe share create` prints it on your Mac)")
    })?);
    ShareToken::parse(&token).with_context(|| format!("reading {TOKEN_VAR}"))
}

/// Fetches the share's snapshot and checks it: sealed with the token's key,
/// not older than one this machine has read, and within its expiry and
/// lease. `None` when there's no snapshot (revoked or expired).
pub fn fetch(token: &ShareToken) -> Result<Option<share::Snapshot>> {
    let Some(bytes) = Fetcher::new().get(&token.snapshot_url())? else { return Ok(None) };
    let snapshot = token.open(&bytes, seen(token.share_id), now_ms())?;
    // Ephemeral machines (CI) can't remember; they trust the first snapshot.
    if let Err(e) = remember(token.share_id, snapshot.stamp) {
        eprintln!("dotsafe: couldn't remember this snapshot, so a rolled-back one wouldn't be noticed: {e}");
    }
    Ok(Some(snapshot))
}

/// What a token reads in the bucket: a standing share's snapshot, or an
/// approve share's mailbox.
pub enum Fetched {
    Snapshot(share::Snapshot),
    Mailbox(Mailbox),
}

/// [`fetch`] for either mode. `None` when there's neither: the share was
/// revoked or has expired.
pub fn fetch_any(token: &ShareToken) -> Result<Option<Fetched>> {
    if let Some(snapshot) = fetch(token)? {
        return Ok(Some(Fetched::Snapshot(snapshot)));
    }
    let Some(bytes) = Fetcher::new().get(&token.mailbox_url())? else { return Ok(None) };
    let mailbox = token.open_mailbox(&bytes, seen(token.share_id), now_ms())?;
    if let Err(e) = remember(token.share_id, mailbox.stamp) {
        eprintln!("dotsafe: couldn't remember this mailbox, so a rolled-back one wouldn't be noticed: {e}");
    }
    Ok(Some(Fetched::Mailbox(mailbox)))
}

/// A request for approval, made on this machine.
#[allow(clippy::too_many_arguments)]
pub fn new_request(
    client: &str,
    kind: &str,
    command: &[String],
    program: Option<PathBuf>,
    cwd: &std::path::Path,
    environment: &str,
    variables: Vec<String>,
    proxied: Vec<String>,
    wait: Duration,
) -> Request {
    let now = now_ms();
    Request {
        id: Uuid::new_v4(),
        at: now,
        expires: now.saturating_add(wait.as_millis() as u64),
        host: approvals::host_name(),
        client: client.into(),
        kind: kind.into(),
        command: command.to_vec(),
        program_path: program.map(|p| p.display().to_string()).unwrap_or_else(|| "not found on PATH".into()),
        cwd: cwd.display().to_string(),
        environment: environment.into(),
        variables,
        proxied,
        files: Vec::new(),
    }
}

/// Approve mode, on the machine: writes `request` into a free slot of the
/// share's mailbox, rings the doorbell, and waits up to `wait` for one of
/// the user's devices to answer. Deletes the answer once read, so its
/// values don't stay in the bucket.
pub fn ask(token: &ShareToken, mailbox: &Mailbox, request: &Request, wait: Duration) -> Result<Answer> {
    let fetcher = Fetcher::new();
    let slot = (0..SLOTS).find(|&slot| matches!(fetcher.get(&token.request_url(slot)), Ok(None))).ok_or_else(|| {
        anyhow!("{SLOTS} requests are already waiting for approval on this share; try again once they're answered")
    })?;
    let urls = &mailbox.slots[usize::from(slot)];
    fetcher.put_signed(&urls.request, &token.seal_request(slot, request)?)?;
    fetcher.put_signed(&mailbox.doorbell, Uuid::new_v4().to_string().as_bytes())?;
    let deadline = Instant::now() + wait;
    loop {
        // An answer to an earlier request in this slot doesn't open; keep waiting.
        if let Ok(Some(bytes)) = fetcher.get(&token.answer_url(slot))
            && let Ok(answer) = token.open_answer(slot, request.id, &bytes)
        {
            if let Err(e) = fetcher.delete_signed(&urls.answer_delete) {
                eprintln!("dotsafe: couldn't delete the answer from the bucket ({e}); it expires with the share.");
            }
            return Ok(answer);
        }
        if Instant::now() >= deadline {
            bail!(
                "no answer within {}; approve it in the dotsafe app or with `dotsafe share approve` on one of your devices",
                wait_text(wait)
            );
        }
        std::thread::sleep(Duration::from_secs(2));
    }
}

fn wait_text(wait: Duration) -> String {
    match wait.as_secs() {
        s if s < 120 => format!("{s} seconds"),
        s => format!("{} minutes", s / 60),
    }
}

/// What a revoked share could read, for `dotsafe secret queue`.
pub fn queued(records: &[Record], queue: &[Uuid]) -> Vec<(String, dotsafe_core::integration::Exposed)> {
    share::shares(records)
        .into_iter()
        .filter(|s| queue.contains(&s.id))
        .filter_map(|s| Some((s.name.clone(), s.exposed(records, s.revoked?))))
        .collect()
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}
