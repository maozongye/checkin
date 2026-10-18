//! CredentialHandle — short-lived resolved material (memory only).

use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::envelope::CredentialEnvelope;
use crate::error::{KindMismatch, ResolveError};
use crate::types::{
    CredentialMeta, MaterialKind, TraeCnBlobView, TraeCnMaterial, WorkBuddyMaterial,
    WorkBuddyTokensView,
};

#[derive(Zeroize, ZeroizeOnDrop)]
enum ParsedMaterial {
    WorkBuddy(WorkBuddyMaterial),
    TraeCn(TraeCnMaterial),
}

/// Resolved credential; drop to zeroize material.
#[derive(ZeroizeOnDrop)]
pub struct CredentialHandle {
    #[zeroize(skip)]
    kind: MaterialKind,
    #[zeroize(skip)]
    schema_version: u32,
    #[zeroize(skip)]
    meta: CredentialMeta,
    parsed: ParsedMaterial,
}

impl std::fmt::Debug for CredentialHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CredentialHandle([REDACTED])")
    }
}

impl CredentialHandle {
    pub(crate) fn from_envelope(envelope: CredentialEnvelope) -> Result<Self, ResolveError> {
        let kind = envelope.kind;
        let schema_version = envelope.schema_version;
        let meta = envelope.meta.clone();
        let raw = envelope
            .material
            .as_str()
            .map_err(|e| ResolveError::CorruptEnvelope(format!("utf8: {e}")))?;
        let parsed = match kind {
            MaterialKind::WorkBuddyTokens => {
                let m: WorkBuddyMaterial = serde_json::from_str(raw)
                    .map_err(|e| ResolveError::CorruptEnvelope(format!("wb: {e}")))?;
                ParsedMaterial::WorkBuddy(m)
            }
            MaterialKind::TraeCnICubeBlob => {
                let m: TraeCnMaterial = serde_json::from_str(raw)
                    .map_err(|e| ResolveError::CorruptEnvelope(format!("trae: {e}")))?;
                ParsedMaterial::TraeCn(m)
            }
        };
        drop(envelope);
        Ok(Self {
            kind,
            schema_version,
            meta,
            parsed,
        })
    }

    pub fn kind(&self) -> MaterialKind {
        self.kind
    }

    pub fn meta(&self) -> &CredentialMeta {
        &self.meta
    }

    pub fn schema_version(&self) -> u32 {
        self.schema_version
    }

    pub fn as_workbuddy_tokens(&self) -> Result<WorkBuddyTokensView<'_>, KindMismatch> {
        match &self.parsed {
            ParsedMaterial::WorkBuddy(m) => Ok(WorkBuddyTokensView {
                access_token: &m.access_token,
                refresh_token: &m.refresh_token,
                domain: m.domain.as_deref(),
                expires_at: m.expires_at,
            }),
            _ => Err(KindMismatch),
        }
    }

    pub fn as_trae_cn_blob(&self) -> Result<TraeCnBlobView<'_>, KindMismatch> {
        match &self.parsed {
            ParsedMaterial::TraeCn(m) => Ok(TraeCnBlobView {
                icube_auth_info_cipher: &m.icube_auth_info_cipher,
                device_id: &m.device_id,
                access_token: &m.access_token,
                refresh_token: m.refresh_token.as_deref(),
            }),
            _ => Err(KindMismatch),
        }
    }
}
