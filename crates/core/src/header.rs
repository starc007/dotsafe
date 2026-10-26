//! The vault header (`vault.json`): format version, vault id and the key slots
//! that each wrap the same vault key: the password, the recovery key, one per
//! enrolled device (unlocked with Touch ID), and short-lived enrollment codes
//! for adding a device.

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::crypto::{self, KEY_LEN, Kdf, KdfParams, Sealed, SecretKey};
use crate::error::{Error, Result};

pub const HEADER_FORMAT: &str = "dotsafe-vault";
/// v3 added device and enrollment slots, and re-keying.
pub const HEADER_VERSION: u32 = 3;

// These labels only separate domains; they're unchanged from v1 so existing
// slots and MACs stay valid.
const SLOT_AAD: &[u8] = b"dotsafe/v1/slot";
const HEADER_MAC_INFO: &[u8] = b"dotsafe/v1/header-mac";
const RECOVERY_KEK_INFO: &[u8] = b"dotsafe/v1/recovery-kek";
const DEVICE_KEK_INFO: &[u8] = b"dotsafe/v3/device-kek";
const ENROLLMENT_KEK_INFO: &[u8] = b"dotsafe/v3/enrollment-kek";
const PREVIOUS_KEY_AAD: &[u8] = b"dotsafe/v3/previous-key";
/// How long an enrollment code works.
pub const ENROLLMENT_TTL_MS: u64 = 10 * 60 * 1000;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VaultHeader {
    pub format: String,
    pub version: u32,
    pub vault_id: Uuid,
    /// Which vault key seals records. It goes up when the vault is re-keyed.
    /// Absent (0) in v1 headers.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub epoch: u32,
    pub key_slots: Vec<KeySlot>,
    /// Vault keys of earlier epochs, sealed under the current one, kept until
    /// every record is re-sealed. Empty until re-keying exists.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_keys: Vec<PreviousKey>,
    /// HMAC over the header serialized with this field empty, keyed from the
    /// vault key. Detects edits to the header by anyone without the vault key.
    #[serde(with = "crypto::b64")]
    pub mac: Vec<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PreviousKey {
    pub epoch: u32,
    pub wrapped_key: Sealed,
}

fn is_zero(n: &u32) -> bool {
    *n == 0
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct KeySlot {
    pub id: Uuid,
    pub unlock: Unlock,
    /// The vault key, sealed under this slot's key-encryption key.
    pub wrapped_key: Sealed,
    /// The key epoch of the vault key it wraps. A slot from an earlier epoch
    /// is stale (its device must unlock with the password once). Absent (0)
    /// in v1 and v2 slots, which all belong to epoch 1.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub epoch: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Unlock {
    Password {
        kdf: Kdf,
    },
    Recovery {
        #[serde(with = "crypto::b64")]
        salt: Vec<u8>,
    },
    /// One device's own key, kept in its keychain behind Touch ID.
    Device {
        device_id: Uuid,
        name: String,
        /// Unix milliseconds.
        added: u64,
        #[serde(with = "crypto::b64")]
        salt: Vec<u8>,
    },
    /// A one-time code for adding a device, valid until `expires`.
    Enrollment {
        /// Unix milliseconds.
        expires: u64,
        #[serde(with = "crypto::b64")]
        salt: Vec<u8>,
    },
}

/// An enrolled device, as the Devices screen lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceInfo {
    pub device_id: Uuid,
    pub name: String,
    pub added: u64,
    /// Its slot wraps an earlier vault key: it needs the password once.
    pub stale: bool,
}

impl VaultHeader {
    pub(crate) fn new(vault_key: &SecretKey, password: &[u8], params: KdfParams) -> Result<Self> {
        let mut header = VaultHeader {
            format: HEADER_FORMAT.into(),
            version: HEADER_VERSION,
            vault_id: Uuid::new_v4(),
            epoch: 1,
            key_slots: Vec::new(),
            previous_keys: Vec::new(),
            mac: Vec::new(),
        };
        header.set_password(vault_key, password, params)?;
        Ok(header)
    }

    pub(crate) fn check_version(&self) -> Result<()> {
        if self.format != HEADER_FORMAT {
            return Err(Error::Format(format!("not a dotsafe vault (format {:?})", self.format)));
        }
        // v1 headers are read so they can be upgraded.
        if !(1..=HEADER_VERSION).contains(&self.version) {
            return Err(Error::UnsupportedVersion { kind: "vault", version: self.version });
        }
        Ok(())
    }

    /// Brings a v1 or v2 header up to the current version, then re-signs.
    pub(crate) fn upgrade(&mut self, vault_key: &SecretKey) -> Result<()> {
        self.version = HEADER_VERSION;
        self.epoch = self.epoch.max(1);
        self.sign(vault_key)
    }

    /// Replaces any password slot with one for `password`, then re-signs.
    pub(crate) fn set_password(
        &mut self,
        vault_key: &SecretKey,
        password: &[u8],
        params: KdfParams,
    ) -> Result<()> {
        let kdf = Kdf::new(params)?;
        let kek = kdf.derive(password)?;
        self.replace_slot(vault_key, &kek, Unlock::Password { kdf })
    }

    /// Replaces any recovery slot with a fresh recovery key, then re-signs.
    pub(crate) fn set_recovery(&mut self, vault_key: &SecretKey) -> Result<RecoveryKey> {
        let recovery = RecoveryKey(Zeroizing::new(crypto::random_array()?));
        let salt = crypto::random_array::<16>()?.to_vec();
        let kek = crypto::hkdf(Some(&salt), recovery.0.as_ref(), RECOVERY_KEK_INFO);
        self.replace_slot(vault_key, &kek, Unlock::Recovery { salt })?;
        Ok(recovery)
    }

    fn replace_slot(&mut self, vault_key: &SecretKey, kek: &SecretKey, unlock: Unlock) -> Result<()> {
        let same_kind = |s: &KeySlot| std::mem::discriminant(&s.unlock) == std::mem::discriminant(&unlock);
        self.key_slots.retain(|s| !same_kind(s));
        self.add_slot(vault_key, kek, unlock)
    }

    fn add_slot(&mut self, vault_key: &SecretKey, kek: &SecretKey, unlock: Unlock) -> Result<()> {
        let id = Uuid::new_v4();
        let wrapped_key = crypto::seal(kek, vault_key.as_bytes(), &self.slot_aad(id))?;
        self.key_slots.push(KeySlot { id, unlock, wrapped_key, epoch: self.current_epoch() });
        self.sign(vault_key)
    }

    /// Enrolls a device: a fresh device key, wrapping the vault key, replacing
    /// any slot the device had. The caller keeps the key in the device's keychain.
    pub(crate) fn set_device(
        &mut self,
        vault_key: &SecretKey,
        device_id: Uuid,
        name: &str,
        now_ms: u64,
    ) -> Result<DeviceKey> {
        let key = DeviceKey(Zeroizing::new(crypto::random_array()?));
        let salt = crypto::random_array::<16>()?.to_vec();
        let kek = crypto::hkdf(Some(&salt), key.0.as_ref(), DEVICE_KEK_INFO);
        self.key_slots.retain(|s| !matches!(&s.unlock, Unlock::Device { device_id: d, .. } if *d == device_id));
        let unlock = Unlock::Device { device_id, name: name.to_string(), added: now_ms, salt };
        self.add_slot(vault_key, &kek, unlock)?;
        Ok(key)
    }

    /// Removes a device's slot. Returns whether it had one.
    pub(crate) fn remove_device(&mut self, vault_key: &SecretKey, device_id: Uuid) -> Result<bool> {
        let before = self.key_slots.len();
        self.key_slots.retain(|s| !matches!(&s.unlock, Unlock::Device { device_id: d, .. } if *d == device_id));
        self.sign(vault_key)?;
        Ok(self.key_slots.len() < before)
    }

    /// Adds a one-time enrollment code, valid for ten minutes.
    pub(crate) fn add_enrollment(&mut self, vault_key: &SecretKey, now_ms: u64) -> Result<EnrollmentCode> {
        self.prune_enrollments(now_ms);
        let code = EnrollmentCode(Zeroizing::new(crypto::random_array()?));
        let salt = crypto::random_array::<16>()?.to_vec();
        let kek = crypto::hkdf(Some(&salt), code.0.as_ref(), ENROLLMENT_KEK_INFO);
        self.add_slot(vault_key, &kek, Unlock::Enrollment { expires: now_ms + ENROLLMENT_TTL_MS, salt })?;
        Ok(code)
    }

    /// Drops expired enrollment slots. Doesn't re-sign; callers do.
    pub(crate) fn prune_enrollments(&mut self, now_ms: u64) {
        self.key_slots.retain(|s| !matches!(s.unlock, Unlock::Enrollment { expires, .. } if expires <= now_ms));
    }

    pub(crate) fn remove_slot(&mut self, vault_key: &SecretKey, slot_id: Uuid) -> Result<()> {
        self.key_slots.retain(|s| s.id != slot_id);
        self.sign(vault_key)
    }

    pub(crate) fn devices(&self) -> Vec<DeviceInfo> {
        self.key_slots
            .iter()
            .filter_map(|s| match &s.unlock {
                Unlock::Device { device_id, name, added, .. } => Some(DeviceInfo {
                    device_id: *device_id,
                    name: name.clone(),
                    added: *added,
                    stale: self.slot_epoch(s) != self.current_epoch(),
                }),
                _ => None,
            })
            .collect()
    }

    /// Switches to a new vault key in a new epoch. The password and recovery
    /// slots are recreated for it (so a new recovery key comes back), device
    /// slots stay but go stale, and the old key is kept sealed under the new
    /// one until every record is re-sealed.
    pub(crate) fn rekey(
        &mut self,
        old_key: &SecretKey,
        new_key: &SecretKey,
        password: &[u8],
        params: KdfParams,
    ) -> Result<RecoveryKey> {
        let old_epoch = self.current_epoch();
        self.epoch = old_epoch + 1;
        let aad = self.previous_key_aad(old_epoch);
        self.previous_keys =
            vec![PreviousKey { epoch: old_epoch, wrapped_key: crypto::seal(new_key, old_key.as_bytes(), &aad)? }];
        self.key_slots.retain(|s| matches!(s.unlock, Unlock::Device { .. }));
        self.set_password(new_key, password, params)?;
        self.set_recovery(new_key)
    }

    /// The vault key of an earlier epoch, while it's still kept.
    pub(crate) fn previous_key(&self, current: &SecretKey, epoch: u32) -> Option<SecretKey> {
        let previous = self.previous_keys.iter().find(|p| p.epoch == epoch)?;
        let bytes = crypto::open(current, &previous.wrapped_key, &self.previous_key_aad(epoch))?;
        Some(SecretKey::from_bytes(bytes.as_slice().try_into().ok()?))
    }

    pub(crate) fn drop_previous_keys(&mut self, vault_key: &SecretKey) -> Result<()> {
        self.previous_keys.clear();
        self.sign(vault_key)
    }

    pub(crate) fn current_epoch(&self) -> u32 {
        self.epoch.max(1)
    }

    fn slot_epoch(&self, slot: &KeySlot) -> u32 {
        slot.epoch.max(1)
    }

    fn previous_key_aad(&self, epoch: u32) -> Vec<u8> {
        [PREVIOUS_KEY_AAD, self.vault_id.as_bytes(), &epoch.to_be_bytes()].concat()
    }

    pub(crate) fn unlock_with_device(&self, device_id: Uuid, key: &DeviceKey) -> Result<SecretKey> {
        let slot = self
            .key_slots
            .iter()
            .find(|s| matches!(&s.unlock, Unlock::Device { device_id: d, .. } if *d == device_id))
            .ok_or(Error::InvalidCredentials)?;
        if self.slot_epoch(slot) != self.current_epoch() {
            return Err(Error::DeviceStale);
        }
        let Unlock::Device { salt, .. } = &slot.unlock else { unreachable!("matched a device slot") };
        let kek = crypto::hkdf(Some(salt), key.0.as_ref(), DEVICE_KEK_INFO);
        let vault_key = self.unwrap(slot, &kek).ok_or(Error::InvalidCredentials)?;
        self.verified(vault_key)
    }

    /// Unlocks with an enrollment code. Returns the vault key and the code's
    /// slot, which the caller removes once the device is enrolled.
    pub(crate) fn unlock_with_enrollment(&self, code: &EnrollmentCode, now_ms: u64) -> Result<(SecretKey, Uuid)> {
        for slot in &self.key_slots {
            if let Unlock::Enrollment { expires, salt } = &slot.unlock
                && *expires > now_ms
                && self.slot_epoch(slot) == self.current_epoch()
            {
                let kek = crypto::hkdf(Some(salt), code.0.as_ref(), ENROLLMENT_KEK_INFO);
                if let Some(key) = self.unwrap(slot, &kek) {
                    return Ok((self.verified(key)?, slot.id));
                }
            }
        }
        Err(Error::InvalidCredentials)
    }

    pub(crate) fn unlock_with_password(&self, password: &[u8]) -> Result<SecretKey> {
        for slot in &self.key_slots {
            if let Unlock::Password { kdf } = &slot.unlock
                && self.slot_epoch(slot) == self.current_epoch()
            {
                let kek = kdf.derive(password)?;
                if let Some(key) = self.unwrap(slot, &kek) {
                    return self.verified(key);
                }
            }
        }
        Err(Error::InvalidCredentials)
    }

    pub(crate) fn unlock_with_recovery(&self, recovery: &RecoveryKey) -> Result<SecretKey> {
        for slot in &self.key_slots {
            if let Unlock::Recovery { salt } = &slot.unlock
                && self.slot_epoch(slot) == self.current_epoch()
            {
                let kek = crypto::hkdf(Some(salt), recovery.0.as_ref(), RECOVERY_KEK_INFO);
                if let Some(key) = self.unwrap(slot, &kek) {
                    return self.verified(key);
                }
            }
        }
        Err(Error::InvalidCredentials)
    }

    /// Accepts a vault key cached from an earlier unlock, if it still matches
    /// this header.
    pub(crate) fn unlock_with_key(&self, vault_key: SecretKey) -> Result<SecretKey> {
        self.verified(vault_key).map_err(|_| Error::InvalidCredentials)
    }

    fn unwrap(&self, slot: &KeySlot, kek: &SecretKey) -> Option<SecretKey> {
        let bytes = crypto::open(kek, &slot.wrapped_key, &self.slot_aad(slot.id))?;
        let bytes: [u8; KEY_LEN] = bytes.as_slice().try_into().ok()?;
        Some(SecretKey::from_bytes(bytes))
    }

    fn verified(&self, vault_key: SecretKey) -> Result<SecretKey> {
        let mac_key = vault_key.derive(HEADER_MAC_INFO);
        if crypto::verify_mac(&mac_key, &self.signed_bytes()?, &self.mac) {
            Ok(vault_key)
        } else {
            Err(Error::HeaderTampered)
        }
    }

    fn sign(&mut self, vault_key: &SecretKey) -> Result<()> {
        let mac_key = vault_key.derive(HEADER_MAC_INFO);
        self.mac = crypto::mac(&mac_key, &self.signed_bytes()?);
        Ok(())
    }

    fn signed_bytes(&self) -> Result<Vec<u8>> {
        let unsigned = VaultHeader { mac: Vec::new(), ..self.clone() };
        Ok(serde_json::to_vec(&unsigned)?)
    }

    /// Binds each wrapped key to this vault and slot, so slots can't be
    /// copied between vaults or swapped within one.
    fn slot_aad(&self, slot_id: Uuid) -> Vec<u8> {
        [SLOT_AAD, self.vault_id.as_bytes(), slot_id.as_bytes()].concat()
    }
}

/// A 256-bit key shown to the user once, as a fallback if they forget the
/// password. Displayed as Crockford base32 in groups of four.
pub struct RecoveryKey(Zeroizing<[u8; KEY_LEN]>);

/// A device's own key, kept in its keychain. Anyone holding it can unlock
/// the vault on that device, so never store it anywhere else.
pub struct DeviceKey(Zeroizing<[u8; KEY_LEN]>);

/// A one-time 128-bit code that adds a device. Shown on the device that
/// creates it, typed on the new one. Crockford base32 in groups of four.
pub struct EnrollmentCode(Zeroizing<[u8; 16]>);

const CROCKFORD: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

impl RecoveryKey {
    pub fn to_display_string(&self) -> String {
        crockford_encode(self.0.as_ref())
    }

    /// Parses a recovery key as typed by the user: case-insensitive, with
    /// dashes and spaces ignored, and I/L read as 1 and O as 0.
    pub fn parse(input: &str) -> Result<Self> {
        let bytes = crockford_decode::<KEY_LEN>(input).ok_or_else(|| Error::Format("invalid recovery key".into()))?;
        Ok(RecoveryKey(bytes))
    }
}

impl DeviceKey {
    pub fn as_bytes(&self) -> &[u8; KEY_LEN] {
        &self.0
    }

    pub fn from_bytes(bytes: [u8; KEY_LEN]) -> Self {
        DeviceKey(Zeroizing::new(bytes))
    }
}

impl std::fmt::Debug for DeviceKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("DeviceKey(..)")
    }
}

impl EnrollmentCode {
    pub fn to_display_string(&self) -> String {
        crockford_encode(self.0.as_ref())
    }

    pub fn parse(input: &str) -> Result<Self> {
        let bytes = crockford_decode::<16>(input).ok_or_else(|| Error::Format("invalid code".into()))?;
        Ok(EnrollmentCode(bytes))
    }
}

impl std::fmt::Debug for EnrollmentCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("EnrollmentCode(..)")
    }
}

fn crockford_encode(bytes: &[u8]) -> String {
    let mut chars = Vec::with_capacity(bytes.len() * 8 / 5 + 1);
    let mut acc: u32 = 0;
    let mut bits = 0;
    for &byte in bytes {
        acc = (acc << 8) | byte as u32;
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            chars.push(CROCKFORD[((acc >> bits) & 31) as usize]);
        }
    }
    if bits > 0 {
        chars.push(CROCKFORD[((acc << (5 - bits)) & 31) as usize]);
    }
    chars.chunks(4).map(|c| std::str::from_utf8(c).expect("alphabet is ASCII")).collect::<Vec<_>>().join("-")
}

/// Exactly `N` bytes from Crockford base32 as people type it.
fn crockford_decode<const N: usize>(input: &str) -> Option<Zeroizing<[u8; N]>> {
    let mut out = Zeroizing::new([0u8; N]);
    let (mut acc, mut bits, mut len) = (0u32, 0, 0usize);
    for c in input.chars().filter(|c| !matches!(c, '-' | ' ')) {
        let c = match c.to_ascii_uppercase() {
            'I' | 'L' => '1',
            'O' => '0',
            c => c,
        };
        let value = CROCKFORD.iter().position(|&a| a as char == c)?;
        acc = (acc << 5) | value as u32;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            *out.get_mut(len)? = (acc >> bits) as u8;
            len += 1;
        }
    }
    (len == N).then_some(out)
}

impl std::fmt::Debug for RecoveryKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("RecoveryKey(..)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FAST: KdfParams = KdfParams { m_cost_kib: 64, t_cost: 1, p_cost: 1 };

    #[test]
    fn enrollment_codes_expire_and_are_pruned() {
        let key = SecretKey::generate().unwrap();
        let mut header = VaultHeader::new(&key, b"pw", FAST).unwrap();
        let code = header.add_enrollment(&key, 1_000).unwrap();
        assert!(header.unlock_with_enrollment(&code, 1_000 + ENROLLMENT_TTL_MS - 1).is_ok());
        assert!(matches!(header.unlock_with_enrollment(&code, 1_000 + ENROLLMENT_TTL_MS), Err(Error::InvalidCredentials)));

        header.prune_enrollments(1_000 + ENROLLMENT_TTL_MS);
        assert!(!header.key_slots.iter().any(|s| matches!(s.unlock, Unlock::Enrollment { .. })));
    }

    #[test]
    fn codes_round_trip_through_their_display_form() {
        let key = SecretKey::generate().unwrap();
        let mut header = VaultHeader::new(&key, b"pw", FAST).unwrap();
        let code = header.add_enrollment(&key, 0).unwrap();
        let shown = code.to_display_string();
        assert_eq!(shown.len(), 26 + 6, "26 characters in groups of four");
        assert!(header.unlock_with_enrollment(&EnrollmentCode::parse(&shown).unwrap(), 1).is_ok());
    }
}
