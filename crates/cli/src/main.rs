mod link;
mod session;

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use clap::{Args, Parser, Subcommand};
use dotsafe_core::{Config, Environment, FolderStore, KdfParams, Project, Record, Stamp, Vault, config, dotenv, is_valid_var_name};
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
        #[arg(long)]
        path: Option<PathBuf>,
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
    /// Run a command with the variables set in its environment
    Run {
        #[command(flatten)]
        env: EnvArg,
        #[arg(required = true, trailing_var_arg = true, allow_hyphen_values = true, value_name = "COMMAND")]
        command: Vec<String>,
    },
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
        Cmd::Init { path } => init(path)?,
        Cmd::Unlock { ttl } => {
            let config = load_config()?;
            let password = session::password("Master password: ")?;
            let vault = Vault::unlock(FolderStore::new(&config.vault_path), &password).map_err(session::friendly)?;
            session::cache(&vault, &config.vault_path, ttl)?;
            eprintln!("Unlocked for {}.", human(ttl));
        }
        Cmd::Lock => {
            let config = load_config()?;
            let was_cached = session::clear(&config.vault_path)?;
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
                target.env.set(&key, value, ctx.stamp());
                eprintln!("Set {key} in {}.", target.label());
            }
            ctx.vault.put(Record::Environment(target.env))?;
        }
        Cmd::Get { key, env } => {
            let ctx = Ctx::open()?;
            let target = ctx.target(env.env, false)?;
            match target.env.get(&key) {
                Some(value) => println!("{value}"),
                None => bail!("{key} is not set in {}", target.label()),
            }
        }
        Cmd::Unset { keys, env } => {
            let ctx = Ctx::open()?;
            let mut target = ctx.target(env.env, false)?;
            for key in keys {
                if target.env.get(&key).is_none() {
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
            for (key, _) in target.env.vars() {
                println!("{key}");
            }
        }
        Cmd::Import { file, env } => {
            let text = std::fs::read_to_string(&file).with_context(|| format!("reading {}", file.display()))?;
            let vars = dotenv::parse(&text).map_err(|e| anyhow::anyhow!("{}: {e}", file.display()))?;
            let ctx = Ctx::open()?;
            let mut target = ctx.target(env.env, true)?;
            let stamp = ctx.stamp();
            for (key, value) in &vars {
                target.env.set(key, value, stamp);
            }
            let label = target.label();
            ctx.vault.put(Record::Environment(target.env))?;
            eprintln!("Imported {} variables from {} into {label}.", vars.len(), file.display());
            eprintln!("You can delete {} and use `dotsafe run -- <command>` instead.", file.display());
        }
        Cmd::Run { env, command } => {
            let ctx = Ctx::open()?;
            let target = ctx.target(env.env, false)?;
            let mut cmd = Command::new(&command[0]);
            cmd.args(&command[1..]).envs(target.env.vars());
            drop(ctx);
            return exec(cmd, &command[0]);
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
    let mut vault = Vault::create(FolderStore::new(&path), &password, KdfParams::default())?;
    let recovery = vault.reset_recovery_key()?;
    Config::for_vault(path.clone()).save()?;

    eprintln!("Created a vault at {}.", path.display());
    eprintln!();
    eprintln!("Recovery key (the only way in if you forget your password):");
    println!("{}", recovery.to_display_string());
    eprintln!();
    eprintln!("Store it somewhere safe, offline. It won't be shown again.");
    Ok(())
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
        let vault = session::open(&config.vault_path)?;
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

fn human(d: Duration) -> String {
    let s = d.as_secs();
    match s {
        _ if s.is_multiple_of(86400) => format!("{}d", s / 86400),
        _ if s.is_multiple_of(3600) => format!("{}h", s / 3600),
        _ if s.is_multiple_of(60) => format!("{}m", s / 60),
        _ => format!("{s}s"),
    }
}
