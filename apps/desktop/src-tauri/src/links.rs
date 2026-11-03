//! Opening web pages in the browser. The webview only ever names a page;
//! every address lives here or in the integrations registry, so a compromised
//! webview can't send the user anywhere else.

use tauri::{AppHandle, Runtime};

use crate::state::{CmdError, CmdResult};

/// Opens a page on dotsafe.dev by name.
#[tauri::command]
pub async fn open_link<R: Runtime>(app: AppHandle<R>, link: String) -> CmdResult<()> {
    let url = match link.as_str() {
        "pricing" => "https://dotsafe.dev/pricing",
        "docs/agents" => "https://dotsafe.dev/docs/agents",
        "docs/install" => "https://dotsafe.dev/docs/install",
        _ => return Err(CmdError::other("Unknown link.")),
    };
    open(&app, url)
}

/// Opens a fixed address in the default browser. Test builds open nothing.
pub fn open<R: Runtime>(app: &AppHandle<R>, url: &str) -> CmdResult<()> {
    #[cfg(not(test))]
    {
        use tauri_plugin_opener::OpenerExt;
        app.opener().open_url(url, None::<&str>).map_err(|e| CmdError::other(e.to_string()))?;
    }
    #[cfg(test)]
    let _ = (app, url);
    Ok(())
}
