//! Messages between `dotsafe mcp` (or any other client) and the broker: one
//! JSON request per connection, one JSON response back, each on one line.
//!
//! Nothing here ever carries a secret value to the client. Values only leave
//! the broker as environment variables of a process it starts itself.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "method", content = "params", rename_all = "snake_case")]
pub enum Request {
    ListProjects,
    ListVariables(Target),
    Run(RunRequest),
    Generate(GenerateRequest),
}

/// Adds a variable with a random value the agent never sees.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GenerateRequest {
    pub client: String,
    /// The variable to create. Must not exist yet.
    pub name: String,
    #[serde(default)]
    pub target: Target,
    /// `random` (default), `hex`, `base64` or `uuid`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// Characters for `random`, bytes for `hex` and `base64`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub length: Option<u32>,
}

/// Which environment a request is about: named outright, or found from the
/// `.dotsafe.json` link in `cwd` (and its parents), like the CLI does.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Target {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunRequest {
    /// Who's asking, from the MCP client's own description. Shown to the
    /// user, never trusted for decisions.
    pub client: String,
    /// The program and its arguments. Run directly, never through a shell.
    pub command: Vec<String>,
    pub cwd: PathBuf,
    /// Which environment's variables. Its `cwd`, if unset, defaults to `cwd`.
    #[serde(default)]
    pub target: Target,
    /// Only these variables. `None` means all of the environment's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variables: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_secs: Option<u64>,
    /// The client's `PATH`, to find the program the way the agent would.
    /// GUI apps on macOS start with a minimal `PATH`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Send known API keys through the secret proxy instead of injecting
    /// them. On by default.
    #[serde(default = "yes")]
    pub proxy: bool,
}

fn yes() -> bool {
    true
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum Response {
    Projects(Vec<ProjectInfo>),
    Variables(VariablesInfo),
    Ran(RunOutput),
    Generated(Generated),
    Error(ErrorInfo),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Generated {
    pub project: String,
    pub environment: String,
    pub name: String,
    /// What was generated, for example "48 random characters".
    pub description: String,
}

impl Response {
    pub fn error(code: ErrorCode, message: impl Into<String>) -> Response {
        Response::Error(ErrorInfo { code, message: message.into() })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectInfo {
    pub name: String,
    pub environments: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VariablesInfo {
    pub project: String,
    pub environment: String,
    pub variables: Vec<VariableInfo>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VariableInfo {
    pub name: String,
    /// The shared secret it's linked to, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shared: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunOutput {
    /// `None` if the process was killed (timeout) or ended by a signal.
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    pub stdout: String,
    pub stderr: String,
    /// How many secret values were scrubbed from the output.
    pub redactions: usize,
    /// Output was longer than the limit; only the end is included.
    pub truncated: bool,
    /// Variables that went through the secret proxy: the process never had them.
    #[serde(default)]
    pub proxied: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorInfo {
    pub code: ErrorCode,
    pub message: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    /// The broker isn't running (no desktop app), or isn't supported here.
    Unavailable,
    /// The vault is locked; the user has to unlock it.
    Locked,
    /// The user said no, or didn't answer in time.
    Denied,
    /// The command is never allowed for agents.
    Refused,
    NotFound,
    Invalid,
    Internal,
}
