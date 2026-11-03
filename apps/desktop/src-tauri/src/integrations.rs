//! Integrations: connected accounts, each secret's provider and push
//! targets, and rotation through them.
//!
//! Admin tokens go in once, when connecting, and are kept as shared secrets;
//! they never come back to the webview. Calls to the services run with the
//! shared vault off the session lock, like background sync, so a slow
//! provider never freezes the app.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use dotsafe_core::{Binding, Connection, ProviderLink, Record, Secret, Target, Vault, crypto};
use dotsafe_integrations::rotate::{self, PushResult};
use dotsafe_integrations::{Client, Kind, SERVICES, check_settings, service};
use serde::Serialize;
use tauri::State;
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::state::{AppState, CmdError, CmdResult, Session};

impl From<dotsafe_integrations::Error> for CmdError {
    fn from(e: dotsafe_integrations::Error) -> Self {
        match e {
            dotsafe_integrations::Error::Vault(e) => e.into(),
            dotsafe_integrations::Error::Unreachable(service) => CmdError {
                kind: crate::state::ErrorKind::Offline,
                message: format!("Couldn't reach {service}. Check your connection and try again."),
            },
            e => CmdError::other(capitalized(&e.to_string())),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldView {
    key: &'static str,
    label: &'static str,
    hint: &'static str,
    required: bool,
    /// The value is the name of another shared secret.
    secret_ref: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceView {
    id: &'static str,
    label: &'static str,
    /// `provider` or `target`.
    kind: &'static str,
    token_help: &'static str,
    connection_fields: Vec<FieldView>,
    binding_fields: Vec<FieldView>,
    /// Providers: the old key keeps working until it's revoked.
    overlap: bool,
    /// Providers: you make keys in the console and paste them.
    guided: bool,
    /// Providers: old keys are revoked in the console, then marked retired.
    manual_revoke: bool,
    needs_token: bool,
    console_url: Option<&'static str>,
}

fn fields(fields: &[dotsafe_integrations::Field]) -> Vec<FieldView> {
    fields
        .iter()
        .map(|f| FieldView { key: f.key, label: f.label, hint: f.hint, required: f.required, secret_ref: f.secret_ref })
        .collect()
}

fn kind(k: Kind) -> &'static str {
    match k {
        Kind::Provider => "provider",
        Kind::Target => "target",
    }
}

/// What dotsafe can connect to.
#[tauri::command]
pub async fn list_services() -> Vec<ServiceView> {
    SERVICES
        .iter()
        .map(|s| ServiceView {
            id: s.id,
            label: s.label,
            kind: kind(s.kind),
            token_help: s.token_help,
            connection_fields: fields(s.connection_fields),
            binding_fields: fields(s.binding_fields),
            overlap: s.overlap,
            guided: s.guided,
            manual_revoke: s.manual_revoke,
            needs_token: s.needs_token,
            console_url: s.console_url,
        })
        .collect()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionView {
    id: Uuid,
    service: String,
    service_label: &'static str,
    kind: &'static str,
    name: String,
    settings: BTreeMap<String, String>,
    /// How many secrets use it, as a provider or a target.
    used_by: usize,
}

fn connections(records: &[Record]) -> Vec<&Connection> {
    records
        .iter()
        .filter_map(|r| match r {
            Record::Connection(c) if !c.deleted => Some(c),
            _ => None,
        })
        .collect()
}

fn bindings(records: &[Record]) -> impl Iterator<Item = &Binding> {
    records.iter().filter_map(|r| match r {
        Record::Binding(b) => Some(b),
        _ => None,
    })
}

fn uses(binding: &Binding, connection: Uuid) -> bool {
    binding.provider.as_ref().is_some_and(|p| p.connection == connection)
        || binding.targets().any(|(_, t)| t.connection == connection)
}

/// Connected accounts. Names and settings only; tokens stay in Rust.
#[tauri::command]
pub async fn list_connections(state: State<'_, AppState>) -> CmdResult<Vec<ConnectionView>> {
    state
        .run(|s| {
            let records = s.vault()?.list()?;
            let mut views: Vec<ConnectionView> = connections(&records)
                .into_iter()
                .map(|c| {
                    let svc = service(&c.service).ok();
                    ConnectionView {
                        id: c.id,
                        service: c.service.clone(),
                        service_label: svc.map_or("Unknown", |s| s.label),
                        kind: svc.map_or("provider", |s| kind(s.kind)),
                        name: c.name.clone(),
                        settings: c.settings.clone(),
                        used_by: bindings(&records).filter(|b| uses(b, c.id)).count(),
                    }
                })
                .collect();
            views.sort_by_key(|v| v.name.to_lowercase());
            Ok(views)
        })
        .await
}

/// Connects an account. The token is stored as a shared secret.
#[tauri::command]
pub async fn add_connection(
    state: State<'_, AppState>,
    service_id: String,
    name: String,
    settings: BTreeMap<String, String>,
    token: String,
) -> CmdResult<()> {
    let token = Zeroizing::new(token.trim().to_string());
    let name = name.trim().to_string();
    let svc = service(&service_id)?;
    let settings = trimmed(settings);
    check_settings(svc.connection_fields, &settings)?;
    if name.is_empty() {
        return Err(CmdError::other("Give the connection a name."));
    }
    if token.is_empty() && svc.needs_token {
        return Err(CmdError::other("Paste the admin token."));
    }
    state
        .run(move |s| {
            let vault = s.vault()?;
            if connections(&vault.list()?).iter().any(|c| c.name.eq_ignore_ascii_case(&name)) {
                return Err(CmdError::other(format!("A connection named {name} already exists.")));
            }
            let credential = if svc.needs_token {
                let credential = Secret::new(format!("{name} admin token"), token.as_str(), s.stamp()?);
                vault.put(Record::Secret(credential.clone()))?;
                Some(credential.id)
            } else {
                None
            };
            vault.put(Record::Connection(Connection::new(svc.id, &name, credential, settings, s.stamp()?)))?;
            Ok(())
        })
        .await
}

/// Swaps in a new admin token, for when the old one was rotated or revoked
/// at the service. The old one is kept in the token's history, retired.
#[tauri::command]
pub async fn replace_connection_token(state: State<'_, AppState>, id: Uuid, token: String) -> CmdResult<()> {
    let token = Zeroizing::new(token.trim().to_string());
    if token.is_empty() {
        return Err(CmdError::other("Paste the new admin token."));
    }
    state
        .run(move |s| {
            let vault = s.vault()?;
            let connection = match vault.get(id)? {
                Some(Record::Connection(c)) if !c.deleted => c,
                _ => return Err(CmdError::other("That connection was removed.")),
            };
            let Some(credential) = connection.credential else {
                return Err(CmdError::other(format!("{} doesn't use a token.", connection.name)));
            };
            let mut secret = match vault.get(credential)? {
                Some(Record::Secret(secret)) => secret,
                _ => return Err(CmdError::other(format!("{}'s stored token is missing.", connection.name))),
            };
            secret.rotate(token.as_str(), Some("Replaced".into()), s.stamp()?)?;
            secret.retire_previous(s.stamp()?);
            vault.put(Record::Secret(secret))?;
            Ok(())
        })
        .await
}

/// Removes a connection nothing uses, and deletes its stored token.
#[tauri::command]
pub async fn remove_connection(state: State<'_, AppState>, id: Uuid) -> CmdResult<()> {
    state
        .run(move |s| {
            let vault = s.vault()?;
            let records = vault.list()?;
            let Some(mut connection) = connections(&records).into_iter().find(|c| c.id == id).cloned() else {
                return Err(CmdError::other("That connection was already removed."));
            };
            if bindings(&records).any(|b| uses(b, id)) {
                return Err(CmdError::other(format!(
                    "Secrets still use {}. Remove it from them first.",
                    connection.name
                )));
            }
            connection.deleted = true;
            connection.updated = s.stamp()?;
            let credential = connection.credential;
            vault.put(Record::Connection(connection))?;
            if let Some(Record::Secret(mut token)) = credential.map(|id| vault.get(id)).transpose()?.flatten() {
                token.delete(s.stamp()?);
                vault.put(Record::Secret(token))?;
            }
            Ok(())
        })
        .await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderView {
    connection_id: Uuid,
    connection_name: String,
    service_id: String,
    service_label: &'static str,
    settings: BTreeMap<String, String>,
    /// The old key keeps working until revoked.
    overlap: bool,
    /// You make new keys in the console and paste them.
    guided: bool,
    /// Old keys are revoked in the console.
    manual_revoke: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetView {
    id: Uuid,
    connection_id: Uuid,
    connection_name: String,
    service_label: &'static str,
    name: String,
    settings: BTreeMap<String, String>,
    pushed_at: Option<u64>,
    /// The last push matches the current value.
    up_to_date: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretIntegrations {
    provider: Option<ProviderView>,
    targets: Vec<TargetView>,
}

/// A secret's provider and targets. Never values: up-to-date is decided
/// here, from fingerprints.
#[tauri::command]
pub async fn secret_integrations(state: State<'_, AppState>, secret_id: Uuid) -> CmdResult<SecretIntegrations> {
    state
        .run(move |s| {
            let records = s.vault()?.list()?;
            let by_id: HashMap<Uuid, &Connection> = connections(&records).into_iter().map(|c| (c.id, c)).collect();
            let label = |c: &Connection| service(&c.service).map_or("Unknown", |s| s.label);
            let binding = bindings(&records).find(|b| b.secret == secret_id);
            let current = records
                .iter()
                .find_map(|r| match r {
                    Record::Secret(secret) if secret.id == secret_id => secret.current_value().map(crypto::fingerprint),
                    _ => None,
                });
            let provider = binding.and_then(|b| b.provider.as_ref()).and_then(|p| {
                let c = by_id.get(&p.connection)?;
                let svc = service(&c.service).ok();
                Some(ProviderView {
                    connection_id: c.id,
                    connection_name: c.name.clone(),
                    service_id: c.service.clone(),
                    service_label: label(c),
                    settings: p.settings.clone(),
                    overlap: svc.is_some_and(|s| s.overlap),
                    guided: svc.is_some_and(|s| s.guided),
                    manual_revoke: svc.is_some_and(|s| s.manual_revoke),
                })
            });
            let mut targets: Vec<TargetView> = binding
                .into_iter()
                .flat_map(|b| b.targets())
                .map(|(id, t)| {
                    let c = by_id.get(&t.connection);
                    TargetView {
                        id,
                        connection_id: t.connection,
                        connection_name: c.map_or_else(|| "Removed connection".into(), |c| c.name.clone()),
                        service_label: c.map_or("Unknown", |c| label(c)),
                        name: t.name.clone(),
                        settings: t.settings.clone(),
                        pushed_at: t.pushed.as_ref().map(|p| p.at),
                        up_to_date: t.pushed.as_ref().is_some_and(|p| Some(&p.sha256) == current.as_ref()),
                    }
                })
                .collect();
            targets.sort_by(|a, b| (&a.connection_name, &a.name).cmp(&(&b.connection_name, &b.name)));
            Ok(SecretIntegrations { provider, targets })
        })
        .await
}

fn binding_for(s: &Session, secret: Uuid) -> CmdResult<Binding> {
    Ok(match s.vault()?.get(Binding::id_for(secret))? {
        Some(Record::Binding(b)) => b,
        _ => Binding::new(secret, s.stamp()?),
    })
}

fn connection_of(s: &Session, id: Uuid, want: Kind) -> CmdResult<Connection> {
    let connection = match s.vault()?.get(id)? {
        Some(Record::Connection(c)) if !c.deleted => c,
        _ => return Err(CmdError::other("That connection was removed.")),
    };
    if service(&connection.service)?.kind != want {
        return Err(CmdError::other(match want {
            Kind::Provider => format!("{} is a deploy target, not a provider.", connection.name),
            Kind::Target => format!("{} is a provider, not a deploy target.", connection.name),
        }));
    }
    Ok(connection)
}

/// Sets or clears (`connection_id: null`) the provider a secret's values
/// come from.
#[tauri::command]
pub async fn set_secret_provider(
    state: State<'_, AppState>,
    secret_id: Uuid,
    connection_id: Option<Uuid>,
    settings: BTreeMap<String, String>,
) -> CmdResult<()> {
    let lookup = state
        .run(move |s| {
            let mut binding = binding_for(s, secret_id)?;
            let mut lookup = false;
            binding.provider = match connection_id {
                Some(id) => {
                    let connection = connection_of(s, id, Kind::Provider)?;
                    let svc = service(&connection.service)?;
                    let mut settings = trimmed(settings);
                    check_settings(svc.binding_fields, &settings)?;
                    // Settings that name another shared secret are stored as its id.
                    let secrets = s.vault()?.secrets()?;
                    for field in svc.binding_fields.iter().filter(|f| f.secret_ref) {
                        if let Some(name) = settings.get(field.key).cloned() {
                            let found: Vec<&Secret> = secrets.iter().filter(|x| x.name == name).collect();
                            let [one] = found.as_slice() else {
                                return Err(CmdError::other(format!("There's no single shared secret named {name}.")));
                            };
                            settings.insert(field.key.to_string(), one.id.to_string());
                        }
                    }
                    lookup = svc.id == "anthropic";
                    Some(ProviderLink { connection: id, settings })
                }
                None => None,
            };
            binding.updated = s.stamp()?;
            s.vault()?.put(Record::Binding(binding))?;
            Ok(lookup)
        })
        .await?;
    // Find which key the current value is, so the first revoke works. Off
    // the session lock; failing just means revoking by hand later.
    if lookup {
        let (vault, device) = shared(&state).await?;
        let _ = off_lock(move || rotate::adopt_current_key(&vault, &Client::new(), device, secret_id)).await;
    }
    Ok(())
}

#[tauri::command]
pub async fn add_secret_target(
    state: State<'_, AppState>,
    secret_id: Uuid,
    connection_id: Uuid,
    name: String,
    settings: BTreeMap<String, String>,
) -> CmdResult<()> {
    let name = name.trim().to_string();
    if !dotsafe_core::is_valid_var_name(&name) {
        return Err(CmdError::other("Use letters, numbers and underscores, not starting with a number."));
    }
    state
        .run(move |s| {
            let connection = connection_of(s, connection_id, Kind::Target)?;
            let settings = trimmed(settings);
            check_settings(service(&connection.service)?.binding_fields, &settings)?;
            let mut binding = binding_for(s, secret_id)?;
            let changed = s.stamp()?;
            binding
                .targets
                .insert(Uuid::new_v4(), Target { connection: connection_id, name, settings, pushed: None, deleted: false, changed });
            s.vault()?.put(Record::Binding(binding))?;
            Ok(())
        })
        .await
}

#[tauri::command]
pub async fn remove_secret_target(state: State<'_, AppState>, secret_id: Uuid, target_id: Uuid) -> CmdResult<()> {
    state
        .run(move |s| {
            let mut binding = binding_for(s, secret_id)?;
            let stamp = s.stamp()?;
            let target = binding.targets.get_mut(&target_id).ok_or_else(|| CmdError::other("That target was already removed."))?;
            target.deleted = true;
            target.changed = stamp;
            s.vault()?.put(Record::Binding(binding))?;
            Ok(())
        })
        .await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PushView {
    name: String,
    connection: String,
    error: Option<String>,
}

fn pushes(results: Vec<PushResult>) -> Vec<PushView> {
    results
        .into_iter()
        .map(|r| PushView { name: r.name, connection: r.connection, error: r.error.map(|e| capitalized(&e)) })
        .collect()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RotationView {
    /// Why the new key was left staged (or went live unverified).
    check_failed: Option<String>,
    /// The new key is live.
    promoted: bool,
    revoked_old: bool,
    pushes: Vec<PushView>,
}

/// The shared vault and this device, to work with off the session lock.
async fn shared(state: &State<'_, AppState>) -> CmdResult<(Arc<Vault>, Uuid)> {
    state
        .run(|s| {
            s.vault()?; // fails with `locked`
            Ok((s.vault.clone().expect("checked above"), s.config()?.device_id))
        })
        .await
}

async fn off_lock<T: Send + 'static>(f: impl FnOnce() -> dotsafe_integrations::Result<T> + Send + 'static) -> CmdResult<T> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| CmdError::other(e.to_string()))?.map_err(CmdError::from)
}

/// Gets a new key from the secret's provider (or takes the one pasted, for
/// guided providers) and makes it live: saved, checked, promoted, pushed.
/// With `leaked`, the old key is revoked at once.
#[tauri::command]
pub async fn rotate_with_provider(
    state: State<'_, AppState>,
    secret_id: Uuid,
    leaked: bool,
    pasted: Option<String>,
) -> CmdResult<RotationView> {
    let pasted = pasted.map(Zeroizing::new);
    let (vault, device) = shared(&state).await?;
    let out = off_lock(move || {
        rotate::rotate(&vault, &Client::new(), device, secret_id, pasted.as_deref().map(String::as_str), leaked)
    })
    .await?;
    Ok(RotationView {
        check_failed: out.staged,
        promoted: out.promoted,
        revoked_old: out.revoked_old,
        pushes: pushes(out.pushes),
    })
}

/// Pushes the current value to every target.
#[tauri::command]
pub async fn push_secret(state: State<'_, AppState>, secret_id: Uuid) -> CmdResult<Vec<PushView>> {
    let (vault, device) = shared(&state).await?;
    Ok(pushes(off_lock(move || rotate::push(&vault, &Client::new(), device, secret_id)).await?))
}

/// Revokes the previous key at the provider, then retires it. With
/// `by_hand`, you revoked it in the provider's console; it's just retired.
#[tauri::command]
pub async fn revoke_previous_with_provider(state: State<'_, AppState>, secret_id: Uuid, by_hand: bool) -> CmdResult<usize> {
    let (vault, device) = shared(&state).await?;
    off_lock(move || rotate::retire(&vault, &Client::new(), device, secret_id, by_hand)).await
}

/// Opens a provider's console in the browser, where guided keys are made and
/// revoked. Only the addresses dotsafe knows, never one from the webview.
#[tauri::command]
pub async fn open_provider_console<R: tauri::Runtime>(app: tauri::AppHandle<R>, service_id: String) -> CmdResult<()> {
    let url = service(&service_id)?.console_url.ok_or_else(|| CmdError::other("This service has no console link."))?;
    crate::links::open(&app, url)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueuedSecret {
    id: Uuid,
    name: String,
    has_provider: bool,
    /// A connection's admin token: replaced from Settings → Integrations.
    connection_token: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueuedVariable {
    project_id: Uuid,
    project: String,
    environment_id: Uuid,
    environment: String,
    key: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueView {
    since: u64,
    device: String,
    secrets: Vec<QueuedSecret>,
    variables: Vec<QueuedVariable>,
}

/// What still needs rotating after a device was removed from this Mac:
/// every value older than the removal. `None` when there's no queue.
#[tauri::command]
pub async fn rotation_queue(state: State<'_, AppState>) -> CmdResult<Option<QueueView>> {
    state
        .run(|s| {
            let Some(queue) = s.config()?.rotation_queue.clone() else { return Ok(None) };
            let records = s.vault()?.list()?;
            let exposed = dotsafe_core::integration::exposed_since(&records, queue.since);
            let tokens = dotsafe_core::integration::credentials(&records);
            let mut secrets: Vec<QueuedSecret> = exposed
                .secrets
                .iter()
                .filter_map(|id| {
                    records.iter().find_map(|r| match r {
                        Record::Secret(secret) if secret.id == *id => Some(QueuedSecret {
                            id: *id,
                            name: secret.name.clone(),
                            // Guided providers need a pasted key, so they're rotated one by one.
                            has_provider: bindings(&records).any(|b| {
                                b.secret == *id
                                    && b.provider.as_ref().is_some_and(|p| {
                                        connections(&records)
                                            .iter()
                                            .find(|c| c.id == p.connection)
                                            .and_then(|c| service(&c.service).ok())
                                            .is_some_and(|svc| !svc.guided)
                                    })
                            }),
                            connection_token: tokens.contains(id),
                        }),
                        _ => None,
                    })
                })
                .collect();
            secrets.sort_by_key(|s| (!s.has_provider, s.connection_token, s.name.to_lowercase()));
            let mut variables: Vec<QueuedVariable> = exposed
                .variables
                .into_iter()
                .filter_map(|(env_id, key)| {
                    let env = records.iter().find_map(|r| match r {
                        Record::Environment(e) if e.id == env_id => Some(e),
                        _ => None,
                    })?;
                    let project = records.iter().find_map(|r| match r {
                        Record::Project(p) if p.id == env.project_id => Some(p.name.clone()),
                        _ => None,
                    })?;
                    Some(QueuedVariable {
                        project_id: env.project_id,
                        project,
                        environment_id: env_id,
                        environment: env.name.clone(),
                        key,
                    })
                })
                .collect();
            variables.sort_by(|a, b| (&a.project, &a.environment, &a.key).cmp(&(&b.project, &b.environment, &b.key)));
            Ok(Some(QueueView { since: queue.since, device: queue.device, secrets, variables }))
        })
        .await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueuedRotation {
    name: String,
    /// Why it wasn't rotated, or why the new key is only staged.
    error: Option<String>,
    pushes: Vec<PushView>,
}

/// Rotates every queued secret that has a provider, one after another.
#[tauri::command]
pub async fn rotate_queued_with_providers(state: State<'_, AppState>) -> CmdResult<Vec<QueuedRotation>> {
    let queued: Vec<(Uuid, String)> = rotation_queue(state.clone())
        .await?
        .map(|q| q.secrets.into_iter().filter(|s| s.has_provider).map(|s| (s.id, s.name)).collect())
        .unwrap_or_default();
    let (vault, device) = shared(&state).await?;
    off_lock(move || {
        let client = Client::new();
        Ok(queued
            .into_iter()
            .map(|(id, name)| match rotate::rotate(&vault, &client, device, id, None, false) {
                Ok(out) if out.promoted => QueuedRotation { name, error: None, pushes: pushes(out.pushes) },
                Ok(out) => QueuedRotation {
                    name,
                    error: Some(format!("The new key failed its check and is staged: {}", out.staged.unwrap_or_default())),
                    pushes: Vec::new(),
                },
                Err(e) => QueuedRotation { name, error: Some(capitalized(&e.to_string())), pushes: Vec::new() },
            })
            .collect())
    })
    .await
}

#[tauri::command]
pub async fn dismiss_rotation_queue(state: State<'_, AppState>) -> CmdResult<()> {
    state
        .run(|s| {
            let mut config = s.config()?.clone();
            config.rotation_queue = None;
            config.save()?;
            s.config = Some(config);
            Ok(())
        })
        .await
}

fn trimmed(settings: BTreeMap<String, String>) -> BTreeMap<String, String> {
    settings
        .into_iter()
        .map(|(k, v)| (k, v.trim().to_string()))
        .filter(|(_, v)| !v.is_empty())
        .collect()
}

fn capitalized(s: &str) -> String {
    let mut chars = s.chars();
    let mut out: String = chars.next().map(|c| c.to_uppercase().chain(chars).collect()).unwrap_or_default();
    if !out.ends_with('.') {
        out.push('.');
    }
    out
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyProfileView {
    /// `None` for built-in profiles.
    id: Option<Uuid>,
    name: String,
    key_var: String,
    base_url_var: String,
    upstream: String,
    /// Why a custom profile isn't used, if it isn't.
    problem: Option<String>,
}

/// Built-in and custom proxy profiles.
#[tauri::command]
pub async fn list_proxy_profiles(state: State<'_, AppState>) -> CmdResult<Vec<ProxyProfileView>> {
    use dotsafe_agent::proxy::{Profile, builtin_profiles};
    state
        .run(|s| {
            let records = s.vault()?.list()?;
            let mut views: Vec<ProxyProfileView> = builtin_profiles()
                .into_iter()
                .map(|p| ProxyProfileView {
                    id: None,
                    name: p.name,
                    key_var: p.key_var,
                    base_url_var: p.base_url_var,
                    upstream: p.upstream,
                    problem: None,
                })
                .collect();
            views.extend(dotsafe_core::integration::proxy_profiles(&records).into_iter().map(|p| ProxyProfileView {
                problem: Profile::custom(&p).err(),
                id: Some(p.id),
                name: p.name,
                key_var: p.key_var,
                base_url_var: p.base_url_var,
                upstream: p.upstream,
            }));
            Ok(views)
        })
        .await
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyProfileInput {
    name: String,
    key_var: String,
    base_url_var: String,
    upstream: String,
    base_path: String,
    header: String,
    bearer: bool,
}

/// Adds a custom proxy profile, after the proxy's own checks.
#[tauri::command]
pub async fn add_proxy_profile(state: State<'_, AppState>, profile: ProxyProfileInput) -> CmdResult<()> {
    state
        .run(move |s| {
            let vault = s.vault()?;
            let records = vault.list()?;
            let key_var = profile.key_var.trim().to_string();
            if dotsafe_core::integration::proxy_profiles(&records).iter().any(|p| p.key_var == key_var) {
                return Err(CmdError::other(format!("{key_var} already has a profile.")));
            }
            let record = dotsafe_core::ProxyProfile {
                id: Uuid::new_v4(),
                name: profile.name.trim().to_string(),
                key_var,
                base_url_var: profile.base_url_var.trim().to_string(),
                upstream: profile.upstream.trim().to_string(),
                base_path: profile.base_path.trim().to_string(),
                header: profile.header.trim().to_string(),
                bearer: profile.bearer,
                deleted: false,
                updated: s.stamp()?,
            };
            dotsafe_agent::proxy::Profile::custom(&record).map_err(CmdError::other)?;
            vault.put(Record::ProxyProfile(record))?;
            Ok(())
        })
        .await
}

#[tauri::command]
pub async fn remove_proxy_profile(state: State<'_, AppState>, id: Uuid) -> CmdResult<()> {
    state
        .run(move |s| {
            let vault = s.vault()?;
            let mut profile = match vault.get(id)? {
                Some(Record::ProxyProfile(p)) if !p.deleted => p,
                _ => return Err(CmdError::other("That profile was already removed.")),
            };
            profile.deleted = true;
            profile.updated = s.stamp()?;
            vault.put(Record::ProxyProfile(profile))?;
            Ok(())
        })
        .await
}
