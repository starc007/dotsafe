//! The menu bar icon and the global quick-copy shortcut: find and copy a
//! variable from anywhere, without switching to dotsafe first.

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{App, AppHandle, Emitter, Manager, Runtime};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

use crate::state::AppState;

/// ⌥⌘K on macOS, Ctrl+Alt+K elsewhere.
pub const QUICK_COPY: &str = "CmdOrCtrl+Alt+K";

/// The global shortcut plugin, wired to open quick copy.
pub fn shortcut_plugin<R: Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri_plugin_global_shortcut::Builder::new()
        .with_handler(|app, _shortcut, event| {
            if event.state() == ShortcutState::Pressed {
                open_quick_copy(app);
            }
        })
        .build()
}

/// Adds the menu bar icon and registers the shortcut. Neither is essential:
/// if the shortcut is taken by another app, dotsafe works without it.
pub fn setup<R: Runtime>(app: &App<R>) -> tauri::Result<()> {
    let find = MenuItem::with_id(app, "find", "Find and copy", true, Some(QUICK_COPY))?;
    let open = MenuItem::with_id(app, "open", "Open dotsafe", true, None::<&str>)?;
    let lock = MenuItem::with_id(app, "lock", "Lock", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit = PredefinedMenuItem::quit(app, Some("Quit dotsafe"))?;
    let menu = Menu::with_items(app, &[&find, &open, &lock, &separator, &quit])?;
    let mut tray = TrayIconBuilder::with_id("main").tooltip("dotsafe").menu(&menu).on_menu_event(|app, event| {
        match event.id().as_ref() {
            "find" => open_quick_copy(app),
            "open" => show(app),
            "lock" => {
                if let Ok(mut session) = app.state::<AppState>().0.lock() {
                    session.lock();
                }
                let _ = app.emit("vault-locked", ());
            }
            _ => {}
        }
    });
    // macOS gets the one-colour mark as a template, so it follows the menu
    // bar's light or dark look. Elsewhere a black template would vanish on a
    // dark taskbar, so the app icon stays.
    if cfg!(target_os = "macos") {
        tray = tray.icon(tauri::include_image!("icons/tray.png")).icon_as_template(true);
    } else if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    let _ = app.global_shortcut().register(QUICK_COPY);
    Ok(())
}

fn show<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// Brings dotsafe forward and opens the palette. While locked, it shows the
/// lock screen instead.
fn open_quick_copy<R: Runtime>(app: &AppHandle<R>) {
    show(app);
    let _ = app.emit("open-quick-copy", ());
}
