//! Shares: some environments given to a machine (CI, a server, a cloud
//! agent) as a sealed snapshot of their final values, never the vault key.
//! The design is in `docs/architecture/access.md`, the format in
//! `docs/vault-format.md`.
//!
//! A `share` record lives in the vault and holds the share key. Any of the
//! user's devices resolves the share's environments with [`Resolver`],
//! seals the result under a subkey of the share key and writes it to the
//! share bucket at a random path. The machine is given a token (vault id,
//! share id, share key and where the snapshot is) and nothing else, so it
//! can read the values it was given and no others.
//!
//! No network code here: the bucket is reached through [`ShareBucket`],
//! implemented in `crates/stores`, and fetched by the machine there too.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::crypto::{self, KEY_LEN, Sealed, SecretKey};
use crate::error::{Error, Result};
use crate::header::{crockford_decode_vec, crockford_raw};
use crate::integration::{Exposed, Pushed};
use crate::model::{Record, Stamp};
use crate::resolve::{Effective, Raw, Resolver};

const DAY_MS: u64 = 24 * 60 * 60 * 1000;

/// How long a snapshot stays valid without a refresh, unless the share says
/// otherwise.
pub const DEFAULT_LEASE_DAYS: u32 = 14;
/// A snapshot this old is pushed again, which renews its lease.
pub const RENEW_AFTER_MS: u64 = DAY_MS;
/// The largest snapshot object a machine will read.
pub const MAX_SNAPSHOT_BYTES: usize = 4 * 1024 * 1024;

const SNAPSHOT_FORMAT: &str = "dotsafe-share-snapshot";
const SNAPSHOT_VERSION: u32 = 1;
/// HKDF info for the snapshot key, and the snapshot's AAD prefix.
const SNAPSHOT_LABEL: &[u8] = b"dotsafe/v6/share-snapshot";
/// HKDF info for the key `dotsafe approve --share` signs answers with.
const APPROVAL_LABEL: &[u8] = b"dotsafe/v6/share-approve";

/// Approve mode's objects: a mailbox the user's devices write, requests the
/// machine writes into slots, and answers the devices write back. Each kind
/// has its own subkey and AAD label, so one can't be passed off as another.
const MAILBOX_FORMAT: &str = "dotsafe-share-mailbox";
const REQUEST_FORMAT: &str = "dotsafe-share-request";
const ANSWER_FORMAT: &str = "dotsafe-share-answer";
const MAILBOX_LABEL: &[u8] = b"dotsafe/v6/share-mailbox";
const REQUEST_LABEL: &[u8] = b"dotsafe/v6/share-request";
const ANSWER_LABEL: &[u8] = b"dotsafe/v6/share-answer";
const MESSAGE_VERSION: u32 = 1;
/// Requests a machine can have waiting at once.
pub const SLOTS: u8 = 4;
/// How long the mailbox's upload URLs are signed for: the most S3 and R2
/// allow.
pub const MAILBOX_URL_SECS: u64 = 7 * 24 * 60 * 60;
/// A mailbox stops being valid a day before its URLs do, so a machine
/// never tries an expired one.
const MAILBOX_DAYS: u64 = 6;
/// The largest request or answer either side reads.
pub const MAX_MESSAGE_BYTES: usize = 256 * 1024;

const TOKEN_PREFIX: &str = "dss_";
const TOKEN_VERSION: u8 = 1;
/// Random bits in a share's object path.
const PATH_LEN: usize = 16;
/// version, vault id, share id, share key, path.
const TOKEN_FIXED: usize = 1 + 16 + 16 + KEY_LEN + PATH_LEN;

/// What a share gives a machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShareMode {
    /// The snapshot is always in the bucket, until the share expires or is
    /// revoked. For CI, servers and agents working while the user is away.
    Standing,
    /// No standing snapshot: the machine asks for each run through a
    /// mailbox in the bucket, and one of the user's devices answers with
    /// the values approved for it. See [`Mailbox`].
    Approve,
}

/// One environment in a share, optionally narrowed to some variables.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShareEnv {
    pub id: Uuid,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub only: Option<Vec<String>>,
}

/// The share key: 32 random bytes, sealed in the vault with the record.
/// Anyone holding it can read the share's snapshots.
#[derive(Clone, PartialEq, Eq)]
struct ShareKey(Zeroizing<[u8; KEY_LEN]>);

impl ShareKey {
    fn generate() -> Result<ShareKey> {
        Ok(ShareKey(Zeroizing::new(crypto::random_array()?)))
    }
}

impl std::fmt::Debug for ShareKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ShareKey(..)")
    }
}

impl Serialize for ShareKey {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        crypto::b64::serialize(self.0.as_ref(), s)
    }
}

impl<'de> Deserialize<'de> for ShareKey {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        let bytes = Zeroizing::new(crypto::b64::deserialize(d)?);
        let key: [u8; KEY_LEN] =
            bytes.as_slice().try_into().map_err(|_| serde::de::Error::custom("a share key is 32 bytes"))?;
        Ok(ShareKey(Zeroizing::new(key)))
    }
}

/// A machine's access to some environments. Record format v6.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Share {
    pub id: Uuid,
    /// Shown in lists, e.g. `ci`.
    pub name: String,
    pub deleted: bool,
    /// Stamp for everything except `pushed` and `revoked`, which merge on
    /// their own.
    pub updated: Stamp,
    pub mode: ShareMode,
    pub environments: Vec<ShareEnv>,
    /// Unix milliseconds.
    pub expires: u64,
    /// How long a snapshot stays valid without a refresh.
    pub lease_days: u32,
    key: ShareKey,
    /// The object path's 128 random bits, in lowercase hex.
    path: String,
    /// The last thing written to the bucket: a snapshot whose contents hash
    /// to `sha256`, or, with an empty `sha256`, the share's objects deleted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pushed: Option<Pushed>,
    /// The device that created it.
    pub created_by: Uuid,
    /// Unix milliseconds. Set on revoke, never cleared, and kept for the
    /// rotation queue.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revoked: Option<u64>,
}

/// Where a share stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// The bucket has the vault's current values.
    Current,
    /// The vault changed since the last push.
    OutOfDate,
    NeverPushed,
    Expired,
    Revoked,
}

impl Share {
    /// A new share with a fresh key and path.
    pub fn new(
        name: impl Into<String>,
        mode: ShareMode,
        environments: Vec<ShareEnv>,
        expires: u64,
        lease_days: u32,
        device: Uuid,
        stamp: Stamp,
    ) -> Result<Share> {
        let path: [u8; PATH_LEN] = crypto::random_array()?;
        Ok(Share {
            id: Uuid::new_v4(),
            name: name.into(),
            deleted: false,
            updated: stamp,
            mode,
            environments,
            expires,
            lease_days,
            key: ShareKey::generate()?,
            path: hex(&path),
            pushed: None,
            created_by: device,
            revoked: None,
        })
    }

    /// Not revoked, deleted or expired.
    pub fn is_live(&self, now: u64) -> bool {
        !self.deleted && self.revoked.is_none() && self.expires > now
    }

    /// The bucket may still hold this share's objects.
    pub fn in_bucket(&self) -> bool {
        self.pushed.as_ref().is_some_and(|p| !p.sha256.is_empty())
    }

    /// The object that stays in the bucket while the share is live: the
    /// snapshot, or in approve mode the mailbox.
    pub fn object_key(&self) -> Result<String> {
        match self.mode {
            ShareMode::Standing => Ok(format!("s/{}", hex(&self.path_bytes()?))),
            ShareMode::Approve => Ok(format!("{}mailbox", self.prefix()?)),
        }
    }

    /// Everything under this share in the bucket, for its expiry rule.
    pub fn prefix(&self) -> Result<String> {
        match self.mode {
            ShareMode::Standing => self.object_key(),
            ShareMode::Approve => Ok(format!("m/{}/", hex(&self.path_bytes()?))),
        }
    }

    /// Approve mode: the object a machine overwrites after writing a request.
    pub fn doorbell_key(&self) -> Result<String> {
        Ok(format!("{}doorbell", self.prefix()?))
    }

    /// Approve mode: where a machine writes the request in `slot`.
    pub fn request_key(&self, slot: u8) -> Result<String> {
        Ok(format!("{}q/{slot}", self.prefix()?))
    }

    /// Approve mode: where a device writes the answer to the request in `slot`.
    pub fn answer_key(&self, slot: u8) -> Result<String> {
        Ok(format!("{}a/{slot}", self.prefix()?))
    }

    fn path_bytes(&self) -> Result<[u8; PATH_LEN]> {
        crate::vault::decode_hex(&self.path)
            .and_then(|b| b.try_into().ok())
            .ok_or_else(|| Error::Format(format!("share {} has an invalid path", self.name)))
    }

    /// When a snapshot written at `now` stops being valid: after the lease,
    /// and never after the share expires.
    pub fn lease_until(&self, now: u64) -> u64 {
        now.saturating_add(u64::from(self.lease_days) * DAY_MS).min(self.expires)
    }

    /// What the machine is given, once: `dss_` and the Crockford base32 of
    /// version ‖ vault id ‖ share id ‖ share key ‖ path ‖ the bucket's public
    /// URL. A secret: never write it to disk.
    pub fn token(&self, vault_id: Uuid, public_url: &str) -> Result<Zeroizing<String>> {
        let base = public_base(public_url)?;
        let mut bytes = Zeroizing::new(Vec::with_capacity(TOKEN_FIXED + base.len()));
        bytes.push(TOKEN_VERSION);
        bytes.extend_from_slice(vault_id.as_bytes());
        bytes.extend_from_slice(self.id.as_bytes());
        bytes.extend_from_slice(self.key.0.as_ref());
        bytes.extend_from_slice(&self.path_bytes()?);
        bytes.extend_from_slice(base.as_bytes());
        Ok(Zeroizing::new(format!("{TOKEN_PREFIX}{}", crockford_raw(&bytes).as_str())))
    }

    /// The share's environments' names in snapshots, in order: the
    /// environment's name, or `project/environment` where two share a name.
    pub fn labels(&self, records: &[Record]) -> Result<Vec<String>> {
        let projects: HashMap<Uuid, &str> = records
            .iter()
            .filter_map(|r| match r {
                Record::Project(p) => Some((p.id, p.name.as_str())),
                _ => None,
            })
            .collect();
        let envs: HashMap<Uuid, (&str, Uuid)> = records
            .iter()
            .filter_map(|r| match r {
                Record::Environment(e) if !e.deleted => Some((e.id, (e.name.as_str(), e.project_id))),
                _ => None,
            })
            .collect();
        let named: Vec<(&str, &str)> = self
            .environments
            .iter()
            .map(|env| {
                let (name, project) = envs.get(&env.id).ok_or_else(|| {
                    Error::Reference(format!("share {} uses an environment that was deleted", self.name))
                })?;
                Ok((projects.get(project).copied().unwrap_or("?"), *name))
            })
            .collect::<Result<_>>()?;
        Ok(named
            .iter()
            .map(|(project, name)| match named.iter().filter(|(_, n)| n == name).count() {
                1 => name.to_string(),
                _ => format!("{project}/{name}"),
            })
            .collect())
    }

    /// The final values a snapshot of this share holds now, with `only`
    /// applied. Fails on a broken reference rather than leave a value out.
    pub fn contents(&self, records: &[Record]) -> Result<Contents> {
        let resolver = Resolver::from_records(records);
        let mut out = BTreeMap::new();
        for (env, label) in self.environments.iter().zip(self.labels(records)?) {
            let mut vars: BTreeMap<String, String> = resolver.resolve(env.id)?.into_iter().collect();
            if let Some(only) = &env.only {
                vars.retain(|k, _| only.contains(k));
            }
            out.insert(label, vars);
        }
        Ok(Contents(out))
    }

    /// The hash recorded in `pushed` for a snapshot of `contents`. A
    /// mailbox holds names only, so in approve mode only names count.
    pub fn digest(&self, contents: &Contents) -> String {
        #[derive(Serialize)]
        struct Digested<'a> {
            expires: u64,
            environments: &'a BTreeMap<String, BTreeMap<String, String>>,
        }
        #[derive(Serialize)]
        struct Named {
            expires: u64,
            names: BTreeMap<String, Vec<String>>,
        }
        let json = match self.mode {
            ShareMode::Standing => serde_json::to_vec(&Digested { expires: self.expires, environments: &contents.0 }),
            ShareMode::Approve => serde_json::to_vec(&Named { expires: self.expires, names: contents.names() }),
        };
        crypto::sha256_hex(&Zeroizing::new(json.expect("maps of strings serialize")))
    }

    /// When a mailbox written at `now` stops being valid: at the end of its
    /// lease, and always before its upload URLs expire.
    pub fn mailbox_lease_until(&self, now: u64) -> u64 {
        self.lease_until(now).min(now.saturating_add(MAILBOX_DAYS * DAY_MS))
    }

    /// The sealed mailbox of a share in approve mode: the names in
    /// `contents` (never values) and the upload URLs in `urls`.
    pub fn seal_mailbox(
        &self,
        vault_id: Uuid,
        contents: &Contents,
        urls: MailboxUrls,
        stamp: Stamp,
        now: u64,
    ) -> Result<Vec<u8>> {
        if self.mode != ShareMode::Approve {
            return Err(Error::Invalid(format!("share {} isn't in approve mode", self.name)));
        }
        if urls.slots.len() != usize::from(SLOTS) {
            return Err(Error::Invalid(format!("a mailbox has {SLOTS} slots")));
        }
        let mailbox = Mailbox {
            stamp,
            expires: self.expires,
            lease_until: self.mailbox_lease_until(now),
            environments: contents.names(),
            doorbell: urls.doorbell,
            slots: urls.slots,
        };
        let plaintext = crate::vault::padded(serde_json::to_vec(&mailbox)?);
        let aad = message_aad(MAILBOX_LABEL, vault_id, self.id, &[]);
        seal_file(&self.key.0, MAILBOX_FORMAT, MAILBOX_LABEL, &plaintext, &aad)
    }

    /// Opens the request a machine wrote into `slot`. Refuses one that
    /// fails authentication or was sealed for another slot or share.
    pub fn open_request(&self, vault_id: Uuid, slot: u8, bytes: &[u8]) -> Result<Request> {
        let aad = message_aad(REQUEST_LABEL, vault_id, self.id, &[slot]);
        let plaintext = open_file(&self.key.0, REQUEST_FORMAT, REQUEST_LABEL, bytes, &aad)?;
        Ok(serde_json::from_slice(&plaintext)?)
    }

    /// The sealed answer to the request in `slot`, bound to the request's
    /// id so it can't answer another one.
    pub fn seal_answer(&self, vault_id: Uuid, slot: u8, answer: &Answer) -> Result<Vec<u8>> {
        let plaintext = crate::vault::padded(serde_json::to_vec(answer)?);
        let aad = answer_aad(vault_id, self.id, slot, answer.id);
        seal_file(&self.key.0, ANSWER_FORMAT, ANSWER_LABEL, &plaintext, &aad)
    }

    /// The values to answer `request` with: the variables it asks for that
    /// this share gives in the environment it names, resolved now. Never
    /// more than the share gives, whatever the request says.
    pub fn values_for(&self, records: &[Record], request: &Request) -> Result<BTreeMap<String, String>> {
        let contents = self.contents(records)?;
        let vars = contents.0.get(&request.environment).ok_or_else(|| {
            Error::Invalid(format!("share {} has no environment {}", self.name, request.environment))
        })?;
        Ok(vars.iter().filter(|(k, _)| request.variables.contains(k)).map(|(k, v)| (k.clone(), v.clone())).collect())
    }

    /// Whether the bucket has what the vault holds now.
    pub fn status(&self, records: &[Record], now: u64) -> Result<Status> {
        if self.revoked.is_some() {
            return Ok(Status::Revoked);
        }
        if self.expires <= now {
            return Ok(Status::Expired);
        }
        match &self.pushed {
            Some(p) if !p.sha256.is_empty() => {
                Ok(if p.sha256 == self.digest(&self.contents(records)?) { Status::Current } else { Status::OutOfDate })
            }
            _ => Ok(Status::NeverPushed),
        }
    }

    /// The sealed snapshot object for `contents`, stamped with `stamp` (a
    /// [`crate::Vault::stamp`]) so machines can refuse older ones.
    pub fn seal(&self, vault_id: Uuid, contents: &Contents, stamp: Stamp, now: u64) -> Result<Vec<u8>> {
        let snapshot = Snapshot {
            stamp,
            expires: self.expires,
            lease_until: self.lease_until(now),
            environments: contents.0.clone(),
        };
        let plaintext = crate::vault::padded(serde_json::to_vec(&snapshot)?);
        let key = snapshot_key(&self.key.0);
        let file = SnapshotFile {
            format: SNAPSHOT_FORMAT.into(),
            version: SNAPSHOT_VERSION,
            sealed: crypto::seal(&key, &plaintext, &snapshot_aad(vault_id, self.id))?,
        };
        Ok(serde_json::to_vec(&file)?)
    }

    /// The variables of `env` that reach the snapshot, before resolving:
    /// all of them, or those in `only` and every variable they refer to.
    fn included(&self, resolver: &Resolver, env: &ShareEnv) -> Result<Vec<Effective>> {
        let entries = resolver.entries(env.id)?;
        let Some(only) = &env.only else { return Ok(entries) };
        let by_name: HashMap<&str, &Effective> = entries.iter().map(|e| (e.key.as_str(), e)).collect();
        let mut keep: BTreeSet<String> = only.iter().cloned().collect();
        let mut queue: Vec<String> = only.clone();
        while let Some(name) = queue.pop() {
            if let Some(Effective { entry: Raw::Value(template), .. }) = by_name.get(name.as_str()) {
                for referenced in references(template) {
                    if keep.insert(referenced.clone()) {
                        queue.push(referenced);
                    }
                }
            }
        }
        Ok(entries.into_iter().filter(|e| keep.contains(&e.key)).collect())
    }

    /// Whether a snapshot of this share contains `secret`'s value.
    pub fn holds_secret(&self, records: &[Record], secret: Uuid) -> Result<bool> {
        let resolver = Resolver::from_records(records);
        for env in &self.environments {
            if self.included(&resolver, env)?.iter().any(|e| e.entry == Raw::Secret(secret)) {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// What a machine with this share could have read before `since`: each
    /// shared secret whose current value is older, and each variable whose
    /// own value was set before it. Rotating one takes it off the list, as
    /// for a removed device. Environments that no longer exist are skipped.
    pub fn exposed(&self, records: &[Record], since: u64) -> Exposed {
        let resolver = Resolver::from_records(records);
        let envs: HashMap<Uuid, &crate::Environment> = records
            .iter()
            .filter_map(|r| match r {
                Record::Environment(e) if !e.deleted => Some((e.id, e)),
                _ => None,
            })
            .collect();
        let mut exposed = Exposed::default();
        for env in &self.environments {
            let Ok(entries) = self.included(&resolver, env) else { continue };
            for e in entries {
                match e.entry {
                    Raw::Secret(id) => {
                        let old = resolver
                            .secrets()
                            .get(id)
                            .and_then(|s| s.current())
                            .is_some_and(|(_, v)| v.created.at_ms < since && v.value.is_some());
                        if old && !exposed.secrets.contains(&id) {
                            exposed.secrets.push(id);
                        }
                    }
                    Raw::Value(_) => {
                        let old = envs.get(&e.from).and_then(|env| env.changed(&e.key)).is_some_and(|s| s.at_ms < since);
                        let variable = (e.from, e.key);
                        if old && !exposed.variables.contains(&variable) {
                            exposed.variables.push(variable);
                        }
                    }
                }
            }
        }
        exposed
    }

    /// `name`, `deleted`, the environments and the dates merge on `updated`;
    /// `pushed` on its `at`; `revoked`, once set on either side, stays set.
    pub(crate) fn merge(self, other: Share) -> Share {
        let (mut newer, older) = if other.updated > self.updated { (other, self) } else { (self, other) };
        newer.pushed = match (newer.pushed.take(), older.pushed) {
            (Some(a), Some(b)) => Some(if (b.at, &b.sha256) > (a.at, &a.sha256) { b } else { a }),
            (a, b) => a.or(b),
        };
        newer.revoked = match (newer.revoked, older.revoked) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
        newer
    }
}

/// What a snapshot holds: each environment's final variables, by label.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Contents(pub BTreeMap<String, BTreeMap<String, String>>);

impl Contents {
    /// Each environment's variable names, without values.
    pub fn names(&self) -> BTreeMap<String, Vec<String>> {
        self.0.iter().map(|(label, vars)| (label.clone(), vars.keys().cloned().collect())).collect()
    }
}

/// Every share in `records` that isn't deleted, by name.
pub fn shares(records: &[Record]) -> Vec<Share> {
    let mut shares: Vec<Share> = records
        .iter()
        .filter_map(|r| match r {
            Record::Share(s) if !s.deleted => Some(s.clone()),
            _ => None,
        })
        .collect();
    shares.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
    shares
}

/// Checks a share bucket's public URL and returns it without a trailing
/// slash. `https://` only, with no query or fragment, since snapshot paths
/// are appended to it.
pub fn public_base(url: &str) -> Result<String> {
    let url = url.trim().trim_end_matches('/');
    let invalid = |why: &str| Error::Invalid(format!("{url:?} can't be a share bucket's public URL: {why}"));
    let Some(rest) = url.strip_prefix("https://") else { return Err(invalid("it must start with https://")) };
    let host = rest.split('/').next().unwrap_or_default();
    if host.is_empty() || host.contains('@') {
        return Err(invalid("it has no host"));
    }
    if url.chars().any(|c| c.is_whitespace() || c.is_control() || matches!(c, '?' | '#' | '\\')) {
        return Err(invalid("it can't hold spaces, ? or #"));
    }
    Ok(url.to_string())
}

/// A rule that has the bucket delete a share's objects once it expires.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExpiryRule {
    pub share: Uuid,
    /// Everything under this prefix goes.
    pub prefix: String,
    /// Unix milliseconds. Buckets round up to the next midnight, UTC.
    pub expires: u64,
}

/// The share bucket: public-read with listing off, written by the user's
/// devices. Implemented over S3 in `crates/stores`; core never touches the
/// network.
pub trait ShareBucket: Send + Sync {
    /// Where it is, for messages.
    fn location(&self) -> String;
    /// The HTTPS address machines fetch snapshots from.
    fn public_url(&self) -> String;
    /// Writes an object, never cached (`Cache-Control: no-store`).
    fn put(&self, key: &str, bytes: &[u8]) -> Result<()>;
    /// Deletes an object; one that's already gone is fine.
    fn delete(&self, key: &str) -> Result<()>;
    /// Makes `rules` the bucket's dotsafe expiry rules, replacing the ones
    /// set before and keeping any rules of the user's own.
    fn set_expiry_rules(&self, rules: &[ExpiryRule]) -> Result<()>;
}

/// The expiry rules for `records`' shares: one for each live share, and for
/// each share whose objects may still be in the bucket.
pub fn expiry_rules(records: &[Record], now: u64) -> Result<Vec<ExpiryRule>> {
    shares(records)
        .into_iter()
        .filter(|s| s.is_live(now) || s.in_bucket())
        .map(|s| Ok(ExpiryRule { share: s.id, prefix: s.prefix()?, expires: s.expires }))
        .collect()
}

/// A snapshot, as the machine reads it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    /// The writer's clock reading. A machine refuses one older than it has
    /// seen.
    pub stamp: Stamp,
    /// When the share expires, Unix milliseconds.
    pub expires: u64,
    /// When this snapshot stops being valid without a refresh.
    pub lease_until: u64,
    /// Each environment's variables, by label.
    pub environments: BTreeMap<String, BTreeMap<String, String>>,
}

impl Snapshot {
    /// The variables of the environment named `env` (its label, or the part
    /// after `project/` when that's unambiguous), or of the only one.
    pub fn environment(&self, env: Option<&str>) -> Result<(&str, &BTreeMap<String, String>)> {
        let names = || self.environments.keys().map(String::as_str).collect::<Vec<_>>().join(", ");
        let found: Vec<_> = match env {
            None if self.environments.len() == 1 => self.environments.iter().collect(),
            None => return Err(Error::Invalid(format!("this share has several environments; pick one with -e: {}", names()))),
            Some(env) => match self.environments.get_key_value(env) {
                Some(exact) => vec![exact],
                None => self.environments.iter().filter(|(k, _)| k.rsplit_once('/').is_some_and(|(_, n)| n == env)).collect(),
            },
        };
        match found.as_slice() {
            [(name, vars)] => Ok((name.as_str(), vars)),
            [] => Err(Error::Invalid(format!("this share has no {} environment; it has {}", env.unwrap_or("such"), names()))),
            _ => Err(Error::Invalid(format!("{} is ambiguous; use project/{0}", env.unwrap_or("")))),
        }
    }
}

#[derive(Serialize, Deserialize)]
struct SnapshotFile {
    format: String,
    version: u32,
    sealed: Sealed,
}

/// Approve mode: what a machine reads to ask for values. Written by the
/// user's devices, renewed daily like a snapshot. Names only, never values.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mailbox {
    /// A machine refuses one older than it has seen.
    pub stamp: Stamp,
    pub expires: u64,
    /// When this mailbox stops being valid, before its URLs expire.
    pub lease_until: u64,
    /// Each environment's variable names, by label.
    pub environments: BTreeMap<String, Vec<String>>,
    /// A presigned PUT for the doorbell.
    pub doorbell: String,
    /// Presigned URLs for each slot.
    pub slots: Vec<SlotUrls>,
}

/// The URLs a mailbox gives the machine, signed by the device that wrote
/// it: the machine holds no storage key.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MailboxUrls {
    pub doorbell: String,
    pub slots: Vec<SlotUrls>,
}

/// One slot's presigned URLs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlotUrls {
    /// PUT the request.
    pub request: String,
    /// DELETE the answer once read.
    pub answer_delete: String,
}

/// Approve mode: a machine asking for values for one command. What the
/// approval dialog shows.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Request {
    pub id: Uuid,
    /// Unix milliseconds: when it was asked, and when the machine stops
    /// waiting.
    pub at: u64,
    pub expires: u64,
    /// The machine's host name, as it says.
    pub host: String,
    /// Who asked on the machine, e.g. an agent's name or `dotsafe run`.
    pub client: String,
    /// `run` (an agent's, output scrubbed) or `exec` (output unscrubbed).
    pub kind: String,
    pub command: Vec<String>,
    pub program_path: String,
    pub cwd: String,
    /// The environment's label in the mailbox.
    pub environment: String,
    pub variables: Vec<String>,
    /// Variables that go through the secret proxy on the machine.
    #[serde(default)]
    pub proxied: Vec<String>,
    /// Scripts the command runs, with when each changed.
    #[serde(default)]
    pub files: Vec<RequestFile>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestFile {
    pub path: String,
    pub modified_ms: Option<u64>,
}

/// Approve mode: a device's answer to one request.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Answer {
    /// The request's id.
    pub id: Uuid,
    pub allowed: bool,
    /// How long the machine may run the same command again with these
    /// values without asking: 0 for this run only.
    pub reuse_ms: u64,
    /// The approved values. Empty when denied.
    pub values: BTreeMap<String, String>,
    pub at: u64,
}

impl Answer {
    pub fn denied(id: Uuid, at: u64) -> Answer {
        Answer { id, allowed: false, reuse_ms: 0, values: BTreeMap::new(), at }
    }
}

/// A share token, parsed on the machine.
pub struct ShareToken {
    pub vault_id: Uuid,
    pub share_id: Uuid,
    key: Zeroizing<[u8; KEY_LEN]>,
    path: [u8; PATH_LEN],
    public_url: String,
}

impl std::fmt::Debug for ShareToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ShareToken").field("share_id", &self.share_id).finish_non_exhaustive()
    }
}

impl ShareToken {
    pub fn parse(token: &str) -> Result<ShareToken> {
        let not_a_token = || Error::Invalid("that isn't a dotsafe share token (they start with dss_)".into());
        let body = token.trim().strip_prefix(TOKEN_PREFIX).ok_or_else(not_a_token)?;
        let bytes = crockford_decode_vec(body).ok_or_else(not_a_token)?;
        match bytes.first() {
            Some(&TOKEN_VERSION) => {}
            Some(&version) => return Err(Error::UnsupportedVersion { kind: "share token", version: version.into() }),
            None => return Err(not_a_token()),
        }
        if bytes.len() <= TOKEN_FIXED {
            return Err(not_a_token());
        }
        let uuid = |at: usize| Uuid::from_slice(&bytes[at..at + 16]).expect("16 bytes");
        let mut key = Zeroizing::new([0u8; KEY_LEN]);
        key.copy_from_slice(&bytes[33..33 + KEY_LEN]);
        let path: [u8; PATH_LEN] = bytes[33 + KEY_LEN..TOKEN_FIXED].try_into().expect("16 bytes");
        let url = std::str::from_utf8(&bytes[TOKEN_FIXED..]).map_err(|_| not_a_token())?;
        Ok(ShareToken { vault_id: uuid(1), share_id: uuid(17), key, path, public_url: public_base(url)? })
    }

    /// A tag for an answer to `dotsafe daemon --share`, made with a subkey of
    /// the share key. Whoever holds the token can read the share's values
    /// anyway, and the commands the daemon runs never get the token.
    pub fn approval_tag(&self, message: &[u8]) -> String {
        hex(&crypto::mac(&crypto::hkdf(None, &self.key[..], APPROVAL_LABEL), message))
    }

    /// Whether `tag` is [`ShareToken::approval_tag`] of `message`, in constant time.
    pub fn check_approval_tag(&self, message: &[u8], tag: &str) -> bool {
        let Some(bytes) = crate::vault::decode_hex(tag) else { return false };
        crypto::verify_mac(&crypto::hkdf(None, &self.key[..], APPROVAL_LABEL), message, &bytes)
    }

    /// Where the snapshot is: the bucket's public URL, then `/s/<path>`.
    pub fn snapshot_url(&self) -> String {
        format!("{}/s/{}", self.public_url, hex(&self.path))
    }

    /// Approve mode: where the mailbox is.
    pub fn mailbox_url(&self) -> String {
        format!("{}/m/{}/mailbox", self.public_url, hex(&self.path))
    }

    /// Approve mode: where the request in `slot` is, to see if it's free.
    pub fn request_url(&self, slot: u8) -> String {
        format!("{}/m/{}/q/{slot}", self.public_url, hex(&self.path))
    }

    /// Approve mode: where the answer to the request in `slot` appears.
    pub fn answer_url(&self, slot: u8) -> String {
        format!("{}/m/{}/a/{slot}", self.public_url, hex(&self.path))
    }

    /// Opens a mailbox fetched from [`ShareToken::mailbox_url`], with the
    /// same checks as a snapshot: authentic, not older than `seen`, and
    /// within its expiry and lease at `now`.
    pub fn open_mailbox(&self, bytes: &[u8], seen: Option<Stamp>, now: u64) -> Result<Mailbox> {
        let aad = message_aad(MAILBOX_LABEL, self.vault_id, self.share_id, &[]);
        let plaintext = open_file(&self.key, MAILBOX_FORMAT, MAILBOX_LABEL, bytes, &aad)?;
        let mailbox: Mailbox = serde_json::from_slice(&plaintext)?;
        if seen.is_some_and(|seen| mailbox.stamp < seen) {
            return Err(Error::Invalid(
                "the share's mailbox is older than one this machine already used; the bucket may have been rolled back".into(),
            ));
        }
        if mailbox.expires <= now {
            return Err(Error::Invalid("this share has expired; create a new one on your Mac".into()));
        }
        if mailbox.lease_until <= now {
            return Err(Error::Invalid(
                "this share's mailbox wasn't refreshed in time; unlock dotsafe on one of your devices so it writes a fresh one".into(),
            ));
        }
        if mailbox.slots.len() != usize::from(SLOTS) {
            return Err(Error::Invalid("this share's mailbox is malformed".into()));
        }
        Ok(mailbox)
    }

    /// The sealed request for `slot`.
    pub fn seal_request(&self, slot: u8, request: &Request) -> Result<Vec<u8>> {
        let plaintext = crate::vault::padded(serde_json::to_vec(request)?);
        let aad = message_aad(REQUEST_LABEL, self.vault_id, self.share_id, &[slot]);
        seal_file(&self.key, REQUEST_FORMAT, REQUEST_LABEL, &plaintext, &aad)
    }

    /// Opens the answer in `slot` to the request with `id`. An answer to an
    /// earlier request in the same slot fails, as does a tampered one.
    pub fn open_answer(&self, slot: u8, id: Uuid, bytes: &[u8]) -> Result<Answer> {
        let aad = answer_aad(self.vault_id, self.share_id, slot, id);
        let plaintext = open_file(&self.key, ANSWER_FORMAT, ANSWER_LABEL, bytes, &aad)?;
        let answer: Answer = serde_json::from_slice(&plaintext)?;
        if answer.id != id {
            return Err(Error::MessageTampered);
        }
        Ok(answer)
    }

    /// Opens a snapshot object fetched from [`ShareToken::snapshot_url`].
    /// Refuses one that fails authentication, is older than `seen` (the
    /// newest stamp this machine has read), or is past its expiry or lease
    /// at `now`. A machine that has seen nothing trusts the first.
    pub fn open(&self, bytes: &[u8], seen: Option<Stamp>, now: u64) -> Result<Snapshot> {
        let file: SnapshotFile =
            serde_json::from_slice(bytes).map_err(|_| Error::Invalid("that isn't a dotsafe share snapshot".into()))?;
        if file.format != SNAPSHOT_FORMAT {
            return Err(Error::Invalid("that isn't a dotsafe share snapshot".into()));
        }
        if file.version != SNAPSHOT_VERSION {
            return Err(Error::UnsupportedVersion { kind: "share snapshot", version: file.version });
        }
        let key = snapshot_key(&self.key);
        let plaintext =
            crypto::open(&key, &file.sealed, &snapshot_aad(self.vault_id, self.share_id)).ok_or(Error::SnapshotTampered)?;
        let snapshot: Snapshot = serde_json::from_slice(&plaintext)?;
        if seen.is_some_and(|seen| snapshot.stamp < seen) {
            return Err(Error::Invalid(
                "the share's snapshot is older than one this machine already used; the bucket may have been rolled back".into(),
            ));
        }
        if snapshot.expires <= now {
            return Err(Error::Invalid("this share has expired; create a new one on your Mac".into()));
        }
        if snapshot.lease_until <= now {
            return Err(Error::Invalid(
                "this share's snapshot wasn't refreshed in time (its lease ran out); unlock dotsafe on one of your devices so it pushes a fresh one".into(),
            ));
        }
        Ok(snapshot)
    }
}

/// Seals an approve-mode message under the share key's subkey for `label`.
fn seal_file(share_key: &[u8; KEY_LEN], format: &str, label: &[u8], plaintext: &[u8], aad: &[u8]) -> Result<Vec<u8>> {
    let key = crypto::hkdf(None, share_key, label);
    let file = SnapshotFile { format: format.into(), version: MESSAGE_VERSION, sealed: crypto::seal(&key, plaintext, aad)? };
    Ok(serde_json::to_vec(&file)?)
}

/// Opens an approve-mode message sealed by [`seal_file`].
fn open_file(share_key: &[u8; KEY_LEN], format: &str, label: &[u8], bytes: &[u8], aad: &[u8]) -> Result<Zeroizing<Vec<u8>>> {
    if bytes.len() > MAX_MESSAGE_BYTES {
        return Err(Error::Invalid(format!("that {format} is too large")));
    }
    let file: SnapshotFile = serde_json::from_slice(bytes).map_err(|_| Error::Invalid(format!("that isn't a {format}")))?;
    if file.format != format {
        return Err(Error::Invalid(format!("that isn't a {format}")));
    }
    if file.version != MESSAGE_VERSION {
        return Err(Error::UnsupportedVersion { kind: "share message", version: file.version });
    }
    crypto::open(&crypto::hkdf(None, share_key, label), &file.sealed, aad).ok_or(Error::MessageTampered)
}

/// Binds a message to its vault, share and (for requests) slot.
fn message_aad(label: &[u8], vault_id: Uuid, share_id: Uuid, extra: &[u8]) -> Vec<u8> {
    [label, vault_id.as_bytes(), share_id.as_bytes(), extra].concat()
}

/// Binds an answer to its slot and the request it answers.
fn answer_aad(vault_id: Uuid, share_id: Uuid, slot: u8, request: Uuid) -> Vec<u8> {
    message_aad(ANSWER_LABEL, vault_id, share_id, &[&[slot][..], request.as_bytes()].concat())
}

fn snapshot_key(share_key: &[u8; KEY_LEN]) -> SecretKey {
    crypto::hkdf(None, share_key, SNAPSHOT_LABEL)
}

/// Binds a snapshot to its vault and share, so it can't be served for
/// another one.
fn snapshot_aad(vault_id: Uuid, share_id: Uuid) -> Vec<u8> {
    [SNAPSHOT_LABEL, vault_id.as_bytes(), share_id.as_bytes()].concat()
}

/// The names `template` refers to with `${{ NAME }}`, skipping escaped
/// `$${{`. Malformed references are left to the resolver to report.
fn references(template: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut rest = template;
    while let Some(at) = rest.find("${{") {
        let escaped = rest[..at].ends_with('$');
        let after = &rest[at + 3..];
        let Some(end) = after.find("}}") else { break };
        let name = after[..end].trim();
        if !escaped && crate::is_valid_var_name(name) {
            names.push(name.to_string());
        }
        rest = if escaped { after } else { &after[end + 2..] };
    }
    names
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn references_are_found_and_escapes_skipped() {
        assert_eq!(references("postgres://${{ USER }}:${{PASS}}@h/$${{ LITERAL }}"), vec!["USER", "PASS"]);
        assert!(references("no refs").is_empty());
    }

    #[test]
    fn public_urls_are_https_without_queries() {
        assert_eq!(public_base("https://shares.example.com/").unwrap(), "https://shares.example.com");
        for bad in ["http://shares.example.com", "https://", "https://x.com/?a=b", "https://user@x.com", "ftp://x"] {
            assert!(public_base(bad).is_err(), "{bad}");
        }
    }
}
