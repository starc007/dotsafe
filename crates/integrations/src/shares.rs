//! Shares as a push target: creating one, keeping its snapshot in step with
//! the vault, and revoking it. A share is pushed like a deploy target, so a
//! rotation reaches machines, and an old key isn't retired while a share
//! that holds it is out of date.
//!
//! The bucket is reached through [`ShareBucket`] (S3 in `crates/stores`), so
//! these flows run the same in the CLI and, later, the app.

use dotsafe_core::audit::{self, AuditAction, AuditEvent, AuditOutcome};
use dotsafe_core::share::{self, RENEW_AFTER_MS, Status};
use dotsafe_core::{Pushed, Record, Resolver, Share, ShareBucket, ShareEnv, ShareMode, Vault};
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::{Error, Result};

const DAY_MS: u64 = 24 * 60 * 60 * 1000;

/// What happened to one share.
#[derive(Debug)]
pub struct ShareResult {
    pub share: Uuid,
    pub name: String,
    pub action: Action,
    /// Why it failed, if it did. The share stays out of date and is tried
    /// again on the next write or sync.
    pub error: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// The vault changed, so a new snapshot was written.
    Pushed,
    /// Nothing changed, but the snapshot was written again to renew its lease.
    Renewed,
    /// The share was revoked or expired, so its snapshot was deleted.
    Deleted,
}

/// Creates a standing share of `environments` and writes its first
/// snapshot. Returns it and its token, which is shown once and never saved.
pub fn create(
    vault: &Vault,
    bucket: &dyn ShareBucket,
    device: Uuid,
    name: &str,
    environments: Vec<ShareEnv>,
    expires: u64,
    lease_days: u32,
) -> Result<(Share, Zeroizing<String>)> {
    let now = now_ms();
    let name = name.trim();
    if name.is_empty() || name.chars().any(|c| c.is_control()) {
        return Err(Error::Invalid("give the share a name, like ci or staging-vm".into()));
    }
    if environments.is_empty() {
        return Err(Error::Invalid("a share needs at least one environment".into()));
    }
    if expires <= now {
        return Err(Error::Invalid("a share has to expire in the future".into()));
    }
    if lease_days == 0 {
        return Err(Error::Invalid("a lease is at least one day".into()));
    }
    let mut records = vault.list()?;
    let existing = share::shares(&records);
    if existing.iter().any(|s| s.is_live(now) && s.name == name) {
        return Err(Error::Invalid(format!("there's already a live share named {name}")));
    }
    check_only(&records, &environments)?;

    let mut share = Share::new(name, ShareMode::Standing, environments, expires, lease_days, device, vault.stamp(device))?;
    let contents = share.contents(&records)?;
    records.push(Record::Share(share.clone()));
    // The bucket deletes it at expiry even if no device ever runs again.
    bucket.set_expiry_rules(&share::expiry_rules(&records, now)?)?;
    let sealed = share.seal(vault.id(), &contents, vault.stamp(device), now)?;
    bucket.put(&share.object_key()?, &sealed)?;
    share.pushed = Some(Pushed { sha256: share.digest(&contents), at: now });
    vault.put(Record::Share(share.clone()))?;
    let token = share.token(vault.id(), &bucket.public_url())?;
    let labels = share.labels(&records)?.join(", ");
    log(vault, device, AuditAction::Share, &share.name, format!("created for {labels}"), None);
    Ok((share, token))
}

/// Every `only` name is set in its environment, so a typo isn't silently a
/// share with nothing in it.
fn check_only(records: &[Record], environments: &[ShareEnv]) -> Result<()> {
    let resolver = Resolver::from_records(records);
    for env in environments {
        let Some(only) = &env.only else { continue };
        let names: Vec<String> = resolver.entries(env.id)?.into_iter().map(|e| e.key).collect();
        let missing: Vec<&str> = only.iter().filter(|n| !names.contains(n)).map(String::as_str).collect();
        if !missing.is_empty() {
            let env_name = resolver.chain(env.id)?.first().map(|e| e.name.clone()).unwrap_or_default();
            return Err(Error::Invalid(format!("{env_name} has no {}", missing.join(", "))));
        }
    }
    Ok(())
}

/// Revokes a share: marks it revoked (kept for the rotation queue), deletes
/// its snapshot so the URL stops working, and drops its expiry rule.
/// Without a bucket, or if the bucket can't be reached, the snapshot is
/// deleted by the next write or sync that can; the error is returned for
/// that case.
pub fn revoke(vault: &Vault, bucket: Option<&dyn ShareBucket>, device: Uuid, share_id: Uuid) -> Result<Option<String>> {
    let mut share = match vault.get(share_id)? {
        Some(Record::Share(s)) if !s.deleted => s,
        _ => return Err(Error::Invalid("that share no longer exists".into())),
    };
    let now = now_ms();
    if share.revoked.is_none() {
        share.revoked = Some(now);
        share.updated = vault.stamp(device);
        share = match vault.put(Record::Share(share))? {
            Record::Share(s) => s,
            _ => unreachable!("put returns the record it was given"),
        };
    }
    let outcome = match bucket {
        Some(bucket) => remove_objects(vault, bucket, share.clone()).err().map(|e| e.to_string()),
        None => Some("this machine has no share bucket set up".into()),
    };
    log(vault, device, AuditAction::Revoke, &share.name, "revoked".into(), outcome.clone());
    Ok(outcome)
}

/// Deletes a share's snapshot and records that it's gone, then updates the
/// bucket's expiry rules.
fn remove_objects(vault: &Vault, bucket: &dyn ShareBucket, mut share: Share) -> Result<()> {
    bucket.delete(&share.object_key()?)?;
    share.pushed = Some(Pushed { sha256: String::new(), at: now_ms() });
    vault.put(Record::Share(share))?;
    // Best effort: a stale rule only deletes objects that are already gone.
    let _ = bucket.set_expiry_rules(&share::expiry_rules(&vault.list()?, now_ms())?);
    Ok(())
}

/// Brings every share's snapshot in step with the vault: pushes live
/// standing shares whose values changed or whose lease is due for renewal,
/// and deletes snapshots of revoked or expired ones. Run after every write
/// and sync. Failures are reported per share, never raised, so a write
/// never fails because the bucket can't be reached.
pub fn sync(vault: &Vault, bucket: &dyn ShareBucket, device: Uuid) -> Result<Vec<ShareResult>> {
    let records = vault.list()?;
    let now = now_ms();
    let mut results = Vec::new();
    for share in share::shares(&records).into_iter().filter(|s| s.mode == ShareMode::Standing) {
        let (id, name) = (share.id, share.name.clone());
        if !share.is_live(now) {
            if share.in_bucket() {
                let error = remove_objects(vault, bucket, share).err().map(|e| e.to_string());
                results.push(ShareResult { share: id, name, action: Action::Deleted, error });
            }
            continue;
        }
        let outcome = push_one(vault, bucket, device, share, &records, now);
        let summary = match &outcome {
            Ok(Some(Action::Renewed)) => "renewed its lease".to_string(),
            _ => "pushed a new snapshot".to_string(),
        };
        match outcome {
            Ok(None) => {}
            Ok(Some(action)) => {
                log(vault, device, AuditAction::Push, &name, summary, None);
                results.push(ShareResult { share: id, name, action, error: None });
            }
            Err(e) => {
                log(vault, device, AuditAction::Push, &name, summary, Some(e.to_string()));
                results.push(ShareResult { share: id, name, action: Action::Pushed, error: Some(e.to_string()) });
            }
        }
    }
    Ok(results)
}

/// Pushes one share if it needs it. Returns what was done, or `None`.
fn push_one(
    vault: &Vault,
    bucket: &dyn ShareBucket,
    device: Uuid,
    mut share: Share,
    records: &[Record],
    now: u64,
) -> Result<Option<Action>> {
    let contents = share.contents(records)?;
    let digest = share.digest(&contents);
    let action = match &share.pushed {
        Some(p) if p.sha256 == digest && now.saturating_sub(p.at) < RENEW_AFTER_MS => return Ok(None),
        Some(p) if p.sha256 == digest => Action::Renewed,
        _ => Action::Pushed,
    };
    let sealed = share.seal(vault.id(), &contents, vault.stamp(device), now)?;
    bucket.put(&share.object_key()?, &sealed)?;
    share.pushed = Some(Pushed { sha256: digest, at: now });
    vault.put(Record::Share(share))?;
    Ok(Some(action))
}

/// Live standing shares that hold `secret` and whose snapshot is behind the
/// vault: retiring the secret's old value would break them.
pub fn holding_old_value(records: &[Record], secret: Uuid) -> Vec<String> {
    let now = now_ms();
    share::shares(records)
        .into_iter()
        .filter(|s| s.mode == ShareMode::Standing && s.is_live(now))
        .filter(|s| s.holds_secret(records, secret).unwrap_or(true))
        .filter(|s| !matches!(s.status(records, now), Ok(Status::Current)))
        .map(|s| s.name)
        .collect()
}

/// Days until a share's snapshot lease runs out, if it's live and pushed.
pub fn lease_days_left(share: &Share, now: u64) -> Option<u64> {
    let pushed = share.pushed.as_ref().filter(|p| !p.sha256.is_empty())?;
    Some(share.lease_until(pushed.at).saturating_sub(now) / DAY_MS)
}

fn log(vault: &Vault, device: Uuid, action: AuditAction, share: &str, summary: String, error: Option<String>) {
    let event = AuditEvent {
        at: now_ms(),
        actor: "You".into(),
        action,
        outcome: if error.is_some() { AuditOutcome::Failed } else { AuditOutcome::Allowed },
        summary: format!("share {share}: {summary}"),
        environment: String::new(),
        variables: Vec::new(),
        detail: error,
    };
    let _ = audit::record(vault, device, event);
}

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}
