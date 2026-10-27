//! The audit log: what agents asked for and what came of it, and rotations
//! through providers and pushes to deploy targets. Kept in the vault, so
//! it's encrypted like everything else and syncs across devices.
//!
//! One record per device per day keeps records small and writes conflict-free
//! (only one device writes each). Events merge by id. After 90 days a day's
//! record is emptied and tombstoned.
//!
//! Any device holding the vault key could forge or drop events, so the log
//! shows what happened through dotsafe; it isn't evidence against a
//! compromised device.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::Result;
use crate::model::{Record, Stamp};
use crate::vault::Vault;

const DAY_MS: u64 = 24 * 60 * 60 * 1000;
pub const RETENTION_DAYS: u64 = 90;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditDay {
    pub id: Uuid,
    pub device: Uuid,
    /// Days since the Unix epoch, UTC.
    pub day: u64,
    pub deleted: bool,
    pub updated: Stamp,
    events: BTreeMap<Uuid, AuditEvent>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditEvent {
    /// Unix milliseconds.
    pub at: u64,
    /// Who asked, as the agent described itself ("Claude Code"), or "You"
    /// for rotations and pushes.
    pub actor: String,
    pub action: AuditAction,
    pub outcome: AuditOutcome,
    /// The command line, or the variable created.
    pub summary: String,
    /// "project/environment".
    pub environment: String,
    /// Variables involved (names only).
    #[serde(default)]
    pub variables: Vec<String>,
    /// Exit code, refusal reason and similar, for people.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditAction {
    Run,
    Generate,
    /// A provider issued a new key. Record format v4.
    Rotate,
    /// A value was pushed to a deploy target. Record format v4.
    Push,
    /// An old key was revoked at its provider. Record format v4.
    Revoke,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditOutcome {
    /// Approved (or covered by a grant) and done.
    Allowed,
    Denied,
    /// Never allowed for agents.
    Refused,
    /// Approved, but it couldn't be carried out.
    Failed,
}

impl AuditDay {
    pub fn id_for(device: Uuid, day: u64) -> Uuid {
        Uuid::new_v5(&device, format!("audit:{day}").as_bytes())
    }

    pub fn new(device: Uuid, day: u64, stamp: Stamp) -> AuditDay {
        AuditDay {
            id: AuditDay::id_for(device, day),
            device,
            day,
            deleted: false,
            updated: stamp,
            events: BTreeMap::new(),
        }
    }

    pub fn add(&mut self, event: AuditEvent, stamp: Stamp) {
        self.events.insert(Uuid::new_v4(), event);
        self.updated = stamp;
    }

    pub fn events(&self) -> impl Iterator<Item = &AuditEvent> {
        self.events.values()
    }

    /// Holds actions added with integrations, which clients from before
    /// record format v4 can't read.
    pub(crate) fn needs_v4(&self) -> bool {
        self.events.values().any(|e| matches!(e.action, AuditAction::Rotate | AuditAction::Push | AuditAction::Revoke))
    }

    /// Past the retention period.
    pub fn expired(&self, now_ms: u64) -> bool {
        now_ms / DAY_MS > self.day + RETENTION_DAYS
    }

    /// Empties the record and tombstones it.
    pub fn prune(&mut self, stamp: Stamp) {
        self.events.clear();
        self.deleted = true;
        self.updated = stamp;
    }

    pub(crate) fn merge(self, other: AuditDay) -> AuditDay {
        let (mut newer, older) = if other.updated > self.updated { (other, self) } else { (self, other) };
        if newer.deleted || older.deleted {
            newer.events.clear();
            newer.deleted = true;
            return newer;
        }
        for (id, event) in older.events {
            newer.events.entry(id).or_insert(event);
        }
        newer
    }
}

/// Appends an event to this device's record for today, and prunes expired days.
pub fn record(vault: &Vault, device: Uuid, event: AuditEvent) -> Result<()> {
    let day = event.at / DAY_MS;
    let mut today = match vault.get(AuditDay::id_for(device, day))? {
        Some(Record::Audit(existing)) if !existing.deleted => existing,
        _ => AuditDay::new(device, day, vault.stamp(device)),
    };
    let now = event.at;
    today.add(event, vault.stamp(device));
    vault.put(Record::Audit(today))?;

    for record in vault.list()? {
        if let Record::Audit(mut old) = record
            && !old.deleted
            && old.expired(now)
        {
            old.prune(vault.stamp(device));
            vault.put(Record::Audit(old))?;
        }
    }
    Ok(())
}

/// The newest events from every device, newest first.
pub fn recent(vault: &Vault, limit: usize) -> Result<Vec<AuditEvent>> {
    let mut events: Vec<AuditEvent> = vault
        .list()?
        .into_iter()
        .filter_map(|r| match r {
            Record::Audit(day) if !day.deleted => Some(day.events.into_values()),
            _ => None,
        })
        .flatten()
        .collect();
    events.sort_by_key(|e| std::cmp::Reverse(e.at));
    events.truncate(limit);
    Ok(events)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(at: u64, summary: &str) -> AuditEvent {
        AuditEvent {
            at,
            actor: "Claude Code".into(),
            action: AuditAction::Run,
            outcome: AuditOutcome::Allowed,
            summary: summary.into(),
            environment: "api/development".into(),
            variables: vec!["API_KEY".into()],
            detail: Some("exit 0".into()),
        }
    }

    #[test]
    fn copies_merge_and_tombstones_win() {
        let device = Uuid::new_v4();
        let base = AuditDay::new(device, 100, Stamp::new(1, device));
        let (mut a, mut b) = (base.clone(), base.clone());
        a.add(event(1, "bun test"), Stamp::new(2, device));
        b.add(event(2, "cargo test"), Stamp::new(3, device));
        let merged = a.clone().merge(b.clone());
        assert_eq!(merged.events().count(), 2);

        let mut pruned = a.clone();
        pruned.prune(Stamp::new(4, device));
        assert_eq!(b.merge(pruned).events().count(), 0, "a pruned day stays empty");
    }

    #[test]
    fn days_expire_after_retention() {
        let device = Uuid::new_v4();
        let day = AuditDay::new(device, 1000, Stamp::new(1, device));
        assert!(!day.expired((1000 + RETENTION_DAYS) * DAY_MS));
        assert!(day.expired((1001 + RETENTION_DAYS) * DAY_MS));
    }
}
