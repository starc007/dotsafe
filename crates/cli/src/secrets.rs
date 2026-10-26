//! `dotsafe secret ...`: shared secrets, linked into environments, with
//! history and rotation.

use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Result, bail};
use clap::Subcommand;
use dotsafe_core::{Due, Entry, Environment, Policy, Record, Secret, Secrets, VersionState};
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::{Ctx, EnvArg};

const DAY_MS: u64 = 24 * 60 * 60 * 1000;

#[derive(Subcommand)]
pub enum SecretCmd {
    /// List shared secrets with their age and where they're used
    List,
    /// Turn a variable into a shared secret that other environments can link to
    Share {
        key: String,
        /// Name for the secret [default: the variable's name]
        #[arg(long)]
        name: Option<String>,
        /// Also link every other environment where this variable has the same value
        #[arg(long)]
        all: bool,
        #[command(flatten)]
        env: EnvArg,
    },
    /// Link a variable to an existing shared secret
    Link {
        key: String,
        secret: String,
        #[command(flatten)]
        env: EnvArg,
    },
    /// Give a linked variable its own copy of the value again
    Unlink {
        key: String,
        #[command(flatten)]
        env: EnvArg,
    },
    /// Replace a secret's value everywhere it's linked
    Rotate {
        secret: String,
        /// Generate the new value instead of typing it
        #[arg(long)]
        generate: bool,
        /// The old value leaked: retire it right away instead of keeping it as previous
        #[arg(long)]
        leaked: bool,
        /// Why it was rotated, kept in the history
        #[arg(long)]
        note: Option<String>,
    },
    /// Mark the previous value as revoked, once it's no longer in use anywhere
    Retire { secret: String },
    /// Go back to the previous value
    Rollback { secret: String },
    /// Show a secret's versions (never the values)
    History { secret: String },
    /// Make an earlier version current again (numbers from `history`)
    Restore { secret: String, version: usize },
    /// Set how often a secret should be rotated
    Policy {
        secret: String,
        /// Days between rotations; 0 removes the policy
        #[arg(long)]
        every: u32,
    },
    /// List secrets that are due for rotation
    Due,
}

pub fn run(cmd: SecretCmd) -> Result<()> {
    let ctx = Ctx::open()?;
    match cmd {
        SecretCmd::List => {
            let records = ctx.vault.list()?;
            let secrets = Secrets::from_records(&records);
            if secrets.iter().next().is_none() {
                eprintln!("No shared secrets yet. Make one with `dotsafe secret share KEY`.");
            }
            for secret in sorted(&secrets) {
                let uses = usages(&records, secret.id).len();
                println!("{}\t{}\t{}", secret.name, describe_age(secret), plural(uses, "environment"));
            }
        }
        SecretCmd::Due => {
            let secrets = ctx.vault.secrets()?;
            let mut any = false;
            for secret in sorted(&secrets) {
                let due = secret.due(now_ms());
                if matches!(due, Due::Soon | Due::Overdue) {
                    any = true;
                    let label = if due == Due::Overdue { "overdue" } else { "due soon" };
                    println!("{}\t{label}\t{}", secret.name, describe_age(secret));
                }
            }
            if !any {
                eprintln!("Nothing is due for rotation.");
            }
        }
        SecretCmd::Share { key, name, all, env } => {
            let mut target = ctx.target(env.env, false)?;
            let value = match target.env.entry(&key) {
                Some(Entry::Value(value)) => value.to_owned(),
                Some(Entry::Secret(_)) => bail!("{key} is already linked to a shared secret"),
                None => bail!("{key} is not set in {}", target.label()),
            };
            let secret = Secret::new(name.unwrap_or_else(|| key.clone()), value.clone(), ctx.stamp());
            ctx.vault.put(Record::Secret(secret.clone()))?;
            target.env.link(&key, secret.id, ctx.stamp());
            let current_env = target.env.id;
            ctx.vault.put(Record::Environment(target.env))?;
            eprintln!("{key} in {} is now the shared secret {}.", target.project.name, secret.name);

            let copies: Vec<Environment> = ctx
                .environments()?
                .into_iter()
                .filter(|e| e.id != current_env && e.value(&key) == Some(value.as_str()))
                .collect();
            if all {
                for mut copy in copies.iter().cloned() {
                    copy.link(&key, secret.id, ctx.stamp());
                    ctx.vault.put(Record::Environment(copy))?;
                }
                if !copies.is_empty() {
                    eprintln!("Linked {} with the same value.", plural(copies.len(), "other environment"));
                }
            } else if !copies.is_empty() {
                eprintln!(
                    "{} the same value. Link them with `dotsafe secret link {key} {}` in each, or share again with --all.",
                    plural(copies.len(), "other environment has"),
                    secret.name
                );
            }
        }
        SecretCmd::Link { key, secret, env } => {
            let secret = find(&ctx.vault.secrets()?, &secret)?.clone();
            let mut target = ctx.target(env.env, true)?;
            target.env.link(&key, secret.id, ctx.stamp());
            eprintln!("Linked {key} in {} to {}.", target.label(), secret.name);
            ctx.vault.put(Record::Environment(target.env))?;
        }
        SecretCmd::Unlink { key, env } => {
            let mut target = ctx.target(env.env, false)?;
            let Some(Entry::Secret(id)) = target.env.entry(&key) else {
                bail!("{key} isn't linked to a shared secret in {}", target.label());
            };
            let secrets = ctx.vault.secrets()?;
            let value = secrets.value(Entry::Secret(id)).unwrap_or_default().to_owned();
            target.env.set(&key, value, ctx.stamp());
            eprintln!("{key} in {} has its own value now; rotating the secret won't change it.", target.label());
            ctx.vault.put(Record::Environment(target.env))?;
        }
        SecretCmd::Rotate { secret, generate, leaked, note } => {
            let mut secret = find(&ctx.vault.secrets()?, &secret)?.clone();
            let value: Zeroizing<String> = if generate {
                secret.policy.generator.clone().unwrap_or_default().generate()?
            } else {
                // Not session::password: that one reads DOTSAFE_PASSWORD, which is the
                // master password, never a value to store.
                let value = Zeroizing::new(rpassword::prompt_password(format!("New value for {}: ", secret.name))?);
                if value.is_empty() {
                    bail!("the new value can't be empty");
                }
                value
            };
            let note = note.or_else(|| leaked.then(|| "Leaked".to_string()));
            secret.rotate(value.as_str(), note, ctx.stamp())?;
            if leaked {
                secret.retire_previous(ctx.stamp());
            }
            let name = secret.name.clone();
            let uses = usages(&ctx.vault.list()?, secret.id).len();
            ctx.vault.put(Record::Secret(secret))?;
            eprintln!("Rotated {name}; {} use the new value.", plural(uses, "environment"));
            if leaked {
                eprintln!("Revoke the leaked value with its provider now, if you haven't.");
            } else {
                eprintln!(
                    "The old value is kept as previous. Once nothing uses it, revoke it and run `dotsafe secret retire {name}`."
                );
            }
        }
        SecretCmd::Retire { secret } => {
            let mut secret = find(&ctx.vault.secrets()?, &secret)?.clone();
            let retired = secret.retire_previous(ctx.stamp());
            if retired == 0 {
                eprintln!("{} has no previous value to retire.", secret.name);
                return Ok(());
            }
            eprintln!("Retired the previous value of {}.", secret.name);
            ctx.vault.put(Record::Secret(secret))?;
        }
        SecretCmd::Rollback { secret } => {
            let mut secret = find(&ctx.vault.secrets()?, &secret)?.clone();
            secret.roll_back(ctx.stamp())?;
            eprintln!("{} is back to its previous value.", secret.name);
            ctx.vault.put(Record::Secret(secret))?;
        }
        SecretCmd::History { secret } => {
            let secret = find(&ctx.vault.secrets()?, &secret)?.clone();
            for (n, (_, version)) in secret.versions().into_iter().enumerate() {
                let wiped = if version.value.is_none() { " (value wiped)" } else { "" };
                let note = version.note.as_deref().map(|n| format!("\t{n}")).unwrap_or_default();
                println!("{}\t{}\t{}{wiped}{note}", n + 1, state_label(version.state), days_ago(version.created.at_ms));
            }
        }
        SecretCmd::Restore { secret, version } => {
            let mut secret = find(&ctx.vault.secrets()?, &secret)?.clone();
            let versions = secret.versions();
            let Some((id, _)) = version.checked_sub(1).and_then(|i| versions.get(i)) else {
                bail!("{} has no version {version}; see `dotsafe secret history {}`", secret.name, secret.name);
            };
            let id: Uuid = *id;
            secret.restore(id, ctx.stamp())?;
            eprintln!("Restored version {version} of {}.", secret.name);
            ctx.vault.put(Record::Secret(secret))?;
        }
        SecretCmd::Policy { secret, every } => {
            let mut secret = find(&ctx.vault.secrets()?, &secret)?.clone();
            let policy = Policy { rotate_every_days: (every > 0).then_some(every), ..secret.policy.clone() };
            secret.set_policy(policy, ctx.stamp());
            match every {
                0 => eprintln!("{} has no rotation policy now.", secret.name),
                days => eprintln!("{} should be rotated every {}.", secret.name, plural(days as usize, "day")),
            }
            ctx.vault.put(Record::Secret(secret))?;
        }
    }
    Ok(())
}

/// Finds a secret by name. Names aren't unique, so say so if it's ambiguous.
fn find<'a>(secrets: &'a Secrets, name: &str) -> Result<&'a Secret> {
    let matches: Vec<&Secret> = secrets.iter().filter(|s| s.name == name).collect();
    match matches.as_slice() {
        [one] => Ok(one),
        [] => bail!("no shared secret named {name:?}; see `dotsafe secret list`"),
        _ => bail!("more than one shared secret is named {name:?}"),
    }
}

/// The environments that link to `secret`.
pub fn usages(records: &[Record], secret: Uuid) -> Vec<&Environment> {
    records
        .iter()
        .filter_map(|r| match r {
            Record::Environment(e) if !e.deleted && e.entries().any(|(_, entry)| entry == Entry::Secret(secret)) => {
                Some(e)
            }
            _ => None,
        })
        .collect()
}

fn sorted(secrets: &Secrets) -> Vec<&Secret> {
    let mut list: Vec<&Secret> = secrets.iter().collect();
    list.sort_by(|a, b| a.name.cmp(&b.name));
    list
}

fn describe_age(secret: &Secret) -> String {
    let age = secret.age_ms(now_ms()).map(|ms| format!("rotated {}", days_ago_ms(ms))).unwrap_or_default();
    match secret.due(now_ms()) {
        Due::Overdue => format!("{age}, overdue"),
        Due::Soon => format!("{age}, due soon"),
        _ => age,
    }
}

fn state_label(state: VersionState) -> &'static str {
    match state {
        VersionState::Pending => "staged",
        VersionState::Current => "current",
        VersionState::Previous => "previous",
        VersionState::Retired => "retired",
    }
}

fn days_ago(at_ms: u64) -> String {
    days_ago_ms(now_ms().saturating_sub(at_ms))
}

fn days_ago_ms(ms: u64) -> String {
    match ms / DAY_MS {
        0 => "today".into(),
        1 => "yesterday".into(),
        days => format!("{days} days ago"),
    }
}

fn plural(n: usize, noun: &str) -> String {
    match (n, noun.split_once(' ')) {
        (1, _) => format!("1 {noun}"),
        // "other environment has" → "2 other environments have"
        (n, Some((first, rest))) if rest.ends_with(" has") => {
            format!("{n} {first} {}s have", rest.trim_end_matches(" has"))
        }
        (n, _) => format!("{n} {noun}s"),
    }
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}
