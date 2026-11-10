//! `dotsafe check` and `example`: compare an environment with a committed
//! `.env.example`, by name. (`dotsafe diff` uses `Resolver::diff` in core.)
//!
//! Nothing here prints or writes a value from the vault: `example` writes
//! names, keeping only the values the example file already had.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use dotsafe_core::dotenv;

/// First line of every file `example` writes.
pub const EXAMPLE_HEADER: &str = "# Written by `dotsafe example`";

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
