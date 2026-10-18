//! Public material kinds, platform ids, and typed views.

use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop};

/// Material shape; add variants without changing IPC shell.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaterialKind {
    /// WorkBuddy: access + refresh (+ optional domain/expires)
    WorkBuddyTokens,
    /// Trae CN: iCubeAuthInfo cipher blob + deviceId (+ optional JWT for LiveHttp)
    TraeCnICubeBlob,
}

/// Platform slug stored in refs / meta.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlatformId {
    Workbuddy,
    TraeCn,
}

impl PlatformId {
    pub fn as_str(self) -> &'static str {
        match self {
            PlatformId::Workbuddy => "workbuddy",
            PlatformId::TraeCn => "trae_cn",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "workbuddy" => Some(PlatformId::Workbuddy),
            "trae_cn" => Some(PlatformId::TraeCn),
            _ => None,
        }
    }

    /// kind ↔ platform must match on store.
    pub fn expected_kind(self) -> MaterialKind {
        match self {
            PlatformId::Workbuddy => MaterialKind::WorkBuddyTokens,
            PlatformId::TraeCn => MaterialKind::TraeCnICubeBlob,
        }
    }
}

impl std::fmt::Display for PlatformId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl MaterialKind {
    pub fn as_str(self) -> &'static str {
        match self {
            MaterialKind::WorkBuddyTokens => "workbuddy_tokens",
            MaterialKind::TraeCnICubeBlob => "trae_cn_icube_blob",
        }
    }

    pub fn matches_platform(self, platform: PlatformId) -> bool {
        platform.expected_kind() == self
    }
}

/// Non-secret metadata stored inside the envelope (still not in SQLite sensitive cols).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CredentialMeta {
    pub platform: PlatformId,
    pub account_key: String,
    /// unix ms
    pub imported_at: i64,
    pub source_hint: Option<String>,
}

/// WorkBuddy material JSON (schema_version = 1).
#[derive(Clone, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
pub struct WorkBuddyMaterial {
    pub access_token: String,
    pub refresh_token: String,
    #[serde(default)]
    pub domain: Option<String>,
    #[serde(default)]
    pub expires_at: Option<i64>,
}

impl std::fmt::Debug for WorkBuddyMaterial {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("WorkBuddyMaterial([REDACTED])")
    }
}

/// Trae CN material JSON (schema_version = 1).
///
/// `icube_auth_info_cipher` + `device_id` are required.
/// Optional `access_token` / `refresh_token` enable LiveHttp claim without
/// desktop session; cipher-only blobs (import today) still parse.
#[derive(Clone, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
pub struct TraeCnMaterial {
    pub icube_auth_info_cipher: String,
    pub device_id: String,
    /// Cloud-IDE-JWT access token when available (empty = needs desktop/session).
    #[serde(default)]
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
}

impl std::fmt::Debug for TraeCnMaterial {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("TraeCnMaterial([REDACTED])")
    }
}

/// Borrowed view over resolved WorkBuddy tokens.
pub struct WorkBuddyTokensView<'a> {
    pub access_token: &'a str,
    pub refresh_token: &'a str,
    pub domain: Option<&'a str>,
    pub expires_at: Option<i64>,
}

impl std::fmt::Debug for WorkBuddyTokensView<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("WorkBuddyTokensView([REDACTED])")
    }
}

/// Borrowed view over resolved Trae CN blob (+ optional JWT).
pub struct TraeCnBlobView<'a> {
    pub icube_auth_info_cipher: &'a str,
    pub device_id: &'a str,
    pub access_token: &'a str,
    pub refresh_token: Option<&'a str>,
}

impl std::fmt::Debug for TraeCnBlobView<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("TraeCnBlobView([REDACTED])")
    }
}

/// Helper: parse secret_json into typed material + re-serialize canonical JSON bytes.
pub(crate) fn parse_material(
    kind: MaterialKind,
    secret_json: &str,
) -> Result<(Vec<u8>, /*validated*/ ()), crate::error::StoreError> {
    use crate::error::StoreError;
    match kind {
        MaterialKind::WorkBuddyTokens => {
            let m: WorkBuddyMaterial = serde_json::from_str(secret_json).map_err(|e| {
                StoreError::InvalidSecret(format!("workbuddy_tokens parse: {e}"))
            })?;
            if m.access_token.is_empty() || m.refresh_token.is_empty() {
                return Err(StoreError::InvalidSecret(
                    "access_token and refresh_token are required".into(),
                ));
            }
            let bytes = serde_json::to_vec(&m).map_err(|e| {
                StoreError::Internal(format!("serialize workbuddy material: {e}"))
            })?;
            drop(m);
            Ok((bytes, ()))
        }
        MaterialKind::TraeCnICubeBlob => {
            let v: serde_json::Value = serde_json::from_str(secret_json).map_err(|e| {
                StoreError::InvalidSecret(format!("trae_cn_icube_blob parse: {e}"))
            })?;
            let cipher = v
                .get("icube_auth_info_cipher")
                .and_then(|x| x.as_str())
                .filter(|s| !s.is_empty());
            let device_id = v
                .get("device_id")
                .and_then(|x| x.as_str())
                .filter(|s| !s.is_empty());
            match (cipher, device_id) {
                (Some(_), Some(_)) => {}
                (None, _) => {
                    return Err(StoreError::InvalidSecret(
                        "icube_auth_info_cipher is required".into(),
                    ));
                }
                (_, None) => {
                    return Err(StoreError::InvalidSecret("device_id is required".into()));
                }
            }
            let m: TraeCnMaterial = serde_json::from_value(v).map_err(|e| {
                StoreError::InvalidSecret(format!("trae material: {e}"))
            })?;
            let bytes = serde_json::to_vec(&m)
                .map_err(|e| StoreError::Internal(format!("serialize trae material: {e}")))?;
            drop(m);
            Ok((bytes, ()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trae_cipher_only_still_parses() {
        let json = r#"{"icube_auth_info_cipher":"CIPHER_x","device_id":"dev-1"}"#;
        let m: TraeCnMaterial = serde_json::from_str(json).unwrap();
        assert_eq!(m.icube_auth_info_cipher, "CIPHER_x");
        assert_eq!(m.device_id, "dev-1");
        assert!(m.access_token.is_empty());
        assert!(m.refresh_token.is_none());
        let (bytes, _) = parse_material(MaterialKind::TraeCnICubeBlob, json).unwrap();
        let again: TraeCnMaterial = serde_json::from_slice(&bytes).unwrap();
        assert!(again.access_token.is_empty());
    }

    #[test]
    fn trae_with_optional_jwt_parses() {
        let json = r#"{
            "icube_auth_info_cipher":"CIPHER_y",
            "device_id":"dev-2",
            "access_token":"jwt_SECRET_abc",
            "refresh_token":"rt_SECRET_xyz"
        }"#;
        let m: TraeCnMaterial = serde_json::from_str(json).unwrap();
        assert_eq!(m.access_token, "jwt_SECRET_abc");
        assert_eq!(m.refresh_token.as_deref(), Some("rt_SECRET_xyz"));
        let dbg = format!("{m:?}");
        assert!(!dbg.contains("jwt_SECRET"));
        assert!(!dbg.contains("rt_SECRET"));
    }
}
