//! What's inside an encrypted record, and how two versions of one record merge.
//!
//! Merging is last-writer-wins on a [`Stamp`]: per variable for environments,
//! per record otherwise. Deletes are kept as tombstones so they win over
//! older edits from another device instead of being undone by them.

use std::collections::BTreeMap;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::secret::Secret;

/// When and where a change was made: a hybrid logical clock reading. Ordered
/// by time, then counter, then device id as a deterministic tiebreak, so every
/// device picks the same winner. Get new stamps from [`crate::Vault::stamp`],
/// which keeps them ahead of every change the vault has seen.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Stamp {
    pub at_ms: u64,
    /// Orders changes within one millisecond, or after seeing a stamp from a
    /// device whose clock runs ahead. Absent in v1 records.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub counter: u32,
    pub device: Uuid,
}

impl Stamp {
    pub const fn new(at_ms: u64, device: Uuid) -> Self {
        Stamp { at_ms, counter: 0, device }
    }
}

fn is_zero(n: &u32) -> bool {
    *n == 0
}

/// A hybrid logical clock. A new stamp is never older than any stamp the
/// clock has observed, so an edit made after seeing another device's change
/// always orders after it, even if this device's clock is behind.
#[derive(Debug, Default)]
pub(crate) struct Clock {
    last: Mutex<(u64, u32)>,
}

impl Clock {
    pub(crate) fn observe(&self, stamp: Stamp) {
        let mut last = self.last.lock().unwrap_or_else(|e| e.into_inner());
        *last = (*last).max((stamp.at_ms, stamp.counter));
    }

    pub(crate) fn stamp(&self, device: Uuid) -> Stamp {
        let wall = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let mut last = self.last.lock().unwrap_or_else(|e| e.into_inner());
        *last = match *last {
            (at, _) if wall > at => (wall, 0),
            (at, u32::MAX) => (at + 1, 0),
            (at, counter) => (at, counter + 1),
        };
        Stamp { at_ms: last.0, counter: last.1, device }
    }
}

/// Whether `key` is a usable environment variable name: letters, digits and
/// underscores, not starting with a digit.
pub fn is_valid_var_name(key: &str) -> bool {
    let mut chars = key.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Record {
    Project(Project),
    Environment(Environment),
    Secret(Secret),
}

impl Record {
    pub fn id(&self) -> Uuid {
        match self {
            Record::Project(p) => p.id,
            Record::Environment(e) => e.id,
            Record::Secret(s) => s.id,
        }
    }

    /// Whether the record uses anything added in record format v3 (secrets,
    /// linked variables), which older clients must not read and rewrite.
    pub(crate) fn needs_v3(&self) -> bool {
        match self {
            Record::Project(_) => false,
            Record::Environment(e) => e.vars.values().any(|v| v.secret.is_some()),
            Record::Secret(_) => true,
        }
    }

    /// The newest change in the record. A copy with an older newest change
    /// than one already seen is out of date.
    pub fn max_stamp(&self) -> Stamp {
        match self {
            Record::Project(p) => p.updated,
            Record::Environment(e) => e.vars.values().map(|v| v.stamp).fold(e.updated, Stamp::max),
            Record::Secret(s) => s.max_stamp(),
        }
    }

    /// Combines two versions of the same record. Order doesn't matter.
    pub fn merge(self, other: Record) -> Result<Record> {
        match (self, other) {
            (Record::Project(a), Record::Project(b)) if a.id == b.id => {
                Ok(Record::Project(if b.updated > a.updated { b } else { a }))
            }
            (Record::Environment(a), Record::Environment(b)) if a.id == b.id => {
                Ok(Record::Environment(a.merge(b)))
            }
            (Record::Secret(a), Record::Secret(b)) if a.id == b.id => Ok(Record::Secret(a.merge(b))),
            (a, _) => Err(Error::Format(format!("cannot merge different records into {}", a.id()))),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Project {
    pub id: Uuid,
    pub name: String,
    pub deleted: bool,
    pub updated: Stamp,
}

impl Project {
    pub fn new(name: impl Into<String>, stamp: Stamp) -> Self {
        Project { id: Uuid::new_v4(), name: name.into(), deleted: false, updated: stamp }
    }
}

/// One set of variables for a project, such as "development" or "production".
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Environment {
    pub id: Uuid,
    pub project_id: Uuid,
    pub name: String,
    pub deleted: bool,
    /// Stamp for `name` and `deleted`.
    pub updated: Stamp,
    vars: BTreeMap<String, Var>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Var {
    /// The value, for a variable set directly.
    value: Option<String>,
    /// The shared secret it's linked to. Neither set marks a deleted variable
    /// (tombstone).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    secret: Option<Uuid>,
    stamp: Stamp,
}

/// What a variable holds: its own value, or a link to a shared secret.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Entry<'a> {
    Value(&'a str),
    Secret(Uuid),
}

impl Var {
    fn entry(&self) -> Option<Entry<'_>> {
        match (&self.value, self.secret) {
            (_, Some(id)) => Some(Entry::Secret(id)),
            (Some(value), None) => Some(Entry::Value(value)),
            (None, None) => None,
        }
    }
}

impl Environment {
    pub fn new(project_id: Uuid, name: impl Into<String>, stamp: Stamp) -> Self {
        let name = name.into();
        Environment {
            id: Environment::id_for(project_id, &name),
            project_id,
            name,
            deleted: false,
            updated: stamp,
            vars: BTreeMap::new(),
        }
    }

    /// The id a new environment named `name` should get. It's derived from the
    /// project and name, so two devices that each create "development" before
    /// syncing end up with one record that merges, not two.
    pub fn id_for(project_id: Uuid, name: &str) -> Uuid {
        Uuid::new_v5(&project_id, name.as_bytes())
    }

    /// What `key` holds. Resolve a linked secret with [`Secrets::value`].
    pub fn entry(&self, key: &str) -> Option<Entry<'_>> {
        self.vars.get(key)?.entry()
    }

    /// The variable's own value: `None` if it isn't set or links to a secret.
    pub fn value(&self, key: &str) -> Option<&str> {
        match self.entry(key)? {
            Entry::Value(value) => Some(value),
            Entry::Secret(_) => None,
        }
    }

    /// Live variables in key order.
    pub fn entries(&self) -> impl Iterator<Item = (&str, Entry<'_>)> {
        self.vars.iter().filter_map(|(k, v)| Some((k.as_str(), v.entry()?)))
    }

    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.entries().map(|(k, _)| k)
    }

    pub fn set(&mut self, key: impl Into<String>, value: impl Into<String>, stamp: Stamp) {
        self.vars.insert(key.into(), Var { value: Some(value.into()), secret: None, stamp });
    }

    /// Links `key` to a shared secret, replacing any value it had.
    pub fn link(&mut self, key: impl Into<String>, secret: Uuid, stamp: Stamp) {
        self.vars.insert(key.into(), Var { value: None, secret: Some(secret), stamp });
    }

    pub fn remove(&mut self, key: impl Into<String>, stamp: Stamp) {
        self.vars.insert(key.into(), Var { value: None, secret: None, stamp });
    }

    fn merge(self, other: Environment) -> Environment {
        let (mut newer, older) = if other.updated > self.updated { (other, self) } else { (self, other) };
        for (key, theirs) in older.vars {
            match newer.vars.get(&key) {
                Some(ours) if ours.stamp >= theirs.stamp => {}
                _ => {
                    newer.vars.insert(key, theirs);
                }
            }
        }
        newer
    }
}

/// The secrets in a vault, for resolving linked variables.
#[derive(Clone, Debug, Default)]
pub struct Secrets(BTreeMap<Uuid, Secret>);

impl Secrets {
    /// The live secrets among `records`.
    pub fn from_records<'a>(records: impl IntoIterator<Item = &'a Record>) -> Secrets {
        Secrets(
            records
                .into_iter()
                .filter_map(|r| match r {
                    Record::Secret(s) if !s.deleted => Some((s.id, s.clone())),
                    _ => None,
                })
                .collect(),
        )
    }

    pub fn get(&self, id: Uuid) -> Option<&Secret> {
        self.0.get(&id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Secret> {
        self.0.values()
    }

    /// The value an entry stands for: its own, or its secret's current one.
    /// `None` for a link to a secret that's gone or has no current value.
    pub fn value<'a>(&'a self, entry: Entry<'a>) -> Option<&'a str> {
        match entry {
            Entry::Value(value) => Some(value),
            Entry::Secret(id) => self.get(id)?.current_value(),
        }
    }

    /// Every variable in `env` that has a value, with links resolved.
    pub fn resolve<'a>(&'a self, env: &'a Environment) -> impl Iterator<Item = (&'a str, &'a str)> {
        env.entries().filter_map(|(key, entry)| Some((key, self.value(entry)?)))
    }
}