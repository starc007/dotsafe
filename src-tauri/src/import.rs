//! Importing variables from `.env` text or files.
//!
//! File paths only ever come from the native file picker or a drop onto the
//! window, both handled in Rust, never from the webview. Previews return
//! variable names and what would change, never values.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use dotsafe_core::{Environment, dotenv};
use serde::{Deserialize, Serialize};

/// Generous for any real `.env` file, small enough to refuse binaries and logs.
const MAX_BYTES: usize = 1024 * 1024;
const MAX_FILES: usize = 20;

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ImportSource {
    /// `.env` content pasted into the import dialog.
    Text { text: String },
    /// The files last chosen in the picker or dropped on the window.
    Files,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Change {
    New,
    Update,
    Unchanged,
}

/// A parsed variable, and the file it came from when importing files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parsed {
    pub key: String,
    pub value: String,
    pub file: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewEntry {
    pub key: String,
    pub change: Change,
    pub file: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    pub entries: Vec<PreviewEntry>,
    /// Problems with the input, e.g. a line that isn't KEY=value.
    pub errors: Vec<String>,
}

#[derive(Debug, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub added: usize,
    pub updated: usize,
    pub unchanged: usize,
    /// Existing variables left alone because overwriting was off.
    pub skipped: usize,
}

/// Reads and parses a source. Later lines and later files win when a key
/// repeats, matching how a shell would load them.
pub fn collect(source: &ImportSource, files: &[PathBuf]) -> (Vec<Parsed>, Vec<String>) {
    let mut found = Vec::new();
    let mut errors = Vec::new();
    match source {
        ImportSource::Text { text } => {
            if text.len() > MAX_BYTES {
                errors.push("That's too much text to be a .env file.".into());
            } else {
                match dotenv::parse(text) {
                    Ok(vars) => found.extend(vars.into_iter().map(|(key, value)| Parsed { key, value, file: None })),
                    Err(e) => errors.push(capitalize(&e)),
                }
            }
        }
        ImportSource::Files => {
            if files.len() > MAX_FILES {
                errors.push(format!("Choose at most {MAX_FILES} files at a time."));
            }
            for path in files.iter().take(MAX_FILES) {
                let name = file_name(path);
                match read_env_file(path) {
                    Ok(vars) => found.extend(vars.into_iter().map(|(key, value)| Parsed {
                        key,
                        value,
                        file: Some(name.clone()),
                    })),
                    Err(e) => errors.push(format!("{name}: {e}")),
                }
            }
        }
    }
    (dedupe(found), errors)
}

fn read_env_file(path: &Path) -> Result<Vec<(String, String)>, String> {
    let meta = fs::metadata(path).map_err(|_| "couldn't read the file".to_string())?;
    if !meta.is_file() {
        return Err("not a file".into());
    }
    if meta.len() as usize > MAX_BYTES {
        return Err("too large to be a .env file".into());
    }
    let text = fs::read_to_string(path).map_err(|_| "not a text file".to_string())?;
    dotenv::parse(&text)
}

/// Keeps each key once, at its first position, with its last value.
fn dedupe(found: Vec<Parsed>) -> Vec<Parsed> {
    let mut order: Vec<Parsed> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    for p in found {
        match index.get(&p.key) {
            Some(&i) => order[i] = p,
            None => {
                index.insert(p.key.clone(), order.len());
                order.push(p);
            }
        }
    }
    order
}

/// What importing each variable would do to `env`.
pub fn plan(env: &Environment, parsed: &[Parsed]) -> Vec<(Change, Parsed)> {
    parsed
        .iter()
        .map(|p| {
            let change = match env.get(&p.key) {
                None => Change::New,
                Some(current) if current == p.value => Change::Unchanged,
                Some(_) => Change::Update,
            };
            (change, p.clone())
        })
        .collect()
}

pub fn file_name(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.display().to_string())
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dotsafe_core::Stamp;
    use uuid::Uuid;

    fn text(t: &str) -> ImportSource {
        ImportSource::Text { text: t.into() }
    }

    #[test]
    fn later_values_win_and_first_position_is_kept() {
        let (found, errors) = collect(&text("A=1\nB=2\nA=3\n"), &[]);
        assert!(errors.is_empty());
        let pairs: Vec<_> = found.iter().map(|p| (p.key.as_str(), p.value.as_str())).collect();
        assert_eq!(pairs, [("A", "3"), ("B", "2")]);
    }

    #[test]
    fn parse_errors_are_reported_not_thrown() {
        let (found, errors) = collect(&text("GOOD=1\nnot a line"), &[]);
        assert!(found.is_empty());
        assert_eq!(errors.len(), 1);
        assert!(errors[0].starts_with("Line 2"));
    }

    #[test]
    fn plan_marks_new_updated_and_unchanged() {
        let mut env = Environment::new(Uuid::new_v4(), "development", Stamp::new(1, Uuid::nil()));
        env.set("SAME", "x", Stamp::new(1, Uuid::nil()));
        env.set("CHANGED", "old", Stamp::new(1, Uuid::nil()));
        let (found, _) = collect(&text("SAME=x\nCHANGED=new\nFRESH=1\n"), &[]);
        let changes: Vec<_> = plan(&env, &found).into_iter().map(|(c, p)| (p.key, c)).collect();
        assert_eq!(
            changes,
            [
                ("SAME".to_string(), Change::Unchanged),
                ("CHANGED".to_string(), Change::Update),
                ("FRESH".to_string(), Change::New),
            ]
        );
    }

    #[test]
    fn files_are_read_in_order_with_their_names() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join(".env");
        let b = dir.path().join(".env.local");
        fs::write(&a, "API=base\nONLY_A=1\n").unwrap();
        fs::write(&b, "API=local\n").unwrap();
        let (found, errors) = collect(&ImportSource::Files, &[a, b]);
        assert!(errors.is_empty());
        assert_eq!(found[0], Parsed { key: "API".into(), value: "local".into(), file: Some(".env.local".into()) });
        assert_eq!(found[1].key, "ONLY_A");
    }

    #[test]
    fn unreadable_and_oversized_files_are_errors() {
        let dir = tempfile::tempdir().unwrap();
        let big = dir.path().join("big.env");
        fs::write(&big, "A=".to_string() + &"x".repeat(MAX_BYTES)).unwrap();
        let missing = dir.path().join("missing.env");
        let (found, errors) = collect(&ImportSource::Files, &[big, missing]);
        assert!(found.is_empty());
        assert!(errors.iter().any(|e| e.starts_with("big.env: too large")));
        assert!(errors.iter().any(|e| e.starts_with("missing.env:")));
    }
}
