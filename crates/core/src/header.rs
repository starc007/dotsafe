//! The vault header (`vault.json`): format version, vault id and the key slots
//! that each wrap the same vault key.

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::crypto::{self, KEY_LEN, Kdf, KdfParams, Sealed, SecretKey};
use crate::error::{Error, Result};

pub const HEADER_FORMAT: &str = "dotsafe-vault";
pub const HEADER_VERSION: u32 = 1;

const SLOT_AAD: &[u8] = b"dotsafe/v1/slot";
const HEADER_MAC_INFO: &[u8] = b"dotsafe/v1/header-mac";
const RECOVERY_KEK_INFO: &[u8] = b"dotsafe/v1/recovery-kek";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VaultHeader {
    pub format: String,
    pub version: u32,
    pub vault_id: Uuid,
    pub key_slots: Vec<KeySlot>,
    /// HMAC over the header serialized with this field empty, keyed from the
    /// vault key. Detects edits to the header by anyone without the vault key.
    #[serde(with = "crypto::b64")]
    pub mac: Vec<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct KeySlot {
    pub id: Uuid,
    pub unlock: Unlock,
    /// The vault key, sealed under this slot's key-encryption key.
    pub wrapped_key: Sealed,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Unlock {
    Password { kdf: Kdf },
    Recovery {
        #[serde(with = "crypto::b64")]
        salt: Vec<u8>,
    },
}

impl VaultHeader {
    pub(crate) fn new(vault_key: &SecretKey, password: &[u8], params: KdfParams) -> Result<Self> {
        let mut header = VaultHeader {
            format: HEADER_FORMAT.into(),
            version: HEADER_VERSION,
            vault_id: Uuid::new_v4(),
            key_slots: Vec::new(),
            mac: Vec::new(),
        };
        header.set_password(vault_key, password, params)?;
        Ok(header)
    }

    pub(crate) fn check_version(&self) -> Result<()> {
        if self.format != HEADER_FORMAT {
            return Err(Error::Format(format!("not a dotsafe vault (format {:?})", self.format)));
        }
        if self.version != HEADER_VERSION {
            return Err(Error::UnsupportedVersion { kind: "vault", version: self.version });
        }
        Ok(())
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
        let id = Uuid::new_v4();
        let wrapped_key = crypto::seal(kek, vault_key.as_bytes(), &self.slot_aad(id))?;
        let same_kind = |s: &KeySlot| std::mem::discriminant(&s.unlock) == std::mem::discriminant(&unlock);
        self.key_slots.retain(|s| !same_kind(s));
        self.key_slots.push(KeySlot { id, unlock, wrapped_key });
        self.sign(vault_key)
    }

    pub(crate) fn unlock_with_password(&self, password: &[u8]) -> Result<SecretKey> {
        for slot in &self.key_slots {
            if let Unlock::Password { kdf } = &slot.unlock {
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
            if let Unlock::Recovery { salt } = &slot.unlock {
                let kek = crypto::hkdf(Some(salt), recovery.0.as_ref(), RECOVERY_KEK_INFO);
                if let Some(key) = self.unwrap(slot, &kek) {
                    return self.verified(key);
                }
            }
        }
        Err(Error::InvalidCredentials)
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

const CROCKFORD: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

impl RecoveryKey {
    pub fn to_display_string(&self) -> String {
        let mut chars = Vec::with_capacity(52);
        let mut acc: u32 = 0;
        let mut bits = 0;
        for &byte in self.0.iter() {
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
        chars
            .chunks(4)
            .map(|c| std::str::from_utf8(c).expect("alphabet is ASCII"))
            .collect::<Vec<_>>()
            .join("-")
    }

    /// Parses a recovery key as typed by the user: case-insensitive, with
    /// dashes and spaces ignored, and I/L read as 1 and O as 0.
    pub fn parse(input: &str) -> Result<Self> {
        let invalid = || Error::Format("invalid recovery key".into());
        let mut out = Zeroizing::new([0u8; KEY_LEN]);
        let (mut acc, mut bits, mut len) = (0u32, 0, 0usize);
        for c in input.chars().filter(|c| !matches!(c, '-' | ' ')) {
            let c = match c.to_ascii_uppercase() {
                'I' | 'L' => '1',
                'O' => '0',
                c => c,
            };
            let value = CROCKFORD.iter().position(|&a| a as char == c).ok_or_else(invalid)?;
            acc = (acc << 5) | value as u32;
            bits += 5;
            if bits >= 8 {
                bits -= 8;
                *out.get_mut(len).ok_or_else(invalid)? = (acc >> bits) as u8;
                len += 1;
            }
        }
        if len != KEY_LEN {
            return Err(invalid());
        }
        Ok(RecoveryKey(out))
    }
}

impl std::fmt::Debug for RecoveryKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("RecoveryKey(..)")
    }
}
