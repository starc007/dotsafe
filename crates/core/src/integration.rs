//! Integrations: accounts at providers that issue keys (OpenAI, Cloudflare)
//! and at services that run code (Vercel, Cloudflare Workers), and which
//! secrets use them.
//!
//! Only what's needed to call the service lives here. The admin token for a
//! connection is an ordinary shared [`crate::Secret`], so it has history and
//! can be rotated too. The calls themselves are in `crates/integrations`;
//! core has no network code.

use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::model::{Record, Stamp};

/// The shared secrets that hold live connections' admin tokens. Lists of
/// the user's own secrets leave these out; they're managed from the
/// connection instead.
pub fn credentials(records: &[Record]) -> HashSet<Uuid> {
    records
        .iter()
        .filter_map(|r| match r {
            Record::Connection(c) if !c.deleted => c.credential,
            _ => None,
        })
        .collect()
}

/// A custom profile for the secret proxy: which variable holds an API key,
/// where requests with it go, and how the key is sent. Kept in the vault, not
/// in config or the repo, because whoever can change a profile decides where
/// a key is sent, and agents can edit files but not the vault. Validated by
/// the proxy (`dotsafe_agent::proxy`) before use.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProxyProfile {
    pub id: Uuid,
    pub name: String,
    /// The variable holding the key, e.g. `MISTRAL_API_KEY`.
    pub key_var: String,
    /// The variable the SDK reads its base URL from, e.g. `MISTRAL_BASE_URL`.
    pub base_url_var: String,
    /// The one place requests go, `https://` only.
    pub upstream: String,
    /// Appended to the proxy URL given to the SDK, e.g. `/v1`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub base_path: String,
    /// The request header the key goes in, e.g. `authorization`.
    pub header: String,
    /// Send the key as `Bearer <key>`.
    pub bearer: bool,
    pub deleted: bool,
    pub updated: Stamp,
}

impl ProxyProfile {
    pub(crate) fn merge(self, other: ProxyProfile) -> ProxyProfile {
        if other.updated > self.updated { other } else { self }
    }
}

/// Live custom proxy profiles.
pub fn proxy_profiles(records: &[Record]) -> Vec<ProxyProfile> {
    let mut profiles: Vec<ProxyProfile> = records
        .iter()
        .filter_map(|r| match r {
            Record::ProxyProfile(p) if !p.deleted => Some(p.clone()),
            _ => None,
        })
        .collect();
    profiles.sort_by_key(|p| p.name.to_lowercase());
    profiles
}

/// Values a removed device could still know: shared secrets whose current
/// value is older than `since`, and variables with their own value last set
/// before it. Rotating one takes it off the list.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Exposed {
    pub secrets: Vec<Uuid>,
    /// (environment id, variable name)
    pub variables: Vec<(Uuid, String)>,
}

pub fn exposed_since(records: &[Record], since: u64) -> Exposed {
    let mut exposed = Exposed::default();
    let projects: HashSet<Uuid> = records
        .iter()
        .filter_map(|r| match r {
            Record::Project(p) if !p.deleted => Some(p.id),
            _ => None,
        })
        .collect();
    for record in records {
        match record {
            Record::Secret(s) if !s.deleted => {
                if s.current().is_some_and(|(_, v)| v.created.at_ms < since && v.value.is_some()) {
                    exposed.secrets.push(s.id);
                }
            }
            Record::Environment(e) if !e.deleted && projects.contains(&e.project_id) => {
                for key in e.keys() {
                    if e.value(key).is_some() && e.changed(key).is_some_and(|stamp| stamp.at_ms < since) {
                        exposed.variables.push((e.id, key.to_string()));
                    }
                }
            }
            _ => {}
        }
    }
    exposed
}

/// An account at a service, and the admin token dotsafe calls it with.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Connection {
    pub id: Uuid,
    /// Which integration: `openai`, `cloudflare_token`, `vercel`,
    /// `cloudflare_workers`.
    pub service: String,
    /// Shown in lists, e.g. "Acme on Vercel".
    pub name: String,
    /// The shared secret holding the admin token. `None` for services that
    /// need none (Stripe: dotsafe only checks new keys with themselves).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential: Option<Uuid>,
    /// What the service needs besides the token: a project or account id.
    #[serde(default)]
    pub settings: BTreeMap<String, String>,
    pub deleted: bool,
    pub updated: Stamp,
}

impl Connection {
    pub fn new(
        service: &str,
        name: &str,
        credential: Option<Uuid>,
        settings: BTreeMap<String, String>,
        stamp: Stamp,
    ) -> Self {
        Connection {
            id: Uuid::new_v4(),
            service: service.into(),
            name: name.into(),
            credential,
            settings,
            deleted: false,
            updated: stamp,
        }
    }

    pub(crate) fn merge(self, other: Connection) -> Connection {
        if other.updated > self.updated { other } else { self }
    }
}

/// Where a secret's values come from and where they're pushed. One per
/// secret, id derived from the secret's.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Binding {
    pub id: Uuid,
    pub secret: Uuid,
    /// The provider that issues this secret's values, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<ProviderLink>,
    /// Stamp for `provider`.
    pub updated: Stamp,
    /// The provider's id for each version's key, so the old one can be
    /// revoked. Keyed by secret version id.
    #[serde(default)]
    pub keys: BTreeMap<Uuid, String>,
    #[serde(default)]
    pub targets: BTreeMap<Uuid, Target>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderLink {
    pub connection: Uuid,
    /// Per-secret options, such as the Cloudflare token to roll.
    #[serde(default)]
    pub settings: BTreeMap<String, String>,
}

/// A place a secret's current value is pushed to.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Target {
    pub connection: Uuid,
    /// The variable name at the service.
    pub name: String,
    /// Per-target options: a Vercel project and environments, a Worker name.
    #[serde(default)]
    pub settings: BTreeMap<String, String>,
    /// What was last pushed: the SHA-256 of the value (most services can't
    /// be read back) and when.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pushed: Option<Pushed>,
    pub deleted: bool,
    pub changed: Stamp,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pushed {
    pub sha256: String,
    pub at: u64,
}

impl Binding {
    /// The id of the binding for `secret`.
    pub fn id_for(secret: Uuid) -> Uuid {
        Uuid::new_v5(&secret, b"binding")
    }

    pub fn new(secret: Uuid, stamp: Stamp) -> Binding {
        Binding {
            id: Binding::id_for(secret),
            secret,
            provider: None,
            updated: stamp,
            keys: BTreeMap::new(),
            targets: BTreeMap::new(),
        }
    }

    /// Live targets.
    pub fn targets(&self) -> impl Iterator<Item = (Uuid, &Target)> {
        self.targets.iter().filter(|(_, t)| !t.deleted).map(|(id, t)| (*id, t))
    }

    pub(crate) fn max_stamp(&self) -> Stamp {
        self.targets.values().map(|t| t.changed).fold(self.updated, Stamp::max)
    }

    /// `provider` merges on `updated`, targets on their own stamps, and key
    /// ids are kept from both sides (each version's id is set once).
    pub(crate) fn merge(self, other: Binding) -> Binding {
        let (mut newer, older) = if other.updated > self.updated { (other, self) } else { (self, other) };
        for (version, key) in older.keys {
            newer.keys.entry(version).or_insert(key);
        }
        for (id, theirs) in older.targets {
            match newer.targets.get(&id) {
                Some(ours) if ours.changed >= theirs.changed => {}
                _ => {
                    newer.targets.insert(id, theirs);
                }
            }
        }
        newer
    }
}
