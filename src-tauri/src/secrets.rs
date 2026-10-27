//! Commands for shared secrets: making one from a variable, linking and
//! unlinking, and the rotation flow (stage → promote → retire). Like
//! variables, values only reach the webview through `reveal_secret`; copying
//! goes straight from Rust to the clipboard.

use std::time::{Instant, SystemTime, UNIX_EPOCH};

use dotsafe_core::{Due, Entry, Environment, Policy, Record, Secret, Secrets, VersionState};
use serde::Serialize;
use tauri::{AppHandle, State};
use tauri_plugin_clipboard_manager::ClipboardExt;
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::state::{AppState, CmdError, CmdResult, Session, hash_text};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretView {
    id: Uuid,
    name: String,
    /// When the current value was created (ms), for its age.
    rotated_at: Option<u64>,
    due: Due,
    rotate_every_days: Option<u32>,
    staged: bool,
    has_previous: bool,
    used_in: Vec<Usage>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    project_id: Uuid,
    project: String,
    environment_id: Uuid,
    environment: String,
    key: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretDetail {
    #[serde(flatten)]
    summary: SecretView,
    versions: Vec<VersionView>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionView {
    id: Uuid,
    state: VersionState,
    created_at: u64,
    changed_at: u64,
    note: Option<String>,
    /// The value has been wiped (retired long enough ago, or past the history limit).
    wiped: bool,
}

/// Environments elsewhere that hold the same value under the same name, which
/// sharing can link too.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SharePreview {
    copies: Vec<Usage>,
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// Every live record, with project names to describe usages.
struct Snapshot {
    records: Vec<Record>,
    secrets: Secrets,
}

impl Snapshot {
    fn take(s: &Session) -> CmdResult<Snapshot> {
        let records = s.vault()?.list()?;
        let secrets = Secrets::from_records(&records);
        Ok(Snapshot { records, secrets })
    }

    fn project_name(&self, id: Uuid) -> String {
        self.records
            .iter()
            .find_map(|r| match r {
                Record::Project(p) if p.id == id => Some(p.name.clone()),
                _ => None,
            })
            .unwrap_or_default()
    }

    fn environments(&self) -> impl Iterator<Item = &Environment> {
        self.records.iter().filter_map(|r| match r {
            Record::Environment(e) if !e.deleted => Some(e),
            _ => None,
        })
    }

    fn usage(&self, env: &Environment, key: &str) -> Usage {
        Usage {
            project_id: env.project_id,
            project: self.project_name(env.project_id),
            environment_id: env.id,
            environment: env.name.clone(),
            key: key.to_string(),
        }
    }

    fn usages(&self, secret: Uuid) -> Vec<Usage> {
        let mut usages: Vec<Usage> = self
            .environments()
            .flat_map(|env| {
                env.entries()
                    .filter(move |(_, entry)| *entry == Entry::Secret(secret))
                    .map(move |(key, _)| self.usage(env, key))
            })
            .collect();
        usages.sort_by(|a, b| (&a.project, &a.environment, &a.key).cmp(&(&b.project, &b.environment, &b.key)));
        usages
    }

    fn view(&self, secret: &Secret) -> SecretView {
        SecretView {
            id: secret.id,
            name: secret.name.clone(),
            rotated_at: secret.current().map(|(_, v)| v.created.at_ms),
            due: secret.due(now_ms()),
            rotate_every_days: secret.policy.rotate_every_days,
            staged: secret.pending().is_some(),
            has_previous: secret.versions().iter().any(|(_, v)| v.state == VersionState::Previous),
            used_in: self.usages(secret.id),
        }
    }

    fn secret(&self, id: Uuid) -> CmdResult<Secret> {
        self.secrets.get(id).cloned().ok_or_else(|| CmdError::other("That shared secret no longer exists."))
    }
}

fn detail(snapshot: &Snapshot, secret: &Secret) -> SecretDetail {
    SecretDetail {
        summary: snapshot.view(secret),
        versions: secret
            .versions()
            .into_iter()
            .map(|(id, v)| VersionView {
                id,
                state: v.state,
                created_at: v.created.at_ms,
                changed_at: v.changed.at_ms,
                note: v.note.clone(),
                wiped: v.value.is_none(),
            })
            .collect(),
    }
}

/// Applies `change` to a secret, saves it, and returns its new detail.
fn update_secret(
    s: &mut Session,
    id: Uuid,
    change: impl FnOnce(&mut Secret, &Session) -> CmdResult<()>,
) -> CmdResult<SecretDetail> {
    let snapshot = Snapshot::take(s)?;
    let mut secret = snapshot.secret(id)?;
    change(&mut secret, s)?;
    let saved = match s.vault()?.put(Record::Secret(secret))? {
        Record::Secret(saved) => saved,
        _ => unreachable!("a secret record merges into a secret"),
    };
    Ok(detail(&snapshot, &saved))
}

#[tauri::command]
pub async fn list_secrets(state: State<'_, AppState>) -> CmdResult<Vec<SecretView>> {
    state
        .run(|s| {
            let snapshot = Snapshot::take(s)?;
            // Connection tokens are managed in Settings → Integrations.
            let tokens = dotsafe_core::integration::credentials(&snapshot.records);
            let mut views: Vec<SecretView> = snapshot
                .secrets
                .iter()
                .filter(|secret| !tokens.contains(&secret.id))
                .map(|secret| snapshot.view(secret))
                .collect();
            views.sort_by_key(|v| v.name.to_lowercase());
            Ok(views)
        })
        .await
}

#[tauri::command]
pub async fn get_secret(state: State<'_, AppState>, id: Uuid) -> CmdResult<SecretDetail> {
    state
        .run(move |s| {
            let snapshot = Snapshot::take(s)?;
            Ok(detail(&snapshot, &snapshot.secret(id)?))
        })
        .await
}

#[tauri::command]
pub async fn reveal_secret(state: State<'_, AppState>, id: Uuid) -> CmdResult<String> {
    state
        .run(move |s| {
            let snapshot = Snapshot::take(s)?;
            let secret = snapshot.secret(id)?;
            secret.current_value().map(str::to_string).ok_or_else(|| CmdError::other("This secret has no value."))
        })
        .await
}

#[tauri::command]
pub async fn copy_secret(app: AppHandle, state: State<'_, AppState>, id: Uuid) -> CmdResult<()> {
    state
        .run(move |s| {
            let snapshot = Snapshot::take(s)?;
            let secret = snapshot.secret(id)?;
            let value = secret.current_value().ok_or_else(|| CmdError::other("This secret has no value."))?;
            app.clipboard().write_text(value).map_err(|e| CmdError::other(e.to_string()))?;
            s.clipboard = Some((hash_text(value), Instant::now()));
            Ok(())
        })
        .await
}

/// Other environments that hold the same value as `key` here.
#[tauri::command]
pub async fn share_preview(state: State<'_, AppState>, environment_id: Uuid, key: String) -> CmdResult<SharePreview> {
    state
        .run(move |s| {
            let snapshot = Snapshot::take(s)?;
            let env = s.environment(environment_id)?;
            let value = match env.entry(&key) {
                Some(Entry::Value(value)) => value.to_string(),
                Some(Entry::Secret(_)) => return Err(CmdError::other(format!("{key} is already shared."))),
                None => return Err(CmdError::other(format!("{key} isn't set."))),
            };
            let copies = snapshot
                .environments()
                .filter(|e| e.id != environment_id && e.value(&key) == Some(value.as_str()))
                .map(|e| snapshot.usage(e, &key))
                .collect();
            Ok(SharePreview { copies })
        })
        .await
}

/// Turns a variable into a shared secret, linking it here and, if asked, in
/// the environments listed by `share_preview`.
#[tauri::command]
pub async fn share_variable(
    state: State<'_, AppState>,
    environment_id: Uuid,
    key: String,
    name: String,
    link_copies: bool,
) -> CmdResult<SecretView> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err(CmdError::other("Give the secret a name."));
    }
    state
        .run(move |s| {
            let mut env = s.environment(environment_id)?;
            let value = match env.entry(&key) {
                Some(Entry::Value(value)) => value.to_string(),
                Some(Entry::Secret(_)) => return Err(CmdError::other(format!("{key} is already shared."))),
                None => return Err(CmdError::other(format!("{key} isn't set."))),
            };
            let secret = Secret::new(name, value.clone(), s.stamp()?);
            let vault = s.vault()?;
            vault.put(Record::Secret(secret.clone()))?;
            let copies: Vec<Environment> = if link_copies {
                Snapshot::take(s)?
                    .environments()
                    .filter(|e| e.id != environment_id && e.value(&key) == Some(value.as_str()))
                    .cloned()
                    .collect()
            } else {
                Vec::new()
            };
            env.link(&key, secret.id, s.stamp()?);
            vault.put(Record::Environment(env))?;
            for mut copy in copies {
                copy.link(&key, secret.id, s.stamp()?);
                vault.put(Record::Environment(copy))?;
            }
            Ok(Snapshot::take(s)?.view(&secret))
        })
        .await
}

/// Links a variable to an existing shared secret, replacing any value it had.
#[tauri::command]
pub async fn link_variable(
    state: State<'_, AppState>,
    environment_id: Uuid,
    key: String,
    secret_id: Uuid,
) -> CmdResult<()> {
    let key = key.trim().to_string();
    if !dotsafe_core::is_valid_var_name(&key) {
        return Err(CmdError::other("Use letters, numbers and underscores, not starting with a number."));
    }
    state
        .run(move |s| {
            Snapshot::take(s)?.secret(secret_id)?;
            let mut env = s.environment(environment_id)?;
            env.link(&key, secret_id, s.stamp()?);
            s.vault()?.put(Record::Environment(env))?;
            Ok(())
        })
        .await
}

/// Gives a linked variable its own copy of the current value again.
#[tauri::command]
pub async fn unlink_variable(state: State<'_, AppState>, environment_id: Uuid, key: String) -> CmdResult<()> {
    state
        .run(move |s| {
            let mut env = s.environment(environment_id)?;
            let Some(Entry::Secret(id)) = env.entry(&key) else {
                return Err(CmdError::other(format!("{key} isn't linked to a shared secret.")));
            };
            let secrets = s.vault()?.secrets()?;
            let value = Zeroizing::new(secrets.value(Entry::Secret(id)).unwrap_or_default().to_string());
            env.set(&key, value.as_str(), s.stamp()?);
            s.vault()?.put(Record::Environment(env))?;
            Ok(())
        })
        .await
}

/// Stages a new value: typed in, or generated with the secret's generator.
/// Linked variables keep the current value until it's promoted.
#[tauri::command]
pub async fn stage_secret(
    state: State<'_, AppState>,
    id: Uuid,
    value: Option<String>,
    note: Option<String>,
) -> CmdResult<SecretDetail> {
    let value = value.map(Zeroizing::new);
    state
        .run(move |s| {
            update_secret(s, id, |secret, s| {
                let value = match value {
                    Some(v) if !v.is_empty() => v,
                    Some(_) => return Err(CmdError::other("Enter the new value.")),
                    None => secret.policy.generator.clone().unwrap_or_default().generate()?,
                };
                let note = note.map(|n| n.trim().to_string()).filter(|n| !n.is_empty());
                secret.stage(value.as_str(), note, s.stamp()?)?;
                Ok(())
            })
        })
        .await
}

#[tauri::command]
pub async fn promote_secret(state: State<'_, AppState>, id: Uuid) -> CmdResult<SecretDetail> {
    state.run(move |s| update_secret(s, id, |secret, s| Ok(secret.promote(s.stamp()?)?))).await
}

#[tauri::command]
pub async fn cancel_staged_secret(state: State<'_, AppState>, id: Uuid) -> CmdResult<SecretDetail> {
    state.run(move |s| update_secret(s, id, |secret, s| Ok(secret.cancel(s.stamp()?)?))).await
}

#[tauri::command]
pub async fn roll_back_secret(state: State<'_, AppState>, id: Uuid) -> CmdResult<SecretDetail> {
    state.run(move |s| update_secret(s, id, |secret, s| Ok(secret.roll_back(s.stamp()?)?))).await
}

/// Marks previous values as revoked.
#[tauri::command]
pub async fn retire_previous_secret(state: State<'_, AppState>, id: Uuid) -> CmdResult<SecretDetail> {
    state
        .run(move |s| {
            update_secret(s, id, |secret, s| {
                secret.retire_previous(s.stamp()?);
                Ok(())
            })
        })
        .await
}

/// The fast path for a leaked value: a new value (typed or generated) goes
/// live at once, and the old one is retired with no overlap.
#[tauri::command]
pub async fn rotate_leaked_secret(
    state: State<'_, AppState>,
    id: Uuid,
    value: Option<String>,
) -> CmdResult<SecretDetail> {
    let value = value.map(Zeroizing::new);
    state
        .run(move |s| {
            update_secret(s, id, |secret, s| {
                if secret.pending().is_some() {
                    secret.cancel(s.stamp()?)?;
                }
                let value = match value {
                    Some(v) if !v.is_empty() => v,
                    Some(_) => return Err(CmdError::other("Enter the new value.")),
                    None => secret.policy.generator.clone().unwrap_or_default().generate()?,
                };
                secret.rotate(value.as_str(), Some("Leaked".into()), s.stamp()?)?;
                secret.retire_previous(s.stamp()?);
                Ok(())
            })
        })
        .await
}

#[tauri::command]
pub async fn restore_secret_version(state: State<'_, AppState>, id: Uuid, version_id: Uuid) -> CmdResult<SecretDetail> {
    state
        .run(move |s| {
            update_secret(s, id, |secret, s| {
                if secret.pending().is_some() {
                    return Err(CmdError::other("Promote or cancel the staged value first."));
                }
                secret.restore(version_id, s.stamp()?)?;
                Ok(())
            })
        })
        .await
}

#[tauri::command]
pub async fn update_secret_settings(
    state: State<'_, AppState>,
    id: Uuid,
    name: String,
    rotate_every_days: Option<u32>,
) -> CmdResult<SecretDetail> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err(CmdError::other("Give the secret a name."));
    }
    if rotate_every_days == Some(0) || rotate_every_days.is_some_and(|d| d > 3650) {
        return Err(CmdError::other("Choose between 1 and 3650 days."));
    }
    state
        .run(move |s| {
            update_secret(s, id, |secret, s| {
                let stamp = s.stamp()?;
                if secret.name != name {
                    secret.rename(name, stamp);
                }
                if secret.policy.rotate_every_days != rotate_every_days {
                    let policy = Policy { rotate_every_days, ..secret.policy.clone() };
                    secret.set_policy(policy, s.stamp()?);
                }
                Ok(())
            })
        })
        .await
}

/// Deletes a secret. Linked variables first get their own copy of the
/// current value, so nothing stops working.
#[tauri::command]
pub async fn delete_secret(state: State<'_, AppState>, id: Uuid) -> CmdResult<()> {
    state
        .run(move |s| {
            let snapshot = Snapshot::take(s)?;
            let mut secret = snapshot.secret(id)?;
            let value = Zeroizing::new(secret.current_value().unwrap_or_default().to_string());
            let vault = s.vault()?;
            for env in snapshot.environments() {
                let keys: Vec<String> =
                    env.entries().filter(|(_, e)| *e == Entry::Secret(id)).map(|(k, _)| k.to_string()).collect();
                if keys.is_empty() {
                    continue;
                }
                let mut env = env.clone();
                for key in keys {
                    env.set(key, value.as_str(), s.stamp()?);
                }
                vault.put(Record::Environment(env))?;
            }
            secret.delete(s.stamp()?);
            vault.put(Record::Secret(secret))?;
            Ok(())
        })
        .await
}
