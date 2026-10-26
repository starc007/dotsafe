//! `.dotsafe.json`: links a directory to a project in the vault. It holds only
//! ids, never secrets, so it's safe to commit; teammates' and other machines'
//! checkouts then find the project automatically.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const LINK_FILE: &str = ".dotsafe.json";
pub const DEFAULT_ENV: &str = "development";

#[derive(Debug, Serialize, Deserialize)]
pub struct Link {
    pub project_id: Uuid,
    /// Environment used when `--env` isn't given.
    pub environment: String,
}

impl Link {
    /// Finds the nearest link file in `dir` or its parents.
    pub fn find(dir: &Path) -> Result<Option<(Link, PathBuf)>> {
        for d in dir.ancestors() {
            let path = d.join(LINK_FILE);
            if path.is_file() {
                let link = serde_json::from_slice(&fs::read(&path)?)
                    .with_context(|| format!("parsing {}", path.display()))?;
                return Ok(Some((link, path)));
            }
        }
        Ok(None)
    }

    pub fn write(&self, dir: &Path) -> Result<PathBuf> {
        let path = dir.join(LINK_FILE);
        let mut json = serde_json::to_string_pretty(self)?;
        json.push('\n');
        fs::write(&path, json).with_context(|| format!("writing {}", path.display()))?;
        Ok(path)
    }
}
