//! CredentialEnvelope seal / unseal (schema_version = 1).

use serde::{Deserialize, Serialize};
use zeroize::ZeroizeOnDrop;

use crate::error::ResolveError;
use crate::secret::MaterialBytes;
use crate::types::{CredentialMeta, MaterialKind};

pub const SCHEMA_VERSION: u32 = 1;

/// Sealed credential stored in the Backend (serialized JSON bytes).
#[derive(Serialize, Deserialize, ZeroizeOnDrop)]
pub struct CredentialEnvelope {
    #[zeroize(skip)]
    pub kind: MaterialKind,
    #[zeroize(skip)]
    pub schema_version: u32,
    pub material: MaterialBytes,
    #[zeroize(skip)]
    pub meta: CredentialMeta,
}

impl std::fmt::Debug for CredentialEnvelope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CredentialEnvelope")
            .field("kind", &self.kind)
            .field("schema_version", &self.schema_version)
            .field("material", &"MaterialBytes([REDACTED])")
            .field("meta", &self.meta)
            .finish()
    }
}

impl CredentialEnvelope {
    pub fn new(kind: MaterialKind, material: MaterialBytes, meta: CredentialMeta) -> Self {
        Self {
            kind,
            schema_version: SCHEMA_VERSION,
            material,
            meta,
        }
    }

    pub fn seal_to_bytes(&self) -> Result<Vec<u8>, String> {
        serde_json::to_vec(self).map_err(|e| format!("seal: {e}"))
    }

    pub fn unseal(bytes: &[u8]) -> Result<Self, ResolveError> {
        let env: CredentialEnvelope = serde_json::from_slice(bytes)
            .map_err(|e| ResolveError::CorruptEnvelope(format!("json: {e}")))?;
        if env.schema_version != SCHEMA_VERSION {
            return Err(ResolveError::CorruptEnvelope(format!(
                "unsupported schema_version {}",
                env.schema_version
            )));
        }
        Ok(env)
    }
}
