//! Encrypted backup: Argon2id + XChaCha20-Poly1305 (`.cred.bak`).
//!
//! File layout:
//! ```text
//! magic:  b"WBTCRED1"   (8)
//! salt:   16 bytes
//! nonce:  24 bytes
//! ct+tag: rest (XChaCha20-Poly1305)
//! ```
//! KDF: Argon2id m=65536, t=3, p=1 → 32-byte key

use std::fs;
use std::path::{Path, PathBuf};

use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use rand::RngCore;
use zeroize::Zeroizing;

use crate::error::BackupError;
use crate::secret::SecretString;

pub const MAGIC: &[u8; 8] = b"WBTCRED1";
const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 24;
const KEY_LEN: usize = 32;

fn argon2() -> Result<Argon2<'static>, BackupError> {
    let params = Params::new(65536, 3, 1, Some(KEY_LEN))
        .map_err(|e| BackupError::Crypto(format!("argon2 params: {e}")))?;
    Ok(Argon2::new(Algorithm::Argon2id, Version::V0x13, params))
}

fn derive_key(password: &str, salt: &[u8]) -> Result<Zeroizing<[u8; KEY_LEN]>, BackupError> {
    let a2 = argon2()?;
    let mut key = Zeroizing::new([0u8; KEY_LEN]);
    a2.hash_password_into(password.as_bytes(), salt, &mut *key)
        .map_err(|e| BackupError::Crypto(format!("argon2: {e}")))?;
    Ok(key)
}

/// Encrypt `plaintext` (envelope seal bytes) to `.cred.bak` bytes.
pub fn encrypt_backup(plaintext: &[u8], password: &SecretString) -> Result<Vec<u8>, BackupError> {
    let mut salt = [0u8; SALT_LEN];
    let mut nonce_bytes = [0u8; NONCE_LEN];
    rand::thread_rng().fill_bytes(&mut salt);
    rand::thread_rng().fill_bytes(&mut nonce_bytes);

    let key = derive_key(password.expose(), &salt)?;
    let cipher = XChaCha20Poly1305::new_from_slice(&*key)
        .map_err(|e| BackupError::Crypto(format!("cipher init: {e}")))?;
    let nonce = XNonce::from_slice(&nonce_bytes);
    let ct = cipher
        .encrypt(nonce, plaintext)
        .map_err(|e| BackupError::Crypto(format!("encrypt: {e}")))?;

    let mut out = Vec::with_capacity(8 + SALT_LEN + NONCE_LEN + ct.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&salt);
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ct);
    Ok(out)
}

/// Decrypt `.cred.bak` bytes → envelope seal plaintext.
pub fn decrypt_backup(file_bytes: &[u8], password: &SecretString) -> Result<Vec<u8>, BackupError> {
    if file_bytes.len() < 8 + SALT_LEN + NONCE_LEN + 16 {
        return Err(BackupError::InvalidFile("too short".into()));
    }
    if &file_bytes[..8] != MAGIC {
        return Err(BackupError::InvalidFile("bad magic".into()));
    }
    let salt = &file_bytes[8..8 + SALT_LEN];
    let nonce_bytes = &file_bytes[8 + SALT_LEN..8 + SALT_LEN + NONCE_LEN];
    let ct = &file_bytes[8 + SALT_LEN + NONCE_LEN..];

    let key = derive_key(password.expose(), salt)?;
    let cipher = XChaCha20Poly1305::new_from_slice(&*key)
        .map_err(|e| BackupError::Crypto(format!("cipher init: {e}")))?;
    let nonce = XNonce::from_slice(nonce_bytes);
    cipher
        .decrypt(nonce, ct)
        .map_err(|_| BackupError::DecryptFailed)
}

/// Write encrypted backup to `dir/{id}-{unix_ms}.cred.bak`.
pub fn write_backup_file(
    dir: &Path,
    id_stem: &str,
    plaintext: &[u8],
    password: &SecretString,
) -> Result<PathBuf, BackupError> {
    fs::create_dir_all(dir).map_err(|e| BackupError::Io(e.to_string()))?;
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let path = dir.join(format!("{id_stem}-{ms}.cred.bak"));
    let bytes = encrypt_backup(plaintext, password)?;
    fs::write(&path, &bytes).map_err(|e| BackupError::Io(e.to_string()))?;
    Ok(path)
}

pub fn read_backup_file(path: &Path, password: &SecretString) -> Result<Vec<u8>, BackupError> {
    let bytes = fs::read(path).map_err(|e| BackupError::Io(e.to_string()))?;
    decrypt_backup(&bytes, password)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let pw = SecretString::new("test-pass-飞");
        let pt = b"{\"kind\":\"workbuddy_tokens\"}";
        let enc = encrypt_backup(pt, &pw).unwrap();
        assert!(enc.starts_with(MAGIC));
        let dec = decrypt_backup(&enc, &pw).unwrap();
        assert_eq!(dec, pt);
    }

    #[test]
    fn wrong_password_fails() {
        let enc = encrypt_backup(b"secret-data", &SecretString::new("right")).unwrap();
        assert!(matches!(
            decrypt_backup(&enc, &SecretString::new("wrong")),
            Err(BackupError::DecryptFailed)
        ));
    }
}
