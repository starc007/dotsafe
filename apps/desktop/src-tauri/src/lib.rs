mod broker;
mod cli;
mod commands;
mod devices;
mod import;
mod integrations;
mod licensing;
mod links;
mod presence;
mod reminders;
mod secrets;
mod state;
mod storage;
mod sync;
mod tray;
mod updates;
#[cfg(test)]
mod tests;

use tauri::{DragDropEvent, Emitter, Manager, WindowEvent};

use state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tray::shortcut_plugin())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(AppState::default())
        .manage(broker::Approvals::default())
        .manage(updates::PendingUpdate::default())
        .setup(|app| {
            let state = app.state::<AppState>();
            state.spawn_housekeeping(app.handle().clone());
            licensing::spawn_checker(app.handle().clone());
            sync::spawn_syncer(state.inner().clone(), app.handle().clone());
            sync::spawn_watcher(state.inner().clone(), app.handle().clone());
            reminders::spawn_reminders(state.inner().clone(), app.handle().clone());
            storage::spawn_backups(state.inner().clone(), app.handle().clone());
            updates::spawn_checker(app.handle().clone());
            tray::setup(app)?;
            let approvals = app.state::<broker::Approvals>().inner().clone();
            broker::spawn(state.inner().clone(), approvals, app.handle().clone());
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
            storage::webdav_has_vault,
            storage::create_webdav_vault,
            storage::connect_webdav_vault,
            storage::set_backup_dir,
            storage::back_up_now,
            storage::move_vault_to_folder,
            updates::check_for_update,
            updates::install_update,
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
            commands::all_variables,
            commands::vault_health,
            commands::reveal_variable,
            commands::copy_variable,
            commands::set_variable,
            commands::delete_variable,
            commands::hide_variable,
            commands::restore_variable,
            commands::set_environment_parent,
            commands::pick_env_files,
            commands::clear_import_files,
            commands::preview_import,
            commands::import_variables,
            secrets::list_secrets,
            secrets::get_secret,
            secrets::reveal_secret,
            secrets::generate_secret_value,
            secrets::copy_secret,
            secrets::share_preview,
            secrets::share_variable,
            secrets::link_variable,
            secrets::unlink_variable,
            secrets::stage_secret,
            secrets::promote_secret,
            secrets::cancel_staged_secret,
            secrets::roll_back_secret,
            secrets::retire_previous_secret,
            secrets::rotate_leaked_secret,
            secrets::restore_secret_version,
            secrets::update_secret_settings,
            secrets::delete_secret,
            integrations::list_services,
            integrations::list_connections,
            integrations::add_connection,
            integrations::remove_connection,
            integrations::replace_connection_token,
            integrations::secret_integrations,
            integrations::set_secret_provider,
            integrations::add_secret_target,
            integrations::remove_secret_target,
            integrations::rotate_with_provider,
            integrations::push_secret,
            integrations::revoke_previous_with_provider,
            integrations::open_provider_console,
            integrations::list_proxy_profiles,
            integrations::add_proxy_profile,
            integrations::remove_proxy_profile,
            integrations::rotation_queue,
            integrations::rotate_queued_with_providers,
            integrations::dismiss_rotation_queue,
            devices::unlock_with_touch_id,
            devices::set_touch_id_unlock,
            devices::list_devices,
            devices::create_enrollment_code,
            devices::copy_enrollment_code,
            devices::revoke_device,
            devices::connect_with_code,
            broker::answer_agent_request,
            broker::agent_activity,
            broker::revoke_agent_grants,
            broker::set_agent_approval_auth,
            commands::get_settings,
            commands::set_auto_lock,
            licensing::activate_license,
            licensing::check_license,
            licensing::remove_license,
            links::open_link,
            cli::cli_status,
            cli::install_cli,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
