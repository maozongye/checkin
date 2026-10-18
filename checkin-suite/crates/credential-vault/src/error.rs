//! Error types for store / delete / resolve / backend.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum BackendError {
    #[error("backend unavailable")]
    Unavailable,
    #[error("backend denied")]
    Denied,
    #[error("not found")]
    NotFound,
    #[error("internal backend error: {0}")]
    Internal(String),
}

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("invalid secret: {0}")]
    InvalidSecret(String),
    #[error("kind does not match platform")]
    KindPlatformMismatch,
    #[error("replace_ref not found")]
    ReplaceNotFound,
    #[error("invalid replace_ref: {0}")]
    InvalidReplaceRef(String),
    /// File-mode required (Trae / oversized) but CredentialStore has no vault_dir.
    #[error("vault_dir required: {0}")]
    VaultDirRequired(String),
    /// Payload cannot fit CredMan even as pointer (should be rare; prefer file mode).
    #[error("payload too large: {size} bytes (limit {limit})")]
    PayloadTooLarge { size: usize, limit: usize },
    #[error("backend unavailable")]
    BackendUnavailable,
    #[error("backend denied")]
    BackendDenied,
    #[error("internal: {0}")]
    Internal(String),
}

impl From<BackendError> for StoreError {
    fn from(e: BackendError) -> Self {
        match e {
            BackendError::Unavailable => StoreError::BackendUnavailable,
            BackendError::Denied => StoreError::BackendDenied,
            BackendError::NotFound => StoreError::ReplaceNotFound,
            BackendError::Internal(s) => StoreError::Internal(s),
        }
    }
}

impl StoreError {
    pub fn code(&self) -> &'static str {
        match self {
            StoreError::InvalidSecret(_) | StoreError::KindPlatformMismatch => "invalid_secret",
            StoreError::ReplaceNotFound => "replace_not_found",
            StoreError::InvalidReplaceRef(_) => "invalid_secret",
            StoreError::VaultDirRequired(_) => "vault_dir_required",
            StoreError::PayloadTooLarge { .. } => "payload_too_large",
            StoreError::BackendUnavailable => "backend_unavailable",
            StoreError::BackendDenied => "backend_denied",
            StoreError::Internal(_) => "internal",
        }
    }
}

#[derive(Debug, Error)]
pub enum DeleteError {
    #[error("invalid credential_ref: {0}")]
    InvalidRef(String),
    #[error("not found")]
    NotFound,
    #[error("backup required: provide backup_password and backup_dir before delete")]
    BackupRequired,
    #[error("backup failed: {0}")]
    BackupFailed(String),
    #[error("backend unavailable")]
    BackendUnavailable,
    #[error("backend denied")]
    BackendDenied,
    #[error("internal: {0}")]
    Internal(String),
}

impl From<BackendError> for DeleteError {
    fn from(e: BackendError) -> Self {
        match e {
            BackendError::Unavailable => DeleteError::BackendUnavailable,
            BackendError::Denied => DeleteError::BackendDenied,
            BackendError::NotFound => DeleteError::NotFound,
            BackendError::Internal(s) => DeleteError::Internal(s),
        }
    }
}

impl DeleteError {
    pub fn code(&self) -> &'static str {
        match self {
            DeleteError::InvalidRef(_) => "invalid_ref",
            DeleteError::NotFound => "not_found",
            DeleteError::BackupRequired => "backup_required",
            DeleteError::BackupFailed(_) => "internal",
            DeleteError::BackendUnavailable => "backend_unavailable",
            DeleteError::BackendDenied => "backend_denied",
            DeleteError::Internal(_) => "internal",
        }
    }
}

#[derive(Debug, Error)]
pub enum ResolveError {
    #[error("invalid credential_ref: {0}")]
    InvalidRef(String),
    #[error("not found")]
    NotFound,
    #[error("corrupt envelope: {0}")]
    CorruptEnvelope(String),
    #[error("vault_dir required: {0}")]
    VaultDirRequired(String),
    #[error("backend unavailable")]
    BackendUnavailable,
    #[error("backend denied")]
    BackendDenied,
    #[error("internal: {0}")]
    Internal(String),
}

impl From<BackendError> for ResolveError {
    fn from(e: BackendError) -> Self {
        match e {
            BackendError::Unavailable => ResolveError::BackendUnavailable,
            BackendError::Denied => ResolveError::BackendDenied,
            BackendError::NotFound => ResolveError::NotFound,
            BackendError::Internal(s) => ResolveError::Internal(s),
        }
    }
}

#[derive(Debug, Error)]
#[error("material kind mismatch")]
pub struct KindMismatch;

#[derive(Debug, Error)]
pub enum BackupError {
    #[error("io: {0}")]
    Io(String),
    #[error("crypto: {0}")]
    Crypto(String),
    #[error("invalid backup file: {0}")]
    InvalidFile(String),
    #[error("wrong password or corrupt ciphertext")]
    DecryptFailed,
    #[error("backend: {0}")]
    Backend(#[from] BackendError),
    #[error("resolve: {0}")]
    Resolve(String),
    #[error("store: {0}")]
    Store(String),
}

impl ResolveError {
    pub fn code(&self) -> &'static str {
        match self {
            ResolveError::InvalidRef(_) => "invalid_ref",
            ResolveError::NotFound => "not_found",
            ResolveError::CorruptEnvelope(_) => "corrupt_envelope",
            ResolveError::VaultDirRequired(_) => "vault_dir_required",
            ResolveError::BackendUnavailable => "backend_unavailable",
            ResolveError::BackendDenied => "backend_denied",
            ResolveError::Internal(_) => "internal",
        }
    }
}
