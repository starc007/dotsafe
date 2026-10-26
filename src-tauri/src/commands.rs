//! Commands the UI can call. Decrypted values only cross into the webview
//! through `reveal_variable`; listing returns names only, and copying goes
//! straight from Rust to the clipboard.

use std::path::PathBuf;
use std::time::Instant;

use dotsafe_core::license::unix_now;
use dotsafe_core::{
    Config, Environment, KdfParams, License, LicenseStatus, Project, Record, RecoveryKey, Vault, config, is_valid_var_name,
};
use serde::Serialize;
use tauri::{AppHandle, State};
use tauri_plugin_clipboard_manager::ClipboardExt;
use uuid::Uuid;
use zeroize::Zeroizing;

use tauri_plugin_dialog::DialogExt;

use crate::import::{self, Change, ImportResult, ImportSource, Preview, PreviewEntry};
use crate::licensing::{self, ApiError};
use crate::state::{AppState, CLIPBOARD_CLEAR, CmdError, CmdResult, ErrorKind, IDLE_LOCK, hash_text};

const MIN_PASSWORD_LEN: usize = 8;
const DEFAULT_ENV: &str = "development";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    configured: bool,
    unlocked: bool,
    vault_path: Option<String>,
    default_vault_path: Option<String>,
    must_change_password: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectView {
    id: Uuid,
    name: String,
    environments: Vec<EnvironmentView>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentView {
    id: Uuid,
    name: String,
    var_count: usize,
}

impl From<&Environment> for EnvironmentView {
    fn from(e: &Environment) -> Self {
        EnvironmentView { id: e.id, name: e.name.clone(), var_count: e.vars().count() }
    }
}

#[tauri::command]
pub async fn status(state: State<'_, AppState>) -> CmdResult<Status> {
    state
        .run(|s| {
            Ok(Status {
                configured: s.config.is_some(),
                unlocked: s.vault.is_some(),
                vault_path: s.config.as_ref().map(|c| c.vault_path.display().to_string()),
                default_vault_path: config::default_vault_path().ok().map(|p| p.display().to_string()),
                must_change_password: s.must_change_password,
            })
        })
        .await
}

#[tauri::command]
pub async fn folder_has_vault(path: String) -> bool {
    PathBuf::from(path).join("vault.json").is_file()
}

/// Creates a vault and returns its recovery key, shown to the user once.
#[tauri::command]
pub async fn create_vault(state: State<'_, AppState>, path: String, password: String) -> CmdResult<String> {
    let password = Zeroizing::new(password);
    let path = absolute(path)?;
    check_new_password(&password).map_err(CmdError::other)?;
    state
        .run(move |s| {
            let mut vault = Vault::create(&path, &password, KdfParams::default())?;
            let recovery = vault.reset_recovery_key()?.to_display_string();
            let config = Config::for_vault(path);
            config.save()?;
            s.config = Some(config);
            s.vault = Some(vault);
            Ok(recovery)
        })
        .await
}

/// Connects this machine to an existing vault (e.g. one synced from another Mac).
#[tauri::command]
pub async fn connect_vault(state: State<'_, AppState>, path: String, password: String) -> CmdResult<()> {
    let password = Zeroizing::new(password);
    let path = absolute(path)?;
    state
        .run(move |s| {
            let vault = Vault::unlock(&path, &password)?;
            let config = Config::for_vault(path);
            config.save()?;
            s.config = Some(config);
            s.vault = Some(vault);
            Ok(())
        })
        .await
}

#[tauri::command]
pub async fn unlock(state: State<'_, AppState>, password: String) -> CmdResult<()> {
    let password = Zeroizing::new(password);
    state
        .run(move |s| {
            let vault = Vault::unlock(&s.config()?.vault_path, &password)?;
            s.vault = Some(vault);
            s.must_change_password = false;
            Ok(())
        })
        .await
}

/// Unlocks with the recovery key when the password is forgotten. The user
/// must then choose a new password (`must_change_password`).
#[tauri::command]
pub async fn unlock_with_recovery(state: State<'_, AppState>, recovery_key: String) -> CmdResult<()> {
    let recovery_key = Zeroizing::new(recovery_key);
    let key = RecoveryKey::parse(&recovery_key).map_err(|_| {
        CmdError::other("That isn't a recovery key. It's 13 groups of 4 letters and numbers.")
    })?;
    state
        .run(move |s| {
            let vault = Vault::unlock_with_recovery(&s.config()?.vault_path, &key).map_err(|e| match e {
                dotsafe_core::Error::InvalidCredentials => CmdError {
                    kind: ErrorKind::InvalidPassword,
                    message: "That recovery key doesn't match this vault.".into(),
                },
                e => e.into(),
            })?;
            s.vault = Some(vault);
            s.must_change_password = true;
            Ok(())
        })
        .await
}

/// Sets a new master password. Needs the current one, unless the vault was
/// just unlocked with the recovery key (the password is forgotten).
#[tauri::command]
pub async fn change_password(
    state: State<'_, AppState>,
    current_password: Option<String>,
    new_password: String,
) -> CmdResult<()> {
    let current = current_password.map(Zeroizing::new);
    let new_password = Zeroizing::new(new_password);
    check_new_password(&new_password).map_err(CmdError::other)?;
    state
        .run(move |s| {
            if !s.must_change_password {
                verify_password(s, current.as_deref().map(String::as_str))?;
            }
            let vault = s.vault.as_mut().ok_or(CmdError { kind: ErrorKind::Locked, message: "The vault is locked.".into() })?;
            vault.change_password(&new_password, KdfParams::default())?;
            s.must_change_password = false;
            Ok(())
        })
        .await
}

/// Replaces the recovery key after confirming the password, and returns the
/// new key to show once. The old key stops working.
#[tauri::command]
pub async fn reset_recovery_key(state: State<'_, AppState>, password: String) -> CmdResult<String> {
    let password = Zeroizing::new(password);
    state
        .run(move |s| {
            verify_password(s, Some(&password))?;
            let vault = s.vault.as_mut().ok_or(CmdError { kind: ErrorKind::Locked, message: "The vault is locked.".into() })?;
            Ok(vault.reset_recovery_key()?.to_display_string())
        })
        .await
}

/// Re-checks the master password before a sensitive change, so an unlocked
/// Mac left unattended can't be used to take over the vault.
fn verify_password(s: &crate::state::Session, password: Option<&str>) -> CmdResult<()> {
    let password = password.filter(|p| !p.is_empty()).ok_or_else(|| CmdError::other("Enter your current password."))?;
    match Vault::unlock(&s.config()?.vault_path, password) {
        Ok(_) => Ok(()),
        Err(dotsafe_core::Error::InvalidCredentials) => Err(CmdError {
            kind: ErrorKind::InvalidPassword,
            message: "Your current password is wrong.".into(),
        }),
        Err(e) => Err(e.into()),
    }
}

#[tauri::command]
pub async fn lock(state: State<'_, AppState>) -> CmdResult<()> {
    state
        .run(|s| {
            s.lock();
            Ok(())
        })
        .await
}

#[tauri::command]
pub async fn list_projects(state: State<'_, AppState>) -> CmdResult<Vec<ProjectView>> {
    state
        .run(|s| {
            let records = s.vault()?.list()?;
            let mut projects: Vec<ProjectView> = records
                .iter()
                .filter_map(|r| match r {
                    Record::Project(p) if !p.deleted => {
                        Some(ProjectView { id: p.id, name: p.name.clone(), environments: Vec::new() })
                    }
                    _ => None,
                })
                .collect();
            for r in &records {
                if let Record::Environment(e) = r
                    && !e.deleted
                    && let Some(p) = projects.iter_mut().find(|p| p.id == e.project_id)
                {
                    p.environments.push(e.into());
                }
            }
            projects.sort_by_key(|p| p.name.to_lowercase());
            for p in &mut projects {
                p.environments.sort_by_key(|e| (env_rank(&e.name), e.name.to_lowercase()));
            }
            Ok(projects)
        })
        .await
}

/// Creates a project with a `development` environment, ready to fill in.
#[tauri::command]
pub async fn create_project(state: State<'_, AppState>, name: String) -> CmdResult<ProjectView> {
    let name = required(name, "project")?;
    state
        .run(move |s| {
            let stamp = s.stamp()?;
            let vault = s.vault()?;
            let taken = vault.list()?.iter().any(|r| {
                matches!(r, Record::Project(p) if !p.deleted && p.name.eq_ignore_ascii_case(&name))
            });
            if taken {
                return Err(CmdError::other(format!("A project named {name} already exists.")));
            }
            let project = Project::new(&name, stamp);
            let env = Environment::new(project.id, DEFAULT_ENV, stamp);
            vault.put(Record::Project(project.clone()))?;
            vault.put(Record::Environment(env.clone()))?;
            Ok(ProjectView { id: project.id, name: project.name, environments: vec![(&env).into()] })
        })
        .await
}

#[tauri::command]
pub async fn delete_project(state: State<'_, AppState>, project_id: Uuid) -> CmdResult<()> {
    state
        .run(move |s| {
            let stamp = s.stamp()?;
            let mut project = s.project(project_id)?;
            let vault = s.vault()?;
            // Tombstone the environments too, so they don't linger in storage
            // as orphans that other devices keep merging.
            for r in vault.list()? {
                if let Record::Environment(mut e) = r
                    && e.project_id == project_id
                    && !e.deleted
                {
                    e.deleted = true;
                    e.updated = stamp;
                    vault.put(Record::Environment(e))?;
                }
            }
            project.deleted = true;
            project.updated = stamp;
            vault.put(Record::Project(project))?;
            Ok(())
        })
        .await
}

#[tauri::command]
pub async fn create_environment(
    state: State<'_, AppState>,
    project_id: Uuid,
    name: String,
) -> CmdResult<EnvironmentView> {
    let name = required(name, "environment")?;
    state
        .run(move |s| {
            let stamp = s.stamp()?;
            s.project(project_id)?;
            let vault = s.vault()?;
            let taken = vault.list()?.iter().any(|r| {
                matches!(r, Record::Environment(e)
                    if !e.deleted && e.project_id == project_id && e.name.eq_ignore_ascii_case(&name))
            });
            if taken {
                return Err(CmdError::other(format!("This project already has a {name} environment.")));
            }
            let mut env = Environment::new(project_id, &name, stamp);
            // The derived id is taken by a deleted or renamed environment.
            if vault.get(env.id)?.is_some() {
                env.id = Uuid::new_v4();
            }
            vault.put(Record::Environment(env.clone()))?;
            Ok((&env).into())
        })
        .await
}

#[tauri::command]
pub async fn delete_environment(state: State<'_, AppState>, environment_id: Uuid) -> CmdResult<()> {
    state
        .run(move |s| {
            let mut env = s.environment(environment_id)?;
            env.deleted = true;
            env.updated = s.stamp()?;
            s.vault()?.put(Record::Environment(env))?;
            Ok(())
        })
        .await
}

/// Variable names in an environment. Values stay in Rust until revealed.
#[tauri::command]
pub async fn list_variables(state: State<'_, AppState>, environment_id: Uuid) -> CmdResult<Vec<String>> {
    state
        .run(move |s| Ok(s.environment(environment_id)?.vars().map(|(k, _)| k.to_string()).collect()))
        .await
}

#[tauri::command]
pub async fn reveal_variable(state: State<'_, AppState>, environment_id: Uuid, key: String) -> CmdResult<String> {
    state
        .run(move |s| {
            let env = s.environment(environment_id)?;
            env.get(&key).map(str::to_string).ok_or_else(|| CmdError::other(format!("{key} isn't set.")))
        })
        .await
}

/// Copies a value to the clipboard and clears it again after 30 seconds,
/// unless something else was copied in the meantime.
#[tauri::command]
pub async fn copy_variable(
    app: AppHandle,
    state: State<'_, AppState>,
    environment_id: Uuid,
    key: String,
) -> CmdResult<()> {
    state
        .run(move |s| {
            let env = s.environment(environment_id)?;
            let value = env.get(&key).ok_or_else(|| CmdError::other(format!("{key} isn't set.")))?;
            app.clipboard().write_text(value).map_err(|e| CmdError::other(e.to_string()))?;
            s.clipboard = Some((hash_text(value), Instant::now()));
            Ok(())
        })
        .await
}

/// Adds or edits a variable. `previous_key` renames an existing one.
#[tauri::command]
pub async fn set_variable(
    state: State<'_, AppState>,
    environment_id: Uuid,
    key: String,
    value: String,
    previous_key: Option<String>,
) -> CmdResult<()> {
    let value = Zeroizing::new(value);
    let key = key.trim().to_string();
    if !is_valid_var_name(&key) {
        return Err(CmdError::other(
            "Use letters, numbers and underscores, not starting with a number.",
        ));
    }
    state
        .run(move |s| {
            let stamp = s.stamp()?;
            let mut env = s.environment(environment_id)?;
            // Only an in-place edit may write over an existing name; adds and
            // renames must not silently replace another variable.
            let editing_in_place = previous_key.as_deref() == Some(key.as_str());
            if !editing_in_place && env.get(&key).is_some() {
                return Err(CmdError::other(format!("{key} already exists.")));
            }
            if let Some(old) = previous_key.filter(|p| *p != key) {
                env.remove(old, stamp);
            }
            env.set(&key, value.as_str(), stamp);
            s.vault()?.put(Record::Environment(env))?;
            Ok(())
        })
        .await
}

#[tauri::command]
pub async fn delete_variable(state: State<'_, AppState>, environment_id: Uuid, key: String) -> CmdResult<()> {
    state
        .run(move |s| {
            let stamp = s.stamp()?;
            let mut env = s.environment(environment_id)?;
            env.remove(&key, stamp);
            s.vault()?.put(Record::Environment(env))?;
            Ok(())
        })
        .await
}

/// Opens the native picker for `.env` files and remembers the choice for the
/// next preview or import. Returns the chosen file names.
#[tauri::command]
pub async fn pick_env_files(app: AppHandle, state: State<'_, AppState>) -> CmdResult<Vec<String>> {
    state.run(|s| s.vault().map(|_| ())).await?;
    // The picker runs outside the session lock so other commands keep working.
    let picked = tauri::async_runtime::spawn_blocking(move || {
        app.dialog().file().set_title("Import .env files").blocking_pick_files()
    })
    .await
    .map_err(|e| CmdError::other(e.to_string()))?;
    let paths: Vec<PathBuf> = picked
        .unwrap_or_default()
        .into_iter()
        .filter_map(|p| p.into_path().ok())
        .collect();
    state
        .run(move |s| {
            s.import_files = paths;
            Ok(s.import_files.iter().map(|p| import::file_name(p)).collect())
        })
        .await
}

#[tauri::command]
pub async fn clear_import_files(state: State<'_, AppState>) -> CmdResult<()> {
    state
        .run(|s| {
            s.import_files.clear();
            Ok(())
        })
        .await
}

/// What importing `source` into an environment would change. Names only.
#[tauri::command]
pub async fn preview_import(
    state: State<'_, AppState>,
    environment_id: Uuid,
    source: ImportSource,
) -> CmdResult<Preview> {
    state
        .run(move |s| {
            let env = s.environment(environment_id)?;
            let (parsed, errors) = import::collect(&source, &s.import_files);
            let entries = import::plan(&env, &parsed)
                .into_iter()
                .map(|(change, p)| PreviewEntry { key: p.key, change, file: p.file })
                .collect();
            Ok(Preview { entries, errors })
        })
        .await
}

/// Writes the variables from `source`. With `overwrite` off, existing values
/// are kept and only new variables are added.
#[tauri::command]
pub async fn import_variables(
    state: State<'_, AppState>,
    environment_id: Uuid,
    source: ImportSource,
    overwrite: bool,
) -> CmdResult<ImportResult> {
    state
        .run(move |s| {
            let stamp = s.stamp()?;
            let mut env = s.environment(environment_id)?;
            let (parsed, errors) = import::collect(&source, &s.import_files);
            if parsed.is_empty() {
                return Err(CmdError::other(
                    errors.into_iter().next().unwrap_or_else(|| "There's nothing to import.".into()),
                ));
            }
            let mut result = ImportResult::default();
            for (change, p) in import::plan(&env, &parsed) {
                match change {
                    Change::New => result.added += 1,
                    Change::Unchanged => {
                        result.unchanged += 1;
                        continue;
                    }
                    Change::Update if overwrite => result.updated += 1,
                    Change::Update => {
                        result.skipped += 1;
                        continue;
                    }
                }
                env.set(p.key, p.value, stamp);
            }
            if result.added + result.updated > 0 {
                s.vault()?.put(Record::Environment(env))?;
            }
            if matches!(source, ImportSource::Files) {
                s.import_files.clear();
            }
            Ok(result)
        })
        .await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    vault_path: Option<String>,
    license: Option<LicenseView>,
    version: &'static str,
    auto_lock_minutes: u64,
    clipboard_clear_seconds: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LicenseView {
    key: String,
    device_name: String,
    /// Milliseconds, for JavaScript dates.
    activated_at: u64,
    checked_at: u64,
    status: LicenseStatus,
}

impl From<&License> for LicenseView {
    fn from(l: &License) -> Self {
        LicenseView {
            key: l.key.clone(),
            device_name: l.device_name.clone(),
            activated_at: l.activated_at * 1000,
            checked_at: l.checked_at * 1000,
            status: l.status(unix_now()),
        }
    }
}

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> CmdResult<Settings> {
    state
        .run(|s| {
            Ok(Settings {
                vault_path: s.config.as_ref().map(|c| c.vault_path.display().to_string()),
                license: s.config.as_ref().and_then(|c| c.license.as_ref()).map(LicenseView::from),
                version: env!("CARGO_PKG_VERSION"),
                auto_lock_minutes: IDLE_LOCK.as_secs() / 60,
                clipboard_clear_seconds: CLIPBOARD_CLEAR.as_secs(),
            })
        })
        .await
}

/// Activates a license key for this machine with the license server and
/// saves the activation.
#[tauri::command]
pub async fn activate_license(state: State<'_, AppState>, key: String) -> CmdResult<LicenseView> {
    let key = normalize_license(&key).map_err(CmdError::other)?;
    state.run(|s| s.config().map(|_| ())).await?;
    let device_name = licensing::device_name();
    let instance_id = {
        let (key, device_name) = (key.clone(), device_name.clone());
        off_thread(move || licensing::activate(&key, &device_name)).await?
    };
    state
        .run(move |s| {
            let now = unix_now();
            let license = License { key, instance_id, device_name, activated_at: now, checked_at: now, revoked: false };
            let view = LicenseView::from(&license);
            let mut config = s.config()?.clone();
            config.license = Some(license);
            config.save()?;
            s.config = Some(config);
            Ok(view)
        })
        .await
}

/// Asks the license server about the saved license now. Errors with kind
/// `offline` if it can't be reached, leaving the license as it was.
#[tauri::command]
pub async fn check_license(state: State<'_, AppState>) -> CmdResult<Option<LicenseView>> {
    let Some(license) = state.run(|s| Ok(s.config()?.license.clone())).await? else { return Ok(None) };
    let valid = {
        let license = license.clone();
        off_thread(move || licensing::validate(&license)).await?
    };
    let inner = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || licensing::record_check(&inner, &license.instance_id, valid))
        .await
        .map_err(|e| CmdError::other(e.to_string()))??;
    state.run(|s| Ok(s.config()?.license.as_ref().map(LicenseView::from))).await
}

/// Deactivates the license on the server, freeing this device's slot, then
/// forgets it here. With `force`, forgets it even when the server can't be
/// reached (the slot then stays taken).
#[tauri::command]
pub async fn remove_license(state: State<'_, AppState>, force: bool) -> CmdResult<()> {
    let Some(license) = state.run(|s| Ok(s.config()?.license.clone())).await? else { return Ok(()) };
    match off_thread(move || licensing::deactivate(&license)).await {
        Ok(()) => {}
        Err(e) if force && matches!(e.kind, ErrorKind::Offline) => {}
        Err(e) => return Err(e),
    }
    state
        .run(|s| {
            let mut config = s.config()?.clone();
            config.license = None;
            config.save()?;
            s.config = Some(config);
            Ok(())
        })
        .await
}

/// Runs a license server request on a worker thread without holding the
/// session lock.
async fn off_thread<T: Send + 'static>(f: impl FnOnce() -> Result<T, ApiError> + Send + 'static) -> CmdResult<T> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| CmdError::other(e.to_string()))?.map_err(CmdError::from)
}

fn check_new_password(password: &str) -> Result<(), String> {
    if password.chars().count() < MIN_PASSWORD_LEN {
        return Err(format!("Use at least {MIN_PASSWORD_LEN} characters."));
    }
    Ok(())
}

/// Trims a pasted key and rejects anything that can't be a license key.
fn normalize_license(input: &str) -> Result<String, &'static str> {
    let key = input.trim();
    if key.is_empty() {
        return Err("Enter your license key.");
    }
    if key.chars().any(char::is_whitespace) || key.len() < 8 || key.len() > 128 {
        return Err("That doesn't look like a license key. Copy it again from your purchase email.");
    }
    Ok(key.to_string())
}

fn absolute(path: String) -> CmdResult<PathBuf> {
    let path = PathBuf::from(path.trim());
    if !path.is_absolute() {
        return Err(CmdError::other("Choose a folder."));
    }
    Ok(path)
}

fn required(name: String, what: &str) -> CmdResult<String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err(CmdError::other(format!("Give the {what} a name.")));
    }
    Ok(name)
}

/// Familiar environments first, in deploy order.
fn env_rank(name: &str) -> u8 {
    match name.to_lowercase().as_str() {
        "development" | "dev" | "local" => 0,
        "staging" | "preview" | "test" => 1,
        "production" | "prod" => 2,
        _ => 3,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_passwords_need_eight_characters() {
        assert!(check_new_password("1234567").is_err());
        assert!(check_new_password("12345678").is_ok());
        // Counted in characters, not bytes.
        assert!(check_new_password("pässwörd").is_ok());
    }

    #[test]
    fn license_keys_are_trimmed_and_shape_checked() {
        assert_eq!(normalize_license("  DOTSAFE-ABCD-1234-EFGH \n"), Ok("DOTSAFE-ABCD-1234-EFGH".into()));
        assert!(normalize_license("").is_err());
        assert!(normalize_license("short").is_err());
        assert!(normalize_license("has a space in it").is_err());
        assert!(normalize_license(&"x".repeat(129)).is_err());
    }
}
