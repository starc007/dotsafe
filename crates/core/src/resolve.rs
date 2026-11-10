//! What an environment's variables actually are: inherited from its parent
//! chain, overridden or removed here, linked secrets resolved, and
//! `${{ NAME }}` references expanded.
//!
//! Every consumer of values (`dotsafe run`, export, reveal and copy, agents)
//! goes through [`Resolver`], so they can't disagree. A reference to a
//! missing variable, a loop of references, or a loop of parents is an error,
//! never a silently wrong value.

use std::collections::{BTreeMap, HashMap};

use uuid::Uuid;

use crate::error::{Error, Result};
use crate::model::{Entry, Environment, Record, Secrets};

/// How deep a parent chain may go.
const MAX_DEPTH: usize = 8;
/// How deep references may nest.
const MAX_NESTING: usize = 16;

/// A variable as an environment sees it, and where it comes from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Effective {
    pub key: String,
    /// The environment that sets it: this one, or an ancestor.
    pub from: Uuid,
    /// Its raw content: a value (possibly with references) or a secret link.
    pub entry: Raw,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Raw {
    Value(String),
    Secret(Uuid),
}

/// The live environments and secrets of a vault, for resolving.
pub struct Resolver {
    envs: HashMap<Uuid, Environment>,
    secrets: Secrets,
}

impl Resolver {
    pub fn from_records(records: &[Record]) -> Resolver {
        let envs = records
            .iter()
            .filter_map(|r| match r {
                Record::Environment(e) if !e.deleted => Some((e.id, e.clone())),
                _ => None,
            })
            .collect();
        Resolver { envs, secrets: Secrets::from_records(records) }
    }

    pub fn secrets(&self) -> &Secrets {
        &self.secrets
    }

    /// `env` and its ancestors, nearest first.
    pub fn chain(&self, env: Uuid) -> Result<Vec<&Environment>> {
        let mut chain: Vec<&Environment> = Vec::new();
        let mut next = Some(env);
        while let Some(id) = next {
            let current = self.envs.get(&id).ok_or_else(|| match chain.last() {
                Some(child) => Error::Reference(format!("{} inherits from an environment that was deleted", child.name)),
                None => Error::Reference("that environment no longer exists".into()),
            })?;
            if chain.iter().any(|e| e.id == id) {
                return Err(Error::Reference(format!("{} is part of a loop of environments inheriting from each other", current.name)));
            }
            if let Some(first) = chain.first()
                && current.project_id != first.project_id
            {
                return Err(Error::Reference(format!("{} can only inherit from its own project", first.name)));
            }
            if chain.len() == MAX_DEPTH {
                return Err(Error::Reference(format!("{} inherits through more than {MAX_DEPTH} environments", chain[0].name)));
            }
            chain.push(current);
            next = current.parent;
        }
        Ok(chain)
    }

    /// Every variable `env` has, own and inherited, in key order.
    pub fn entries(&self, env: Uuid) -> Result<Vec<Effective>> {
        let mut found: BTreeMap<String, Effective> = BTreeMap::new();
        // Apply from the root down, so nearer environments win.
        for layer in self.chain(env)?.into_iter().rev() {
            for key in layer.hidden() {
                found.remove(key);
            }
            for (key, entry) in layer.entries() {
                let entry = match entry {
                    Entry::Value(v) => Raw::Value(v.to_string()),
                    Entry::Secret(id) => Raw::Secret(id),
                };
                found.insert(key.to_string(), Effective { key: key.to_string(), from: layer.id, entry });
            }
        }
        Ok(found.into_values().collect())
    }

    /// Every variable with a value, links resolved and references expanded.
    /// A link to a secret with no value is left out, as before inheritance.
    pub fn resolve(&self, env: Uuid) -> Result<Vec<(String, String)>> {
        let entries = self.entries(env)?;
        let raw: HashMap<&str, &Raw> = entries.iter().map(|e| (e.key.as_str(), &e.entry)).collect();
        let mut done: HashMap<String, Option<String>> = HashMap::new();
        let mut out = Vec::new();
        for e in &entries {
            if let Some(value) = self.expand(&e.key, &raw, &mut done, &mut Vec::new())? {
                out.push((e.key.clone(), value));
            }
        }
        Ok(out)
    }

    /// How `b`'s variables differ from `a`'s, in key order, by name. Values
    /// are compared here and never returned. With `names_only` they aren't
    /// compared at all.
    pub fn diff(&self, a: Uuid, b: Uuid, names_only: bool) -> Result<Vec<Change>> {
        Ok(diff(&self.resolve(a)?, &self.resolve(b)?, names_only))
    }

    /// One variable's final value, if it has one.
    pub fn value(&self, env: Uuid, key: &str) -> Result<Option<String>> {
        Ok(self.resolve(env)?.into_iter().find(|(k, _)| k == key).map(|(_, v)| v))
    }

    fn expand(
        &self,
        key: &str,
        raw: &HashMap<&str, &Raw>,
        done: &mut HashMap<String, Option<String>>,
        stack: &mut Vec<String>,
    ) -> Result<Option<String>> {
        if let Some(value) = done.get(key) {
            return Ok(value.clone());
        }
        if stack.iter().any(|k| k == key) {
            stack.push(key.to_string());
            return Err(Error::Reference(format!("references go round in a loop: {}", stack.join(" → "))));
        }
        if stack.len() == MAX_NESTING {
            return Err(Error::Reference(format!("references nest more than {MAX_NESTING} deep at {key}")));
        }
        let value = match raw.get(key) {
            // Secrets hold values, not templates.
            Some(Raw::Secret(id)) => self.secrets.get(*id).and_then(|s| s.current_value()).map(str::to_string),
            Some(Raw::Value(template)) => {
                stack.push(key.to_string());
                let expanded = self.substitute(key, template, raw, done, stack)?;
                stack.pop();
                Some(expanded)
            }
            None => None,
        };
        done.insert(key.to_string(), value.clone());
        Ok(value)
    }

    /// Replaces each `${{ NAME }}` in `template`. `$${{` is a literal `${{`.
    fn substitute(
        &self,
        key: &str,
        template: &str,
        raw: &HashMap<&str, &Raw>,
        done: &mut HashMap<String, Option<String>>,
        stack: &mut Vec<String>,
    ) -> Result<String> {
        let mut out = String::with_capacity(template.len());
        let mut rest = template;
        while let Some(at) = rest.find("${{") {
            if rest[..at].ends_with('$') {
                // Escaped: keep `${{` literally, dropping the extra `$`.
                out.push_str(&rest[..at - 1]);
                out.push_str("${{");
                rest = &rest[at + 3..];
                continue;
            }
            out.push_str(&rest[..at]);
            let after = &rest[at + 3..];
            let end = after.find("}}").ok_or_else(|| Error::Reference(format!("{key} has a ${{{{ with no closing }}}}")))?;
            let name = after[..end].trim();
            if !crate::is_valid_var_name(name) {
                return Err(Error::Reference(format!("{key} refers to {:?}, which isn't a variable name", after[..end].trim())));
            }
            match self.expand(name, raw, done, stack)? {
                Some(value) => out.push_str(&value),
                None => {
                    return Err(Error::Reference(format!("{key} refers to {name}, which isn't set")));
                }
            }
            rest = &after[end + 2..];
        }
        out.push_str(rest);
        Ok(out)
    }
}

/// One difference between two environments, by name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Change {
    /// Only the first environment has it.
    Removed(String),
    /// Only the second environment has it.
    Added(String),
    /// Both have it, with different values.
    Changed(String),
}

impl Change {
    pub fn key(&self) -> &str {
        match self {
            Change::Removed(k) | Change::Added(k) | Change::Changed(k) => k,
        }
    }
}

/// How `b` differs from `a`, in key order. With `names_only`, values aren't
/// compared.
pub fn diff(a: &[(String, String)], b: &[(String, String)], names_only: bool) -> Vec<Change> {
    let a: BTreeMap<&str, &str> = a.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    let b: BTreeMap<&str, &str> = b.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    let keys: std::collections::BTreeSet<&str> = a.keys().chain(b.keys()).copied().collect();
    keys.into_iter()
        .filter_map(|k| match (a.get(k), b.get(k)) {
            (Some(_), None) => Some(Change::Removed(k.to_string())),
            (None, Some(_)) => Some(Change::Added(k.to_string())),
            (Some(x), Some(y)) if !names_only && x != y => Some(Change::Changed(k.to_string())),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{Change, diff};

    fn vars(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn diff_lists_missing_extra_and_changed() {
        let a = vars(&[("A", "1"), ("B", "2"), ("C", "3")]);
        let b = vars(&[("B", "2"), ("C", "x"), ("D", "4")]);
        assert_eq!(
            diff(&a, &b, false),
            vec![Change::Removed("A".into()), Change::Changed("C".into()), Change::Added("D".into())]
        );
        assert_eq!(diff(&a, &b, true), vec![Change::Removed("A".into()), Change::Added("D".into())]);
        assert!(diff(&a, &a, false).is_empty());
    }
}
