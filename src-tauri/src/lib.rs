mod commands;
mod import;
mod licensing;
mod state;
mod sync;

use tauri::{DragDropEvent, Emitter, Manager, WindowEvent};

use state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .manage(AppState::default())
        .setup(|app| {
            let state = app.state::<AppState>();
            state.spawn_housekeeping(app.handle().clone());
            licensing::spawn_checker(state.inner().clone(), app.handle().clone());
            sync::spawn_syncer(state.inner().clone(), app.handle().clone());
            Ok(())
        })
        .on_window_event(|window, event| {
            // Files dropped on the window are kept in Rust for import; the UI
            // only learns their names, so paths never pass through the webview.
            if let WindowEvent::DragDrop(DragDropEvent::Drop { paths, .. }) = event {
                let state = window.state::<AppState>();
                let Ok(mut session) = state.0.lock() else { return };
                if session.vault.is_none() {
                    return;
                }
                session.import_files = paths.iter().filter(|p| p.is_file()).cloned().collect();
                let names: Vec<String> = session.import_files.iter().map(|p| import::file_name(p)).collect();
                drop(session);
                if !names.is_empty() {
                    let _ = window.emit("env-files-dropped", names);
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::status,
            commands::folder_has_vault,
            commands::create_vault,
            commands::connect_vault,
            commands::bucket_has_vault,
            commands::create_bucket_vault,
            commands::connect_bucket_vault,
            commands::sync_now,
            commands::unlock,
            commands::unlock_with_recovery,
            commands::change_password,
            commands::reset_recovery_key,
            commands::lock,
            commands::list_projects,
            commands::create_project,
            commands::delete_project,
            commands::create_environment,
            commands::delete_environment,
            commands::list_variables,
            commands::reveal_variable,
            commands::copy_variable,
            commands::set_variable,
            commands::delete_variable,
            commands::pick_env_files,
            commands::clear_import_files,
            commands::preview_import,
            commands::import_variables,
            commands::get_settings,
            commands::activate_license,
            commands::check_license,
            commands::remove_license,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
