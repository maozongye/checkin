//! Backend pointer format `wbtcred_ptr_v1` (inline envelope or file+key).
//!
//! CredMan blob limit is 2560; pointers stay ≪ that. Legacy raw envelope
//! bytes (no pointer wrapper) remain readable for one-round compatibility.

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::error::{ResolveError, StoreError};

/// Max sealed envelope length stored inline in the Backend pointer.
/// Chosen so base64(seal)+JSON overhead stays under CredMan 2560 (~2460 worst case).
pub const INLINE_MAX: usize = 1800;

/// Soft max for the full Backend blob (pointer JSON). Above this → file mode.
pub const BACKEND_BLOB_SOFT_MAX: usize = 2400;

pub const POINTER_FORMAT: &str = "wbtcred_ptr_v1";

/// Decoded Backend value.
pub enum DecodedBackendValue {
    /// Pointer JSON (v1).
    Pointer(StoragePointer),
    /// Pre-pointer era: raw CredentialEnvelope seal bytes.
    LegacyEnvelope(Vec<u8>),
}

#[derive(Clone, Serialize, Deserialize)]
pub struct StoragePointer {
    pub format: String,
    #[serde(flatten)]
    pub body: PointerBody,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "mode")]
pub enum PointerBody {
    #[serde(rename = "inline")]
    Inline {
        /// Base64 of CredentialEnvelope seal bytes.
        envelope: String,
    },
    #[serde(rename = "file")]
    File {
        /// Relative to vault_dir, e.g. `blobs/<uuid>.cve`.
        rel_path: String,
        /// Base64 of 32-byte XChaCha20-Poly1305 key (only in Backend).
        file_key_b64: String,
    },
}

impl std::fmt::Debug for StoragePointer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.body {
            PointerBody::Inline { .. } => f
                .debug_struct("StoragePointer")
                .field("format", &self.format)
                .field("mode", &"inline")
                .field("envelope", &"[REDACTED]")
                .finish(),
            PointerBody::File { rel_path, .. } => f
                .debug_struct("StoragePointer")
                .field("format", &self.format)
                .field("mode", &"file")
                .field("rel_path", rel_path)
                .field("file_key_b64", &"[REDACTED]")
                .finish(),
        }
    }
}

impl StoragePointer {
    pub fn inline(envelope_seal: &[u8]) -> Self {
        Self {
            format: POINTER_FORMAT.to_string(),
            body: PointerBody::Inline {
                envelope: B64.encode(envelope_seal),
            },
        }
    }

    pub fn file(rel_path: String, file_key: &[u8; 32]) -> Self {
        Self {
            format: POINTER_FORMAT.to_string(),
            body: PointerBody::File {
                rel_path,
                file_key_b64: B64.encode(file_key),
            },
        }
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, StoreError> {
        serde_json::to_vec(self).map_err(|e| StoreError::Internal(format!("pointer seal: {e}")))
    }

    pub fn decode_inline_envelope(&self) -> Result<Vec<u8>, ResolveError> {
        match &self.body {
            PointerBody::Inline { envelope } => B64
                .decode(envelope.as_bytes())
                .map_err(|e| ResolveError::CorruptEnvelope(format!("pointer envelope b64: {e}"))),
            PointerBody::File { .. } => Err(ResolveError::Internal(
                "decode_inline_envelope called on file pointer".into(),
            )),
        }
    }

    pub fn file_parts(&self) -> Result<(&str, Zeroizing<[u8; 32]>), ResolveError> {
        match &self.body {
            PointerBody::File {
                rel_path,
                file_key_b64,
            } => {
                let raw = B64.decode(file_key_b64.as_bytes()).map_err(|e| {
                    ResolveError::CorruptEnvelope(format!("pointer file_key b64: {e}"))
                })?;
                if raw.len() != 32 {
                    return Err(ResolveError::CorruptEnvelope(format!(
                        "file_key length {}, expected 32",
                        raw.len()
                    )));
                }
                let mut key = Zeroizing::new([0u8; 32]);
                key.copy_from_slice(&raw);
                Ok((rel_path.as_str(), key))
            }
            PointerBody::Inline { .. } => Err(ResolveError::Internal(
                "file_parts called on inline pointer".into(),
            )),
        }
    }

    pub fn is_file(&self) -> bool {
        matches!(self.body, PointerBody::File { .. })
    }
}

/// Parse Backend blob: pointer JSON or legacy raw envelope.
pub fn decode_backend_value(bytes: &[u8]) -> Result<DecodedBackendValue, ResolveError> {
    if let Ok(ptr) = serde_json::from_slice::<StoragePointer>(bytes) {
        if ptr.format == POINTER_FORMAT {
            return Ok(DecodedBackendValue::Pointer(ptr));
        }
    }
    Ok(DecodedBackendValue::LegacyEnvelope(bytes.to_vec()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inline_pointer_roundtrip_and_size() {
        let seal = b"{\"kind\":\"workbuddy_tokens\",\"schema_version\":1}";
        let ptr = StoragePointer::inline(seal);
        let bytes = ptr.to_bytes().unwrap();
        assert!(bytes.len() < 512, "pointer len={}", bytes.len());
        let decoded = decode_backend_value(&bytes).unwrap();
        match decoded {
            DecodedBackendValue::Pointer(p) => {
                let env = p.decode_inline_envelope().unwrap();
                assert_eq!(env, seal);
            }
            _ => panic!("expected pointer"),
        }
    }

    #[test]
    fn legacy_raw_envelope_detected() {
        let raw = br#"{"kind":"workbuddy_tokens","schema_version":1,"material":[],"meta":{"platform":"workbuddy","account_key":"u","imported_at":0,"source_hint":null}}"#;
        match decode_backend_value(raw).unwrap() {
            DecodedBackendValue::LegacyEnvelope(b) => assert_eq!(b, raw),
            _ => panic!("expected legacy"),
        }
    }
}
