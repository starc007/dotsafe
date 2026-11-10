//! `dotsafe diff`, `check` and `example`: compare environments with each
//! other and with a committed `.env.example`, by name.
//!
//! Nothing here prints or writes a value from the vault. `diff` compares
//! values in memory and only says whether they differ; `example` writes
//! names, keeping only the values the example file already had.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use dotsafe_core::dotenv;

/// First line of every file `example` writes.
pub const EXAMPLE_HEADER: &str = "# Written by `dotsafe example`";

#[derive(Debug, PartialEq, Eq)]
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
    let keys: BTreeSet<&str> = a.keys().chain(b.keys()).copied().collect();
    keys.into_iter()
        .filter_map(|k| match (a.get(k), b.get(k)) {
            (Some(_), None) => Some(Change::Removed(k.to_string())),
            (None, Some(_)) => Some(Change::Added(k.to_string())),
            (Some(x), Some(y)) if !names_only && x != y => Some(Change::Changed(k.to_string())),
            _ => None,
        })
        .collect()
}

/// The names in an example file, with the values it gives them.
pub fn read_example(path: &Path) -> Result<Vec<(String, String)>> {
    let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    dotenv::parse(&text).map_err(|e| anyhow::anyhow!("{}: {e}", path.display()))
}

/// What an environment is missing from an example file, and what it has that
/// the file doesn't list.
pub struct Check {
    pub missing: Vec<String>,
    pub extra: Vec<String>,
}

pub fn check(example: &[(String, String)], names: &[String]) -> Check {
    let have: BTreeSet<&str> = names.iter().map(String::as_str).collect();
    let listed: BTreeSet<&str> = example.iter().map(|(k, _)| k.as_str()).collect();
    Check {
        missing: listed.difference(&have).map(|k| k.to_string()).collect(),
        extra: have.difference(&listed).map(|k| k.to_string()).collect(),
    }
}

/// `.env.example` text listing `names`. A value is kept only if `existing`
/// (the file being replaced) already had it; everything else is empty.
pub fn example_text(names: &[String], existing: &[(String, String)], label: &str) -> String {
    let kept: BTreeMap<&str, &str> = existing.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    let mut out = format!(
        "{EXAMPLE_HEADER} from {label}. Values live in dotsafe; run `dotsafe example -o <file>` to update it.\n"
    );
    let vars = names.iter().map(|k| (k.as_str(), kept.get(k.as_str()).copied().unwrap_or("")));
    out.push_str(&dotenv::format(vars));
    out
}

/// Writes an example file. One dotsafe didn't write is only replaced with
/// `force`, since its comments don't survive.
pub fn write_example(path: &Path, text: &str, force: bool) -> Result<()> {
    if !force
        && let Ok(existing) = fs::read_to_string(path)
        && !existing.starts_with(EXAMPLE_HEADER)
    {
        bail!(
            "{} exists and dotsafe didn't write it; pass --force to replace it (its values are kept, its comments aren't)",
            path.display()
        );
    }
    fs::write(path, text).with_context(|| format!("writing {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn check_finds_missing_and_extra() {
        let example = vars(&[("A", ""), ("B", "")]);
        let c = check(&example, &["B".into(), "C".into()]);
        assert_eq!(c.missing, vec!["A"]);
        assert_eq!(c.extra, vec!["C"]);
    }

    #[test]
    fn example_keeps_only_values_the_file_had() {
        let existing = vars(&[("PORT", "3000"), ("OLD", "gone")]);
        let text = example_text(&["API_KEY".into(), "PORT".into()], &existing, "app/dev");
        let parsed = dotenv::parse(&text).unwrap();
        assert_eq!(parsed, vars(&[("API_KEY", ""), ("PORT", "3000")]));
        assert!(text.starts_with(EXAMPLE_HEADER));
    }
}
