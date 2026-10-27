//! `dotsafe secret ...`: shared secrets, linked into environments, with
//! history and rotation.

use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Result, bail};
use clap::Subcommand;
use dotsafe_core::{Due, Entry, Environment, Policy, ProviderLink, Record, Secret, Secrets, Target, VersionState};
use dotsafe_integrations::rotate::{self as provider, PushResult};
use dotsafe_integrations::{Client, Kind, check_settings, service};
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::{Ctx, EnvArg, connect};

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
    /// Replace a secret's value everywhere it's linked (through its provider, if it has one)
    Rotate {
        secret: String,
        /// Generate the new value instead of typing it
        #[arg(long)]
        generate: bool,
        /// Type the new value even though the secret has a provider
        #[arg(long, conflicts_with = "generate")]
        manual: bool,
        /// The old value leaked: retire it right away instead of keeping it as previous
        #[arg(long)]
        leaked: bool,
        /// Why it was rotated, kept in the history
        #[arg(long)]
        note: Option<String>,
    },
    /// Revoke the previous value (at its provider, if it has one) and mark it retired
    Retire { secret: String },
    /// Set the provider that issues this secret's values, so `rotate` gets new keys from it
    Provider {
        secret: String,
        /// The connection, from `dotsafe connections`
        #[arg(long, required_unless_present = "none")]
        connection: Option<String>,
        /// Provider settings for this secret, e.g. --set token_id=…
        #[arg(long = "set", value_name = "KEY=VALUE")]
        set: Vec<String>,
        /// The provider's id for the current key, so it can be revoked after the next rotation
        #[arg(long)]
        key_id: Option<String>,
        /// Remove the provider
        #[arg(long, conflicts_with_all = ["connection", "set", "key_id"])]
        none: bool,
    },
    /// Places this secret's current value is pushed to on every rotation
    #[command(subcommand)]
    Target(TargetCmd),
    /// Push the current value to every target now
    Push { secret: String },
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
    /// What to rotate after removing a device: every value it could have read
    Queue {
        /// Rotate every queued secret that has a provider
        #[arg(long, conflicts_with = "dismiss")]
        rotate: bool,
        /// Clear the queue
        #[arg(long)]
        dismiss: bool,
    },
}

#[derive(Subcommand)]
pub enum TargetCmd {
    /// Push this secret to a deploy target
    Add {
        secret: String,
        /// The connection, from `dotsafe connections`
        #[arg(long)]
        connection: String,
        /// Variable name at the target [default: the secret's name]
        #[arg(long = "as")]
        name: Option<String>,
        /// Target settings, e.g. --set project=web --set environments=production
        #[arg(long = "set", value_name = "KEY=VALUE")]
        set: Vec<String>,
    },
    /// List a secret's targets and when each was last pushed
    List { secret: String },
    /// Stop pushing to a target (numbers from `target list`)
    Remove { secret: String, number: usize },
}

pub fn run(cmd: SecretCmd) -> Result<()> {
    let ctx = Ctx::open()?;
    match cmd {
        SecretCmd::List => {
            let records = ctx.vault.list()?;
            let secrets = Secrets::from_records(&records);
            // Connection tokens are listed by `dotsafe connections`.
            let tokens = dotsafe_core::integration::credentials(&records);
            let own: Vec<&Secret> = sorted(&secrets).into_iter().filter(|s| !tokens.contains(&s.id)).collect();
            if own.is_empty() {
                eprintln!("No shared secrets yet. Make one with `dotsafe secret share KEY`.");
            }
            for secret in own {
                let uses = usages(&records, secret.id).len();
                println!("{}\t{}\t{}", secret.name, describe_age(secret), plural(uses, "environment"));
            }
        }
        SecretCmd::Due => {
            let records = ctx.vault.list()?;
            let secrets = Secrets::from_records(&records);
            let tokens = dotsafe_core::integration::credentials(&records);
            let mut any = false;
            for secret in sorted(&secrets).into_iter().filter(|s| !tokens.contains(&s.id)) {
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
        SecretCmd::Rotate { secret, generate, manual, leaked, note } => {
            let mut secret = find(&ctx.vault.secrets()?, &secret)?.clone();
            let binding = connect::binding(&ctx, secret.id)?;
            if binding.provider.is_some() && !generate && !manual {
                let out = provider::rotate(&ctx.vault, &Client::new(), ctx.device_id, secret.id, leaked)?;
                if let (false, Some(why)) = (out.promoted, &out.staged) {
                    eprintln!("Got a new key for {}, but it didn't pass the check: {why}", secret.name);
                    eprintln!("It's saved as staged; nothing uses it yet. Try `dotsafe secret rotate {}` again later.", secret.name);
                    return Ok(());
                }
                let uses = usages(&ctx.vault.list()?, secret.id).len();
                eprintln!("Rotated {} with a new key from its provider; {} use it.", secret.name, plural(uses, "environment"));
                if let Some(why) = &out.staged {
                    eprintln!("The provider's check failed ({why}), but the old key was already ended, so the new one is live.");
                }
                report_pushes(&out.pushes);
                if out.revoked_old {
                    eprintln!("The old key is revoked.");
                } else {
                    eprintln!("The old key still works. Once nothing uses it, run `dotsafe secret retire {}` to revoke it.", secret.name);
                }
                return Ok(());
            }
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
            let id = secret.id;
            let uses = usages(&ctx.vault.list()?, secret.id).len();
            ctx.vault.put(Record::Secret(secret))?;
            eprintln!("Rotated {name}; {} use the new value.", plural(uses, "environment"));
            if binding.targets().next().is_some() {
                report_pushes(&provider::push(&ctx.vault, &Client::new(), ctx.device_id, id)?);
            }
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
            if connect::binding(&ctx, secret.id)?.provider.is_some() {
                let retired = provider::retire(&ctx.vault, &Client::new(), ctx.device_id, secret.id)?;
                match retired {
                    0 => eprintln!("{} has no previous value to retire.", secret.name),
                    _ => eprintln!("Revoked the previous key of {} at its provider and retired it.", secret.name),
                }
                return Ok(());
            }
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
        SecretCmd::Provider { secret, connection, set, key_id, none } => {
            let secret = find(&ctx.vault.secrets()?, &secret)?.clone();
            let mut binding = connect::binding(&ctx, secret.id)?;
            if none {
                binding.provider = None;
                binding.updated = ctx.stamp();
                ctx.vault.put(Record::Binding(binding))?;
                eprintln!("{} has no provider now; rotations take a typed or generated value.", secret.name);
                return Ok(());
            }
            let connection = connect::find(&ctx, connection.as_deref().unwrap_or_default())?;
            let svc = service(&connection.service)?;
            if svc.kind != Kind::Provider {
                bail!("{} is a deploy target, not a provider; use `dotsafe secret target add`", connection.name);
            }
            let settings = connect::settings(&set)?;
            check_settings(svc.binding_fields, &settings)?;
            binding.provider = Some(ProviderLink { connection: connection.id, settings });
            binding.updated = ctx.stamp();
            if let (Some(key_id), Some((current, _))) = (key_id, secret.current()) {
                binding.keys.insert(current, key_id);
            }
            ctx.vault.put(Record::Binding(binding))?;
            eprintln!("{} now gets new keys from {}. Rotate it with `dotsafe secret rotate {}`.", secret.name, connection.name, secret.name);
        }
        SecretCmd::Target(TargetCmd::Add { secret, connection, name, set }) => {
            let secret = find(&ctx.vault.secrets()?, &secret)?.clone();
            let connection = connect::find(&ctx, &connection)?;
            let svc = service(&connection.service)?;
            if svc.kind != Kind::Target {
                bail!("{} is a provider, not a deploy target; use `dotsafe secret provider`", connection.name);
            }
            let settings = connect::settings(&set)?;
            check_settings(svc.binding_fields, &settings)?;
            let name = name.unwrap_or_else(|| secret.name.clone());
            if !dotsafe_core::is_valid_var_name(&name) {
                bail!("invalid variable name {name:?}; pass --as");
            }
            let mut binding = connect::binding(&ctx, secret.id)?;
            let target = Target { connection: connection.id, name: name.clone(), settings, pushed: None, deleted: false, changed: ctx.stamp() };
            binding.targets.insert(Uuid::new_v4(), target);
            ctx.vault.put(Record::Binding(binding))?;
            eprintln!("{} will be pushed to {} as {name} on every rotation. Push it now with `dotsafe secret push {}`.", secret.name, connection.name, secret.name);
        }
        SecretCmd::Target(TargetCmd::List { secret }) => {
            let secret = find(&ctx.vault.secrets()?, &secret)?.clone();
            let binding = connect::binding(&ctx, secret.id)?;
            let names = connection_names(&ctx)?;
            let targets: Vec<_> = binding.targets().collect();
            if targets.is_empty() {
                eprintln!("{} isn't pushed anywhere. Add a target with `dotsafe secret target add`.", secret.name);
            }
            let current = secret.current_value().map(dotsafe_core::crypto::fingerprint);
            for (n, (_, t)) in targets.iter().enumerate() {
                let settings: Vec<String> = t.settings.iter().map(|(k, v)| format!("{k}={v}")).collect();
                let pushed = match &t.pushed {
                    None => "never pushed".to_string(),
                    Some(p) if Some(&p.sha256) == current.as_ref() => format!("pushed {}", days_ago(p.at)),
                    Some(p) => format!("out of date (pushed {})", days_ago(p.at)),
                };
                let connection = names.get(&t.connection).map(String::as_str).unwrap_or("removed connection");
                println!("{}	{connection}	{}	{}	{pushed}", n + 1, t.name, settings.join(" "));
            }
        }
        SecretCmd::Target(TargetCmd::Remove { secret, number }) => {
            let secret = find(&ctx.vault.secrets()?, &secret)?.clone();
            let mut binding = connect::binding(&ctx, secret.id)?;
            let ids: Vec<Uuid> = binding.targets().map(|(id, _)| id).collect();
            let Some(id) = number.checked_sub(1).and_then(|i| ids.get(i)) else {
                bail!("{} has no target {number}; see `dotsafe secret target list {}`", secret.name, secret.name);
            };
            let stamp = ctx.stamp();
            let target = binding.targets.get_mut(id).expect("listed above");
            target.deleted = true;
            target.changed = stamp;
            let name = target.name.clone();
            ctx.vault.put(Record::Binding(binding))?;
            eprintln!("Stopped pushing {} as {name}. The value stays at the target until you remove it there.", secret.name);
        }
        SecretCmd::Push { secret } => {
            let secret = find(&ctx.vault.secrets()?, &secret)?.clone();
            let results = provider::push(&ctx.vault, &Client::new(), ctx.device_id, secret.id)?;
            if results.is_empty() {
                bail!("{} has no targets; add one with `dotsafe secret target add`", secret.name);
            }
            let failed = report_pushes(&results);
            if failed > 0 {
                bail!("{} failed", plural(failed, "push"));
            }
        }
        SecretCmd::Queue { rotate, dismiss } => {
            let mut config = crate::load_config()?;
            let Some(queue) = config.rotation_queue.clone() else {
                eprintln!("Nothing to rotate. The queue fills when you remove a device with `dotsafe device remove`.");
                return Ok(());
            };
            if dismiss {
                config.rotation_queue = None;
                config.save()?;
                eprintln!("Cleared the rotation queue.");
                return Ok(());
            }
            let records = ctx.vault.list()?;
            let exposed = dotsafe_core::integration::exposed_since(&records, queue.since);
            let secrets = Secrets::from_records(&records);
            let with_provider = |id: &Uuid| {
                records.iter().any(|r| matches!(r, Record::Binding(b) if b.secret == *id && b.provider.is_some()))
            };
            if rotate {
                let queued: Vec<&Secret> =
                    exposed.secrets.iter().filter(|id| with_provider(id)).filter_map(|id| secrets.get(*id)).collect();
                if queued.is_empty() {
                    eprintln!("No queued secret has a provider; rotate them with `dotsafe secret rotate`.");
                }
                for secret in queued {
                    match provider::rotate(&ctx.vault, &Client::new(), ctx.device_id, secret.id, false) {
                        Ok(out) if out.promoted => {
                            eprintln!("Rotated {}.", secret.name);
                            report_pushes(&out.pushes);
                        }
                        Ok(out) => eprintln!("{}: the new key failed its check and is staged ({}).", secret.name, out.staged.unwrap_or_default()),
                        Err(e) => eprintln!("Couldn't rotate {}: {e}", secret.name),
                    }
                }
                return Ok(());
            }
            let total = exposed.secrets.len() + exposed.variables.len();
            if total == 0 {
                eprintln!("Everything {} could read has been rotated. Clear the queue with `dotsafe secret queue --dismiss`.", queue.device);
                return Ok(());
            }
            eprintln!("{} to rotate since removing {}:", plural(total, "value"), queue.device);
            for id in &exposed.secrets {
                if let Some(secret) = secrets.get(*id) {
                    let how = if with_provider(id) { "has a provider" } else { "rotate by hand" };
                    println!("secret\t{}\t{how}", secret.name);
                }
            }
            let labels = env_labels(&records);
            for (env, key) in &exposed.variables {
                println!("variable\t{}/{key}", labels.get(env).map(String::as_str).unwrap_or("?"));
            }
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

/// "project/environment" for every live environment.
fn env_labels(records: &[Record]) -> std::collections::HashMap<Uuid, String> {
    let projects: std::collections::HashMap<Uuid, &str> = records
        .iter()
        .filter_map(|r| match r {
            Record::Project(p) => Some((p.id, p.name.as_str())),
            _ => None,
        })
        .collect();
    records
        .iter()
        .filter_map(|r| match r {
            Record::Environment(e) => Some((e.id, format!("{}/{}", projects.get(&e.project_id).unwrap_or(&"?"), e.name))),
            _ => None,
        })
        .collect()
}

/// Prints each push's outcome. Returns how many failed.
fn report_pushes(results: &[PushResult]) -> usize {
    let mut failed = 0;
    for r in results {
        match &r.error {
            None => eprintln!("Pushed {} to {}.", r.name, r.connection),
            Some(e) => {
                failed += 1;
                eprintln!("Couldn't push {}: {e}", r.name);
            }
        }
    }
    failed
}

fn connection_names(ctx: &Ctx) -> Result<std::collections::HashMap<Uuid, String>> {
    Ok(connect::connections(ctx)?.into_iter().map(|c| (c.id, c.name)).collect())
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
