//! Files a command runs, like `check.js` in `node check.js`. An agent can
//! write a script and then ask to run it, and the command line alone says
//! nothing about what the script does. So the approval shows each file and
//! when it last changed, and a grant covers only the content that was
//! approved: edit the file and the next run asks again.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use dotsafe_core::crypto::sha256_hex;

/// At most this many files are looked at.
const MAX_FILES: usize = 8;
/// Bigger files are fingerprinted by size and change time instead of read.
const MAX_HASHED: u64 = 10 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunFile {
    /// Absolute path.
    pub path: PathBuf,
    /// SHA-256 of the contents, or size and change time for big files.
    pub fingerprint: String,
    /// When it last changed, in Unix milliseconds.
    pub modified_ms: Option<u64>,
}

/// The files among the program and its arguments: the program itself when
/// it's inside `cwd` (a script there, not an installed tool), and each
/// argument that names an existing file. Flags are skipped.
pub fn files_run(program: &Path, args: &[String], cwd: &Path) -> Vec<RunFile> {
    let mut paths = Vec::new();
    if program.starts_with(cwd) {
        paths.push(program.to_path_buf());
    }
    for arg in args.iter().filter(|a| !a.starts_with('-')) {
        let path = cwd.join(arg);
        if path.is_file() {
            paths.push(path.canonicalize().unwrap_or(path));
        }
    }
    paths.dedup();
    paths.into_iter().take(MAX_FILES).filter_map(|path| stamp(&path)).collect()
}

fn stamp(path: &Path) -> Option<RunFile> {
    let meta = fs::metadata(path).ok()?;
    let modified_ms = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64);
    let fingerprint = if meta.len() <= MAX_HASHED {
        sha256_hex(&fs::read(path).ok()?)
    } else {
        format!("{}-{}", meta.len(), modified_ms.unwrap_or(0))
    };
    Some(RunFile { path: path.to_path_buf(), fingerprint, modified_ms })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_scripts_not_flags_or_installed_tools() {
        let dir = tempfile::tempdir().unwrap();
        let cwd = dir.path().canonicalize().unwrap();
        fs::write(cwd.join("check.js"), "console.log(1)").unwrap();
        fs::write(cwd.join("run.sh"), "echo hi").unwrap();
        let args = ["--inspect".to_string(), "check.js".to_string(), "missing.js".to_string()];

        let found = files_run(Path::new("/usr/bin/node"), &args, &cwd);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].path, cwd.join("check.js"));
        assert_eq!(found[0].fingerprint, sha256_hex(b"console.log(1)"));
        assert!(found[0].modified_ms.is_some());

        // A program inside the project is a script too.
        let found = files_run(&cwd.join("run.sh"), &[], &cwd);
        assert_eq!(found[0].path, cwd.join("run.sh"));

        // Editing the file changes what was approved.
        fs::write(cwd.join("check.js"), "console.log(process.env)").unwrap();
        let after = files_run(Path::new("/usr/bin/node"), &args, &cwd);
        assert_ne!(after[0].fingerprint, sha256_hex(b"console.log(1)"));
    }
}
