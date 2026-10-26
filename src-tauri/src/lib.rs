mod commands;
mod state;

use tauri::Manager;

use state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .manage(AppState::default())
        .setup(|app| {
            app.state::<AppState>().spawn_housekeeping(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::status,
            commands::folder_has_vault,
            commands::create_vault,
            commands::connect_vault,
            commands::unlock,
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
