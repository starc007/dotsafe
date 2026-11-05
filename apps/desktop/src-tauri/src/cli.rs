//! The dotsafe CLI ships inside the app, as `Contents/MacOS/dotsafe` (added by
//! scripts/release.sh), so it's signed and notarized with it. On Linux the
//! .deb puts it in /usr/bin instead, and there's nothing to install here. Installing it
//! links `/usr/local/bin/dotsafe` to that copy: no separate download, it
//! updates with the app, and agents can run `dotsafe mcp`. Debug builds don't
//! bundle it, so they report it as unavailable.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::state::{CmdError, CmdResult};

/// Where the CLI is linked: on the default PATH of macOS shells.
#[cfg(target_os = "macos")]
const LINK: &str = "/usr/local/bin/dotsafe";
/// On Linux the .deb installs the CLI here, next to the app, so it shows as
/// installed; the AppImage doesn't carry it (its files move on every launch).
#[cfg(not(target_os = "macos"))]
const LINK: &str = "/usr/bin/dotsafe";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CliStatus {
    /// This build carries the CLI (release builds on macOS).
    available: bool,
    /// The link points at this app's copy.
    installed: bool,
    /// Something else is at the link path: an older manual install, or Homebrew.
    other_install: bool,
    path: &'static str,
}

/// The CLI next to the app's own executable, if this build has one. Never in
/// debug builds, where `target/debug/dotsafe` sits next to the app and isn't
/// one to link into /usr/local/bin.
fn bundled() -> Option<PathBuf> {
    if cfg!(debug_assertions) {
        return None;
    }
    let path = std::env::current_exe().ok()?.parent()?.join("dotsafe");
    path.is_file().then_some(path)
}

fn status() -> CliStatus {
    let bundled = bundled();
    let linked = std::fs::canonicalize(LINK).ok();
    let exists = Path::new(LINK).symlink_metadata().is_ok();
    let installed = match (&bundled, &linked) {
        (Some(b), Some(l)) => std::fs::canonicalize(b).is_ok_and(|b| &b == l),
        _ => false,
    };
    CliStatus { available: cfg!(target_os = "macos") && bundled.is_some(), installed, other_install: exists && !installed, path: LINK }
}

#[tauri::command]
pub async fn cli_status() -> CliStatus {
    status()
}

/// Links the bundled CLI into /usr/local/bin. macOS asks for an admin password
/// there, so this goes through `osascript … with administrator privileges`.
/// Returns false if the user cancels the password prompt.
#[tauri::command]
pub async fn install_cli() -> CmdResult<bool> {
    let bundled = bundled().ok_or_else(|| CmdError::other("This build doesn't include the command-line tool."))?;
    if !cfg!(target_os = "macos") {
        return Err(CmdError::other("Installing the command-line tool is only available on macOS."));
    }
    tauri::async_runtime::spawn_blocking(move || link_with_admin(&bundled))
        .await
        .map_err(|e| CmdError::other(e.to_string()))?
}

fn link_with_admin(bundled: &Path) -> CmdResult<bool> {
    let command = format!("mkdir -p /usr/local/bin && ln -sf {} {LINK}", shell_quote(&bundled.to_string_lossy()));
    let script = format!(
        "do shell script \"{}\" with administrator privileges with prompt \"dotsafe wants to install its command-line tool.\"",
        applescript_escape(&command)
    );
    let out = std::process::Command::new("/usr/bin/osascript")
        .args(["-e", &script])
        .output()
        .map_err(|e| CmdError::other(format!("Couldn't ask for permission: {e}")))?;
    if out.status.success() {
        return Ok(true);
    }
    let stderr = String::from_utf8_lossy(&out.stderr);
    // -128 is "User canceled" from the password prompt.
    if stderr.contains("-128") {
        return Ok(false);
    }
    Err(CmdError::other(format!("Couldn't install the command-line tool: {}", stderr.trim())))
}

/// Single-quotes a path for /bin/sh.
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// Escapes text for an AppleScript string literal.
fn applescript_escape(s: &str) -> String {
    s.replace('\\', r"\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_are_quoted_for_the_shell_and_applescript() {
        let quoted = shell_quote("/Applications/it's \"dotsafe\".app/Contents/MacOS/dotsafe");
        assert_eq!(quoted, r#"'/Applications/it'\''s "dotsafe".app/Contents/MacOS/dotsafe'"#);
        assert_eq!(applescript_escape(&quoted), r#"'/Applications/it'\\''s \"dotsafe\".app/Contents/MacOS/dotsafe'"#);
    }
}
