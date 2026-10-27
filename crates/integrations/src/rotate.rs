//! Rotation through a provider, and pushes to deploy targets, written back
//! to the vault.
//!
//! The order matters. A new key is saved to the vault the moment the
//! provider returns it, before verifying or pushing, so a crash or a lost
//! connection never loses the only working key (with Cloudflare's roll, the
//! old one is already dead by then).

use dotsafe_core::audit::{self, AuditAction, AuditEvent, AuditOutcome};
use dotsafe_core::{Binding, Connection, Pushed, Record, Secret, VersionState, Vault, crypto};
use uuid::Uuid;

use crate::{Client, Error, Result, service};

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

/// A connection and its admin token.
pub fn connection(vault: &Vault, id: Uuid) -> Result<(Connection, zeroize::Zeroizing<String>)> {
    let connection = match vault.get(id)? {
        Some(Record::Connection(c)) if !c.deleted => c,
        _ => return Err(Error::Invalid("that connection was removed".into())),
    };
    let token = match vault.get(connection.credential)? {
        Some(Record::Secret(s)) if !s.deleted => s.current_value().map(|v| zeroize::Zeroizing::new(v.to_string())),
        _ => None,
    };
    let token = token.ok_or_else(|| Error::Invalid(format!("{}'s admin token is missing", connection.name)))?;
    Ok((connection, token))
}

/// Rotates `secret_id` through its provider: a new key, saved, verified,
/// promoted and pushed to every target. With `leaked`, or for a provider
/// with no overlap, the old key is revoked straight away; otherwise it stays
/// valid as the previous version until [`retire`].
pub fn rotate(vault: &Vault, client: &Client, device: Uuid, secret_id: Uuid, leaked: bool) -> Result<Rotated> {
    let ctx = load(vault, secret_id)?;
    let link = ctx.binding.provider.clone().ok_or_else(|| Error::Invalid(format!("{} has no provider", ctx.secret.name)))?;
    if ctx.secret.pending().is_some() {
        return Err(Error::Invalid(format!("{} already has a staged value; promote or cancel it first", ctx.secret.name)));
    }
    let (connection, token) = connection(vault, link.connection)?;
    let overlap = service(&connection.service)?.overlap;

    let name = format!("dotsafe {} {}", ctx.secret.name, now_ms() / 1000);
    let secret_name = ctx.secret.name.clone();
    let new = match client.create_key(&connection, &token, &link, &name) {
        Ok(new) => new,
        Err(e) => {
            log(vault, device, AuditAction::Rotate, &secret_name, format!("new key from {}", connection.name), Some(e.to_string()));
            return Err(e);
        }
    };

    // Save it before anything else can fail.
    let mut secret = ctx.secret;
    let version = secret.stage(new.value.as_str(), Some(format!("Issued by {}", connection.name)), vault.stamp(device))?;
    let mut binding = ctx.binding;
    binding.keys.insert(version, new.key_id.clone());
    binding.updated = vault.stamp(device);
    vault.put(Record::Secret(secret.clone()))?;
    vault.put(Record::Binding(binding))?;

    // Without overlap the old key is already dead, so the new one must go
    // live whatever the check says.
    let check = client.verify_key(&connection, &new.value);
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

    let pushes = push(vault, client, device, secret_id)?;
    let revoked_old = if leaked || !overlap {
        retire(vault, client, device, secret_id)?;
        true
    } else {
        false
    };
    Ok(Rotated { promoted: true, staged: check.err().map(|e| e.to_string()), revoked_old, pushes })
}

/// Revokes the previous versions' keys at the provider, then retires them.
/// Without a provider, it just retires them. Stops at the first revoke that
/// fails, so nothing is marked retired that still works.
pub fn retire(vault: &Vault, client: &Client, device: Uuid, secret_id: Uuid) -> Result<usize> {
    let ctx = load(vault, secret_id)?;
    if let Some(link) = &ctx.binding.provider {
        let (connection, token) = connection(vault, link.connection)?;
        for (version, v) in ctx.secret.versions() {
            if v.state == VersionState::Previous
                && let Some(key_id) = ctx.binding.keys.get(&version)
            {
                let result = client.revoke_key(&connection, &token, key_id);
                let summary = format!("revoked the old key at {}", connection.name);
                log(vault, device, AuditAction::Revoke, &ctx.secret.name, summary, result.as_ref().err().map(|e| e.to_string()));
                result?;
            }
        }
    }
    let mut secret = ctx.secret;
    let retired = secret.retire_previous(vault.stamp(device));
    vault.put(Record::Secret(secret))?;
    Ok(retired)
}

/// Pushes the secret's current value to each of its targets and records
/// what was pushed. Failures are reported per target, not raised.
pub fn push(vault: &Vault, client: &Client, device: Uuid, secret_id: Uuid) -> Result<Vec<PushResult>> {
    let ctx = load(vault, secret_id)?;
    let value = zeroize::Zeroizing::new(
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
