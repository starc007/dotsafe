//! Agent access: AI agents use the vault's secrets without being handed them.
//!
//! An agent talks to `dotsafe mcp`, which holds no keys and forwards requests
//! to the broker (in the desktop app) over a local socket. Agents can list
//! names, and ask to run a command with secrets in its environment. The user
//! approves each command; the broker starts it, and the agent gets back its
//! output with the values scrubbed. No request ever returns a value.
//! See docs/architecture/agents.md.

pub mod broker;
pub mod files;
pub mod grants;
#[cfg(unix)]
pub mod ipc;
pub mod policy;
pub mod protocol;
pub mod proxy;
pub mod redact;
pub mod run;

/// Where the broker listens: next to the machine's config, so
/// `DOTSAFE_CONFIG_DIR` keeps tests and portable setups separate.
pub fn socket_path() -> Option<std::path::PathBuf> {
    dotsafe_core::config::config_dir().ok().map(|dir| dir.join("agent.sock"))
}
