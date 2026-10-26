//! What agents may run at all, before the user is even asked.
//!
//! Commands are run directly, never through a shell, and some are refused
//! outright: shells and interpreters given code inline, programs that print
//! the environment, and wrappers that would hide the real program. This is a
//! speed bump, not a boundary: an approved command can still be made to leak
//! what it holds (see docs/architecture/agents.md). It stops the obvious
//! one-liners and keeps approval prompts honest about what runs.

use std::path::{Path, PathBuf};

/// Programs that print or expose the environment, or run another command
/// named in their arguments.
const NEVER: &[&str] = &[
    "env", "printenv", "set", "export", "declare", "compgen", // print the environment
    "awk", "gawk", "mawk", "nawk", "jq", // read it inline (ENVIRON, $ENV)
    "nice", "nohup", "timeout", "time", "stdbuf", "xargs", "sudo", "doas", "su", "exec", "command", "builtin", "eval",
    "watch", "script", "unbuffer", // run another program
];

/// Shells: refused when given a command string.
const SHELLS: &[&str] =
    &["sh", "bash", "zsh", "fish", "dash", "ksh", "mksh", "csh", "tcsh", "nu", "xonsh", "elvish", "pwsh", "powershell"];

/// Interpreters and the flags that make them run code from the command line.
const INLINE: &[(&str, &[&str])] = &[
    ("node", &["-e", "--eval", "-p", "--print", "-r", "--require", "--import", "-i", "--interactive"]),
    ("nodejs", &["-e", "--eval", "-p", "--print", "-r", "--require", "--import"]),
    ("bun", &["-e", "--eval", "-p", "--print", "-r", "--preload"]),
    ("deno", &["eval", "repl"]),
    ("python", &["-c", "-i"]),
    ("python3", &["-c", "-i"]),
    ("python2", &["-c", "-i"]),
    ("pypy", &["-c"]),
    ("pypy3", &["-c"]),
    ("ruby", &["-e"]),
    ("irb", &[]),
    ("perl", &["-e", "-E"]),
    ("php", &["-r", "-a"]),
    ("lua", &["-e", "-i"]),
    ("luajit", &["-e"]),
    ("osascript", &["-e"]),
    ("Rscript", &["-e"]),
    ("R", &["-e"]),
    ("julia", &["-e", "-E"]),
    ("tclsh", &[]),
    ("cmd", &["/c", "/k"]),
];

/// Why `command` may never run for an agent, or `None` if it may be asked for.
pub fn refusal(command: &[String]) -> Option<String> {
    let Some(program) = command.first() else { return Some("No command given.".into()) };
    let name = program_name(program);
    let args = &command[1..];

    if NEVER.contains(&name.as_str()) {
        return Some(format!(
            "{name} isn't allowed for agents: it can print the environment or run another program. Run the program directly."
        ));
    }
    if SHELLS.contains(&name.as_str()) {
        let inline = args.iter().any(|a| {
            let a = a.to_lowercase();
            (a.starts_with('-') && !a.starts_with("--") && a.contains('c'))
                || matches!(a.as_str(), "-command" | "-encodedcommand" | "--command")
        });
        if inline || args.is_empty() {
            return Some(format!(
                "{name} with a command string isn't allowed for agents. Run the program directly, or a script file."
            ));
        }
    }
    if let Some((_, flags)) = INLINE.iter().find(|(n, _)| *n == name) {
        if flags.is_empty() && args.is_empty() {
            return Some(format!("An interactive {name} session isn't allowed for agents."));
        }
        let inline = args.iter().any(|a| {
            let a = a.to_lowercase();
            flags.iter().any(|f| a == *f || (f.starts_with("--") && a.starts_with(&format!("{f}="))))
                // Combined short flags like `-ce`.
                || (a.starts_with('-') && !a.starts_with("--") && a.len() > 2
                    && flags.iter().any(|f| f.len() == 2 && f.starts_with('-') && a.contains(&f[1..])))
        });
        if inline || args.is_empty() {
            return Some(format!(
                "{name} running code from the command line isn't allowed for agents. Put it in a file and run that."
            ));
        }
    }
    None
}

/// The bare program name: no directories, no `.exe`.
pub fn program_name(program: &str) -> String {
    let base = Path::new(program).file_name().and_then(|n| n.to_str()).unwrap_or(program);
    base.strip_suffix(".exe").unwrap_or(base).to_string()
}

/// Finds `program` the way a shell would: as a path if it has a separator
/// (relative to `cwd`), otherwise on `path`.
pub fn resolve_program(program: &str, path: Option<&str>, cwd: &Path) -> Option<PathBuf> {
    if program.contains('/') || program.contains('\\') {
        let candidate = cwd.join(program);
        return is_executable(&candidate).then(|| candidate.canonicalize().unwrap_or(candidate));
    }
    let path = path.map(str::to_owned).or_else(|| std::env::var("PATH").ok())?;
    std::env::split_paths(&path).map(|dir| dir.join(program)).find(|c| is_executable(c))
}

fn is_executable(path: &Path) -> bool {
    let Ok(meta) = path.metadata() else { return false };
    if !meta.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cmd(s: &str) -> Vec<String> {
        s.split_whitespace().map(String::from).collect()
    }

    #[test]
    fn ordinary_commands_may_be_asked_for() {
        for ok in [
            "bun test",
            "npm run dev",
            "cargo test",
            "python manage.py migrate",
            "node server.js",
            "bash ./deploy.sh",
            "go run .",
        ] {
            assert_eq!(refusal(&cmd(ok)), None, "{ok}");
        }
    }

    #[test]
    fn environment_dumps_and_inline_code_are_refused() {
        for bad in [
            "env",
            "printenv STRIPE_KEY",
            "/usr/bin/env node x.js",
            "sh -c echo",
            "bash -lc make",
            "zsh",
            "node -e console.log(process.env)",
            "node --eval=1",
            "node -pe 1",
            "python3 -c print(1)",
            "ruby -e p",
            "perl -E say",
            "awk BEGIN{print}",
            "jq -n env",
            "xargs echo",
            "timeout 5 node x.js",
            "deno eval 1",
            "osascript -e beep",
            "pwsh -Command ls",
            "irb",
        ] {
            assert!(refusal(&cmd(bad)).is_some(), "{bad} should be refused");
        }
        assert!(refusal(&[]).is_some());
    }

    #[test]
    fn programs_are_found_on_the_given_path() {
        let dir = tempfile::tempdir().unwrap();
        let tool = dir.path().join("mytool");
        std::fs::write(&tool, "#!/bin/sh\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let path = dir.path().to_str().unwrap();
        assert_eq!(resolve_program("mytool", Some(path), Path::new("/")), Some(tool.clone()));
        assert_eq!(resolve_program("missing-tool", Some(path), Path::new("/")), None);
        assert!(resolve_program("./mytool", None, dir.path()).is_some());
    }
}
