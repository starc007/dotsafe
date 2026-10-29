//! `dotsafe scan`: finds values from the vault in files, before they're
//! committed. Matching uses the agent redactor, so a secret is caught raw,
//! hex, base64, URL-encoded or JSON-escaped. Only names and locations are
//! printed, never the values.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use dotsafe_agent::redact::Redactor;
use dotsafe_core::{Entry, Record, Vault, VersionState};

use crate::link::LINK_FILE;

/// Files bigger than this are skipped: lockfiles and build output, not code.
const MAX_FILE: u64 = 10 * 1024 * 1024;
/// Folders skipped when walking outside a git checkout.
const SKIP_DIRS: &[&str] = &[".git", "node_modules", "target", "dist", "build", ".next", ".venv", "vendor"];
/// Values too common to be secrets, like `NODE_ENV=development`, which would
/// otherwise match wherever the word appears. Environment names are added.
const PLAIN_WORDS: &[&str] = &[
    "development", "production", "staging", "localhost", "debug", "info", "warning", "error", "enabled", "disabled",
];

const HOOK: &str = "#!/bin/sh\n\
# Added by `dotsafe scan --install-hook`: stops commits that contain a value\n\
# from your dotsafe vault. Skip it once with `git commit --no-verify`.\n\
exec dotsafe scan --staged\n";

/// Every value in the vault worth looking for, named for the report:
/// `project/environment/KEY` for variables and `secret NAME` for shared
/// secrets, including a previous value that's still valid.
pub fn matcher(vault: &Vault) -> Result<Redactor> {
    let records = vault.list()?;
    let mut plain: HashSet<String> = PLAIN_WORDS.iter().map(|w| w.to_string()).collect();
    for record in &records {
        if let Record::Environment(env) = record {
            plain.insert(env.name.to_lowercase());
        }
    }
    let secretish = |value: &str| !plain.contains(&value.trim().to_lowercase());
    let mut values = Vec::new();
    for record in &records {
        if let Record::Secret(secret) = record
            && !secret.deleted
        {
            for (_, version) in secret.versions() {
                if let Some(value) = &version.value
                    && version.state != VersionState::Retired
                    && secretish(value)
                {
                    values.push((format!("secret {}", secret.name), value.clone()));
                }
            }
        }
    }
    let projects: HashMap<_, _> = records
        .iter()
        .filter_map(|r| match r {
            Record::Project(p) if !p.deleted => Some((p.id, p.name.as_str())),
            _ => None,
        })
        .collect();
    for record in &records {
        if let Record::Environment(env) = record
            && !env.deleted
            && let Some(project) = projects.get(&env.project_id)
        {
            for (key, entry) in env.entries() {
                // Linked variables are covered by their secret above.
                if let Entry::Value(value) = entry
                    && secretish(value)
                {
                    values.push((format!("{project}/{}/{key}", env.name), value.to_string()));
                }
            }
        }
    }
    Ok(Redactor::new(&values))
}

/// A file to scan: where to report it, and how to read it.
pub enum Source {
    Disk(PathBuf),
    /// The staged copy, read with `git show :path`.
    Staged(String),
}

/// Lists what `--staged` or the given paths cover.
pub fn sources(paths: &[PathBuf], staged: bool) -> Result<Vec<(String, Source)>> {
    if staged {
        let out = git(Path::new("."), &["diff", "--cached", "--name-only", "-z", "--diff-filter=ACMR"])?
            .context("`dotsafe scan --staged` runs inside a git checkout")?;
        return Ok(split_z(&out).map(|p| (p.clone(), Source::Staged(p))).collect());
    }
    let paths = if paths.is_empty() { vec![PathBuf::from(".")] } else { paths.to_vec() };
    let mut files = Vec::new();
    for path in paths {
        if path.is_file() {
            files.push((path.display().to_string(), Source::Disk(path)));
        } else if path.is_dir() {
            // In a checkout, scan what git would commit: tracked files and new
            // ones that aren't ignored.
            match git(&path, &["ls-files", "-co", "--exclude-standard", "-z"])? {
                Some(out) => {
                    for rel in split_z(&out) {
                        let file = path.join(&rel);
                        if file.is_file() {
                            files.push((file.display().to_string(), Source::Disk(file)));
                        }
                    }
                }
                None => walk(&path, &mut files)?,
            }
        } else {
            bail!("{} doesn't exist", path.display());
        }
    }
    Ok(files)
}

/// One secret found in a file.
pub struct Hit {
    pub file: String,
    pub line: usize,
    pub column: usize,
    pub name: String,
}

/// Scans every source, skipping binary and very large files. Returns the
/// hits and how many files were read.
pub fn scan(matcher: &Redactor, sources: Vec<(String, Source)>) -> Result<(Vec<Hit>, usize)> {
    let mut hits = Vec::new();
    let mut scanned = 0;
    for (label, source) in sources {
        // dotsafe's own link file holds a project id and an environment name, never a value.
        if Path::new(&label).file_name().is_some_and(|name| name == LINK_FILE) {
            continue;
        }
        let Some(bytes) = read(&source)? else { continue };
        if bytes.iter().take(8000).any(|&b| b == 0) {
            continue;
        }
        scanned += 1;
        let text = String::from_utf8_lossy(&bytes);
        for found in matcher.find(&text) {
            let before = &text[..found.start];
            let line = before.matches('\n').count() + 1;
            let column = before.len() - before.rfind('\n').map_or(0, |i| i + 1) + 1;
            hits.push(Hit { file: label.clone(), line, column, name: found.name.to_string() });
        }
    }
    Ok((hits, scanned))
}

/// Writes the pre-commit hook into this checkout, unless one that does
/// something else is already there.
pub fn install_hook() -> Result<PathBuf> {
    let out = git(Path::new("."), &["rev-parse", "--git-path", "hooks/pre-commit"])?
        .context("run `dotsafe scan --install-hook` inside a git checkout")?;
    let path = PathBuf::from(String::from_utf8_lossy(&out).trim());
    if let Ok(existing) = fs::read_to_string(&path) {
        if existing.contains("dotsafe scan") {
            return Ok(path);
        }
        bail!("{} already exists; add this line to it instead:\n  dotsafe scan --staged || exit 1", path.display());
    }
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(&path, HOOK).with_context(|| format!("writing {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
    }
    Ok(path)
}

fn read(source: &Source) -> Result<Option<Vec<u8>>> {
    match source {
        Source::Disk(path) => {
            let Ok(meta) = fs::metadata(path) else { return Ok(None) };
            if meta.len() > MAX_FILE {
                return Ok(None);
            }
            Ok(fs::read(path).ok())
        }
        Source::Staged(path) => {
            let out = git(Path::new("."), &["show", &format!(":{path}")])?.unwrap_or_default();
            Ok((out.len() as u64 <= MAX_FILE).then_some(out))
        }
    }
}

fn walk(dir: &Path, files: &mut Vec<(String, Source)>) -> Result<()> {
    for entry in fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        let kind = entry.file_type()?;
        if kind.is_dir() {
            if !SKIP_DIRS.contains(&entry.file_name().to_string_lossy().as_ref()) {
                walk(&path, files)?;
            }
        } else if kind.is_file() {
            files.push((path.display().to_string(), Source::Disk(path)));
        }
    }
    Ok(())
}

/// Runs git in `dir`. `None` when git isn't installed or `dir` isn't in a
/// checkout.
fn git(dir: &Path, args: &[&str]) -> Result<Option<Vec<u8>>> {
    let Ok(out) = Command::new("git").arg("-C").arg(dir).args(args).output() else { return Ok(None) };
    Ok(out.status.success().then_some(out.stdout))
}

fn split_z(bytes: &[u8]) -> impl Iterator<Item = String> + '_ {
    bytes.split(|&b| b == 0).filter(|p| !p.is_empty()).map(|p| String::from_utf8_lossy(p).into_owned())
}
