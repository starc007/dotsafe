//! Cryptographic primitives. Everything else in the crate goes through here.
//!
//! - AEAD: XChaCha20-Poly1305 with random 24-byte nonces.
//! - Password KDF: Argon2id.
//! - Subkeys: HKDF-SHA256.
//! - Header MAC: HMAC-SHA256.

use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

use crate::error::{Error, Result};

pub const KEY_LEN: usize = 32;
const NONCE_LEN: usize = 24;
const SALT_LEN: usize = 16;

// Upper bounds on KDF parameters read from disk, so a tampered header can't
// make unlock exhaust memory or hang.
const MAX_M_COST_KIB: u32 = 2 * 1024 * 1024;
const MAX_T_COST: u32 = 64;
const MAX_P_COST: u32 = 16;

/// A 256-bit symmetric key, wiped from memory on drop.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct SecretKey([u8; KEY_LEN]);

impl SecretKey {
    pub fn generate() -> Result<Self> {
        Ok(Self(random_array()?))
    }

    pub(crate) fn from_bytes(bytes: [u8; KEY_LEN]) -> Self {
        Self(bytes)
    }

    pub(crate) fn as_bytes(&self) -> &[u8; KEY_LEN] {
        &self.0
    }

    /// Derives an independent subkey for one purpose, so the same key is never
    /// used by two different constructions.
    pub(crate) fn derive(&self, info: &[u8]) -> SecretKey {
        hkdf(None, &self.0, info)
    }
}

impl std::fmt::Debug for SecretKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SecretKey(..)")
    }
}

pub(crate) fn random_array<const N: usize>() -> Result<[u8; N]> {
    let mut buf = [0u8; N];
    getrandom::fill(&mut buf).map_err(|_| Error::Rng)?;
    Ok(buf)
}

/// `n` bytes from the OS random number generator.
pub(crate) fn random_bytes(n: usize) -> Result<Zeroizing<Vec<u8>>> {
    let mut buf = Zeroizing::new(vec![0u8; n]);
    getrandom::fill(&mut buf).map_err(|_| Error::Rng)?;
    Ok(buf)
}

pub(crate) fn hkdf(salt: Option<&[u8]>, ikm: &[u8], info: &[u8]) -> SecretKey {
    let mut okm = [0u8; KEY_LEN];
    Hkdf::<Sha256>::new(salt, ikm)
        .expand(info, &mut okm)
        .expect("32 bytes is a valid HKDF-SHA256 output length");
    SecretKey(okm)
}

/// Ciphertext plus the nonce it was sealed with.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sealed {
    #[serde(with = "b64")]
    pub nonce: Vec<u8>,
    #[serde(with = "b64")]
    pub ct: Vec<u8>,
}

pub(crate) fn seal(key: &SecretKey, plaintext: &[u8], aad: &[u8]) -> Result<Sealed> {
    let cipher = XChaCha20Poly1305::new(&Key::from(*key.as_bytes()));
    let nonce = XNonce::from(random_array::<NONCE_LEN>()?);
    let ct = cipher
        .encrypt(&nonce, Payload { msg: plaintext, aad })
        .map_err(|_| Error::Format("encryption failed".into()))?;
    Ok(Sealed { nonce: nonce.to_vec(), ct })
}

/// Returns `None` when authentication fails (wrong key, or data/AAD changed).
pub(crate) fn open(key: &SecretKey, sealed: &Sealed, aad: &[u8]) -> Option<Zeroizing<Vec<u8>>> {
    let nonce: [u8; NONCE_LEN] = sealed.nonce.as_slice().try_into().ok()?;
    let cipher = XChaCha20Poly1305::new(&Key::from(*key.as_bytes()));
    cipher
        .decrypt(&XNonce::from(nonce), Payload { msg: &sealed.ct, aad })
        .ok()
        .map(Zeroizing::new)
}

pub(crate) fn mac(key: &SecretKey, data: &[u8]) -> Vec<u8> {
    let mut m = <Hmac<Sha256> as KeyInit>::new_from_slice(key.as_bytes())
        .expect("HMAC accepts any key length");
    m.update(data);
    m.finalize().into_bytes().to_vec()
}

pub(crate) fn verify_mac(key: &SecretKey, data: &[u8], tag: &[u8]) -> bool {
    let mut m = <Hmac<Sha256> as KeyInit>::new_from_slice(key.as_bytes())
        .expect("HMAC accepts any key length");
    m.update(data);
    m.verify_slice(tag).is_ok()
}

/// Argon2id cost settings used when creating a password slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KdfParams {
    pub m_cost_kib: u32,
    pub t_cost: u32,
    pub p_cost: u32,
}

impl Default for KdfParams {
    /// 64 MiB, 3 passes, 1 lane, in line with OWASP's Argon2id guidance.
    fn default() -> Self {
        Self { m_cost_kib: 64 * 1024, t_cost: 3, p_cost: 1 }
    }
}

/// How a password slot turns the password into a key. Stored in the header.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "alg", rename_all = "lowercase")]
pub enum Kdf {
    Argon2id {
        m_cost_kib: u32,
        t_cost: u32,
        p_cost: u32,
        #[serde(with = "b64")]
        salt: Vec<u8>,
    },
}

impl Kdf {
    pub(crate) fn new(params: KdfParams) -> Result<Self> {
        Ok(Kdf::Argon2id {
            m_cost_kib: params.m_cost_kib,
            t_cost: params.t_cost,
            p_cost: params.p_cost,
            salt: random_array::<SALT_LEN>()?.to_vec(),
        })
    }

    pub(crate) fn derive(&self, password: &[u8]) -> Result<SecretKey> {
        let Kdf::Argon2id { m_cost_kib, t_cost, p_cost, salt } = self;
        if *m_cost_kib > MAX_M_COST_KIB || *t_cost > MAX_T_COST || *p_cost > MAX_P_COST {
            return Err(Error::Format("KDF parameters out of range".into()));
        }
        let params = Params::new(*m_cost_kib, *t_cost, *p_cost, Some(KEY_LEN))
            .map_err(|e| Error::Format(format!("invalid KDF parameters: {e}")))?;
        let mut out = [0u8; KEY_LEN];
        Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
            .hash_password_into(password, salt, &mut out)
            .map_err(|e| Error::Format(format!("key derivation failed: {e}")))?;
        Ok(SecretKey(out))
    }
}

/// Serde helper: bytes as standard base64 strings.
pub(crate) mod b64 {
    use base64::Engine;
    use base64::engine::general_purpose::STANDARD;
    use serde::{Deserialize, Deserializer, Serializer, de::Error};

    pub fn serialize<S: Serializer>(bytes: &[u8], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&STANDARD.encode(bytes))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let s = String::deserialize(d)?;
        STANDARD.decode(s).map_err(D::Error::custom)
    }
}
