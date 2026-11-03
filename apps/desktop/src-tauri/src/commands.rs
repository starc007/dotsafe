//! Commands the UI can call. Decrypted values only cross into the webview
//! through `reveal_variable`; listing returns names only, and copying goes
//! straight from Rust to the clipboard.

use std::path::PathBuf;
use std::sync::Arc;

use dotsafe_core::resolve::{Effective, Raw};
use dotsafe_core::{
    CachedStore, Config, Entry, Environment, FolderStore, KdfParams, License, Project, Record, RecoveryKey, Resolver,
    S3Location, Store, Vault, config, is_valid_var_name,
};
use dotsafe_stores::{S3Credentials, S3Store};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Runtime, State};
use uuid::Uuid;
use zeroize::Zeroizing;

use tauri_plugin_dialog::DialogExt;

use crate::import::{self, Change, ImportResult, ImportSource, Preview, PreviewEntry};
use crate::licensing::{self, LicenseView};
use crate::state::{AppState, CLIPBOARD_CLEAR, CmdError, CmdResult, ErrorKind};

const MIN_PASSWORD_LEN: usize = 8;
const DEFAULT_ENV: &str = "development";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    /// A license is activated on this machine. dotsafe is paid: without one,
    /// the UI asks for a key before anything else.
    licensed: bool,
    configured: bool,
    unlocked: bool,
    /// Where the vault lives, for display: a folder path or `s3://bucket/prefix`.
    vault_path: Option<String>,
    default_vault_path: Option<String>,
    must_change_password: bool,
    /// Offer Touch ID on the unlock screen.
    touch_id_unlock: bool,
    /// Touch ID is set up, but it's time to enter the password again.
    password_due: bool,
    /// This Mac can join a vault with a code instead of the password.
    can_join_with_code: bool,
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
    /// Its own variables (inherited ones aren't counted).
    var_count: usize,
    /// The environment it inherits from.
    parent: Option<Uuid>,
}

impl From<&Environment> for EnvironmentView {
    fn from(e: &Environment) -> Self {
        EnvironmentView { id: e.id, name: e.name.clone(), var_count: e.entries().count(), parent: e.parent }
    }
}

#[tauri::command]
pub async fn status(state: State<'_, AppState>) -> CmdResult<Status> {
    let licensed = License::load()?.is_some();
    state
        .run(move |s| {
            Ok(Status {
                licensed,
                configured: s.config.is_some(),
                unlocked: s.vault.is_some(),
                vault_path: s.config.as_ref().map(Config::describe_location),
                default_vault_path: config::default_vault_path().ok().map(|p| p.display().to_string()),
                must_change_password: s.must_change_password,
                touch_id_unlock: s.config.as_ref().is_some_and(crate::devices::touch_id_offered),
                password_due: s
                    .config
                    .as_ref()
                    .is_some_and(|c| c.device_unlock && crate::devices::password_due(c)),
                can_join_with_code: crate::presence::available(),
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
            let mut vault = Vault::create(FolderStore::new(&path), &password, KdfParams::default())?;
            let recovery = vault.reset_recovery_key()?.to_display_string();
            let config = Config::for_vault(path);
            config.save()?;
            s.config = Some(config);
            s.vault = Some(Arc::new(vault));
            Ok(recovery)
        })
        .await
}

/// A bucket as the setup form describes it, with its access key.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BucketInput {
    endpoint: String,
    region: String,
    bucket: String,
    prefix: String,
    access_key_id: String,
    secret_access_key: String,
}

impl BucketInput {
    pub(crate) fn parse(self) -> CmdResult<(S3Location, S3Credentials)> {
        let endpoint = self.endpoint.trim().trim_end_matches('/').to_string();
        let local = endpoint.starts_with("http://localhost") || endpoint.starts_with("http://127.0.0.1");
        if !endpoint.starts_with("https://") && !local {
            return Err(CmdError::other("The endpoint must start with https://."));
        }
        let bucket = required(self.bucket, "bucket")?;
        let access_key_id = self.access_key_id.trim().to_string();
        if access_key_id.is_empty() || self.secret_access_key.trim().is_empty() {
            return Err(CmdError::other("Enter the access key ID and secret."));
        }
        let region = match self.region.trim() {
            "" => "auto".to_string(),
            region => region.to_string(),
        };
        let location = S3Location { endpoint, region, bucket, prefix: self.prefix.trim().trim_matches('/').to_string() };
        let credentials =
            S3Credentials { access_key_id, secret_access_key: Zeroizing::new(self.secret_access_key.trim().to_string()) };
        Ok((location, credentials))
    }
}

/// Whether the bucket already holds a vault. Also checks the access key works.
#[tauri::command]
pub async fn bucket_has_vault(bucket: BucketInput) -> CmdResult<bool> {
    let (location, credentials) = bucket.parse()?;
    tauri::async_runtime::spawn_blocking(move || -> CmdResult<bool> {
        let store = S3Store::new(&location, &credentials)?;
        Ok(store.get("vault.json")?.is_some())
    })
    .await
    .map_err(|e| CmdError::other(e.to_string()))?
}

/// Creates a vault in a bucket and returns its recovery key, shown once.
#[tauri::command]
pub async fn create_bucket_vault(
    state: State<'_, AppState>,
    bucket: BucketInput,
    password: String,
) -> CmdResult<String> {
    let password = Zeroizing::new(password);
    check_new_password(&password).map_err(CmdError::other)?;
    let (location, credentials) = bucket.parse()?;
    state
        .run(move |s| {
            let store = CachedStore::open(S3Store::new(&location, &credentials)?, config::cache_dir(&location)?)?;
            let mut vault = Vault::create(store, &password, KdfParams::default())?;
            let recovery = vault.reset_recovery_key()?.to_display_string();
            dotsafe_stores::save_credentials(&location, &credentials)?;
            let config = Config::for_s3(location)?;
            config.save()?;
            s.config = Some(config);
            s.vault = Some(Arc::new(vault));
            Ok(recovery)
        })
        .await
}

/// Connects this machine to a vault that's already in a bucket.
#[tauri::command]
pub async fn connect_bucket_vault(state: State<'_, AppState>, bucket: BucketInput, password: String) -> CmdResult<()> {
    let password = Zeroizing::new(password);
    let (location, credentials) = bucket.parse()?;
    state
        .run(move |s| {
            let store = CachedStore::open(S3Store::new(&location, &credentials)?, config::cache_dir(&location)?)?;
            let vault = Vault::unlock(store, &password)?;
            dotsafe_stores::save_credentials(&location, &credentials)?;
            let config = Config::for_s3(location)?;
            config.save()?;
            s.config = Some(config);
            s.vault = Some(Arc::new(vault));
            Ok(())
        })
        .await
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncView {
    pub(crate) last_synced_at: Option<u64>,
    pub(crate) error: Option<String>,
}

/// Syncs a bucket vault now, instead of waiting for the background sync.
#[tauri::command]
pub async fn sync_now<R: Runtime>(app: AppHandle<R>, state: State<'_, AppState>) -> CmdResult<SyncView> {
    let inner = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || crate::sync::sync_once(&inner, Some(&app)))
        .await
        .map_err(|e| CmdError::other(e.to_string()))??;
    state
        .run(|s| Ok(SyncView { last_synced_at: s.sync.last_synced_at, error: s.sync.error.clone() }))
        .await
}

/// Connects this machine to an existing vault (e.g. one synced from another Mac).
#[tauri::command]
pub async fn connect_vault(state: State<'_, AppState>, path: String, password: String) -> CmdResult<()> {
    let password = Zeroizing::new(password);
    let path = absolute(path)?;
    state
        .run(move |s| {
            let vault = Vault::unlock(FolderStore::new(&path), &password)?;
            let config = Config::for_vault(path);
            config.save()?;
            s.config = Some(config);
            s.vault = Some(Arc::new(vault));
            Ok(())
        })
        .await
}

#[tauri::command]
pub async fn unlock(state: State<'_, AppState>, password: String) -> CmdResult<()> {
    let password = Zeroizing::new(password);
    state
        .run(move |s| {
            let vault = Vault::unlock(dotsafe_stores::open(s.config()?)?, &password)?;
            s.vault = Some(Arc::new(vault));
            s.sync = Default::default();
            s.must_change_password = false;
            crate::devices::after_password_unlock(s)?;
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
            let store = dotsafe_stores::open(s.config()?)?;
            let vault = Vault::unlock_with_recovery(store, &key).map_err(|e| match e {
                dotsafe_core::Error::InvalidCredentials => CmdError {
                    kind: ErrorKind::InvalidPassword,
                    message: "That recovery key doesn't match this vault.".into(),
                },
                e => e.into(),
            })?;
            s.vault = Some(Arc::new(vault));
            s.sync = Default::default();
            s.must_change_password = true;
            crate::devices::after_password_unlock(s)?;
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
            s.vault_mut()?.change_password(&new_password, KdfParams::default())?;
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
            Ok(s.vault_mut()?.reset_recovery_key()?.to_display_string())
        })
        .await
}

/// Re-checks the master password before a sensitive change, so an unlocked
/// Mac left unattended can't be used to take over the vault.
fn verify_password(s: &crate::state::Session, password: Option<&str>) -> CmdResult<()> {
    let password = password.filter(|p| !p.is_empty()).ok_or_else(|| CmdError::other("Enter your current password."))?;
    match s.vault()?.check_password(password) {
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VariableView {
    key: String,
    /// The shared secret it's linked to, if any.
    secret: Option<SecretRef>,
    /// Its value uses `${{ NAME }}` references.
    references: bool,
    /// Set by an ancestor, not this environment: the ancestor's name.
    inherited_from: Option<String>,
    /// Set here, replacing what the named ancestor has.
    overrides: Option<String>,
    /// Removed here: the ancestor named in `inherited_from` has it, but it
    /// doesn't apply.
    removed: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretRef {
    id: Uuid,
    name: String,
}

/// Variable names in an environment, own and inherited, which are linked to
/// shared secrets, and inherited ones removed here. Values stay in Rust
/// until revealed.
#[tauri::command]
pub async fn list_variables(state: State<'_, AppState>, environment_id: Uuid) -> CmdResult<Vec<VariableView>> {
    state
        .run(move |s| {
            let env = s.environment(environment_id)?;
            let records = s.vault()?.list()?;
            let resolver = Resolver::from_records(&records);
            let name_of = |id: Uuid| {
                records.iter().find_map(|r| match r {
                    Record::Environment(e) if e.id == id => Some(e.name.clone()),
                    _ => None,
                })
            };
            // What the parent chain provides, to mark overrides and removals.
            let inherited: Vec<Effective> = match env.parent {
                Some(parent) => resolver.entries(parent)?,
                None => Vec::new(),
            };
            let from_parent = |key: &str| inherited.iter().find(|e| e.key == key).and_then(|e| name_of(e.from));
            let mut views: Vec<VariableView> = resolver
                .entries(environment_id)?
                .into_iter()
                .map(|e| {
                    let own = e.from == environment_id;
                    VariableView {
                        secret: match e.entry {
                            Raw::Secret(id) => Some(SecretRef {
                                id,
                                name: resolver
                                    .secrets()
                                    .get(id)
                                    .map_or_else(|| "Deleted secret".into(), |s| s.name.clone()),
                            }),
                            Raw::Value(_) => None,
                        },
                        references: matches!(&e.entry, Raw::Value(v) if v.contains("${{")),
                        inherited_from: if own { None } else { name_of(e.from) },
                        overrides: if own { from_parent(&e.key) } else { None },
                        removed: false,
                        key: e.key,
                    }
                })
                .collect();
            for key in env.hidden() {
                if let Some(parent) = from_parent(key) {
                    views.push(VariableView {
                        key: key.to_string(),
                        secret: None,
                        references: false,
                        inherited_from: Some(parent),
                        overrides: None,
                        removed: true,
                    });
                }
            }
            views.sort_by(|a, b| a.key.cmp(&b.key));
            Ok(views)
        })
        .await
}

/// Removes an inherited variable here, so the parent's value doesn't apply.
#[tauri::command]
pub async fn hide_variable(state: State<'_, AppState>, environment_id: Uuid, key: String) -> CmdResult<()> {
    state
        .run(move |s| {
            let mut env = s.environment(environment_id)?;
            env.hide(&key, s.stamp()?);
            s.vault()?.put(Record::Environment(env))?;
            Ok(())
        })
        .await
}

/// Undoes [`hide_variable`]: the inherited value applies again.
#[tauri::command]
pub async fn restore_variable(state: State<'_, AppState>, environment_id: Uuid, key: String) -> CmdResult<()> {
    state
        .run(move |s| {
            let mut env = s.environment(environment_id)?;
            env.remove(&key, s.stamp()?);
            s.vault()?.put(Record::Environment(env))?;
            Ok(())
        })
        .await
}

/// Makes an environment inherit another in its project (`parent: null`
/// stops it). Refused if that would make a loop.
#[tauri::command]
pub async fn set_environment_parent(
    state: State<'_, AppState>,
    environment_id: Uuid,
    parent: Option<Uuid>,
) -> CmdResult<()> {
    state
        .run(move |s| {
            let mut env = s.environment(environment_id)?;
            if let Some(parent) = parent {
                let p = s.environment(parent)?;
                if p.project_id != env.project_id || p.id == env.id {
                    return Err(CmdError::other("Pick another environment in this project."));
                }
            }
            env.parent = parent;
            env.updated = s.stamp()?;
            let mut records = s.vault()?.list()?;
            records.retain(|r| r.id() != environment_id);
            records.push(Record::Environment(env.clone()));
            Resolver::from_records(&records).chain(environment_id)?;
            s.vault()?.put(Record::Environment(env))?;
            Ok(())
        })
        .await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VariableHit {
    project_id: Uuid,
    project: String,
    environment_id: Uuid,
    environment: String,
    key: String,
    linked: bool,
}

/// Every variable name in the vault, for the quick-copy palette. Names only,
/// like `list_variables`; copying goes through `copy_variable`.
#[tauri::command]
pub async fn all_variables(state: State<'_, AppState>) -> CmdResult<Vec<VariableHit>> {
    state
        .run(|s| {
            let records = s.vault()?.list()?;
            let projects: std::collections::HashMap<Uuid, &str> = records
                .iter()
                .filter_map(|r| match r {
                    Record::Project(p) if !p.deleted => Some((p.id, p.name.as_str())),
                    _ => None,
                })
                .collect();
            let resolver = Resolver::from_records(&records);
            let mut hits = Vec::new();
            for r in &records {
                if let Record::Environment(e) = r
                    && !e.deleted
                    && let Some(project) = projects.get(&e.project_id)
                    // A broken parent chain just leaves that environment out.
                    && let Ok(entries) = resolver.entries(e.id)
                {
                    for entry in entries {
                        hits.push(VariableHit {
                            project_id: e.project_id,
                            project: project.to_string(),
                            environment_id: e.id,
                            environment: e.name.clone(),
                            linked: matches!(entry.entry, Raw::Secret(_)),
                            key: entry.key,
                        });
                    }
                }
            }
            hits.sort_by_cached_key(|h| {
                (h.project.to_lowercase(), env_rank(&h.environment), h.environment.clone(), h.key.clone())
            });
            Ok(hits)
        })
        .await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultHealth {
    /// Records storage served an older copy of than one already seen.
    stale: usize,
    /// Records sealed with a key this vault doesn't have (written by a
    /// removed device after the re-key).
    unreadable: usize,
    /// Records that failed authentication or aren't valid: damaged in sync,
    /// or changed outside dotsafe.
    damaged: usize,
}

/// What the integrity checks found while reading the vault this session.
#[tauri::command]
pub async fn vault_health(state: State<'_, AppState>) -> CmdResult<VaultHealth> {
    state
        .run(|s| {
            let vault = s.vault()?;
            // Reading everything runs the checks on every record.
            vault.list()?;
            Ok(VaultHealth {
                stale: vault.stale_records().len(),
                unreadable: vault.unreadable_records().len(),
                damaged: vault.damaged_records().len(),
            })
        })
        .await
}

/// The variable's final value: inherited, with a linked secret's current
/// value and `${{ NAME }}` references worked out.
fn resolved_value(s: &crate::state::Session, env: &Environment, key: &str) -> CmdResult<String> {
    let resolver = Resolver::from_records(&s.vault()?.list()?);
    if !resolver.entries(env.id)?.iter().any(|e| e.key == key) {
        return Err(CmdError::other(format!("{key} isn't set.")));
    }
    resolver
        .value(env.id, key)?
        .ok_or_else(|| CmdError::other(format!("{key} is linked to a secret that has no value.")))
}

#[tauri::command]
pub async fn reveal_variable(
    state: State<'_, AppState>,
    environment_id: Uuid,
    key: String,
    raw: Option<bool>,
) -> CmdResult<String> {
    state
        .run(move |s| {
            let env = s.environment(environment_id)?;
            if raw == Some(true) {
                return raw_value(s, &env, &key);
            }
            resolved_value(s, &env, &key)
        })
        .await
}

/// The value as written, `${{ NAME }}` references and all, for editing. An
/// inherited variable's comes from the ancestor that sets it.
fn raw_value(s: &crate::state::Session, env: &Environment, key: &str) -> CmdResult<String> {
    let resolver = Resolver::from_records(&s.vault()?.list()?);
    let found = resolver.entries(env.id)?.into_iter().find(|e| e.key == key);
    match found.map(|e| e.entry) {
        Some(Raw::Value(value)) => Ok(value),
        Some(Raw::Secret(_)) => resolved_value(s, env, key),
        None => Err(CmdError::other(format!("{key} isn't set."))),
    }
}

/// Copies a value to the clipboard and clears it again after 30 seconds,
/// unless something else was copied in the meantime.
#[tauri::command]
pub async fn copy_variable<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    environment_id: Uuid,
    key: String,
) -> CmdResult<()> {
    state
        .run(move |s| {
            let env = s.environment(environment_id)?;
            let value = Zeroizing::new(resolved_value(s, &env, &key)?);
            crate::state::copy_to_clipboard(&app, s, &value)?;
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
            if !editing_in_place && env.entry(&key).is_some() {
                return Err(CmdError::other(format!("{key} already exists.")));
            }
            if editing_in_place && matches!(env.entry(&key), Some(Entry::Secret(_))) {
                return Err(CmdError::other(format!(
                    "{key} is linked to a secret. Rotate the secret, or unlink {key} first."
                )));
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
                    Change::Update | Change::Linked => {
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
    /// `folder`, `s3` or `webdav`.
    storage: &'static str,
    /// Where backup snapshots go, if set.
    backup_dir: Option<String>,
    /// When the last backup was taken (Unix ms).
    last_backup: Option<u64>,
    last_synced_at: Option<u64>,
    sync_error: Option<String>,
    license: Option<LicenseView>,
    version: &'static str,
    auto_lock_minutes: u32,
    clipboard_clear_seconds: u64,
}

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> CmdResult<Settings> {
    let license = licensing::saved()?;
    state
        .run(move |s| {
            Ok(Settings {
                vault_path: s.config.as_ref().map(Config::describe_location),
                storage: match s.config.as_ref() {
                    Some(c) if c.s3.is_some() => "s3",
                    Some(c) if c.webdav.is_some() => "webdav",
                    _ => "folder",
                },
                backup_dir: s.config.as_ref().and_then(|c| c.backup_dir.as_ref()).map(|p| p.display().to_string()),
                last_backup: s.config.as_ref().and_then(|c| c.last_backup),
                last_synced_at: s.sync.last_synced_at,
                sync_error: s.sync.error.clone(),
                license,
                version: env!("CARGO_PKG_VERSION"),
                auto_lock_minutes: s
                    .config
                    .as_ref()
                    .and_then(|c| c.auto_lock_minutes)
                    .unwrap_or(dotsafe_core::DEFAULT_AUTO_LOCK_MINUTES),
                clipboard_clear_seconds: CLIPBOARD_CLEAR.as_secs(),
            })
        })
        .await
}

/// Sets how many idle minutes pass before the app locks, on this machine.
#[tauri::command]
pub async fn set_auto_lock(state: State<'_, AppState>, minutes: u32) -> CmdResult<()> {
    if !(1..=24 * 60).contains(&minutes) {
        return Err(CmdError::other("Choose between 1 minute and 24 hours."));
    }
    state
        .run(move |s| {
            let mut config = s.config()?.clone();
            config.auto_lock_minutes = Some(minutes);
            config.save()?;
            s.config = Some(config);
            Ok(())
        })
        .await
}

pub(crate) fn check_new_password(password: &str) -> Result<(), String> {
    if password.chars().count() < MIN_PASSWORD_LEN {
        return Err(format!("Use at least {MIN_PASSWORD_LEN} characters."));
    }
    Ok(())
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
}
