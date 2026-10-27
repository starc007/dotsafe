mod export;
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
        #[arg(long, conflicts_with = "bucket")]
        path: Option<PathBuf>,
        #[command(flatten)]
        s3: S3Args,
    },
    /// Exchange changes with the bucket now (vaults in S3 or R2)
    Sync,
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
    /// Delete variables
    Unset {
        #[arg(required = true)]
        keys: Vec<String>,
        #[command(flatten)]
        env: EnvArg,
    },
    /// List variable names
    List {
        #[command(flatten)]
        env: EnvArg,
    },
    /// Import variables from a .env file
    Import {
        #[arg(default_value = ".env")]
        file: PathBuf,
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
    /// Shared secrets: link one value into many environments and rotate it in one place
    #[command(subcommand, visible_alias = "secrets")]
    Secret(secrets::SecretCmd),
    /// Serve the MCP tools AI agents use to run commands with secrets they never see
    Mcp {
        #[command(subcommand)]
        action: Option<McpAction>,
    },
    /// Run a command with the variables set in its environment
    Run {
        #[command(flatten)]
        env: EnvArg,
        /// Keep OpenAI and Anthropic keys out of the process: it gets a local proxy URL that adds the key
        #[arg(long)]
        proxy: bool,
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
    match cli.command {
        Cmd::Init { path, s3 } => match (s3.bucket, s3.endpoint) {
            (Some(bucket), Some(endpoint)) => {
                let prefix = s3.prefix.trim_matches('/').to_owned();
                init_s3(S3Location { endpoint, region: s3.region, bucket, prefix })?
            }
            _ => init(path)?,
        },
        Cmd::Sync => {
            let config = load_config()?;
            if config.s3.is_none() {
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
                        let v = rpassword::prompt_password(format!("Value for {var}: "))?;
                        (var, v)
                    }
                };
                if !is_valid_var_name(&key) {
                    bail!("invalid variable name {key:?}");
                }
                if let Some(Entry::Secret(_)) = target.env.entry(&key) {
                    bail!(
                        "{key} is linked to a shared secret; rotate it with `dotsafe secret rotate`, or `dotsafe secret unlink {key}` first"
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
            let secrets = ctx.vault.secrets()?;
            match target.env.entry(&key).and_then(|entry| secrets.value(entry)) {
                Some(value) => println!("{value}"),
                None => bail!("{key} is not set in {}", target.label()),
            }
        }
        Cmd::Unset { keys, env } => {
            let ctx = Ctx::open()?;
            let mut target = ctx.target(env.env, false)?;
            for key in keys {
                if target.env.entry(&key).is_none() {
                    eprintln!("{key} wasn't set.");
                    continue;
                }
                target.env.remove(&key, ctx.stamp());
                eprintln!("Removed {key} from {}.", target.label());
            }
            ctx.vault.put(Record::Environment(target.env))?;
        }
        Cmd::List { env } => {
            let ctx = Ctx::open()?;
            let target = ctx.target(env.env, false)?;
            for key in target.env.keys() {
                println!("{key}");
            }
        }
        Cmd::Import { file, env } => {
            let text = std::fs::read_to_string(&file).with_context(|| format!("reading {}", file.display()))?;
            let vars = dotenv::parse(&text).map_err(|e| anyhow::anyhow!("{}: {e}", file.display()))?;
            let ctx = Ctx::open()?;
            let mut target = ctx.target(env.env, true)?;
            let stamp = ctx.stamp();
            let mut imported = 0;
            for (key, value) in &vars {
                if let Some(Entry::Secret(_)) = target.env.entry(key) {
                    eprintln!("Skipped {key}: it's linked to a shared secret.");
                    continue;
                }
                target.env.set(key, value, stamp);
                imported += 1;
            }
            let label = target.label();
            ctx.vault.put(Record::Environment(target.env))?;
            eprintln!("Imported {imported} variables from {} into {label}.", file.display());
            eprintln!("You can delete {} and use `dotsafe run -- <command>` instead.", file.display());
        }
        Cmd::Export { env, output, force } => {
            let ctx = Ctx::open()?;
            let target = ctx.target(env.env, false)?;
            let secrets = ctx.vault.secrets()?;
            let vars: Vec<(String, String)> =
                secrets.resolve(&target.env).map(|(k, v)| (k.to_string(), v.to_string())).collect();
            let text = export::text(&vars, &target.label(), output.is_some());
            match output {
                Some(path) => {
                    export::write(&path, &text, force)?;
                    eprintln!("Wrote {} variables from {} to {}.", vars.len(), target.label(), path.display());
                }
                None => print!("{}", *text),
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
                eprintln!("No vault values in {scanned} files.");
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
        Cmd::Secret(cmd) => secrets::run(cmd)?,
        Cmd::Mcp { action: None } => mcp::serve()?,
        Cmd::Mcp { action: Some(McpAction::Setup) } => mcp::print_setup()?,
        Cmd::Run { env, proxy, command } => {
            let ctx = Ctx::open()?;
            let target = ctx.target(env.env, false)?;
            let secrets = ctx.vault.secrets()?;
            let vars: Vec<(String, String)> =
                secrets.resolve(&target.env).map(|(k, v)| (k.to_string(), v.to_string())).collect();
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
            let proxied = dotsafe_agent::proxy::apply(&server, vars);
            if proxied.names.is_empty() {
                eprintln!("dotsafe: no OPENAI_API_KEY or ANTHROPIC_API_KEY to proxy; running normally.");
            } else {
                eprintln!("dotsafe: {} go through a local proxy; the process never has the key.", proxied.names.join(", "));
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
                println!("{}\tadded {}{this}{stale}", d.name, utc(d.added));
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
                println!("{}\t{}\t{outcome}\t{}\t{}{detail}", utc(e.at), e.actor, e.summary, e.environment);
            }
        }
    }
    Ok(ExitCode::SUCCESS)
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

fn load_config() -> Result<Config> {
    Config::load()?.context("no vault set up yet; run `dotsafe init` first")
}

fn dir_name(dir: &Path) -> Result<String> {
    dir.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .context("can't name a project after this directory; pass a name to `dotsafe link`")
}

/// Unix milliseconds as `YYYY-MM-DD HH:MM` UTC.
fn utc(ms: u64) -> String {
    let secs = ms / 1000;
    let (days, rem) = ((secs / 86_400) as i64, secs % 86_400);
    // Days since 1970-01-01 to a civil date (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02} {:02}:{:02}", rem / 3600, rem % 3600 / 60)
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
    use super::utc;

    #[test]
    fn utc_dates() {
        assert_eq!(utc(0), "1970-01-01 00:00");
        assert_eq!(utc(951_782_400_000), "2000-02-29 00:00");
        assert_eq!(utc(1_793_015_254_719), "2026-10-26 11:47");
    }
}
