//! Encrypted overflow blob files (`.cve`): magic `WBTBLOB1` + XChaCha20-Poly1305.
//!
//! Layout:
//! ```text
//! magic:  b"WBTBLOB1"   (8)
//! nonce:  24 bytes
//! ct+tag: rest
//! ```
//! Key (32 bytes) lives only in the Backend pointer — never on disk beside the file.

use std::fs;
use std::path::{Path, PathBuf};

use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use rand::RngCore;
use uuid::Uuid;

pub const MAGIC: &[u8; 8] = b"WBTBLOB1";
const NONCE_LEN: usize = 24;
pub const KEY_LEN: usize = 32;

/// Encrypt plaintext (envelope seal bytes) → file bytes.
pub fn encrypt_blob(plaintext: &[u8], key: &[u8; KEY_LEN]) -> Result<Vec<u8>, String> {
    let mut nonce_bytes = [0u8; NONCE_LEN];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);

    let cipher = XChaCha20Poly1305::new_from_slice(key)
        .map_err(|e| format!("blob cipher init: {e}"))?;
    let nonce = XNonce::from_slice(&nonce_bytes);
    let ct = cipher
        .encrypt(nonce, plaintext)
        .map_err(|e| format!("blob encrypt: {e}"))?;

    let mut out = Vec::with_capacity(8 + NONCE_LEN + ct.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ct);
    Ok(out)
}

/// Decrypt `.cve` file bytes → envelope seal plaintext.
pub fn decrypt_blob(file_bytes: &[u8], key: &[u8; KEY_LEN]) -> Result<Vec<u8>, String> {
    if file_bytes.len() < 8 + NONCE_LEN + 16 {
        return Err("blob too short".into());
    }
    if &file_bytes[..8] != MAGIC {
        return Err("bad blob magic".into());
    }
    let nonce_bytes = &file_bytes[8..8 + NONCE_LEN];
    let ct = &file_bytes[8 + NONCE_LEN..];

    let cipher = XChaCha20Poly1305::new_from_slice(key)
        .map_err(|e| format!("blob cipher init: {e}"))?;
    let nonce = XNonce::from_slice(nonce_bytes);
    cipher
        .decrypt(nonce, ct)
        .map_err(|_| "blob decrypt failed".into())
}

/// Write encrypted blob under `{vault_dir}/blobs/{uuid}.cve`.
/// Returns `(rel_path, abs_path)`.
pub fn write_blob_file(
    vault_dir: &Path,
    plaintext: &[u8],
    key: &[u8; KEY_LEN],
) -> Result<(String, PathBuf), String> {
    let blobs = vault_dir.join("blobs");
    fs::create_dir_all(&blobs).map_err(|e| format!("create blobs dir: {e}"))?;
    let id = Uuid::new_v4();
    let rel_path = format!("blobs/{id}.cve");
    let abs = vault_dir.join(&rel_path);
    let bytes = encrypt_blob(plaintext, key)?;
    fs::write(&abs, &bytes).map_err(|e| format!("write blob: {e}"))?;
    Ok((rel_path, abs))
}

/// Reject path-unsafe snapshot_id segments (`..`, `/`, `\\`, absolute).
pub fn sanitize_snapshot_id(snapshot_id: &str) -> Result<(), String> {
    if snapshot_id.is_empty() {
        return Err("snapshot_id empty".into());
    }
    if snapshot_id.contains("..")
        || snapshot_id.contains('/')
        || snapshot_id.contains('\\')
        || Path::new(snapshot_id).is_absolute()
    {
        return Err(format!("unsafe snapshot_id: {snapshot_id}"));
    }
    Ok(())
}

/// Write Switcher backup under `{vault_dir}/switch_blobs/{snapshot_id}.cve`.
/// Always file mode; filename is the (sanitized) snapshot_id.
/// Returns `(rel_path, abs_path)`.
pub fn write_switch_blob_file(
    vault_dir: &Path,
    snapshot_id: &str,
    plaintext: &[u8],
    key: &[u8; KEY_LEN],
) -> Result<(String, PathBuf), String> {
    sanitize_snapshot_id(snapshot_id)?;
    let dir = vault_dir.join("switch_blobs");
    fs::create_dir_all(&dir).map_err(|e| format!("create switch_blobs dir: {e}"))?;
    let rel_path = format!("switch_blobs/{snapshot_id}.cve");
    let abs = vault_dir.join(&rel_path);
    let bytes = encrypt_blob(plaintext, key)?;
    fs::write(&abs, &bytes).map_err(|e| format!("write switch blob: {e}"))?;
    Ok((rel_path, abs))
}

/// Best-effort delete of a blob file; returns Ok even if missing.
pub fn delete_blob_file(vault_dir: &Path, rel_path: &str) -> Result<(), String> {
    // Reject path traversal
    if rel_path.contains("..") || Path::new(rel_path).is_absolute() {
        return Err(format!("unsafe rel_path: {rel_path}"));
    }
    let abs = vault_dir.join(rel_path);
    match fs::remove_file(&abs) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("delete blob: {e}")),
    }
}

pub fn read_blob_file(vault_dir: &Path, rel_path: &str) -> Result<Vec<u8>, String> {
    if rel_path.contains("..") || Path::new(rel_path).is_absolute() {
        return Err(format!("unsafe rel_path: {rel_path}"));
    }
    fs::read(vault_dir.join(rel_path)).map_err(|e| format!("read blob: {e}"))
}

pub fn generate_file_key() -> [u8; KEY_LEN] {
    let mut key = [0u8; KEY_LEN];
    rand::thread_rng().fill_bytes(&mut key);
    key
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let key = generate_file_key();
        let pt = b"envelope-seal-bytes-here";
        let enc = encrypt_blob(pt, &key).unwrap();
        assert!(enc.starts_with(MAGIC));
        let dec = decrypt_blob(&enc, &key).unwrap();
        assert_eq!(dec, pt);
    }

    #[test]
    fn wrong_key_fails() {
        let enc = encrypt_blob(b"secret", &generate_file_key()).unwrap();
        assert!(decrypt_blob(&enc, &generate_file_key()).is_err());
    }
}
