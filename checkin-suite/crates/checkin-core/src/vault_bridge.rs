//! Bridge checkin CredentialsRef → credential-vault CredentialStore.
//!
//! Hard rules (Architect):
//! - Use existing `CredentialStore` / `CredentialRef` only — no parallel vault trait.
//! - Checkin accepts **account** refs only; `switch_backup` is rejected.
//! - DesktopPaths are import sources, not a LiveHttp substitute.
//! - dry_run / probe must **not** resolve vault plaintext.
//! - Debug of [`ResolvedSession`] must redact secrets.

use credential_vault::{
    CredentialBackend, CredentialRef, CredentialStore, MaterialKind, PlatformId, ResolveError,
};
use thiserror::Error;
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::types::CredentialsRef;

/// Errors from vault resolve for checkin (safe to log — no token material).
#[derive(Debug, Error)]
pub enum VaultBridgeError {
    #[error("credentials_ref is not a cred:// URI (legacy/dry_run only)")]
    NotCredUri,
    #[error("invalid credential_ref: {0}")]
    InvalidRef(String),
    #[error("switch_backup ref cannot be used for checkin")]
    SwitchBackupRejected,
    #[error("credential not found in vault")]
    NotFound,
    #[error("auth invalid: {0}")]
    AuthInvalid(String),
    #[error("resolve failed: {0}")]
    Resolve(String),
    #[error("platform/material mismatch for checkin")]
    KindMismatch,
}

impl VaultBridgeError {
    pub fn to_adapter_hint(&self) -> &'static str {
        match self {
            Self::NotFound | Self::AuthInvalid(_) | Self::InvalidRef(_) | Self::NotCredUri => {
                "AuthInvalid"
            }
            Self::SwitchBackupRejected | Self::KindMismatch => "Failed",
            Self::Resolve(_) => "Failed",
        }
    }
}

impl From<ResolveError> for VaultBridgeError {
    fn from(e: ResolveError) -> Self {
        match e {
            ResolveError::NotFound => VaultBridgeError::NotFound,
            ResolveError::InvalidRef(s) => VaultBridgeError::InvalidRef(s),
            ResolveError::BackendUnavailable => {
                VaultBridgeError::Resolve("backend_unavailable".into())
            }
            ResolveError::BackendDenied => VaultBridgeError::Resolve("backend_denied".into()),
            ResolveError::CorruptEnvelope(s) => {
                VaultBridgeError::AuthInvalid(format!("corrupt_envelope:{s}"))
            }
            other => VaultBridgeError::Resolve(other.to_string()),
        }
    }
}

/// Internal resolved session for LiveHttp — **Debug redacts secrets**.
#[derive(Zeroize, ZeroizeOnDrop)]
pub enum ResolvedSession {
    WorkBuddy {
        access_token: String,
        refresh_token: String,
        #[zeroize(skip)]
        domain: Option<String>,
        #[zeroize(skip)]
        account_key: String,
        #[zeroize(skip)]
        expires_at: Option<i64>,
    },
    /// Trae CN session. `needs_desktop_session` is true when `access_token` is empty
    /// (cipher-only import); LiveHttp claim then fails with `trae_jwt_unavailable`.
    Trae {
        icube_auth_info_cipher: String,
        device_id: String,
        access_token: String,
        refresh_token: String,
        #[zeroize(skip)]
        account_key: String,
        #[zeroize(skip)]
        needs_desktop_session: bool,
    },
}

impl std::fmt::Debug for ResolvedSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WorkBuddy {
                domain,
                account_key,
                expires_at,
                ..
            } => f
                .debug_struct("ResolvedSession::WorkBuddy")
                .field("account_key", account_key)
                .field("domain", domain)
                .field("expires_at", expires_at)
                .field("access_token", &"[REDACTED]")
                .field("refresh_token", &"[REDACTED]")
                .finish(),
            Self::Trae {
                account_key,
                needs_desktop_session,
                device_id,
                ..
            } => f
                .debug_struct("ResolvedSession::Trae")
                .field("account_key", account_key)
                .field("device_id_len", &device_id.len())
                .field("needs_desktop_session", needs_desktop_session)
                .field("icube_auth_info_cipher", &"[REDACTED]")
                .field("access_token", &"[REDACTED]")
                .field("refresh_token", &"[REDACTED]")
                .finish(),
        }
    }
}

impl ResolvedSession {
    pub fn platform_slug(&self) -> &'static str {
        match self {
            Self::WorkBuddy { .. } => "workbuddy",
            Self::Trae { .. } => "trae",
        }
    }
}

/// Validate CredentialsRef shape without resolving secrets (dry_run / probe).
pub fn validate_ref_shape(cred: &CredentialsRef) -> Result<CredentialRef, VaultBridgeError> {
    let uri = cred.as_cred_uri().ok_or(VaultBridgeError::NotCredUri)?;
    let r = CredentialRef::parse(uri).map_err(VaultBridgeError::InvalidRef)?;
    match &r {
        CredentialRef::Account { .. } => Ok(r),
        CredentialRef::SwitchBackup { .. } => Err(VaultBridgeError::SwitchBackupRejected),
    }
}

/// Resolve vault material for LiveHttp checkin.
///
/// - Rejects non-`cred://` and `switch_backup` refs.
/// - Returns owned [`ResolvedSession`]; drop to zeroize.
/// - Caller must not log Debug if a custom formatter ever changes — current Debug is redacted.
pub fn resolve_for_checkin<B: CredentialBackend>(
    store: &CredentialStore<B>,
    cred: &CredentialsRef,
) -> Result<ResolvedSession, VaultBridgeError> {
    let vault_ref = validate_ref_shape(cred)?;
    let handle = store.resolve(&vault_ref)?;
    let meta = handle.meta().clone();
    let session = match handle.kind() {
        MaterialKind::WorkBuddyTokens => {
            let view = handle
                .as_workbuddy_tokens()
                .map_err(|_| VaultBridgeError::KindMismatch)?;
            ResolvedSession::WorkBuddy {
                access_token: view.access_token.to_string(),
                refresh_token: view.refresh_token.to_string(),
                domain: view.domain.map(|s| s.to_string()),
                account_key: meta.account_key.clone(),
                expires_at: view.expires_at,
            }
        }
        MaterialKind::TraeCnICubeBlob => {
            let view = handle
                .as_trae_cn_blob()
                .map_err(|_| VaultBridgeError::KindMismatch)?;
            let access_token = view.access_token.to_string();
            let needs_desktop_session = access_token.is_empty();
            ResolvedSession::Trae {
                icube_auth_info_cipher: view.icube_auth_info_cipher.to_string(),
                device_id: view.device_id.to_string(),
                access_token,
                refresh_token: view.refresh_token.unwrap_or("").to_string(),
                account_key: meta.account_key.clone(),
                needs_desktop_session,
            }
        }
    };
    drop(handle);

    match (&session, meta.platform) {
        (ResolvedSession::WorkBuddy { .. }, PlatformId::Workbuddy) => {}
        (ResolvedSession::Trae { .. }, PlatformId::TraeCn) => {}
        _ => return Err(VaultBridgeError::KindMismatch),
    }

    Ok(session)
}

#[cfg(test)]
mod tests {
    use super::*;
    use credential_vault::{MaterialKind, MemoryBackend, PlatformId, StoreArgs};

    fn memory_store_wb() -> (CredentialStore<MemoryBackend>, CredentialsRef) {
        let store = CredentialStore::new(MemoryBackend::new());
        let secret = r#"{
            "access_token": "tok_access_SECRET_abc",
            "refresh_token": "tok_refresh_SECRET_xyz",
            "domain": "copilot.tencent.com"
        }"#;
        let ok = store
            .store(StoreArgs {
                platform: PlatformId::Workbuddy,
                account_key: "wb-test".into(),
                kind: MaterialKind::WorkBuddyTokens,
                secret_json: secret.into(),
                source_hint: Some(".info".into()),
                replace_ref: None,
            })
            .expect("store");
        let cred = CredentialsRef::from(&ok.credential_ref);
        (store, cred)
    }

    #[test]
    fn resolve_wb_and_debug_redacts_token_substrings() {
        let (store, cred) = memory_store_wb();
        let session = resolve_for_checkin(&store, &cred).expect("resolve");
        let dbg = format!("{session:?}");
        assert!(dbg.contains("REDACTED"));
        assert!(!dbg.contains("tok_access_SECRET_abc"));
        assert!(!dbg.contains("tok_refresh_SECRET_xyz"));
        assert!(matches!(session, ResolvedSession::WorkBuddy { .. }));
        drop(session);
    }

    #[test]
    fn resolve_trae_cipher_only_needs_desktop_session() {
        let dir = tempfile::tempdir().unwrap();
        let store =
            CredentialStore::with_vault_dir(MemoryBackend::new(), dir.path().to_path_buf());
        let secret = r#"{
            "icube_auth_info_cipher": "CIPHER_SECRET_only",
            "device_id": "dev-cipher-only"
        }"#;
        let ok = store
            .store(StoreArgs {
                platform: PlatformId::TraeCn,
                account_key: "dev-cipher-only".into(),
                kind: MaterialKind::TraeCnICubeBlob,
                secret_json: secret.into(),
                source_hint: Some("storage.json".into()),
                replace_ref: None,
            })
            .expect("store");
        let cred = CredentialsRef::from(&ok.credential_ref);
        let session = resolve_for_checkin(&store, &cred).expect("resolve");
        match &session {
            ResolvedSession::Trae {
                needs_desktop_session,
                access_token,
                ..
            } => {
                assert!(*needs_desktop_session);
                assert!(access_token.is_empty());
            }
            _ => panic!("expected Trae"),
        }
        let dbg = format!("{session:?}");
        assert!(!dbg.contains("CIPHER_SECRET_only"));
        drop(session);
    }

    #[test]
    fn resolve_trae_with_jwt_ready_for_live() {
        let dir = tempfile::tempdir().unwrap();
        let store =
            CredentialStore::with_vault_dir(MemoryBackend::new(), dir.path().to_path_buf());
        let secret = r#"{
            "icube_auth_info_cipher": "CIPHER_keep",
            "device_id": "dev-jwt",
            "access_token": "jwt_SECRET_live",
            "refresh_token": "rt_SECRET_live"
        }"#;
        let ok = store
            .store(StoreArgs {
                platform: PlatformId::TraeCn,
                account_key: "dev-jwt".into(),
                kind: MaterialKind::TraeCnICubeBlob,
                secret_json: secret.into(),
                source_hint: None,
                replace_ref: None,
            })
            .expect("store");
        let cred = CredentialsRef::from(&ok.credential_ref);
        let session = resolve_for_checkin(&store, &cred).expect("resolve");
        match &session {
            ResolvedSession::Trae {
                needs_desktop_session,
                access_token,
                refresh_token,
                ..
            } => {
                assert!(!*needs_desktop_session);
                assert_eq!(access_token, "jwt_SECRET_live");
                assert_eq!(refresh_token, "rt_SECRET_live");
            }
            _ => panic!("expected Trae"),
        }
        assert!(!format!("{session:?}").contains("jwt_SECRET_live"));
        drop(session);
    }

    #[test]
    fn reject_switch_backup() {
        let store = CredentialStore::new(MemoryBackend::new());
        let cred = CredentialsRef::new("cred://v1/switch_backup/workbuddy/snap-1");
        let err = resolve_for_checkin(&store, &cred).unwrap_err();
        assert!(matches!(err, VaultBridgeError::SwitchBackupRejected));
    }

    #[test]
    fn reject_legacy_uri_without_resolve() {
        let store = CredentialStore::new(MemoryBackend::new());
        let cred = CredentialsRef::legacy("accounts_json", "x");
        let err = resolve_for_checkin(&store, &cred).unwrap_err();
        assert!(matches!(err, VaultBridgeError::NotCredUri));
    }

    #[test]
    fn validate_shape_ok_for_account() {
        let id = uuid::Uuid::parse_str("aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee").unwrap();
        let r = CredentialRef::account(PlatformId::Workbuddy, "u", id);
        let cred = CredentialsRef::from(&r);
        assert!(validate_ref_shape(&cred).is_ok());
    }
}
