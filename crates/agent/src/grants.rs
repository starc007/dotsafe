//! Standing permissions the user gave in an approval: this exact command, in
//! this directory, with these variables from this environment, for a while.
//! Anything different asks again. Grants live in memory only and end when the
//! vault locks.

use std::path::PathBuf;
use std::time::{Duration, Instant};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scope {
    /// The resolved program, so a different binary with the same name doesn't match.
    pub program: PathBuf,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub environment: String,
    /// Sorted variable names.
    pub variables: Vec<String>,
}

/// How long an approval lasts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lifetime {
    Once,
    For(Duration),
    UntilLock,
}

#[derive(Clone, Debug)]
struct Grant {
    scope: Scope,
    /// `None`: until the vault locks.
    expires: Option<Instant>,
}

#[derive(Debug, Default)]
pub struct Grants(Vec<Grant>);

impl Grants {
    /// Whether `scope` is covered by a grant that hasn't expired.
    pub fn allows(&mut self, scope: &Scope, now: Instant) -> bool {
        self.0.retain(|g| g.expires.is_none_or(|e| e > now));
        self.0.iter().any(|g| g.scope == *scope)
    }

    /// Records an approval. A one-time approval isn't stored.
    pub fn add(&mut self, scope: Scope, lifetime: Lifetime, now: Instant) {
        let expires = match lifetime {
            Lifetime::Once => return,
            Lifetime::For(d) => Some(now + d),
            Lifetime::UntilLock => None,
        };
        self.0.retain(|g| g.scope != scope);
        self.0.push(Grant { scope, expires });
    }

    /// Live grants and when each expires (`None`: when the vault locks).
    pub fn entries(&mut self, now: Instant) -> Vec<(Scope, Option<Instant>)> {
        self.0.retain(|g| g.expires.is_none_or(|e| e > now));
        self.0.iter().map(|g| (g.scope.clone(), g.expires)).collect()
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope(args: &[&str]) -> Scope {
        Scope {
            program: "/usr/local/bin/bun".into(),
            args: args.iter().map(|s| s.to_string()).collect(),
            cwd: "/work/app".into(),
            environment: "env-1".into(),
            variables: vec!["API_KEY".into()],
        }
    }

    #[test]
    fn grants_match_exactly_and_expire() {
        let now = Instant::now();
        let mut grants = Grants::default();
        grants.add(scope(&["test"]), Lifetime::Once, now);
        assert!(!grants.allows(&scope(&["test"]), now), "once isn't remembered");

        grants.add(scope(&["test"]), Lifetime::For(Duration::from_secs(60)), now);
        assert!(grants.allows(&scope(&["test"]), now));
        assert!(!grants.allows(&scope(&["run", "dev"]), now), "different arguments ask again");
        assert!(!grants.allows(&scope(&["test"]), now + Duration::from_secs(61)), "expired");

        grants.add(scope(&["test"]), Lifetime::UntilLock, now);
        assert!(grants.allows(&scope(&["test"]), now + Duration::from_secs(86_400)));
        grants.clear();
        assert!(grants.is_empty());
    }
}
