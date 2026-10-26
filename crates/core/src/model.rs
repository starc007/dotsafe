//! What's inside an encrypted record, and how two versions of one record merge.
//!
//! Merging is last-writer-wins on a [`Stamp`]: per variable for environments,
//! per record otherwise. Deletes are kept as tombstones so they win over
//! older edits from another device instead of being undone by them.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{Error, Result};

/// When and where a change was made. Ordered by time, then device id as a
/// deterministic tiebreak, so every device picks the same winner.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Stamp {
    pub at_ms: u64,
    pub device: Uuid,
}

impl Stamp {
    pub fn now(device: Uuid) -> Self {
        let at_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        Stamp { at_ms, device }
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
}

impl Record {
    pub fn id(&self) -> Uuid {
        match self {
            Record::Project(p) => p.id,
            Record::Environment(e) => e.id,
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
    /// `None` marks a deleted variable (tombstone).
    value: Option<String>,
    stamp: Stamp,
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

    pub fn get(&self, key: &str) -> Option<&str> {
        self.vars.get(key)?.value.as_deref()
    }

    /// Live variables in key order.
    pub fn vars(&self) -> impl Iterator<Item = (&str, &str)> {
        self.vars.iter().filter_map(|(k, v)| Some((k.as_str(), v.value.as_deref()?)))
    }

    pub fn set(&mut self, key: impl Into<String>, value: impl Into<String>, stamp: Stamp) {
        self.vars.insert(key.into(), Var { value: Some(value.into()), stamp });
    }

    pub fn remove(&mut self, key: impl Into<String>, stamp: Stamp) {
        self.vars.insert(key.into(), Var { value: None, stamp });
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
