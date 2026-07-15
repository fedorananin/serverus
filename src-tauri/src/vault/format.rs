//! Vault file format and crypto (SPEC §2.2).
//!
//! ```text
//! master password ──Argon2id(salt)──▶ KEK
//! KEK  ──AES-256-GCM──▶ DEK (random 256-bit data key)
//! DEK  ──AES-256-GCM──▶ payload (vault JSON)
//! ```
//!
//! Binary layout (all integers little-endian):
//!
//! ```text
//! offset  size  field
//! 0       4     magic "SRVS"
//! 4       1     format version (1)
//! 5       4     argon2 m_cost (KiB)
//! 9       4     argon2 t_cost
//! 13      4     argon2 p_cost
//! 17      16    argon2 salt
//! 33      12    DEK wrap nonce
//! 45      48    DEK ciphertext (32 bytes + 16 tag)
//! 93      12    payload nonce
//! 105     ..    payload ciphertext
//! ```
//!
//! The header (bytes 0..33) is authenticated as AAD of the DEK wrap; the
//! header plus the wrapped DEK (bytes 0..93) is the AAD of the payload. Any
//! tampering with KDF params, salt or wrapped key fails decryption.

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use argon2::{Algorithm, Argon2, Params, Version};
use rand::rngs::OsRng;
use rand::RngCore;
use zeroize::Zeroizing;

use crate::error::{AppError, AppResult};

pub const MAGIC: &[u8; 4] = b"SRVS";
pub const FORMAT_VERSION: u8 = 1;

const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 12;
const DEK_LEN: usize = 32;
const TAG_LEN: usize = 16;
const DEK_CT_LEN: usize = DEK_LEN + TAG_LEN;
const HEADER_LEN: usize = 4 + 1 + 12 + SALT_LEN; // through salt
const PREFIX_LEN: usize = HEADER_LEN + NONCE_LEN + DEK_CT_LEN; // through wrapped DEK

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KdfParams {
    pub m_cost_kib: u32,
    pub t_cost: u32,
    pub p_cost: u32,
}

impl Default for KdfParams {
    /// SPEC §2.2: m=64 MiB, t=3, p=4.
    fn default() -> Self {
        KdfParams {
            m_cost_kib: 64 * 1024,
            t_cost: 3,
            p_cost: 4,
        }
    }
}

/// Everything needed to re-encrypt and save the vault without re-deriving
/// the KEK: KDF params, salt and the wrapped DEK stay stable across saves.
#[derive(Debug, Clone)]
pub struct VaultHeader {
    pub kdf: KdfParams,
    pub salt: [u8; SALT_LEN],
    pub dek_nonce: [u8; NONCE_LEN],
    pub dek_ct: [u8; DEK_CT_LEN],
}

impl VaultHeader {
    /// Serialized bytes 0..PREFIX_LEN (header + wrapped DEK).
    fn prefix_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(PREFIX_LEN);
        out.extend_from_slice(MAGIC);
        out.push(FORMAT_VERSION);
        out.extend_from_slice(&self.kdf.m_cost_kib.to_le_bytes());
        out.extend_from_slice(&self.kdf.t_cost.to_le_bytes());
        out.extend_from_slice(&self.kdf.p_cost.to_le_bytes());
        out.extend_from_slice(&self.salt);
        out.extend_from_slice(&self.dek_nonce);
        out.extend_from_slice(&self.dek_ct);
        out
    }

    fn header_bytes(&self) -> Vec<u8> {
        self.prefix_bytes()[..HEADER_LEN].to_vec()
    }
}

pub fn random_bytes<const N: usize>() -> [u8; N] {
    let mut buf = [0u8; N];
    OsRng.fill_bytes(&mut buf);
    buf
}

/// Derive the key-encryption key from the master password.
pub fn derive_kek(
    password: &str,
    salt: &[u8],
    kdf: &KdfParams,
) -> AppResult<Zeroizing<[u8; DEK_LEN]>> {
    let params = Params::new(kdf.m_cost_kib, kdf.t_cost, kdf.p_cost, Some(DEK_LEN))
        .map_err(|e| AppError::Other(format!("invalid KDF params: {e}")))?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut kek = Zeroizing::new([0u8; DEK_LEN]);
    argon
        .hash_password_into(password.as_bytes(), salt, kek.as_mut())
        .map_err(|e| AppError::Other(format!("key derivation failed: {e}")))?;
    Ok(kek)
}

fn cipher(key: &[u8; DEK_LEN]) -> Aes256Gcm {
    Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key))
}

/// Create a fresh header wrapping `dek` under a KEK derived from `password`.
pub fn wrap_new(password: &str, dek: &[u8; DEK_LEN], kdf: KdfParams) -> AppResult<VaultHeader> {
    let salt: [u8; SALT_LEN] = random_bytes();
    let kek = derive_kek(password, &salt, &kdf)?;
    let dek_nonce: [u8; NONCE_LEN] = random_bytes();

    let mut header = VaultHeader {
        kdf,
        salt,
        dek_nonce,
        dek_ct: [0u8; DEK_CT_LEN],
    };
    let aad = header.header_bytes();
    let ct = cipher(&kek)
        .encrypt(
            Nonce::from_slice(&dek_nonce),
            Payload {
                msg: dek.as_ref(),
                aad: &aad,
            },
        )
        .map_err(|_| AppError::Other("DEK encryption failed".into()))?;
    header.dek_ct.copy_from_slice(&ct);
    Ok(header)
}

/// Unwrap the DEK using the master password. Wrong password → InvalidPassword.
pub fn unwrap_dek(header: &VaultHeader, password: &str) -> AppResult<Zeroizing<[u8; DEK_LEN]>> {
    let kek = derive_kek(password, &header.salt, &header.kdf)?;
    let aad = header.header_bytes();
    let pt = cipher(&kek)
        .decrypt(
            Nonce::from_slice(&header.dek_nonce),
            Payload {
                msg: &header.dek_ct,
                aad: &aad,
            },
        )
        .map_err(|_| AppError::InvalidPassword)?;
    let mut dek = Zeroizing::new([0u8; DEK_LEN]);
    dek.copy_from_slice(&pt);
    Ok(dek)
}

/// Encrypt the payload and produce the complete file image.
pub fn seal(header: &VaultHeader, dek: &[u8; DEK_LEN], payload_json: &[u8]) -> AppResult<Vec<u8>> {
    let payload_nonce: [u8; NONCE_LEN] = random_bytes();
    let prefix = header.prefix_bytes();
    let ct = cipher(dek)
        .encrypt(
            Nonce::from_slice(&payload_nonce),
            Payload {
                msg: payload_json,
                aad: &prefix,
            },
        )
        .map_err(|_| AppError::Other("payload encryption failed".into()))?;

    let mut out = prefix;
    out.extend_from_slice(&payload_nonce);
    out.extend_from_slice(&ct);
    Ok(out)
}

/// Parse a file image into header + encrypted payload parts.
pub fn parse(bytes: &[u8]) -> AppResult<(VaultHeader, [u8; NONCE_LEN], Vec<u8>)> {
    if bytes.len() < PREFIX_LEN + NONCE_LEN + TAG_LEN {
        return Err(AppError::Corrupted("file too short".into()));
    }
    if &bytes[0..4] != MAGIC {
        return Err(AppError::Corrupted("bad magic".into()));
    }
    let version = bytes[4];
    if version != FORMAT_VERSION {
        return Err(AppError::UnsupportedVersion(version));
    }
    let le_u32 = |off: usize| u32::from_le_bytes(bytes[off..off + 4].try_into().unwrap());
    let kdf = KdfParams {
        m_cost_kib: le_u32(5),
        t_cost: le_u32(9),
        p_cost: le_u32(13),
    };
    let mut salt = [0u8; SALT_LEN];
    salt.copy_from_slice(&bytes[17..17 + SALT_LEN]);
    let mut dek_nonce = [0u8; NONCE_LEN];
    dek_nonce.copy_from_slice(&bytes[HEADER_LEN..HEADER_LEN + NONCE_LEN]);
    let mut dek_ct = [0u8; DEK_CT_LEN];
    dek_ct.copy_from_slice(&bytes[HEADER_LEN + NONCE_LEN..PREFIX_LEN]);

    let mut payload_nonce = [0u8; NONCE_LEN];
    payload_nonce.copy_from_slice(&bytes[PREFIX_LEN..PREFIX_LEN + NONCE_LEN]);
    let payload_ct = bytes[PREFIX_LEN + NONCE_LEN..].to_vec();

    Ok((
        VaultHeader {
            kdf,
            salt,
            dek_nonce,
            dek_ct,
        },
        payload_nonce,
        payload_ct,
    ))
}

/// Decrypt the payload part parsed by [`parse`].
pub fn open_payload(
    header: &VaultHeader,
    dek: &[u8; DEK_LEN],
    payload_nonce: &[u8; NONCE_LEN],
    payload_ct: &[u8],
) -> AppResult<Zeroizing<Vec<u8>>> {
    let prefix = header.prefix_bytes();
    let pt = cipher(dek)
        .decrypt(
            Nonce::from_slice(payload_nonce),
            Payload {
                msg: payload_ct,
                aad: &prefix,
            },
        )
        .map_err(|_| AppError::Corrupted("payload authentication failed".into()))?;
    Ok(Zeroizing::new(pt))
}

#[cfg(test)]
mod tests;
