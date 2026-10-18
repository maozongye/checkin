//! Production credential vault state for Tauri IPC.
//!
//! - Windows: `OsKeychainBackend` + `with_vault_dir`
//! - Non-Windows (Linux box / mac dry-run): `MemoryBackend` + `with_vault_dir`
//! - Never expose `resolve` over IPC.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use credential_vault::{
    CredentialHandle, CredentialRef, CredentialStore, DeleteArgs, DeleteError, DeleteOk,
    MaterialKind, MemoryBackend, OsKeychainBackend, PlatformId, ResolveError, StoreArgs, StoreError,
    StoreOk,
};
use serde::Serialize;

/// Runtime backend: OS keychain on Windows, in-memory elsewhere.
#[allow(dead_code)] // Os variant used on Windows; Mem on Linux box
pub enum VaultBackend {
    Os(CredentialStore<OsKeychainBackend>),
    Mem(CredentialStore<MemoryBackend>),
}

impl VaultBackend {
    #[allow(dead_code)]
    pub fn vault_dir(&self) -> Option<&Path> {
        match self {
            VaultBackend::Os(s) => s.vault_dir(),
            VaultBackend::Mem(s) => s.vault_dir(),
        }
    }

    #[allow(dead_code)]
    pub fn default_backup_dir(&self) -> PathBuf {
        self.vault_dir()
            .map(|d| d.join("backups"))
            .unwrap_or_else(|| PathBuf::from("backups"))
    }

    pub fn store(&self, args: StoreArgs) -> Result<StoreOk, StoreError> {
        match self {
            VaultBackend::Os(s) => s.store(args),
            VaultBackend::Mem(s) => s.store(args),
        }
    }

    pub fn delete(&self, args: DeleteArgs) -> Result<DeleteOk, DeleteError> {
        match self {
            VaultBackend::Os(s) => s.delete(args),
            VaultBackend::Mem(s) => s.delete(args),
        }
    }

    /// Switcher large-blob API — always file pointer under `{vault_dir}/switch_blobs/`.
    /// Never use `put_raw` for switch snapshots.
    pub fn put_switch_backup(
        &self,
        product: &str,
        snapshot_id: &str,
        plaintext: &[u8],
    ) -> Result<CredentialRef, StoreError> {
        match self {
            VaultBackend::Os(s) => s.put_switch_backup(product, snapshot_id, plaintext),
            VaultBackend::Mem(s) => s.put_switch_backup(product, snapshot_id, plaintext),
        }
    }

    pub fn get_switch_backup_seal(
        &self,
        credential_ref: &CredentialRef,
    ) -> Result<Vec<u8>, ResolveError> {
        match self {
            VaultBackend::Os(s) => s.get_switch_backup_seal(credential_ref),
            VaultBackend::Mem(s) => s.get_switch_backup_seal(credential_ref),
        }
    }

    pub fn delete_switch_backup(
        &self,
        credential_ref: &CredentialRef,
    ) -> Result<(), DeleteError> {
        match self {
            VaultBackend::Os(s) => s.delete_switch_backup(credential_ref),
            VaultBackend::Mem(s) => s.delete_switch_backup(credential_ref),
        }
    }

    /// Internal resolve — **not** registered as IPC. Account materials only.
    pub fn resolve_str(&self, credential_ref: &str) -> Result<CredentialHandle, ResolveError> {
        match self {
            VaultBackend::Os(s) => s.resolve_str(credential_ref),
            VaultBackend::Mem(s) => s.resolve_str(credential_ref),
        }
    }
}

/// Managed Tauri state. Holds vault_dir for default backup path.
pub struct VaultState {
    pub backend: Arc<VaultBackend>,
    pub vault_dir: PathBuf,
}

impl VaultState {
    pub fn new(backend: VaultBackend, vault_dir: PathBuf) -> Self {
        Self {
            backend: Arc::new(backend),
            vault_dir,
        }
    }
}

/// Build production store: always `with_vault_dir`.
/// Windows → CredMan; else Memory (Linux box testable).
pub fn build_vault(vault_dir: PathBuf) -> VaultBackend {
    #[cfg(windows)]
    {
        VaultBackend::Os(CredentialStore::with_vault_dir(
            OsKeychainBackend::new(),
            vault_dir,
        ))
    }
    #[cfg(not(windows))]
    {
        VaultBackend::Mem(CredentialStore::with_vault_dir(
            MemoryBackend::new(),
            vault_dir,
        ))
    }
}

/// Resolve app data dir. Prefer Tauri `app.path().app_data_dir()`;
/// fallback: `dirs::data_dir()/buddy-gasstation` (document if used).
pub fn resolve_app_data_dir(app: &tauri::App) -> Result<PathBuf, String> {
    use tauri::Manager;
    match app.path().app_data_dir() {
        Ok(p) => Ok(p),
        Err(e) => {
            // Production prefers app.path(); dirs is emergency fallback for
            // environments where PathResolver fails (e.g. missing identifier).
            let base = dirs::data_dir().ok_or_else(|| {
                format!("app.path().app_data_dir failed ({e}); dirs::data_dir also unavailable")
            })?;
            Ok(base.join("buddy-gasstation"))
        }
    }
}

#[derive(Debug, Serialize)]
pub struct IpcErr {
    pub code: String,
    pub message: String,
}

impl IpcErr {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            // message must never echo secrets / tokens / cipher blobs
            message: message.into(),
        }
    }
}

pub fn parse_platform(s: &str) -> Result<PlatformId, IpcErr> {
    PlatformId::parse(s).ok_or_else(|| {
        IpcErr::new(
            "invalid_secret",
            format!("unknown platform (expected workbuddy|trae_cn): {s}"),
        )
    })
}

pub fn parse_kind(s: &str) -> Result<MaterialKind, IpcErr> {
    match s {
        "workbuddy_tokens" => Ok(MaterialKind::WorkBuddyTokens),
        "trae_cn_icube_blob" => Ok(MaterialKind::TraeCnICubeBlob),
        _ => Err(IpcErr::new(
            "invalid_secret",
            format!("unknown kind (expected workbuddy_tokens|trae_cn_icube_blob): {s}"),
        )),
    }
}

pub fn map_store_err(e: StoreError) -> IpcErr {
    // StoreError Display may mention sizes/paths but not secrets; still prefer code()+safe msg.
    let code = e.code().to_string();
    let message: String = match &e {
        StoreError::InvalidSecret(_) => "invalid secret payload".into(),
        StoreError::KindPlatformMismatch => "kind does not match platform".into(),
        StoreError::ReplaceNotFound => "replace_ref not found".into(),
        StoreError::InvalidReplaceRef(_) => "invalid replace_ref".into(),
        StoreError::VaultDirRequired(_) => "vault_dir required for file-mode storage".into(),
        StoreError::PayloadTooLarge { size, limit } => {
            format!("payload too large: {size} bytes (limit {limit})")
        }
        StoreError::BackendUnavailable => "backend unavailable".into(),
        StoreError::BackendDenied => "backend denied".into(),
        StoreError::Internal(_) => "internal store error".into(),
    };
    IpcErr::new(code, message)
}

pub fn map_delete_err(e: DeleteError) -> IpcErr {
    let code = e.code().to_string();
    let message: String = match &e {
        DeleteError::InvalidRef(_) => "invalid credential_ref".into(),
        DeleteError::NotFound => "not found".into(),
        DeleteError::BackupRequired => {
            "backup required: provide backup_password before delete".into()
        }
        DeleteError::BackupFailed(_) => "backup failed".into(),
        DeleteError::BackendUnavailable => "backend unavailable".into(),
        DeleteError::BackendDenied => "backend denied".into(),
        DeleteError::Internal(_) => "internal delete error".into(),
    };
    IpcErr::new(code, message)
}
