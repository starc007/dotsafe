//! Runs an approved command with secrets in its environment, and hands back
//! its output, scrubbed.
//!
//! The broker starts the process itself, so the agent never holds the
//! values: they exist only in the child's environment. The child gets a
//! small, fixed set of the broker's own variables (no `DYLD_*`, `LD_*`,
//! `NODE_OPTIONS` and the like from anyone), the agent's `PATH`, and the
//! secrets. Stdin is closed.

use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::protocol::RunOutput;
use crate::redact::Redactor;

pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(120);
pub const MAX_TIMEOUT: Duration = Duration::from_secs(15 * 60);
/// Output kept per stream for the agent; the end is kept when it's longer.
const MAX_OUTPUT: usize = 64 * 1024;

/// Variables passed through from the broker's own environment.
const PASSTHROUGH: &[&str] =
    &["HOME", "USER", "LOGNAME", "SHELL", "LANG", "LC_ALL", "LC_CTYPE", "TERM", "TMPDIR", "TZ"];
/// A sensible `PATH` when the client didn't send one.
const DEFAULT_PATH: &str = "/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin";

pub struct RunPlan {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    /// The client's `PATH`.
    pub path: Option<String>,
    /// (name, value) pairs injected as environment variables.
    pub secrets: Vec<(String, String)>,
    pub timeout: Duration,
}

pub fn run(plan: RunPlan) -> std::io::Result<RunOutput> {
    let mut cmd = Command::new(&plan.program);
    cmd.args(&plan.args).current_dir(&plan.cwd).env_clear();
    for name in PASSTHROUGH {
        if let Ok(value) = std::env::var(name) {
            cmd.env(name, value);
        }
    }
    cmd.env("PATH", plan.path.as_deref().unwrap_or(DEFAULT_PATH));
    cmd.envs(plan.secrets.iter().map(|(k, v)| (k, v)));
    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());

    let mut child = cmd.spawn()?;
    let stdout = child.stdout.take().map(collect);
    let stderr = child.stderr.take().map(collect);

    let started = Instant::now();
    let (status, timed_out) = loop {
        if let Some(status) = child.try_wait()? {
            break (Some(status), false);
        }
        if started.elapsed() >= plan.timeout {
            let _ = child.kill();
            let _ = child.wait();
            break (None, true);
        }
        thread::sleep(Duration::from_millis(25));
    };

    let redactor = Redactor::new(&plan.secrets);
    let (stdout, out_truncated) = stdout.map(|h| h.join().unwrap_or_default()).map(tail).unwrap_or_default();
    let (stderr, err_truncated) = stderr.map(|h| h.join().unwrap_or_default()).map(tail).unwrap_or_default();
    let (stdout, out_redactions) = redactor.redact(&String::from_utf8_lossy(&stdout));
    let (stderr, err_redactions) = redactor.redact(&String::from_utf8_lossy(&stderr));
    Ok(RunOutput {
        exit_code: status.and_then(|s| s.code()),
        timed_out,
        stdout,
        stderr,
        redactions: out_redactions + err_redactions,
        truncated: out_truncated || err_truncated,
    })
}

/// Reads a pipe to the end on its own thread, so a chatty process can't
/// block on a full pipe while we wait for it.
fn collect(mut pipe: impl Read + Send + 'static) -> thread::JoinHandle<Vec<u8>> {
    thread::spawn(move || {
        let mut buf = Vec::new();
        let mut chunk = [0u8; 8192];
        while let Ok(n) = pipe.read(&mut chunk) {
            if n == 0 {
                break;
            }
            buf.extend_from_slice(&chunk[..n]);
            // Keep memory bounded: twice the limit, then drop the front.
            if buf.len() > MAX_OUTPUT * 2 {
                buf.drain(..buf.len() - MAX_OUTPUT);
            }
        }
        buf
    })
}

fn tail(mut bytes: Vec<u8>) -> (Vec<u8>, bool) {
    if bytes.len() <= MAX_OUTPUT {
        return (bytes, false);
    }
    bytes.drain(..bytes.len() - MAX_OUTPUT);
    (bytes, true)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn plan(args: &[&str], secrets: &[(&str, &str)]) -> RunPlan {
        RunPlan {
            program: "/bin/sh".into(),
            args: args.iter().map(|s| s.to_string()).collect(),
            cwd: std::env::temp_dir(),
            path: None,
            secrets: secrets.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
            timeout: Duration::from_secs(10),
        }
    }

    #[test]
    fn secrets_reach_the_process_but_not_the_output() {
        let out = run(plan(&["-c", "echo \"key=$API_KEY\"; echo oops >&2; exit 3"], &[("API_KEY", "sk_live_abc123")]))
            .unwrap();
        assert_eq!(out.stdout, "key=[dotsafe:API_KEY]\n");
        assert_eq!(out.stderr, "oops\n");
        assert_eq!(out.exit_code, Some(3));
        assert_eq!(out.redactions, 1);
    }

    #[test]
    fn only_a_fixed_environment_is_passed_through() {
        // SAFETY: tests in this module don't read this variable concurrently.
        unsafe { std::env::set_var("DOTSAFE_TEST_LEAK", "parent-only") };
        let out = run(plan(&["-c", "echo \"[$DOTSAFE_TEST_LEAK][$PATH]\""], &[])).unwrap();
        assert!(out.stdout.starts_with("[]["), "{}", out.stdout);
    }

    #[test]
    fn slow_commands_time_out() {
        let mut p = plan(&["-c", "sleep 5"], &[]);
        p.timeout = Duration::from_millis(200);
        let out = run(p).unwrap();
        assert!(out.timed_out);
        assert_eq!(out.exit_code, None);
    }

    #[test]
    fn long_output_keeps_the_end() {
        let out = run(plan(&["-c", "yes line | head -n 20000; echo END"], &[])).unwrap();
        assert!(out.truncated);
        assert!(out.stdout.ends_with("END\n"));
        assert!(out.stdout.len() <= MAX_OUTPUT);
    }
}
