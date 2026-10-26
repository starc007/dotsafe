//! Commands the UI can call. Decrypted values only cross into the webview
//! through `reveal_variable`; listing returns names only, and copying goes
//! straight from Rust to the clipboard.

use std::path::PathBuf;
use std::time::Instant;

use dotsafe_core::{Config, Environment, KdfParams, Project, Record, Vault, config, is_valid_var_name};
use serde::Serialize;
use tauri::{AppHandle, State};
use tauri_plugin_clipboard_manager::ClipboardExt;
use uuid::Uuid;
use zeroize::Zeroizing;

use tauri_plugin_dialog::DialogExt;

use crate::import::{self, Change, ImportResult, ImportSource, Preview, PreviewEntry};
use crate::state::{AppState, CmdError, CmdResult, hash_text};

const MIN_PASSWORD_LEN: usize = 8;
const DEFAULT_ENV: &str = "development";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    configured: bool,
    unlocked: bool,
    vault_path: Option<String>,
    default_vault_path: Option<String>,
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
    if password.chars().count() < MIN_PASSWORD_LEN {
        return Err(CmdError::other(format!("Use at least {MIN_PASSWORD_LEN} characters.")));
    }
    state
        .run(move |s| {
            let mut vault = Vault::create(&path, &password, KdfParams::default())?;
            let recovery = vault.reset_recovery_key()?.to_display_string();
            let config = Config { vault_path: path, device_id: Config::device_id() };
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
            let config = Config { vault_path: path, device_id: Config::device_id() };
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
            Ok(())
        })
        .await
}

#[tauri::command]
pub async fn lock(state: State<'_, AppState>) -> CmdResult<()> {
    state
        .run(|s| {
            s.vault = None;
            s.import_files.clear();
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
