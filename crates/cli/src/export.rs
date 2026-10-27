//! `dotsafe export`: writes an environment as `.env` text, for tools that
//! insist on a file (Docker Compose, Prisma, IDE run configs).
//!
//! A file on disk is the weakest place a secret can be, so writing one is
//! fenced in: it's created readable by you alone, refused if git would pick
//! it up, and only overwritten when dotsafe wrote it.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use dotsafe_core::dotenv;
use zeroize::Zeroizing;

/// First line of every file `export` writes; marks it as safe to overwrite.
const HEADER: &str = "# Written by `dotsafe export`";

/// `.env` text for `vars`, with dotsafe's header when it's going to a file.
pub fn text(vars: &[(String, String)], label: &str, to_file: bool) -> Zeroizing<String> {
    let mut out = Zeroizing::new(String::new());
    if to_file {
        out.push_str(&format!(
            "{HEADER} from {label}. Don't commit it or edit it; change values in dotsafe and export again.\n"
        ));
    }
    out.push_str(&dotenv::format(vars.iter().map(|(k, v)| (k.as_str(), v.as_str()))));
    out
}

/// Writes `contents` to `path` with owner-only permissions, after the checks
/// above. `force` skips them.
pub fn write(path: &Path, contents: &str, force: bool) -> Result<()> {
    if !force {
        if let Ok(existing) = fs::read_to_string(path)
            && !existing.starts_with(HEADER)
        {
            bail!("{} exists and dotsafe didn't write it; move it away first, or pass --force", path.display());
        }
        if git_would_commit(path) {
            bail!(
                "{} isn't ignored by git, so it could be committed; add it to .gitignore first, or pass --force",
                path.display()
            );
        }
    }
    let tmp = sibling(path, ".dotsafe-tmp");
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&tmp).with_context(|| format!("writing {}", tmp.display()))?;
    file.write_all(contents.as_bytes())?;
    file.sync_all()?;
    drop(file);
    fs::rename(&tmp, path).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

/// Whether `path` is inside a git checkout and not ignored. Without git, or
/// outside a checkout, there's nothing to protect against.
fn git_would_commit(path: &Path) -> bool {
    let dir = match path.parent() {
        Some(d) if !d.as_os_str().is_empty() => d,
        _ => Path::new("."),
    };
    let Some(name) = path.file_name() else { return false };
    let status = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["check-ignore", "-q", "--"])
        .arg(name)
        .stderr(std::process::Stdio::null())
        .status();
    // 0: ignored. 1: not ignored. 128: not a checkout.
    matches!(status.map(|s| s.code()), Ok(Some(1)))
}

fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(suffix);
    path.with_file_name(name)
}
