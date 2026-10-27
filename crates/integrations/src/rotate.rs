//! Rotation through a provider, and pushes to deploy targets, written back
//! to the vault.
//!
//! The order matters. A new key is saved to the vault the moment the
//! provider returns it (or you paste it, for guided providers), before
//! verifying or pushing, so a crash or a lost connection never loses the
//! only working key (with Cloudflare's roll, the old one is already dead by
//! then).

use dotsafe_core::audit::{self, AuditAction, AuditEvent, AuditOutcome};
use dotsafe_core::{Binding, Connection, Pushed, Record, Secret, VersionState, Vault, crypto};
use uuid::Uuid;

use zeroize::Zeroizing;

use crate::{Client, Error, NewKey, Result, service};

/// How a provider rotation went.
pub struct Rotated {
    /// The new key is live. False when it failed its check and was left
    /// staged.
    pub promoted: bool,
    /// Why the new key failed its check, if it did. Without overlap it went
    /// live anyway: the old key was already dead.
    pub staged: Option<String>,
    /// The old key was revoked and retired (leaked, or a provider like
    /// Cloudflare that ends it on rotation).
    pub revoked_old: bool,
    pub pushes: Vec<PushResult>,
}

/// One target's push.
pub struct PushResult {
    pub target: Uuid,
    pub name: String,
    pub connection: String,
    pub error: Option<String>,
}

/// The secret, its binding and the provider connection, read fresh.
struct Context {
    secret: Secret,
    binding: Binding,
}

fn load(vault: &Vault, secret_id: Uuid) -> Result<Context> {
    let secret = match vault.get(secret_id)? {
        Some(Record::Secret(s)) if !s.deleted => s,
        _ => return Err(Error::Invalid("that secret no longer exists".into())),
    };
    let binding = match vault.get(Binding::id_for(secret_id))? {
        Some(Record::Binding(b)) => b,
        _ => Binding::new(secret_id, vault.stamp(Uuid::nil())),
    };
    Ok(Context { secret, binding })
}

/// A connection and its admin token. Services that need no token (Stripe)
/// get an empty one.
pub fn connection(vault: &Vault, id: Uuid) -> Result<(Connection, Zeroizing<String>)> {
    let connection = match vault.get(id)? {
        Some(Record::Connection(c)) if !c.deleted => c,
        _ => return Err(Error::Invalid("that connection was removed".into())),
    };
    let token = match connection.credential.map(|id| vault.get(id)).transpose()?.flatten() {
        Some(Record::Secret(s)) if !s.deleted => s.current_value().map(|v| Zeroizing::new(v.to_string())),
        _ => None,
    };
    let token = match token {
        Some(token) => token,
        None if !service(&connection.service)?.needs_token => Zeroizing::new(String::new()),
        None => return Err(Error::Invalid(format!("{}'s admin token is missing", connection.name))),
    };
    Ok((connection, token))
}

/// Records the provider's id for the secret's current key, where the
/// provider can find it from the key (Anthropic), so it can be revoked after
/// the next rotation. Returns whether it was found.
pub fn adopt_current_key(vault: &Vault, client: &Client, device: Uuid, secret_id: Uuid) -> Result<bool> {
    let ctx = load(vault, secret_id)?;
    let (Some(link), Some((version, v))) = (&ctx.binding.provider, ctx.secret.current()) else { return Ok(false) };
    let Some(value) = &v.value else { return Ok(false) };
    if ctx.binding.keys.contains_key(&version) {
        return Ok(true);
    }
    let (connection, token) = connection(vault, link.connection)?;
    let Some(key_id) = client.find_key_id(&connection, &token, value)? else { return Ok(false) };
    let mut binding = ctx.binding;
    binding.keys.insert(version, key_id);
    binding.updated = vault.stamp(device);
    vault.put(Record::Binding(binding))?;
    Ok(true)
}

/// Rotates `secret_id` through its provider: a new key, saved, verified,
/// promoted and pushed to every target. Guided providers (Anthropic,
/// Stripe) take the key you made in their console as `pasted`. With
/// `leaked`, or for a provider with no overlap, the old key is revoked
/// straight away; otherwise it stays valid as the previous version until
/// [`retire`].
pub fn rotate(
    vault: &Vault,
    client: &Client,
    device: Uuid,
    secret_id: Uuid,
    pasted: Option<&str>,
    leaked: bool,
) -> Result<Rotated> {
    let ctx = load(vault, secret_id)?;
    let link = ctx.binding.provider.clone().ok_or_else(|| Error::Invalid(format!("{} has no provider", ctx.secret.name)))?;
    if ctx.secret.pending().is_some() {
        return Err(Error::Invalid(format!("{} already has a staged value; promote or cancel it first", ctx.secret.name)));
    }
    let (connection, token) = connection(vault, link.connection)?;
    let svc = service(&connection.service)?;
    let overlap = svc.overlap;
    let secret_name = ctx.secret.name.clone();

    let new = match (svc.guided, pasted) {
        (true, Some(value)) if !value.trim().is_empty() => {
            // Found now if the provider can (Anthropic), so it can be revoked later.
            let key_id = client.find_key_id(&connection, &token, value.trim()).ok().flatten().unwrap_or_default();
            NewKey { value: Zeroizing::new(value.trim().to_string()), key_id }
        }
        (true, _) => return Err(Error::Invalid(format!("paste the new key you made in {}", svc.label))),
        (false, Some(_)) => return Err(Error::Invalid(format!("{} issues its own keys", svc.label))),
        (false, None) => {
            let name = format!("dotsafe {} {}", ctx.secret.name, now_ms() / 1000);
            match client.create_key(&connection, &token, &link, &name) {
                Ok(new) => new,
                Err(e) => {
                    let summary = format!("new key from {}", connection.name);
                    log(vault, device, AuditAction::Rotate, &secret_name, summary, Some(e.to_string()));
                    return Err(e);
                }
            }
        }
    };
    let source = if svc.guided { format!("Made in {}", svc.label) } else { format!("Issued by {}", connection.name) };

    // Save it before anything else can fail.
    let mut secret = ctx.secret;
    let version = secret.stage(new.value.as_str(), Some(source), vault.stamp(device))?;
    let mut binding = ctx.binding;
    if !new.key_id.is_empty() {
        binding.keys.insert(version, new.key_id.clone());
    }
    binding.updated = vault.stamp(device);
    vault.put(Record::Secret(secret.clone()))?;
    vault.put(Record::Binding(binding))?;

    // Without overlap the old key is already dead, so the new one must go
    // live whatever the check says.
    let key_id = (!new.key_id.is_empty()).then_some(new.key_id.as_str());
    let check = client.verify_key(&connection, &new.value, key_id);
    if let Err(e) = &check
        && overlap
        && !leaked
    {
        let why = format!("the new key failed its check and is staged: {e}");
        log(vault, device, AuditAction::Rotate, &secret_name, format!("new key from {}", connection.name), Some(why));
        return Ok(Rotated { promoted: false, staged: Some(e.to_string()), revoked_old: false, pushes: Vec::new() });
    }
    secret = fresh_secret(vault, secret_id)?;
    secret.promote(vault.stamp(device))?;
    vault.put(Record::Secret(secret))?;
    let summary = format!("new key from {}{}", connection.name, if leaked { " (leaked)" } else { "" });
    log(vault, device, AuditAction::Rotate, &secret_name, summary, None);

    // AWS: the access key ID changes too.
    let mut pushes = push(vault, client, device, secret_id)?;
    if let Some(companion) = companion(&link)
        && !new.key_id.is_empty()
    {
        let mut id_secret = fresh_secret(vault, companion)?;
        id_secret.rotate(new.key_id.as_str(), Some(format!("Issued by {}", connection.name)), vault.stamp(device))?;
        vault.put(Record::Secret(id_secret))?;
        pushes.extend(push(vault, client, device, companion)?);
    }
    // Stripe's old key can only be revoked in its dashboard, so a leak there
    // leaves that step to you.
    let revoked_old = if (leaked || !overlap) && !svc.manual_revoke {
        retire(vault, client, device, secret_id, false)?;
        true
    } else {
        false
    };
    Ok(Rotated { promoted: true, staged: check.err().map(|e| e.to_string()), revoked_old, pushes })
}

/// Revokes the previous versions' keys at the provider, then retires them.
/// With `by_hand` (you revoked it in the provider's console, or it can only
/// be revoked there, like Stripe's), or without a provider, it just retires
/// them. Stops at the first revoke that fails, so nothing is marked retired
/// that still works.
pub fn retire(vault: &Vault, client: &Client, device: Uuid, secret_id: Uuid, by_hand: bool) -> Result<usize> {
    let ctx = load(vault, secret_id)?;
    if let Some(link) = &ctx.binding.provider
        && !by_hand
    {
        let (connection, token) = connection(vault, link.connection)?;
        let svc = service(&connection.service)?;
        if svc.manual_revoke {
            return Err(Error::Invalid(format!(
                "{} keys can only be revoked in its dashboard; do that, then mark the old one retired",
                svc.label
            )));
        }
        for (version, v) in ctx.secret.versions() {
            if v.state != VersionState::Previous {
                continue;
            }
            let key_id = match (ctx.binding.keys.get(&version), &v.value) {
                (Some(id), _) => Some(id.clone()),
                // Look it up now, if the provider can.
                (None, Some(value)) => client.find_key_id(&connection, &token, value)?,
                (None, None) => None,
            };
            let Some(key_id) = key_id else {
                return Err(Error::Invalid(format!(
                    "dotsafe doesn't know which key at {} the old value is; revoke it there, then mark it retired",
                    connection.name
                )));
            };
            let result = client.revoke_key(&connection, &token, link, &key_id);
            let summary = format!("revoked the old key at {}", connection.name);
            log(vault, device, AuditAction::Revoke, &ctx.secret.name, summary, result.as_ref().err().map(|e| e.to_string()));
            result?;
        }
    }
    let mut secret = ctx.secret;
    let retired = secret.retire_previous(vault.stamp(device));
    vault.put(Record::Secret(secret))?;
    if let Some(companion) = ctx.binding.provider.as_ref().and_then(companion)
        && let Ok(mut id_secret) = fresh_secret(vault, companion)
    {
        id_secret.retire_previous(vault.stamp(device));
        vault.put(Record::Secret(id_secret))?;
    }
    Ok(retired)
}

/// The shared secret that holds an AWS key's access key ID, if set.
fn companion(link: &dotsafe_core::ProviderLink) -> Option<Uuid> {
    link.settings.get("access_key_id_secret").and_then(|id| id.parse().ok())
}

/// Pushes the secret's current value to each of its targets and records
/// what was pushed. Failures are reported per target, not raised.
pub fn push(vault: &Vault, client: &Client, device: Uuid, secret_id: Uuid) -> Result<Vec<PushResult>> {
    let ctx = load(vault, secret_id)?;
    let value = Zeroizing::new(
        ctx.secret.current_value().ok_or_else(|| Error::Invalid(format!("{} has no current value", ctx.secret.name)))?.to_string(),
    );
    let mut binding = ctx.binding;
    let mut results = Vec::new();
    let targets: Vec<_> = binding.targets().map(|(id, t)| (id, t.clone())).collect();
    for (id, target) in targets {
        let outcome = connection(vault, target.connection)
            .and_then(|(connection, token)| client.push(&connection, &token, &target, &value).map(|_| connection.name));
        let where_to = match &outcome {
            Ok(name) => format!("pushed to {name} as {}", target.name),
            Err(_) => format!("push as {}", target.name),
        };
        log(vault, device, AuditAction::Push, &ctx.secret.name, where_to, outcome.as_ref().err().map(|e| e.to_string()));
        let (connection_name, error) = match outcome {
            Ok(name) => {
                let stored = binding.targets.get_mut(&id).expect("listed above");
                stored.pushed = Some(Pushed { sha256: crypto::fingerprint(&value), at: now_ms() });
                stored.changed = vault.stamp(device);
                (name, None)
            }
            Err(e) => (String::new(), Some(e.to_string())),
        };
        results.push(PushResult { target: id, name: target.name, connection: connection_name, error });
    }
    if results.iter().any(|r| r.error.is_none()) {
        vault.put(Record::Binding(binding))?;
    }
    Ok(results)
}

/// Writes an audit event. A failure to log never fails the rotation itself.
fn log(vault: &Vault, device: Uuid, action: AuditAction, secret: &str, summary: String, error: Option<String>) {
    let event = AuditEvent {
        at: now_ms(),
        actor: "You".into(),
        action,
        outcome: if error.is_some() { AuditOutcome::Failed } else { AuditOutcome::Allowed },
        summary: format!("{secret}: {summary}"),
        environment: String::new(),
        variables: vec![secret.to_string()],
        detail: error,
    };
    let _ = audit::record(vault, device, event);
}

fn fresh_secret(vault: &Vault, id: Uuid) -> Result<Secret> {
    match vault.get(id)? {
        Some(Record::Secret(s)) => Ok(s),
        _ => Err(Error::Invalid("that secret no longer exists".into())),
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}
