//! Shares in the app (Settings → Access): the share bucket, creating,
//! listing and revoking shares, and pushing their snapshots whenever the
//! vault changes, here or on another device.
//!
//! A new share's token stays in Rust. The webview gets a short preview and
//! copies it with `copy_share_token`, like the enrollment code, so it never
//! sits in the webview's memory. The share bucket's write key goes into the
//! keychain when it's set up and never comes back.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use dotsafe_core::share::{self, DEFAULT_LEASE_DAYS, Status};
use dotsafe_core::{Config, Record, ShareBucket, ShareBucketLocation, ShareEnv, Vault};
use dotsafe_integrations::shares::{self as flow, Action, ShareResult};
use dotsafe_stores::{Fetcher, S3Credentials, S3ShareBucket};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Runtime, State};
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::state::{AppState, CmdError, CmdResult};
use crate::sync::FolderWatch;

const DAY_MS: u64 = 24 * 60 * 60 * 1000;
/// How often the pusher looks for a change to the vault.
const CHECK: Duration = Duration::from_secs(5);
/// How often it pushes anyway, so leases are renewed while the vault is
/// unlocked even when nothing changes.
const RENEW_CHECK: Duration = Duration::from_secs(60 * 60);
/// The app warns this many days before a lease runs out.
pub const LEASE_WARNING_DAYS: u64 = 3;
/// Revoked and expired shares stay listed this long after they end.
const ENDED_SHOWN_MS: u64 = 30 * DAY_MS;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BucketView {
    bucket: String,
    endpoint: String,
    region: String,
    public_url: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareEnvView {
    project: String,
    environment: String,
    /// Only these variables, if narrowed.
    only: Option<Vec<String>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareView {
    id: Uuid,
    name: String,
    environments: Vec<ShareEnvView>,
    /// `current`, `outOfDate`, `neverPushed`, `expired`, `revoked` or `broken`.
    status: &'static str,
    /// Why it can't be pushed, when `broken` (a deleted environment, a broken
    /// reference).
    problem: Option<String>,
    expires: u64,
    revoked: Option<u64>,
    pushed_at: Option<u64>,
    /// Days until the snapshot's lease runs out, for live pushed shares.
    lease_days_left: Option<u64>,
    lease_days: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SharesView {
    /// The bucket this Mac writes shares to, if one is set up here.
    bucket: Option<BucketView>,
    shares: Vec<ShareView>,
}

/// The share bucket and every share, newest first, with whether each holds
/// the vault's current values. Names and metadata only.
#[tauri::command]
pub async fn list_shares(state: State<'_, AppState>) -> CmdResult<SharesView> {
    state
        .run(|s| {
            let records = s.vault()?.list()?;
            let bucket = s.config()?.share_bucket.as_ref().map(|l| BucketView {
                bucket: l.bucket.clone(),
                endpoint: l.endpoint.clone(),
                region: l.region.clone(),
                public_url: l.public_url.clone(),
            });
            Ok(SharesView { bucket, shares: views(&records, now_ms()) })
        })
        .await
}

fn views(records: &[Record], now: u64) -> Vec<ShareView> {
    let projects: HashMap<Uuid, &str> = records
        .iter()
        .filter_map(|r| match r {
            Record::Project(p) => Some((p.id, p.name.as_str())),
            _ => None,
        })
        .collect();
    let envs: HashMap<Uuid, (&str, Uuid)> = records
        .iter()
        .filter_map(|r| match r {
            Record::Environment(e) if !e.deleted => Some((e.id, (e.name.as_str(), e.project_id))),
            _ => None,
        })
        .collect();
    let mut shares: Vec<_> = share::shares(records)
        .into_iter()
        .filter(|s| s.is_live(now) || s.in_bucket() || now.saturating_sub(s.revoked.unwrap_or(s.expires)) < ENDED_SHOWN_MS)
        .collect();
    shares.sort_by_key(|s| (!s.is_live(now), std::cmp::Reverse(s.updated.at_ms)));
    shares
        .into_iter()
        .map(|s| {
            let (status, problem) = match s.status(records, now) {
                Ok(Status::Current) => ("current", None),
                Ok(Status::OutOfDate) => ("outOfDate", None),
                Ok(Status::NeverPushed) => ("neverPushed", None),
                Ok(Status::Expired) => ("expired", None),
                Ok(Status::Revoked) => ("revoked", None),
                Err(e) => ("broken", Some(capitalized(&e.to_string()))),
            };
            let environments = s
                .environments
                .iter()
                .map(|env| {
                    let (environment, project) = match envs.get(&env.id) {
                        Some((name, project)) => (name.to_string(), projects.get(project).copied().unwrap_or("?").to_string()),
                        None => ("deleted environment".to_string(), String::new()),
                    };
                    ShareEnvView { project, environment, only: env.only.clone() }
                })
                .collect();
            ShareView {
                id: s.id,
                name: s.name.clone(),
                environments,
                status,
                problem,
                expires: s.expires,
                revoked: s.revoked,
                pushed_at: s.pushed.as_ref().filter(|p| !p.sha256.is_empty()).map(|p| p.at),
                lease_days_left: if s.is_live(now) { flow::lease_days_left(&s, now) } else { None },
                lease_days: s.lease_days,
            }
        })
        .collect()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareBucketInput {
    bucket: String,
    endpoint: String,
    region: String,
    public_url: String,
    access_key_id: String,
    secret_access_key: String,
}

/// Sets up the bucket shares go to: checks that it takes writes, serves
/// them at the public URL, hides listing and accepts expiry rules, then
/// keeps the write key in the keychain. Returns any warnings.
#[tauri::command]
pub async fn set_up_share_bucket(state: State<'_, AppState>, input: ShareBucketInput) -> CmdResult<Vec<String>> {
    let secret = Zeroizing::new(input.secret_access_key);
    let endpoint = input.endpoint.trim().trim_end_matches('/').to_string();
    if !endpoint.starts_with("https://") {
        return Err(CmdError::other("The endpoint must start with https://."));
    }
    let bucket = input.bucket.trim().to_string();
    if bucket.is_empty() {
        return Err(CmdError::other("Enter the bucket's name."));
    }
    let public_url = share::public_base(input.public_url.trim()).map_err(|e| CmdError::other(capitalized(&e.to_string())))?;
    let access_key_id = input.access_key_id.trim().to_string();
    if access_key_id.is_empty() || secret.trim().is_empty() {
        return Err(CmdError::other("Enter the access key ID and secret."));
    }
    let region = match input.region.trim() {
        "" => "auto".to_string(),
        region => region.to_string(),
    };
    let location = ShareBucketLocation { endpoint, region, bucket, public_url };
    let credentials = S3Credentials { access_key_id, secret_access_key: Zeroizing::new(secret.trim().to_string()) };
    let records = state.run(|s| Ok(s.vault()?.list()?)).await?;
    let checked = location.clone();
    let warnings = off_thread(move || {
        let store = S3ShareBucket::new(&checked, &credentials)?;
        let warnings = dotsafe_stores::share::probe(&store, &Fetcher::new())?;
        // Also shows the key may set lifecycle rules.
        store.set_expiry_rules(&share::expiry_rules(&records, now_ms())?)?;
        dotsafe_stores::save_share_credentials(&checked, &credentials)?;
        Ok(warnings)
    })
    .await?;
    state
        .run(move |s| {
            let mut config = s.config()?.clone();
            config.share_bucket = Some(location);
            config.save()?;
            s.config = Some(config);
            Ok(warnings.into_iter().map(|w| capitalized(&w)).collect())
        })
        .await
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewShare {
    name: String,
    environment_ids: Vec<Uuid>,
    /// Only these variables, in every environment.
    only: Option<Vec<String>>,
    days: u32,
    lease_days: Option<u32>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatedShare {
    id: Uuid,
    name: String,
    /// The token's start and end, so it can be recognised. The token itself
    /// stays in Rust and is copied with `copy_share_token`.
    preview: String,
    expires: u64,
}

/// Creates a standing share and writes its first snapshot. The token is
/// kept in the session until it's copied or the dialog closes.
#[tauri::command]
pub async fn create_share(state: State<'_, AppState>, share: NewShare) -> CmdResult<CreatedShare> {
    if share.days == 0 {
        return Err(CmdError::other("A share lasts at least a day."));
    }
    let only = share.only.map(|names| names.into_iter().map(|n| n.trim().to_string()).filter(|n| !n.is_empty()).collect::<Vec<_>>());
    let only = only.filter(|names| !names.is_empty());
    let environments: Vec<ShareEnv> = share.environment_ids.iter().map(|id| ShareEnv { id: *id, only: only.clone() }).collect();
    let expires = now_ms().saturating_add(u64::from(share.days) * DAY_MS);
    let lease_days = share.lease_days.unwrap_or(DEFAULT_LEASE_DAYS);
    let (vault, device, config) = shared(&state).await?;
    let name = share.name;
    let (created, token) = off_thread(move || {
        let bucket = bucket_of(&config)?.ok_or_else(|| CmdError::other("Set up a share bucket first."))?;
        Ok(flow::create(&vault, &bucket, device, &name, dotsafe_core::ShareMode::Standing, environments, expires, lease_days)?)
    })
    .await?;
    let preview = preview(&token);
    state
        .run(move |s| {
            s.share_token = Some(token);
            Ok(CreatedShare { id: created.id, name: created.name, preview, expires: created.expires })
        })
        .await
}

/// Copies the token of the share just created. Cleared from the clipboard
/// like any copied secret.
#[tauri::command]
pub async fn copy_share_token<R: Runtime>(app: AppHandle<R>, state: State<'_, AppState>) -> CmdResult<()> {
    state
        .run(move |s| {
            let token = s.share_token.clone().ok_or_else(|| {
                CmdError::other("The token is only kept until this closes. Revoke the share and make a new one.")
            })?;
            crate::state::copy_to_clipboard(&app, s, &token)
        })
        .await
}

/// Forgets the token of the share just created, once its dialog closes.
#[tauri::command]
pub async fn forget_share_token(state: State<'_, AppState>) -> CmdResult<()> {
    state
        .run(|s| {
            s.share_token = None;
            Ok(())
        })
        .await
}

/// Revokes a share: its snapshot is deleted, so the token stops working,
/// and what it could read goes on the rotation queue. Returns why the
/// snapshot couldn't be deleted yet, if it couldn't; the pusher tries again.
#[tauri::command]
pub async fn revoke_share(state: State<'_, AppState>, id: Uuid) -> CmdResult<Option<String>> {
    let (vault, device, config) = shared(&state).await?;
    let failed = off_thread(move || {
        let (bucket, problem) = match bucket_of(&config) {
            Ok(bucket) => (bucket, None),
            Err(e) => (None, Some(e.message)),
        };
        let failed = flow::revoke(&vault, bucket.as_ref().map(|b| b as &dyn ShareBucket), device, id)?;
        Ok(problem.or(failed).map(|why| capitalized(&why)))
    })
    .await?;
    state
        .run(move |s| {
            let mut config = s.config()?.clone();
            if !config.share_queue.contains(&id) {
                config.share_queue.push(id);
                config.save()?;
            }
            s.config = Some(config);
            Ok(failed)
        })
        .await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SharePushView {
    name: String,
    /// `pushed`, `renewed` or `deleted`.
    action: &'static str,
    error: Option<String>,
}

pub fn push_views(results: Vec<ShareResult>) -> Vec<SharePushView> {
    results
        .into_iter()
        .map(|r| SharePushView {
            name: r.name,
            action: match r.action {
                Action::Pushed => "pushed",
                Action::Renewed => "renewed",
                Action::Deleted => "deleted",
            },
            error: r.error.map(|e| capitalized(&e)),
        })
        .collect()
}

/// Pushes every share that's out of date now, and deletes the snapshots of
/// ended ones. The pusher does this on its own; this is "Push now".
#[tauri::command]
pub async fn push_shares(state: State<'_, AppState>) -> CmdResult<Vec<SharePushView>> {
    let (vault, device, config) = shared(&state).await?;
    off_thread(move || {
        let bucket = bucket_of(&config)?.ok_or_else(|| CmdError::other("Set up a share bucket first."))?;
        Ok(push_views(flow::sync(&vault, &bucket, device)?))
    })
    .await
}

/// The share bucket this Mac writes to, with its key from the keychain.
/// `None` when none is set up here.
pub fn bucket_of(config: &Config) -> CmdResult<Option<S3ShareBucket>> {
    dotsafe_stores::share_bucket(config).map_err(|e| CmdError::other(capitalized(&e.to_string())))
}

/// Brings shares in step with the vault, if there are any to keep: no
/// keychain read or network call otherwise.
fn push_all(vault: &Vault, config: &Config, device: Uuid) -> CmdResult<Vec<ShareResult>> {
    let now = now_ms();
    if !share::shares(&vault.list()?).iter().any(|s| s.is_live(now) || s.in_bucket()) {
        return Ok(Vec::new());
    }
    match bucket_of(config)? {
        Some(bucket) => Ok(flow::sync(vault, &bucket, device)?),
        None => Ok(Vec::new()),
    }
}

/// Pushes shares a few seconds after the vault changes (an edit here, the
/// CLI, or a sync from another device), and hourly to renew leases. Runs
/// off the session lock, like background sync. Emits `shares-changed` when
/// it did something, so Settings → Access refreshes.
pub fn spawn_pusher(state: AppState, app: AppHandle) {
    std::thread::spawn(move || {
        let mut watch = FolderWatch::default();
        let mut last: Option<Instant> = None;
        loop {
            std::thread::sleep(CHECK);
            let unlocked = state.0.lock().ok().and_then(|s| Some((s.vault.clone()?, s.config.clone()?)));
            let Some((vault, config)) = unlocked.filter(|(_, c)| c.share_bucket.is_some()) else {
                watch = FolderWatch::default();
                last = None;
                continue;
            };
            let changed = watch.poll(&vault);
            if !changed && last.is_some_and(|t| t.elapsed() < RENEW_CHECK) {
                continue;
            }
            last = Some(Instant::now());
            if let Ok(results) = push_all(&vault, &config, config.device_id)
                && !results.is_empty()
            {
                let _ = app.emit("shares-changed", ());
            }
        }
    });
}

/// How many live shares' leases end within [`LEASE_WARNING_DAYS`]: no device
/// has pushed them for a while, so their machines will stop working.
pub fn lease_ending_count(records: &[Record], now: u64) -> usize {
    share::shares(records)
        .iter()
        .filter(|s| s.is_live(now) && flow::lease_days_left(s, now).is_some_and(|days| days < LEASE_WARNING_DAYS))
        .count()
}

/// The shared vault, this device and the config, to work with off the
/// session lock.
async fn shared(state: &State<'_, AppState>) -> CmdResult<(Arc<Vault>, Uuid, Config)> {
    state
        .run(|s| {
            s.vault()?; // fails with `locked`
            let config = s.config()?.clone();
            Ok((s.vault.clone().expect("checked above"), config.device_id, config))
        })
        .await
}

async fn off_thread<T: Send + 'static>(f: impl FnOnce() -> CmdResult<T> + Send + 'static) -> CmdResult<T> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| CmdError::other(e.to_string()))?
}

fn preview(token: &str) -> String {
    let chars: Vec<char> = token.chars().collect();
    let head: String = chars.iter().take(10).collect();
    let tail: String = chars[chars.len().saturating_sub(4)..].iter().collect();
    format!("{head}…{tail}")
}

/// A message as a sentence: capitalized, ending in a full stop.
fn capitalized(s: &str) -> String {
    let mut chars = s.chars();
    let mut out: String = chars.next().map(|c| c.to_uppercase().chain(chars).collect()).unwrap_or_default();
    if !out.is_empty() && !out.ends_with('.') {
        out.push('.');
    }
    out
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}
