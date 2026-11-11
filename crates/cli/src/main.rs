#[cfg(unix)]
mod approve;
mod compare;
mod connect;
#[cfg(unix)]
mod daemon;
mod export;
mod license;
mod link;
mod mcp;
mod scan;
mod secrets;
mod session;

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use clap::{Args, Parser, Subcommand};
use dotsafe_core::{
    CachedStore, Config, Environment, Error, FolderStore, KdfParams, Project, Record, S3Location, Stamp, Store, Vault,
    Entry, config, dotenv, is_valid_var_name,
};
use dotsafe_core::resolve::Change;
use dotsafe_stores::{S3Credentials, S3Store};
use zeroize::Zeroizing;
use uuid::Uuid;

use link::{DEFAULT_ENV, LINK_FILE, Link};

#[derive(Parser)]
#[command(name = "dotsafe", version, about = "Encrypted secrets for your projects, synced through storage you own")]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Create a vault, or connect this machine to an existing one
    Init {
        /// Vault folder [default: iCloud Drive/dotsafe if available]
        #[arg(long, conflicts_with_all = ["bucket", "webdav"])]
        path: Option<PathBuf>,
        #[command(flatten)]
        s3: S3Args,
        /// A WebDAV folder (Nextcloud, ownCloud, a NAS) instead, e.g. https://cloud.example.com/remote.php/dav/files/me/dotsafe/
        #[arg(long, requires = "user", conflicts_with = "bucket")]
        webdav: Option<String>,
        /// The WebDAV username. The password is asked for, or read from DOTSAFE_WEBDAV_PASSWORD
        #[arg(long, requires = "webdav")]
        user: Option<String>,
    },
    /// Exchange changes with the bucket now (vaults in S3 or R2)
    Sync,
    /// Save a dated, still-encrypted copy of the vault (the last 10 are kept)
    Backup {
        /// Where backups go; remembered for next time
        #[arg(long)]
        to: Option<PathBuf>,
    },
    /// Move the vault to another folder, bucket or WebDAV folder. The old copy stays until you delete it
    Move {
        /// The new folder (must not hold a vault yet)
        #[arg(long, conflicts_with_all = ["bucket", "webdav"])]
        path: Option<PathBuf>,
        #[command(flatten)]
        s3: S3Args,
        /// A WebDAV folder instead
        #[arg(long, requires = "user", conflicts_with = "bucket")]
        webdav: Option<String>,
        /// The WebDAV username
        #[arg(long, requires = "webdav")]
        user: Option<String>,
    },
    /// Remember the vault key in the OS keychain for a while
    Unlock {
        /// How long to stay unlocked, e.g. 30m, 8h, 7d
        #[arg(long = "for", default_value = "8h", value_parser = session::parse_duration)]
        ttl: Duration,
    },
    /// Forget the vault key cached by `unlock`
    Lock,
    /// Link this directory to a project, creating it if needed
    Link {
        /// Project name [default: this directory's name]
        name: Option<String>,
        /// Default environment for this directory
        #[arg(short, long, default_value = DEFAULT_ENV)]
        env: String,
    },
    /// Set variables: KEY=VALUE, or just KEY to type the value privately
    Set {
        #[arg(required = true, value_name = "KEY[=VALUE]")]
        vars: Vec<String>,
        #[command(flatten)]
        env: EnvArg,
    },
    /// Print a variable's value
    Get {
        key: String,
        #[command(flatten)]
        env: EnvArg,
    },
    /// Delete variables. An environment's own value is removed and any inherited one applies again
    Unset {
        #[arg(required = true)]
        keys: Vec<String>,
        /// Remove an inherited variable here entirely, so the parent's value doesn't apply either
        #[arg(long)]
        hide: bool,
        #[command(flatten)]
        env: EnvArg,
    },
    /// Make an environment inherit another's variables (same project); its own override them
    Inherit {
        /// The environment to inherit from
        #[arg(required_unless_present = "none")]
        parent: Option<String>,
        /// Stop inheriting
        #[arg(long, conflicts_with = "parent")]
        none: bool,
        #[command(flatten)]
        env: EnvArg,
    },
    /// List variable names
    List {
        #[command(flatten)]
        env: EnvArg,
    },
    /// Import variables from a .env or JSON file, or straight from another secret manager
    Import {
        /// A .env file, or JSON exported by Doppler, Infisical or 1Password
        #[arg(default_value = ".env", conflicts_with = "from")]
        file: PathBuf,
        /// Read from another manager's CLI instead (it must be installed and signed in)
        #[arg(long, value_enum)]
        from: Option<ImportFrom>,
        /// The 1Password item to read, with --from 1password
        #[arg(long, required_if_eq("from", "1password"))]
        item: Option<String>,
        #[command(flatten)]
        env: EnvArg,
    },
    /// Print the variables as .env lines, or write them to a file for tools that need one
    Export {
        #[command(flatten)]
        env: EnvArg,
        /// Write to this file instead, e.g. .env (owner-only, and it must be gitignored)
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Write the file even if git would commit it, or dotsafe didn't write it
        #[arg(long, requires = "output")]
        force: bool,
    },
    /// Compare two environments by name: what's only in one, and which values differ (values aren't shown)
    Diff {
        /// Compare the linked environment (or -e) with this one, or with the second name if given
        #[arg(value_name = "ENV")]
        first: String,
        /// The environment to compare `first` with
        #[arg(value_name = "OTHER")]
        second: Option<String>,
        /// Only compare names, not values
        #[arg(long)]
        names: bool,
        #[command(flatten)]
        env: EnvArg,
    },
    /// Check that every variable in .env.example is set in the environment
    Check {
        /// The example file
        #[arg(default_value = ".env.example")]
        file: PathBuf,
        /// Also fail when the environment has variables the file doesn't list
        #[arg(long)]
        strict: bool,
        #[command(flatten)]
        env: EnvArg,
    },
    /// Print the variable names as a .env.example, or write them to a file to commit
    Example {
        #[command(flatten)]
        env: EnvArg,
        /// Write to this file instead, e.g. .env.example (values it already has are kept)
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Replace a file dotsafe didn't write (its comments are lost)
        #[arg(long, requires = "output")]
        force: bool,
    },
    /// Look for vault values in files before they're committed
    Scan {
        /// Files or folders to scan [default: this directory]
        paths: Vec<PathBuf>,
        /// Scan what's staged for commit (the pre-commit hook does this)
        #[arg(long, conflicts_with = "paths")]
        staged: bool,
        /// Add a git pre-commit hook that runs `dotsafe scan --staged`
        #[arg(long, conflicts_with_all = ["paths", "staged"])]
        install_hook: bool,
    },
    /// Connect an account at a provider (openai, cloudflare_token, aws_iam, anthropic, stripe) or deploy target (vercel, cloudflare_workers, netlify, fly, aws_ssm)
    Connect {
        service: String,
        /// Name for the connection [default: the service's name]
        #[arg(long)]
        name: Option<String>,
        /// Account settings, e.g. --set project_id=proj_…
        #[arg(long = "set", value_name = "KEY=VALUE")]
        set: Vec<String>,
        /// Read the admin token from stdin instead of asking
        #[arg(long)]
        token_stdin: bool,
    },
    /// Proxy profiles: more API keys that `run --proxy` and agents keep out of processes
    #[command(subcommand)]
    Proxy(ProxyCmd),
    /// List connected accounts
    Connections,
    /// Remove a connection and its stored token
    Disconnect { name: String },
    /// Give a connection a new admin token
    Reconnect {
        name: String,
        /// Read the token from stdin instead of asking
        #[arg(long)]
        token_stdin: bool,
    },
    /// Secrets: link one value into many environments and rotate it in one place
    #[command(subcommand, visible_alias = "secrets")]
    Secret(secrets::SecretCmd),
    /// Keep the vault unlocked in memory for agents and `dotsafe run` (cron jobs, scripts), without the desktop app
    Daemon {
        /// Stop after this long, e.g. 8h (default: until stopped)
        #[arg(long = "for", value_parser = session::parse_duration)]
        stop_after: Option<Duration>,
        /// Unlock, then keep running in the background after this terminal closes
        #[arg(long)]
        detach: bool,
        /// Internal: the vault key from a detaching `dotsafe daemon`
        #[arg(long, hide = true, conflicts_with = "detach")]
        key_from_stdin: bool,
    },
    /// Answer commands waiting for approval in `dotsafe daemon` (asks for the master password)
    Approve {
        /// The request to answer (the start of its id); default: each one in turn
        id: Option<String>,
        /// Allow it: once, 1h, or stop (until the daemon stops)
        #[arg(long = "for", value_parser = ["once", "1h", "stop"], conflicts_with = "deny")]
        lifetime: Option<String>,
        /// Deny it
        #[arg(long)]
        deny: bool,
        /// Only list what's waiting
        #[arg(long, conflicts_with_all = ["lifetime", "deny"])]
        list: bool,
    },
    /// Serve the MCP tools AI agents use to run commands with secrets they never see
    Mcp {
        #[command(subcommand)]
        action: Option<McpAction>,
    },
    /// Run a command with the variables set in its environment
    Run {
        #[command(flatten)]
        env: EnvArg,
        /// Keep API keys out of the process (OpenAI, Anthropic, and `dotsafe proxy` profiles): it gets a local URL that adds the key
        #[arg(long)]
        proxy: bool,
        /// Only these variables, comma-separated (default: all of the environment's)
        #[arg(long, value_delimiter = ',', value_name = "NAMES")]
        only: Option<Vec<String>>,
        /// Run through `dotsafe daemon` even if the vault could be opened here
        #[arg(long)]
        daemon: bool,
        /// Through the daemon: how long to wait for approval, e.g. 30s or 10m
        #[arg(long, value_parser = session::parse_duration, default_value = "5m")]
        wait: Duration,
        #[arg(required = true, trailing_var_arg = true, allow_hyphen_values = true, value_name = "COMMAND")]
        command: Vec<String>,
    },
    /// Devices that can open this vault with their own key (Touch ID or a code)
    #[command(subcommand)]
    Device(DeviceCmd),
    /// Show what AI agents asked dotsafe for, from every device
    Audit {
        /// How many events to show
        #[arg(long, default_value_t = 30)]
        limit: usize,
    },
    /// Activate, show or remove the license on this machine
    #[command(subcommand)]
    License(license::LicenseCmd),
}

/// Keep the vault in an S3-compatible bucket (AWS S3, Cloudflare R2, Backblaze
/// B2, MinIO). The access key is read from DOTSAFE_S3_ACCESS_KEY_ID and
/// DOTSAFE_S3_SECRET_ACCESS_KEY, or asked for, and kept in the OS keychain.
#[derive(Args)]
struct S3Args {
    /// Bucket name; keeps the vault in S3 instead of a folder
    #[arg(long, requires = "endpoint")]
    bucket: Option<String>,
    /// S3 endpoint, e.g. https://<account>.r2.cloudflarestorage.com
    #[arg(long, requires = "bucket")]
    endpoint: Option<String>,
    /// Folder inside the bucket
    #[arg(long, default_value = "dotsafe")]
    prefix: String,
    /// Region (`auto` for R2)
    #[arg(long, default_value = "auto")]
    region: String,
}

#[derive(Subcommand)]
enum DeviceCmd {
    /// List enrolled devices
    List,
    /// Remove a device and switch the vault to a new key. Prints a new recovery key
    Remove {
        /// The device's name, as `device list` shows it
        name: String,
    },
}

#[derive(Subcommand)]
enum McpAction {
    /// Show how to add dotsafe to Claude Code, Cursor and other agents
    Setup,
}

#[derive(Subcommand)]
enum ProxyCmd {
    /// List built-in and custom proxy profiles
    List,
    /// Add a profile: which variable holds the key, and the one place it may go
    Add {
        /// Shown in lists, e.g. Mistral
        name: String,
        /// The variable holding the key, e.g. MISTRAL_API_KEY
        #[arg(long)]
        key_var: String,
        /// The variable the SDK reads its base URL from, e.g. MISTRAL_BASE_URL
        #[arg(long)]
        base_url_var: String,
        /// Where requests go, https:// only, e.g. https://api.mistral.ai
        #[arg(long)]
        upstream: String,
        /// Appended to the proxy URL given to the SDK, e.g. /v1
        #[arg(long, default_value = "")]
        base_path: String,
        /// The header the key goes in
        #[arg(long, default_value = "authorization")]
        header: String,
        /// Send the key as is, not as `Bearer <key>`
        #[arg(long)]
        raw: bool,
    },
    /// Remove a custom profile
    Remove { name: String },
}

/// Other secret managers `dotsafe import --from` can read through their CLIs.
#[derive(Clone, Copy, clap::ValueEnum)]
enum ImportFrom {
    /// `doppler secrets download`, for the project and config set up in this directory
    Doppler,
    /// `infisical export`, for the project linked in this directory
    Infisical,
    /// `op item get`, one item's fields by label
    #[value(name = "1password")]
    OnePassword,
}

impl ImportFrom {
    fn label(self) -> &'static str {
        match self {
            ImportFrom::Doppler => "Doppler",
            ImportFrom::Infisical => "Infisical",
            ImportFrom::OnePassword => "1Password",
        }
    }

    /// Runs the manager's CLI and returns its JSON output.
    fn read(self, item: Option<&str>) -> Result<Zeroizing<String>> {
        let (program, args): (&str, Vec<&str>) = match self {
            ImportFrom::Doppler => ("doppler", vec!["secrets", "download", "--no-file", "--format", "json"]),
            ImportFrom::Infisical => ("infisical", vec!["export", "--format", "json"]),
            ImportFrom::OnePassword => ("op", vec!["item", "get", item.unwrap_or_default(), "--format", "json"]),
        };
        let out = Command::new(program).args(&args).output().map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => anyhow::anyhow!("{program} isn't installed; install {}'s CLI and sign in first", self.label()),
            _ => anyhow::anyhow!("couldn't run {program}: {e}"),
        })?;
        if !out.status.success() {
            let why = String::from_utf8_lossy(&out.stderr);
            bail!("{program} failed: {}", why.trim());
        }
        Ok(Zeroizing::new(String::from_utf8(out.stdout).context("the output isn't text")?))
    }
}

#[derive(Args)]
struct EnvArg {
    /// Environment to use [default: the linked one, usually development]
    #[arg(short, long)]
    env: Option<String>,
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<ExitCode> {
    // dotsafe is paid: everything but `license` needs one on this machine.
    // `mcp` only relays to the app or `dotsafe daemon`, which need one already.
    let command = match cli.command {
        Cmd::License(cmd) => {
            license::run(cmd)?;
            return Ok(ExitCode::SUCCESS);
        }
        command @ Cmd::Mcp { .. } => command,
        command => {
            license::require()?;
            command
        }
    };
    match command {
        Cmd::License(_) => unreachable!("handled above"),
        Cmd::Init { path, s3, webdav, user } => match (s3.bucket, s3.endpoint, webdav, user) {
            (Some(bucket), Some(endpoint), _, _) => {
                let prefix = s3.prefix.trim_matches('/').to_owned();
                init_s3(S3Location { endpoint, region: s3.region, bucket, prefix })?
            }
            (_, _, Some(url), Some(username)) => init_webdav(dotsafe_core::WebDavLocation { url, username })?,
            _ => init(path)?,
        },
        Cmd::Sync => {
            let config = load_config()?;
            if !config.is_remote() {
                eprintln!("Nothing to sync: this vault is a folder, and your sync client keeps it up to date.");
                return Ok(ExitCode::SUCCESS);
            }
            let vault = session::open(&config)?;
            match vault.sync() {
                Ok(_) => eprintln!("Synced with {}.", vault.location()),
                Err(Error::Unreachable(location)) => bail!("couldn't reach {location}; changes stay on this machine for now"),
                Err(e) => return Err(e.into()),
            }
        }
        Cmd::Backup { to } => {
            let mut config = load_config()?;
            let dir = match to.or_else(|| config.backup_dir.clone()) {
                Some(dir) => dir,
                None => bail!("say where backups go once with `dotsafe backup --to <folder>`"),
            };
            let vault = session::open(&config)?;
            if config.is_remote() && let Err(e) = vault.sync() {
                eprintln!("Couldn't sync first ({e}); backing up what this machine has.");
            }
            let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0);
            let path = dotsafe_core::backup::snapshot(vault.store(), &dir, &dotsafe_core::backup::stamp(now))?;
            config.backup_dir = Some(dir);
            config.last_backup = Some(now);
            config.save()?;
            eprintln!("Backed up to {}. It's encrypted like the vault; restore by connecting dotsafe to that folder.", path.display());
        }
        Cmd::Move { path, s3, webdav, user } => move_vault(path, s3, webdav, user)?,
        Cmd::Unlock { ttl } => {
            let config = load_config()?;
            let password = session::password("Master password: ")?;
            let store = dotsafe_stores::open(&config).map_err(session::friendly)?;
            let vault = Vault::unlock(store, &password).map_err(session::friendly)?;
            session::cache(&vault, &config, ttl)?;
            eprintln!("Unlocked for {}.", human(ttl));
        }
        Cmd::Lock => {
            let config = load_config()?;
            let was_cached = session::clear(&config)?;
            eprintln!("{}", if was_cached { "Locked." } else { "Already locked." });
        }
        Cmd::Link { name, env } => {
            let ctx = Ctx::open()?;
            let name = match name {
                Some(n) => n,
                None => dir_name(&ctx.cwd)?,
            };
            let existing = ctx.projects()?.into_iter().find(|p| p.name == name);
            let project = match existing {
                Some(p) => p,
                None => {
                    let p = Project::new(&name, ctx.stamp());
                    ctx.vault.put(Record::Project(p.clone()))?;
                    eprintln!("Created project {name}.");
                    p
                }
            };
            Link { project_id: project.id, environment: env.clone() }.write(&ctx.cwd)?;
            eprintln!("Linked this directory to {name} ({env}). {LINK_FILE} has no secrets; commit it.");
        }
        Cmd::Set { vars, env } => {
            let ctx = Ctx::open()?;
            let mut target = ctx.target(env.env, true)?;
            for var in vars {
                let (key, value) = match var.split_once('=') {
                    Some((k, v)) => (k.to_string(), v.to_string()),
                    None => {
                        let v = private_value(&var)?;
                        (var, v)
                    }
                };
                if !is_valid_var_name(&key) {
                    bail!("invalid variable name {key:?}");
                }
                if let Some(Entry::Secret(_)) = target.env.entry(&key) {
                    bail!(
                        "{key} is linked to a secret; rotate it with `dotsafe secret rotate`, or `dotsafe secret unlink {key}` first"
                    );
                }
                target.env.set(&key, value, ctx.stamp());
                eprintln!("Set {key} in {}.", target.label());
            }
            ctx.vault.put(Record::Environment(target.env))?;
        }
        Cmd::Get { key, env } => {
            let ctx = Ctx::open()?;
            let target = ctx.target(env.env, false)?;
            match ctx.resolver()?.value(target.env.id, &key)? {
                Some(value) => println!("{value}"),
                None => bail!("{key} is not set in {}", target.label()),
            }
        }
        Cmd::Unset { keys, hide, env } => {
            let ctx = Ctx::open()?;
            let mut target = ctx.target(env.env, false)?;
            let inherited: Vec<String> = match target.env.parent {
                Some(parent) => ctx.resolver()?.entries(parent)?.into_iter().map(|e| e.key).collect(),
                None => Vec::new(),
            };
            for key in keys {
                let own = target.env.entry(&key).is_some();
                let from_parent = inherited.contains(&key);
                if hide && !from_parent {
                    bail!("{key} isn't inherited here; plain `dotsafe unset {key}` removes it");
                }
                match (own, from_parent, hide) {
                    (_, true, true) | (false, true, false) => {
                        target.env.hide(&key, ctx.stamp());
                        eprintln!("Removed {key} from {}; it's no longer inherited here.", target.label());
                    }
                    (true, true, false) => {
                        target.env.remove(&key, ctx.stamp());
                        eprintln!("Removed {}'s own {key}; the inherited value applies again. Use --hide to remove it entirely.", target.label());
                    }
                    (true, false, _) => {
                        target.env.remove(&key, ctx.stamp());
                        eprintln!("Removed {key} from {}.", target.label());
                    }
                    (false, false, _) => eprintln!("{key} wasn't set."),
                }
            }
            ctx.vault.put(Record::Environment(target.env))?;
        }
        Cmd::List { env } => {
            let ctx = Ctx::open()?;
            let target = ctx.target(env.env, false)?;
            for entry in ctx.resolver()?.entries(target.env.id)? {
                println!("{}", entry.key);
            }
        }
        Cmd::Inherit { parent, none, env } => {
            let ctx = Ctx::open()?;
            let mut target = ctx.target(env.env, true)?;
            target.env.parent = match (parent, none) {
                (_, true) => None,
                (Some(name), false) => {
                    let found = ctx.environments()?.into_iter().find(|e| e.project_id == target.project.id && e.name == name);
                    let Some(parent) = found else { bail!("{} has no {name} environment", target.project.name) };
                    if parent.id == target.env.id {
                        bail!("an environment can't inherit from itself");
                    }
                    Some(parent.id)
                }
                (None, false) => bail!("name the environment to inherit from, or pass --none"),
            };
            target.env.updated = ctx.stamp();
            // Check the chain before saving: no loops, nothing missing.
            let mut records = ctx.vault.list()?;
            records.retain(|r| r.id() != target.env.id);
            records.push(Record::Environment(target.env.clone()));
            dotsafe_core::Resolver::from_records(&records).chain(target.env.id)?;
            let label = target.label();
            let parent_name = target.env.parent.and_then(|id| ctx.environments().ok()?.into_iter().find(|e| e.id == id).map(|e| e.name));
            ctx.vault.put(Record::Environment(target.env))?;
            match parent_name {
                Some(name) => eprintln!("{label} inherits from {name} now; its own variables override those."),
                None => eprintln!("{label} doesn't inherit from anything now."),
            }
        }
        Cmd::Import { file, from, item, env } => {
            let (text, source) = match from {
                Some(from) => (from.read(item.as_deref())?, from.label().to_string()),
                None => (
                    Zeroizing::new(std::fs::read_to_string(&file).with_context(|| format!("reading {}", file.display()))?),
                    file.display().to_string(),
                ),
            };
            let mut vars = dotenv::parse_any(&text).map_err(|e| anyhow::anyhow!("{source}: {e}"))?;
            // Doppler adds its own bookkeeping variables.
            if matches!(from, Some(ImportFrom::Doppler)) {
                vars.retain(|(k, _)| !k.starts_with("DOPPLER_"));
            }
            let ctx = Ctx::open()?;
            let mut target = ctx.target(env.env, true)?;
            let stamp = ctx.stamp();
            let mut imported = 0;
            for (key, value) in &vars {
                if let Some(Entry::Secret(_)) = target.env.entry(key) {
                    eprintln!("Skipped {key}: it's linked to a secret.");
                    continue;
                }
                target.env.set(key, value, stamp);
                imported += 1;
            }
            let label = target.label();
            ctx.vault.put(Record::Environment(target.env))?;
            eprintln!("Imported {imported} {} from {source} into {label}.", if imported == 1 { "variable" } else { "variables" });
            if from.is_none() {
                eprintln!("You can delete {source} and use `dotsafe run -- <command>` instead.");
            }
        }
        Cmd::Export { env, output, force } => {
            let ctx = Ctx::open()?;
            let target = ctx.target(env.env, false)?;
            let vars = ctx.resolver()?.resolve(target.env.id)?;
            let text = export::text(&vars, &target.label(), output.is_some());
            match output {
                Some(path) => {
                    export::write(&path, &text, force)?;
                    eprintln!(
                        "Wrote {} {} from {} to {}.",
                        vars.len(),
                        if vars.len() == 1 { "variable" } else { "variables" },
                        target.label(),
                        path.display()
                    );
                }
                None => print!("{}", *text),
            }
        }
        Cmd::Diff { first, second, names, env } => {
            let ctx = Ctx::open()?;
            let (a, b) = match second {
                Some(second) => (ctx.target(Some(first), false)?, ctx.target(Some(second), false)?),
                None => (ctx.target(env.env, false)?, ctx.target(Some(first), false)?),
            };
            if a.env.id == b.env.id {
                bail!("that's the same environment twice");
            }
            let changes = ctx.resolver()?.diff(a.env.id, b.env.id, names)?;
            if changes.is_empty() {
                let what = if names { "the same variables" } else { "the same variables and values" };
                eprintln!("{} and {} have {what}.", a.label(), b.label());
                return Ok(ExitCode::SUCCESS);
            }
            let width = changes.iter().map(|c| c.key().len()).max().unwrap_or(0);
            for change in &changes {
                let (mark, note) = match change {
                    Change::Removed(_) => ('-', format!("only in {}", a.env.name)),
                    Change::Added(_) => ('+', format!("only in {}", b.env.name)),
                    Change::Changed(_) => ('~', "different values".to_string()),
                };
                println!("{mark} {:<width$}  {note}", change.key());
            }
            return Ok(ExitCode::FAILURE);
        }
        Cmd::Check { file, strict, env } => {
            let example = compare::read_example(&file)?;
            let ctx = Ctx::open()?;
            let target = ctx.target(env.env, false)?;
            let names: Vec<String> = ctx.resolver()?.resolve(target.env.id)?.into_iter().map(|(k, _)| k).collect();
            let result = compare::check(&example, &names);
            let source = file.display();
            if !result.missing.is_empty() {
                eprintln!(
                    "{} is missing {} from {source}: {}",
                    target.label(),
                    count(result.missing.len()),
                    result.missing.join(", ")
                );
            }
            if !result.extra.is_empty() {
                eprintln!(
                    "{source} doesn't list {}: {}. Update it with `dotsafe example -o {source}`.",
                    count(result.extra.len()),
                    result.extra.join(", ")
                );
            }
            if result.missing.is_empty() && (result.extra.is_empty() || !strict) {
                if result.extra.is_empty() {
                    eprintln!("{} has every variable in {source}.", target.label());
                }
                return Ok(ExitCode::SUCCESS);
            }
            return Ok(ExitCode::FAILURE);
        }
        Cmd::Example { env, output, force } => {
            let ctx = Ctx::open()?;
            let target = ctx.target(env.env, false)?;
            let names: Vec<String> = ctx.resolver()?.entries(target.env.id)?.into_iter().map(|e| e.key).collect();
            let existing = match &output {
                Some(path) if path.exists() => compare::read_example(path)?,
                _ => Vec::new(),
            };
            let text = compare::example_text(&names, &existing, &target.label());
            match output {
                Some(path) => {
                    compare::write_example(&path, &text, force)?;
                    eprintln!("Wrote {} from {} to {}. Commit it.", count(names.len()), target.label(), path.display());
                }
                None => print!("{text}"),
            }
        }
        Cmd::Scan { install_hook: true, .. } => {
            let path = scan::install_hook()?;
            eprintln!("Commits in this checkout now stop if they contain a vault value ({}).", path.display());
        }
        Cmd::Scan { paths, staged, .. } => {
            let ctx = Ctx::open().map_err(|e| {
                if staged {
                    e.context(
                        "dotsafe couldn't check this commit; run `dotsafe unlock` first, or skip the check once with `git commit --no-verify`",
                    )
                } else {
                    e
                }
            })?;
            let matcher = scan::matcher(&ctx.vault)?;
            drop(ctx);
            let (hits, scanned) = scan::scan(&matcher, scan::sources(&paths, staged)?)?;
            for hit in &hits {
                println!("{}:{}:{}\t{}", hit.file, hit.line, hit.column, hit.name);
            }
            if hits.is_empty() {
                eprintln!("No vault values in {scanned} {}.", if scanned == 1 { "file" } else { "files" });
                return Ok(ExitCode::SUCCESS);
            }
            let files = hits.iter().map(|h| &h.file).collect::<std::collections::BTreeSet<_>>().len();
            let one = hits.len() == 1;
            eprintln!(
                "Found {} vault {} in {files} {}. Take {} out; if {} already pushed, rotate {}.",
                hits.len(),
                if one { "value" } else { "values" },
                if files == 1 { "file" } else { "files" },
                if one { "it" } else { "them" },
                if one { "it was" } else { "they were" },
                if one { "it" } else { "them" },
            );
            return Ok(ExitCode::FAILURE);
        }
        Cmd::Connect { service, name, set, token_stdin } => connect::connect(&service, name, &set, token_stdin)?,
        Cmd::Proxy(cmd) => proxy_profiles(cmd)?,
        Cmd::Connections => connect::list()?,
        Cmd::Disconnect { name } => connect::disconnect(&name)?,
        Cmd::Reconnect { name, token_stdin } => connect::reconnect(&name, token_stdin)?,
        Cmd::Secret(cmd) => secrets::run(cmd)?,
        #[cfg(unix)]
        Cmd::Daemon { stop_after, detach, key_from_stdin } => {
            let start = match (detach, key_from_stdin) {
                (true, _) => daemon::Start::Detach,
                (_, true) => daemon::Start::KeyFromStdin,
                _ => daemon::Start::Here,
            };
            daemon::run(stop_after, start)?
        }
        #[cfg(not(unix))]
        Cmd::Daemon { .. } => bail!("dotsafe daemon needs macOS or Linux for now"),
        #[cfg(unix)]
        Cmd::Approve { id, lifetime, deny, list } => approve::run(id, lifetime, deny, list)?,
        #[cfg(not(unix))]
        Cmd::Approve { .. } => bail!("dotsafe daemon needs macOS or Linux for now"),
        Cmd::Mcp { action: None } => mcp::serve()?,
        Cmd::Mcp { action: Some(McpAction::Setup) } => mcp::print_setup()?,
        Cmd::Run { env, proxy, only, daemon: through_daemon, wait, command } => {
            #[cfg(unix)]
            if let Some(code) =
                daemon::exec_through(env.env.as_deref(), proxy, only.as_deref(), through_daemon, wait, &command)?
            {
                return Ok(code);
            }
            #[cfg(not(unix))]
            {
                let _ = wait;
                if through_daemon {
                    bail!("dotsafe daemon needs macOS or Linux for now");
                }
            }
            let ctx = Ctx::open()?;
            let target = ctx.target(env.env, false)?;
            let mut vars = ctx.resolver()?.resolve(target.env.id)?;
            if let Some(only) = &only {
                let missing: Vec<&str> =
                    only.iter().filter(|n| !vars.iter().any(|(k, _)| k == *n)).map(String::as_str).collect();
                if !missing.is_empty() {
                    bail!("{} has no {}", target.env.name, missing.join(", "));
                }
                vars.retain(|(k, _)| only.contains(k));
            }
            let profiles = if proxy { dotsafe_agent::proxy::profiles(&ctx.vault.list()?) } else { Vec::new() };
            drop(ctx);
            let mut cmd = Command::new(&command[0]);
            cmd.args(&command[1..]);
            if !proxy {
                cmd.envs(vars);
                return exec(cmd, &command[0]);
            }
            // The proxy lives in this process, so wait for the command instead
            // of replacing ourselves with it.
            let server = dotsafe_agent::proxy::ProxyServer::start().context("couldn't start the secret proxy")?;
            let proxied = dotsafe_agent::proxy::apply(&server, vars, &profiles);
            if proxied.names.is_empty() {
                eprintln!("dotsafe: no API key with a proxy profile here; running normally.");
            } else {
                let (verb, keys) = if proxied.names.len() == 1 { ("goes", "the key") } else { ("go", "the keys") };
                eprintln!("dotsafe: {} {verb} through a local proxy; the process never has {keys}.", proxied.names.join(", "));
            }
            cmd.envs(proxied.env.iter().cloned());
            let status = cmd.status().with_context(|| format!("couldn't run {}", command[0]))?;
            drop(proxied);
            return Ok(ExitCode::from(status.code().unwrap_or(1).clamp(0, 255) as u8));
        }
        Cmd::Device(DeviceCmd::List) => {
            let ctx = Ctx::open()?;
            let devices = ctx.vault.devices();
            if devices.is_empty() {
                eprintln!("No devices use their own key yet. Turn on Touch ID unlock in the desktop app, or add one with a code.");
            }
            for d in devices {
                let this = if d.device_id == ctx.device_id { "\tthis machine" } else { "" };
                let stale = if d.stale { "\tneeds the password once" } else { "" };
                println!("{}\tadded {}{this}{stale}", d.name, local_time(d.added));
            }
        }
        Cmd::Device(DeviceCmd::Remove { name }) => {
            let mut ctx = Ctx::open()?;
            let matches: Vec<_> = ctx.vault.devices().into_iter().filter(|d| d.name == name).collect();
            let device = match matches.as_slice() {
                [one] => one.clone(),
                [] => bail!("no device named {name:?}; see `dotsafe device list`"),
                _ => bail!("more than one device is named {name:?}"),
            };
            if device.device_id == ctx.device_id {
                bail!("that's this machine; turn off Touch ID unlock in the desktop app instead");
            }
            let password = session::password("Master password: ")?;
            let recovery = ctx.vault.revoke_device(device.device_id, &password, KdfParams::default()).map_err(session::friendly)?;
            eprintln!("Removed {name}. The vault has a new key and everything was re-encrypted.");
            eprintln!("Other devices using Touch ID will ask for the password once.");
            let mut config = load_config()?;
            let since = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);
            config.rotation_queue = Some(dotsafe_core::RotationQueue { since, device: name.clone() });
            config.save()?;
            eprintln!("It can still use what it already knew. See what to rotate with `dotsafe secret queue`.");
            show_recovery_key(&recovery);
        }
        Cmd::Audit { limit } => {
            let ctx = Ctx::open()?;
            let events = dotsafe_core::audit::recent(&ctx.vault, limit)?;
            if events.is_empty() {
                eprintln!("No agent activity yet.");
            }
            for e in events {
                let outcome = format!("{:?}", e.outcome).to_lowercase();
                let detail = e.detail.map(|d| format!("\t{d}")).unwrap_or_default();
                println!("{}\t{}\t{outcome}\t{}\t{}{detail}", local_time(e.at), e.actor, e.summary, e.environment);
            }
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn proxy_profiles(cmd: ProxyCmd) -> Result<()> {
    use dotsafe_agent::proxy::{Profile, builtin_profiles};
    use dotsafe_core::integration::proxy_profiles;
    let ctx = Ctx::open()?;
    match cmd {
        ProxyCmd::List => {
            for p in builtin_profiles() {
                println!("{}\t{}\t{}\tbuilt in", p.name, p.key_var, p.upstream);
            }
            for p in proxy_profiles(&ctx.vault.list()?) {
                let status = match Profile::custom(&p) {
                    Ok(_) => "custom".to_string(),
                    Err(why) => format!("not used: {why}"),
                };
                println!("{}\t{}\t{}\t{status}", p.name, p.key_var, p.upstream);
            }
        }
        ProxyCmd::Add { name, key_var, base_url_var, upstream, base_path, header, raw } => {
            let records = ctx.vault.list()?;
            if proxy_profiles(&records).iter().any(|p| p.name == name || p.key_var == key_var) {
                bail!("there's already a profile named {name:?} or for {key_var}");
            }
            let profile = dotsafe_core::ProxyProfile {
                id: Uuid::new_v4(),
                name,
                key_var,
                base_url_var,
                upstream,
                base_path,
                header,
                bearer: !raw,
                deleted: false,
                updated: ctx.stamp(),
            };
            let checked = Profile::custom(&profile).map_err(|e| anyhow::anyhow!(e))?;
            ctx.vault.put(Record::ProxyProfile(profile))?;
            eprintln!(
                "Added {}. With --proxy, {} gets a placeholder and {} points at a local URL that sends the key to {}.",
                checked.name, checked.key_var, checked.base_url_var, checked.upstream
            );
        }
        ProxyCmd::Remove { name } => {
            let Some(mut profile) = proxy_profiles(&ctx.vault.list()?).into_iter().find(|p| p.name == name) else {
                bail!("no custom proxy profile named {name:?}; see `dotsafe proxy list`");
            };
            profile.deleted = true;
            profile.updated = ctx.stamp();
            ctx.vault.put(Record::ProxyProfile(profile))?;
            eprintln!("Removed {name}.");
        }
    }
    Ok(())
}

fn init(path: Option<PathBuf>) -> Result<()> {
    let path = match path {
        Some(p) => p,
        None => config::default_vault_path()?,
    };

    if path.join("vault.json").exists() {
        eprintln!("Found an existing vault at {}.", path.display());
        let password = session::password("Master password: ")?;
        Vault::unlock(FolderStore::new(&path), &password).map_err(session::friendly)?;
        Config::for_vault(path).save()?;
        eprintln!("This machine is now connected to it.");
        return Ok(());
    }

    let password = new_password()?;
    let mut vault = Vault::create(FolderStore::new(&path), &password, KdfParams::default())?;
    let recovery = vault.reset_recovery_key()?;
    Config::for_vault(path.clone()).save()?;

    eprintln!("Created a vault at {}.", path.display());
    show_recovery_key(&recovery);
    Ok(())
}

fn init_s3(s3: S3Location) -> Result<()> {
    let credentials = match dotsafe_stores::credentials(&s3).ok().flatten() {
        Some(credentials) => credentials,
        None => {
            let access_key_id = prompt_line("Access key ID: ")?;
            let secret = session::password("Secret access key: ")?;
            S3Credentials { access_key_id, secret_access_key: Zeroizing::new(secret.to_string()) }
        }
    };
    let store = CachedStore::open(S3Store::new(&s3, &credentials)?, config::cache_dir(&s3)?).map_err(session::friendly)?;
    let config = Config::for_s3(s3.clone())?;
    let save_key = || -> Result<()> {
        if std::env::var_os("DOTSAFE_S3_ACCESS_KEY_ID").is_none() {
            dotsafe_stores::save_credentials(&s3, &credentials)?;
        }
        Ok(())
    };

    if store.get("vault.json")?.is_some() {
        eprintln!("Found an existing vault in {}.", s3.describe());
        let password = session::password("Master password: ")?;
        Vault::unlock(store, &password).map_err(session::friendly)?;
        save_key()?;
        config.save()?;
        eprintln!("This machine is now connected to it.");
        return Ok(());
    }

    let password = new_password()?;
    let mut vault = Vault::create(store, &password, KdfParams::default())?;
    let recovery = vault.reset_recovery_key()?;
    save_key()?;
    config.save()?;
    eprintln!("Created a vault in {}.", s3.describe());
    show_recovery_key(&recovery);
    Ok(())
}

/// Saves the new location's credentials, once the move has worked.
type SaveCredentials = Box<dyn FnOnce() -> Result<()>>;

/// Copies the vault to a new, empty place, checks it opens there, and points
/// this machine at it. The old copy is left alone.
fn move_vault(path: Option<PathBuf>, s3: S3Args, webdav: Option<String>, user: Option<String>) -> Result<()> {
    let old = load_config()?;
    let vault = session::open(&old)?;
    if old.is_remote() {
        vault.sync().map_err(|e| anyhow::anyhow!("sync first so nothing is left behind: {e}"))?;
    }
    // The destination is written directly (not through a cache), then a
    // fresh cache fills from it on the next open.
    let (dst, new_config, save): (Box<dyn Store>, Config, SaveCredentials) =
        match (path, s3.bucket, s3.endpoint, webdav, user) {
            (Some(path), _, _, _, _) => {
                let path = std::path::absolute(&path)?;
                std::fs::create_dir_all(&path)?;
                (Box::new(FolderStore::new(&path)), Config::for_vault(path), Box::new(|| Ok(())))
            }
            (_, Some(bucket), Some(endpoint), _, _) => {
                let location = S3Location { endpoint, region: s3.region, bucket, prefix: s3.prefix.trim_matches('/').to_owned() };
                let credentials = match dotsafe_stores::credentials(&location).ok().flatten() {
                    Some(c) => c,
                    None => S3Credentials {
                        access_key_id: prompt_line("Access key ID: ")?,
                        secret_access_key: Zeroizing::new(rpassword::prompt_password("Secret access key: ")?),
                    },
                };
                let store = S3Store::new(&location, &credentials)?;
                let config = Config::for_s3(location.clone())?;
                (Box::new(store), config, Box::new(move || Ok(dotsafe_stores::save_credentials(&location, &credentials)?)))
            }
            (_, _, _, Some(url), Some(username)) => {
                let location = dotsafe_core::WebDavLocation { url, username };
                let password = match dotsafe_stores::webdav_password(&location).ok().flatten() {
                    Some(p) => p,
                    None => Zeroizing::new(rpassword::prompt_password(format!("Password for {}: ", location.username))?),
                };
                let store = dotsafe_stores::WebDavStore::new(&location, &password)?;
                let config = Config::for_webdav(location.clone())?;
                (Box::new(store), config, Box::new(move || Ok(dotsafe_stores::save_webdav_password(&location, &password)?)))
            }
            _ => bail!("say where to move it: --path, --bucket with --endpoint, or --webdav with --user"),
        };
    let copied = dotsafe_core::backup::copy_to_empty(vault.store(), &*dst).map_err(|e| match e {
        Error::AlreadyExists(at) => anyhow::anyhow!("{at} already holds a vault; pick an empty place"),
        e => e.into(),
    })?;
    // Check the copy opens with this vault's key before switching.
    let moved = Vault::unlock_with_session_key(dst, &vault.session_key()).map_err(|e| anyhow::anyhow!("the copy doesn't open ({e}); nothing was switched"))?;
    moved.list()?;
    save()?;
    let from = old.describe_location();
    new_config.save()?;
    eprintln!("Moved {copied} files to {}. This machine uses it now.", new_config.describe_location());
    eprintln!("The old copy at {from} is untouched. Connect your other devices to the new place, then delete it.");
    Ok(())
}

fn init_webdav(webdav: dotsafe_core::WebDavLocation) -> Result<()> {
    let password = match dotsafe_stores::webdav_password(&webdav).ok().flatten() {
        Some(password) => password,
        // Not session::password: DOTSAFE_PASSWORD is the master password.
        None => Zeroizing::new(rpassword::prompt_password(format!("Password for {} (an app password is best): ", webdav.username))?),
    };
    let remote = dotsafe_stores::WebDavStore::new(&webdav, &password)?;
    let store = CachedStore::open(remote, config::cache_dir_for(&webdav.id())?).map_err(session::friendly)?;
    let config = Config::for_webdav(webdav.clone())?;
    let save_password = || -> Result<()> {
        if std::env::var_os("DOTSAFE_WEBDAV_PASSWORD").is_none() {
            dotsafe_stores::save_webdav_password(&webdav, &password)?;
        }
        Ok(())
    };
    if store.get("vault.json")?.is_some() {
        eprintln!("Found an existing vault at {}.", webdav.describe());
        let master = session::password("Master password: ")?;
        Vault::unlock(store, &master).map_err(session::friendly)?;
        save_password()?;
        config.save()?;
        eprintln!("This machine is now connected to it.");
        return Ok(());
    }
    let master = new_password()?;
    let mut vault = Vault::create(store, &master, KdfParams::default())?;
    let recovery = vault.reset_recovery_key()?;
    save_password()?;
    config.save()?;
    eprintln!("Created a vault at {}.", webdav.describe());
    show_recovery_key(&recovery);
    Ok(())
}

fn new_password() -> Result<Zeroizing<String>> {
    let password = session::password("New master password: ")?;
    if password.chars().count() < 8 {
        bail!("use a master password of at least 8 characters");
    }
    if std::env::var_os("DOTSAFE_PASSWORD").is_none() {
        let confirm = session::password("Confirm master password: ")?;
        if *confirm != *password {
            bail!("passwords didn't match");
        }
    }
    Ok(password)
}

fn prompt_line(prompt: &str) -> Result<String> {
    use std::io::Write;
    eprint!("{prompt}");
    std::io::stderr().flush()?;
    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;
    let line = line.trim().to_owned();
    if line.is_empty() {
        bail!("nothing entered");
    }
    Ok(line)
}

/// A value for `set KEY`: piped in (`pbpaste | dotsafe set KEY`), or typed
/// without echo at the terminal.
fn private_value(key: &str) -> Result<String> {
    use std::io::{IsTerminal, Read};
    let value = if std::io::stdin().is_terminal() {
        rpassword::prompt_password(format!("Value for {key}: "))?
    } else {
        let mut piped = String::new();
        std::io::stdin().read_to_string(&mut piped).context("couldn't read the value from stdin")?;
        // Only the newline `echo` and most tools add; other whitespace is kept.
        let end = piped.trim_end_matches(['\n', '\r']).len();
        piped.truncate(end);
        piped
    };
    if value.is_empty() {
        bail!("no value for {key}; pipe one in, or set KEY=VALUE");
    }
    Ok(value)
}

fn show_recovery_key(recovery: &dotsafe_core::RecoveryKey) {
    eprintln!();
    eprintln!("Recovery key (the only way in if you forget your password):");
    println!("{}", recovery.to_display_string());
    eprintln!();
    eprintln!("Store it somewhere safe, offline. It won't be shown again.");
}

/// Replaces this process with the command on Unix, so signals and the exit
/// code pass straight through. Elsewhere, waits and forwards the exit code.
fn exec(mut cmd: Command, name: &str) -> Result<ExitCode> {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let err = cmd.exec();
        bail!("couldn't run {name}: {err}");
    }
    #[cfg(not(unix))]
    {
        let status = cmd.status().with_context(|| format!("couldn't run {name}"))?;
        Ok(ExitCode::from(status.code().unwrap_or(1).clamp(0, 255) as u8))
    }
}

struct Ctx {
    vault: Vault,
    device_id: Uuid,
    cwd: PathBuf,
}

/// The environment a command reads or writes, and its project.
struct Target {
    project: Project,
    env: Environment,
}

impl Target {
    fn label(&self) -> String {
        format!("{}/{}", self.project.name, self.env.name)
    }
}

impl Ctx {
    fn open() -> Result<Ctx> {
        let config = load_config()?;
        let vault = session::open(&config)?;
        Ok(Ctx { vault, device_id: config.device_id, cwd: std::env::current_dir()? })
    }

    fn stamp(&self) -> Stamp {
        self.vault.stamp(self.device_id)
    }

    /// Works out variables with inheritance, links and references.
    fn resolver(&self) -> Result<dotsafe_core::Resolver> {
        Ok(dotsafe_core::Resolver::from_records(&self.vault.list()?))
    }

    fn projects(&self) -> Result<Vec<Project>> {
        Ok(self
            .vault
            .list()?
            .into_iter()
            .filter_map(|r| match r {
                Record::Project(p) if !p.deleted => Some(p),
                _ => None,
            })
            .collect())
    }

    /// Every live environment in the vault.
    fn environments(&self) -> Result<Vec<Environment>> {
        Ok(self
            .vault
            .list()?
            .into_iter()
            .filter_map(|r| match r {
                Record::Environment(e) if !e.deleted => Some(e),
                _ => None,
            })
            .collect())
    }

    /// Resolves the linked project and environment. With `create`, a missing
    /// link, project or environment is created (environments are only saved
    /// once the caller puts them).
    fn target(&self, env_name: Option<String>, create: bool) -> Result<Target> {
        let link = match Link::find(&self.cwd)? {
            Some((link, _)) => link,
            None if create => {
                let name = dir_name(&self.cwd)?;
                let project = Project::new(&name, self.stamp());
                self.vault.put(Record::Project(project.clone()))?;
                let link = Link { project_id: project.id, environment: DEFAULT_ENV.into() };
                link.write(&self.cwd)?;
                eprintln!("Created project {name} and linked this directory. {LINK_FILE} has no secrets; commit it.");
                link
            }
            None => bail!(
                "this directory isn't linked to a project; run `dotsafe link`, or `dotsafe set`/`import` to create one"
            ),
        };

        let project = match self.vault.get(link.project_id)? {
            Some(Record::Project(p)) if !p.deleted => p,
            _ => bail!(
                "the project in {LINK_FILE} isn't in this vault (deleted, not synced yet, or a different vault)"
            ),
        };

        let env_name = env_name.unwrap_or(link.environment);
        if env_name.trim().is_empty() {
            bail!("environment name can't be empty");
        }
        let existing = self.vault.list()?.into_iter().find_map(|r| match r {
            Record::Environment(e) if e.project_id == project.id && e.name == env_name && !e.deleted => Some(e),
            _ => None,
        });
        let env = match existing {
            Some(e) => e,
            None if create => {
                let mut env = Environment::new(project.id, &env_name, self.stamp());
                // The derived id is taken if an environment was renamed away
                // from this name; fall back to a random one.
                if self.vault.get(env.id)?.is_some() {
                    env.id = Uuid::new_v4();
                }
                env
            }
            None => bail!("{}/{env_name} has no variables yet; add some with `dotsafe set`", project.name),
        };
        Ok(Target { project, env })
    }
}

pub(crate) fn load_config() -> Result<Config> {
    Config::load()?.context("no vault set up yet; run `dotsafe init` first")
}

fn dir_name(dir: &Path) -> Result<String> {
    dir.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .context("can't name a project after this directory; pass a name to `dotsafe link`")
}

/// Unix milliseconds as `YYYY-MM-DD HH:MM` in this machine's time zone.
fn local_time(ms: u64) -> String {
    time_in(ms, &jiff::tz::TimeZone::system())
}

fn time_in(ms: u64, tz: &jiff::tz::TimeZone) -> String {
    match jiff::Timestamp::from_millisecond(ms as i64) {
        Ok(at) => at.to_zoned(tz.clone()).strftime("%Y-%m-%d %H:%M").to_string(),
        Err(_) => "unknown time".to_string(),
    }
}

/// "1 variable", "3 variables".
fn count(n: usize) -> String {
    format!("{n} {}", if n == 1 { "variable" } else { "variables" })
}

fn human(d: Duration) -> String {
    let s = d.as_secs();
    match s {
        _ if s.is_multiple_of(86400) => format!("{}d", s / 86400),
        _ if s.is_multiple_of(3600) => format!("{}h", s / 3600),
        _ if s.is_multiple_of(60) => format!("{}m", s / 60),
        _ => format!("{s}s"),
    }
}

#[cfg(test)]
mod tests {
    use super::time_in;
    use jiff::tz::TimeZone;

    #[test]
    fn times_in_a_zone() {
        assert_eq!(time_in(0, &TimeZone::UTC), "1970-01-01 00:00");
        assert_eq!(time_in(951_782_400_000, &TimeZone::UTC), "2000-02-29 00:00");
        let kolkata = TimeZone::get("Asia/Kolkata").unwrap();
        assert_eq!(time_in(1_793_015_254_719, &kolkata), "2026-10-26 17:17");
    }
}
