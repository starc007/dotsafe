//! Shared secrets: a value defined once and linked into any number of
//! environments, with its version history and the rotation flow.
//!
//! A version moves through `pending` (staged, not used yet) → `current`
//! (what linked variables resolve to) → `previous` (still valid while the
//! old value is being revoked) → `retired`. Versions merge by id, and each
//! version's state by its own stamp, so two devices rotating at once still
//! converge.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::crypto;
use crate::error::{Error, Result};
use crate::model::Stamp;

const DAY_MS: u64 = 24 * 60 * 60 * 1000;
/// Retired values are wiped this long after retirement.
const RETENTION_MS: u64 = 30 * DAY_MS;
/// Values older than the newest this many versions are wiped.
const HISTORY: usize = 10;
/// A secret counts as due soon this long before its rotation date.
const SOON_MS: u64 = 7 * DAY_MS;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Secret {
    pub id: Uuid,
    /// Shown in lists, and the default variable name when linking.
    pub name: String,
    pub deleted: bool,
    /// Stamp for `name`, `deleted` and `policy`.
    pub updated: Stamp,
    #[serde(default)]
    pub policy: Policy,
    versions: BTreeMap<Uuid, Version>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Policy {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotate_every_days: Option<u32>,
    /// How new values are made when rotating, if they're generated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generator: Option<Generator>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VersionState {
    Pending,
    Current,
    Previous,
    Retired,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Version {
    /// `None` once wiped (retired past retention, or beyond the history limit).
    pub value: Option<String>,
    pub state: VersionState,
    pub created: Stamp,
    /// Stamp for `value`, `state` and `note`. The newest change wins on merge.
    pub changed: Stamp,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// Where a secret stands against its rotation policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Due {
    /// No rotation policy.
    NoPolicy,
    Ok,
    Soon,
    Overdue,
}

impl Secret {
    pub fn new(name: impl Into<String>, value: impl Into<String>, stamp: Stamp) -> Secret {
        let version = Version {
            value: Some(value.into()),
            state: VersionState::Current,
            created: stamp,
            changed: stamp,
            note: None,
        };
        Secret {
            id: Uuid::new_v4(),
            name: name.into(),
            deleted: false,
            updated: stamp,
            policy: Policy::default(),
            versions: BTreeMap::from([(Uuid::new_v4(), version)]),
        }
    }

    /// Versions, newest first.
    pub fn versions(&self) -> Vec<(Uuid, &Version)> {
        let mut versions: Vec<_> = self.versions.iter().map(|(id, v)| (*id, v)).collect();
        versions.sort_by_key(|(_, v)| std::cmp::Reverse(v.created));
        versions
    }

    /// What linked variables resolve to. If two devices promoted at once, the
    /// later promotion wins.
    pub fn current(&self) -> Option<(Uuid, &Version)> {
        self.latest(VersionState::Current)
    }

    pub fn current_value(&self) -> Option<&str> {
        self.current()?.1.value.as_deref()
    }

    pub fn pending(&self) -> Option<(Uuid, &Version)> {
        self.latest(VersionState::Pending)
    }

    fn latest(&self, state: VersionState) -> Option<(Uuid, &Version)> {
        self.versions.iter().filter(|(_, v)| v.state == state).max_by_key(|(_, v)| v.changed).map(|(id, v)| (*id, v))
    }

    /// Stages a new value without using it yet.
    pub fn stage(&mut self, value: impl Into<String>, note: Option<String>, stamp: Stamp) -> Result<Uuid> {
        if self.pending().is_some() {
            return Err(Error::Invalid("a new value is already staged; promote or cancel it first".into()));
        }
        let id = Uuid::new_v4();
        let version =
            Version { value: Some(value.into()), state: VersionState::Pending, created: stamp, changed: stamp, note };
        self.versions.insert(id, version);
        self.tidy(stamp);
        Ok(id)
    }

    /// Makes the staged value current. The old current value becomes
    /// previous: still listed as valid until it's retired. Older previous
    /// values have now been replaced twice, so they're retired.
    pub fn promote(&mut self, stamp: Stamp) -> Result<()> {
        let (pending, _) = self.pending().ok_or_else(|| Error::Invalid("there's no staged value to promote".into()))?;
        for (id, version) in self.versions.iter_mut() {
            let next = match version.state {
                _ if *id == pending => VersionState::Current,
                VersionState::Current => VersionState::Previous,
                VersionState::Previous => VersionState::Retired,
                _ => continue,
            };
            version.state = next;
            version.changed = stamp;
        }
        self.tidy(stamp);
        Ok(())
    }

    /// Drops the staged value. It was never used, so it's wiped right away.
    pub fn cancel(&mut self, stamp: Stamp) -> Result<()> {
        let (pending, _) = self.pending().ok_or_else(|| Error::Invalid("there's no staged value to cancel".into()))?;
        let version = self.versions.get_mut(&pending).expect("pending version exists");
        version.state = VersionState::Retired;
        version.value = None;
        version.changed = stamp;
        Ok(())
    }

    /// Goes back to the most recent previous value; the current one is retired.
    pub fn roll_back(&mut self, stamp: Stamp) -> Result<()> {
        let (current, _) = self.current().ok_or_else(|| Error::Invalid("there's no current value".into()))?;
        let (previous, _) = self
            .latest(VersionState::Previous)
            .filter(|(_, v)| v.value.is_some())
            .ok_or_else(|| Error::Invalid("there's no previous value to go back to".into()))?;
        for (id, state) in [(current, VersionState::Retired), (previous, VersionState::Current)] {
            let version = self.versions.get_mut(&id).expect("version exists");
            version.state = state;
            version.changed = stamp;
        }
        self.tidy(stamp);
        Ok(())
    }

    /// Marks previous values as revoked. Returns how many there were.
    pub fn retire_previous(&mut self, stamp: Stamp) -> usize {
        let mut retired = 0;
        for version in self.versions.values_mut().filter(|v| v.state == VersionState::Previous) {
            version.state = VersionState::Retired;
            version.changed = stamp;
            retired += 1;
        }
        self.tidy(stamp);
        retired
    }

    /// Stages and promotes in one step.
    pub fn rotate(&mut self, value: impl Into<String>, note: Option<String>, stamp: Stamp) -> Result<Uuid> {
        let id = self.stage(value, note, stamp)?;
        self.promote(stamp)?;
        Ok(id)
    }

    /// Makes an earlier value current again, as a new version so history
    /// stays in order.
    pub fn restore(&mut self, version: Uuid, stamp: Stamp) -> Result<Uuid> {
        let value = self
            .versions
            .get(&version)
            .ok_or_else(|| Error::Invalid("that version doesn't exist".into()))?
            .value
            .clone()
            .ok_or_else(|| Error::Invalid("that version's value has been wiped".into()))?;
        self.rotate(value, Some("Restored an earlier value".into()), stamp)
    }

    pub fn rename(&mut self, name: impl Into<String>, stamp: Stamp) {
        self.name = name.into();
        self.updated = stamp;
    }

    pub fn set_policy(&mut self, policy: Policy, stamp: Stamp) {
        self.policy = policy;
        self.updated = stamp;
    }

    /// Deletes the secret and wipes every value.
    pub fn delete(&mut self, stamp: Stamp) {
        self.deleted = true;
        self.updated = stamp;
        for version in self.versions.values_mut() {
            version.value = None;
            version.state = VersionState::Retired;
            version.changed = stamp;
        }
    }

    /// How long ago the current value was created.
    pub fn age_ms(&self, now_ms: u64) -> Option<u64> {
        Some(now_ms.saturating_sub(self.current()?.1.created.at_ms))
    }

    pub fn due(&self, now_ms: u64) -> Due {
        let (Some(days), Some(age)) = (self.policy.rotate_every_days, self.age_ms(now_ms)) else {
            return Due::NoPolicy;
        };
        let limit = u64::from(days) * DAY_MS;
        match age {
            age if age >= limit => Due::Overdue,
            age if age + SOON_MS >= limit => Due::Soon,
            _ => Due::Ok,
        }
    }

    /// Wipes values that are no longer needed: retired past retention, and
    /// anything beyond the history limit that isn't in use.
    fn tidy(&mut self, stamp: Stamp) {
        let mut by_age: Vec<Uuid> = self.versions().into_iter().map(|(id, _)| id).collect();
        let beyond_history: Vec<Uuid> = by_age.split_off(HISTORY.min(by_age.len()));
        for (id, version) in self.versions.iter_mut() {
            let in_use =
                matches!(version.state, VersionState::Pending | VersionState::Current | VersionState::Previous);
            let expired = version.state == VersionState::Retired
                && stamp.at_ms.saturating_sub(version.changed.at_ms) >= RETENTION_MS;
            if version.value.is_some() && !in_use && (expired || beyond_history.contains(id)) {
                version.value = None;
                version.changed = stamp;
            }
        }
    }

    pub(crate) fn max_stamp(&self) -> Stamp {
        self.versions.values().flat_map(|v| [v.created, v.changed]).fold(self.updated, Stamp::max)
    }

    pub(crate) fn merge(self, other: Secret) -> Secret {
        let (mut newer, older) = if other.updated > self.updated { (other, self) } else { (self, other) };
        for (id, theirs) in older.versions {
            match newer.versions.get(&id) {
                Some(ours) if ours.changed >= theirs.changed => {}
                _ => {
                    newer.versions.insert(id, theirs);
                }
            }
        }
        newer
    }
}

/// How rotation makes new values.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Generator {
    /// Random characters from `charset`.
    Random {
        length: u32,
        charset: Charset,
    },
    Uuid,
    /// Random bytes, hex-encoded.
    Hex {
        bytes: u32,
    },
    /// Random bytes, URL-safe base64 without padding.
    Base64 {
        bytes: u32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Charset {
    /// A–Z, a–z, 0–9.
    Alphanumeric,
    /// Alphanumeric plus symbols that rarely need quoting in shells or URLs.
    Symbols,
}

impl Default for Generator {
    fn default() -> Self {
        Generator::Random { length: 40, charset: Charset::Alphanumeric }
    }
}

impl Generator {
    /// A new value from the OS random number generator.
    pub fn generate(&self) -> Result<Zeroizing<String>> {
        use base64::Engine;
        match *self {
            Generator::Random { length, charset } => {
                if !(8..=256).contains(&length) {
                    return Err(Error::Invalid("generated values must be 8 to 256 characters".into()));
                }
                let alphabet: &[u8] = match charset {
                    Charset::Alphanumeric => b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789",
                    Charset::Symbols => b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_.~!*",
                };
                // Rejection sampling, so every character is equally likely.
                let limit = 256 - (256 % alphabet.len());
                let mut out = Zeroizing::new(String::with_capacity(length as usize));
                while out.len() < length as usize {
                    for byte in crypto::random_bytes(length as usize * 2)?.iter() {
                        if (*byte as usize) < limit && out.len() < length as usize {
                            out.push(alphabet[*byte as usize % alphabet.len()] as char);
                        }
                    }
                }
                Ok(out)
            }
            Generator::Uuid => {
                let bytes: [u8; 16] = crypto::random_bytes(16)?.as_slice().try_into().expect("16 bytes");
                Ok(Zeroizing::new(uuid::Builder::from_random_bytes(bytes).into_uuid().to_string()))
            }
            Generator::Hex { bytes } => {
                let bytes = crypto::random_bytes(Self::byte_count(bytes)?)?;
                Ok(Zeroizing::new(bytes.iter().map(|b| format!("{b:02x}")).collect()))
            }
            Generator::Base64 { bytes } => {
                let bytes = crypto::random_bytes(Self::byte_count(bytes)?)?;
                Ok(Zeroizing::new(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes.as_slice())))
            }
        }
    }

    fn byte_count(bytes: u32) -> Result<usize> {
        if (8..=128).contains(&bytes) {
            Ok(bytes as usize)
        } else {
            Err(Error::Invalid("generated values must be 8 to 128 bytes".into()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEV: Uuid = Uuid::nil();

    fn at(ms: u64) -> Stamp {
        Stamp::new(ms, DEV)
    }

    fn states(secret: &Secret) -> Vec<(Option<&str>, VersionState)> {
        secret.versions().into_iter().map(|(_, v)| (v.value.as_deref(), v.state)).collect()
    }

    #[test]
    fn rotation_flow() {
        let mut s = Secret::new("STRIPE_KEY", "old", at(1));
        s.stage("new", Some("quarterly".into()), at(2)).unwrap();
        assert_eq!(s.current_value(), Some("old"), "staged values aren't used yet");
        assert!(s.stage("other", None, at(3)).is_err(), "one staged value at a time");

        s.promote(at(3)).unwrap();
        assert_eq!(s.current_value(), Some("new"));
        assert_eq!(states(&s), [(Some("new"), VersionState::Current), (Some("old"), VersionState::Previous)]);

        assert_eq!(s.retire_previous(at(4)), 1);
        assert_eq!(states(&s)[1], (Some("old"), VersionState::Retired));
    }

    #[test]
    fn cancel_and_roll_back() {
        let mut s = Secret::new("K", "one", at(1));
        s.stage("typo", None, at(2)).unwrap();
        s.cancel(at(3)).unwrap();
        assert_eq!(states(&s)[0], (None, VersionState::Retired), "cancelled values are wiped");
        assert_eq!(s.current_value(), Some("one"));

        s.rotate("two", None, at(4)).unwrap();
        s.roll_back(at(5)).unwrap();
        assert_eq!(s.current_value(), Some("one"));
        assert!(s.roll_back(at(6)).is_err(), "nothing previous left");
    }

    #[test]
    fn restore_makes_a_new_version() {
        let mut s = Secret::new("K", "one", at(1));
        let first = s.versions()[0].0;
        s.rotate("two", None, at(2)).unwrap();
        s.restore(first, at(3)).unwrap();
        assert_eq!(s.current_value(), Some("one"));
        assert_eq!(s.versions().len(), 3);
    }

    #[test]
    fn old_values_are_wiped() {
        let mut s = Secret::new("K", "v0", at(1));
        s.rotate("v1", None, at(2)).unwrap();
        s.retire_previous(at(3));
        // 30 days after retiring, the next change wipes it.
        s.rotate("v2", None, at(3 + RETENTION_MS)).unwrap();
        assert!(s.versions().iter().any(|(_, v)| v.value.is_none() && v.state == VersionState::Retired));

        let mut s = Secret::new("K", "v0", at(1));
        for i in 1..=12u64 {
            s.rotate(format!("v{i}"), None, at(1 + i)).unwrap();
        }
        let kept = s.versions().iter().filter(|(_, v)| v.value.is_some()).count();
        assert!(kept <= HISTORY + 1, "kept {kept}");
        assert_eq!(s.current_value(), Some("v12"));
    }

    #[test]
    fn concurrent_rotations_converge() {
        let base = Secret::new("K", "base", at(1));
        let (mut a, mut b) = (base.clone(), base.clone());
        a.rotate("from-a", None, Stamp::new(10, Uuid::from_u128(1))).unwrap();
        b.rotate("from-b", None, Stamp::new(11, Uuid::from_u128(2))).unwrap();
        let ab = a.clone().merge(b.clone());
        let ba = b.merge(a);
        assert_eq!(ab.current_value(), Some("from-b"), "the later promotion wins");
        assert_eq!(ab.current_value(), ba.current_value());
        assert_eq!(ab.versions().len(), 3);
    }

    #[test]
    fn due_dates() {
        let mut s = Secret::new("K", "v", at(0));
        assert_eq!(s.due(100 * DAY_MS), Due::NoPolicy);
        s.set_policy(Policy { rotate_every_days: Some(90), generator: None }, at(1));
        assert_eq!(s.due(10 * DAY_MS), Due::Ok);
        assert_eq!(s.due(85 * DAY_MS), Due::Soon);
        assert_eq!(s.due(90 * DAY_MS), Due::Overdue);
    }

    #[test]
    fn generators() {
        let value = Generator::Random { length: 48, charset: Charset::Alphanumeric }.generate().unwrap();
        assert_eq!(value.len(), 48);
        assert!(value.chars().all(|c| c.is_ascii_alphanumeric()));
        assert_ne!(*value, *Generator::default().generate().unwrap());
        assert_eq!(Generator::Hex { bytes: 16 }.generate().unwrap().len(), 32);
        assert_eq!(Generator::Base64 { bytes: 30 }.generate().unwrap().len(), 40);
        assert!(Uuid::parse_str(&Generator::Uuid.generate().unwrap()).is_ok());
        assert!(Generator::Random { length: 4, charset: Charset::Symbols }.generate().is_err());
        assert!(Generator::Hex { bytes: 1000 }.generate().is_err());
    }
}
