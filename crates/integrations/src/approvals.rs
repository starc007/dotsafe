//! Approve mode on the user's devices: reading what machines ask for and
//! answering. Shared by the app (its approval dialog, with Touch ID) and
//! `dotsafe share approve` (with the master password).
//!
//! A machine writes a sealed request into one of its share's slots and
//! rings the doorbell. A device reads the slots with its storage key,
//! shows each request, and writes back a sealed answer: the values the
//! share gives for the variables asked, resolved now, or a denial. Then it
//! clears the slot. The machine deletes the answer once it has read it.

use dotsafe_core::audit::{self, AuditAction, AuditEvent, AuditOutcome};
use dotsafe_core::share::{self, Answer, Request, SLOTS};
use dotsafe_core::{Record, Share, ShareBucket, ShareMode, Vault};
use uuid::Uuid;

use crate::{Error, Result};

/// A request waiting for an answer.
#[derive(Clone, Debug)]
pub struct Waiting {
    pub share: Uuid,
    pub share_name: String,
    pub slot: u8,
    pub request: Request,
}

/// How a device answers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    /// This run only.
    Once,
    /// This run, and the same command again for this long (milliseconds).
    For(u64),
    Deny,
}

/// The live approve shares in `records`.
pub fn approve_shares(records: &[Record], now: u64) -> Vec<Share> {
    share::shares(records).into_iter().filter(|s| s.mode == ShareMode::Approve && s.is_live(now)).collect()
}

/// Whether `share`'s doorbell rang: its ETag now, to compare with the last
/// one seen. `None` while nothing has rung.
pub fn doorbell(bucket: &dyn ShareBucket, share: &Share) -> Result<Option<String>> {
    Ok(bucket.etag(&share.doorbell_key()?)?)
}

/// Requests waiting in `shares`' slots. Expired or unreadable ones are
/// cleared, so a stale slot is free again.
pub fn waiting(vault: &Vault, bucket: &dyn ShareBucket, shares: &[Share], now: u64) -> Result<Vec<Waiting>> {
    let mut out = Vec::new();
    for share in shares {
        for slot in 0..SLOTS {
            let key = share.request_key(slot)?;
            let Some(bytes) = bucket.get(&key)? else { continue };
            match share.open_request(vault.id(), slot, &bytes) {
                Ok(request) if request.expires > now => {
                    out.push(Waiting { share: share.id, share_name: share.name.clone(), slot, request });
                }
                _ => {
                    let _ = bucket.delete(&key);
                }
            }
        }
    }
    out.sort_by_key(|w| w.request.at);
    Ok(out)
}

/// Answers a waiting request: with the values it asks for that the share
/// gives, resolved now, or a denial. Writes the answer, clears the slot and
/// logs it as an agent run would be, with the share as the one asking.
/// Returns the variables given.
pub fn answer(vault: &Vault, bucket: &dyn ShareBucket, device: Uuid, waiting: &Waiting, decision: Decision) -> Result<Vec<String>> {
    let records = vault.list()?;
    let now = now_ms();
    let share = approve_shares(&records, now)
        .into_iter()
        .find(|s| s.id == waiting.share)
        .ok_or_else(|| Error::Invalid(format!("share {} was revoked or has expired", waiting.share_name)))?;
    let request = &waiting.request;
    if request.expires <= now {
        let _ = bucket.delete(&share.request_key(waiting.slot)?);
        return Err(Error::Invalid("that request stopped waiting".into()));
    }
    let answer = match decision {
        Decision::Deny => Answer::denied(request.id, now),
        Decision::Once | Decision::For(_) => Answer {
            id: request.id,
            allowed: true,
            reuse_ms: if let Decision::For(ms) = decision { ms } else { 0 },
            values: share.values_for(&records, request)?,
            at: now,
        },
    };
    let given: Vec<String> = answer.values.keys().cloned().collect();
    bucket.put(&share.answer_key(waiting.slot)?, &share.seal_answer(vault.id(), waiting.slot, &answer)?)?;
    // The answer is written; a request left behind only expires.
    let _ = bucket.delete(&share.request_key(waiting.slot)?);
    let event = AuditEvent {
        at: now,
        actor: format!("share:{}", share.name),
        action: AuditAction::Run,
        outcome: if answer.allowed { AuditOutcome::Allowed } else { AuditOutcome::Denied },
        summary: request.command.join(" "),
        environment: format!("{}/{}", share.name, request.environment),
        variables: if answer.allowed { given.clone() } else { request.variables.clone() },
        detail: Some(format!("on {}, for {}", request.host, request.client)),
    };
    let _ = audit::record(vault, device, event);
    Ok(given)
}

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}
