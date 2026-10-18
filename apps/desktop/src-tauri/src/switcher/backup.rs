//! Switch backup store — **真 API**：`credential_vault::CredentialStore::{put,get,delete}_switch_backup*`。
//!
//! - URI：`cred://v1/switch_backup/{product}/{snapshot_id}`
//! - 磁盘：`{vault_dir}/switch_blobs/{snapshot_id}.cve`（由 vault 写入；强制 file 指针）
//! - **禁止**对大 snapshot 调用 `put_raw`（CredMan 上限）
//! - `login_snapshot.credential_ref` 只存 URI 字符串

use std::sync::Arc;

use credential_vault::{CredentialRef, DeleteError, ResolveError, StoreError};

use crate::vault::VaultBackend;

pub trait SwitchBackupStore: Send + Sync {
    fn put_switch_backup(
        &self,
        product: &str,
        snapshot_id: &str,
        plaintext: &[u8],
    ) -> Result<String, String>;

    fn get_switch_backup(&self, cred_ref: &str) -> Result<Vec<u8>, String>;

    fn delete_switch_backup(&self, cred_ref: &str) -> Result<(), String>;
}

/// 包装桌面端已有的 [`VaultBackend`]（必须 `with_vault_dir`）。
pub struct VaultSwitchBackupStore {
    backend: Arc<VaultBackend>,
}

impl VaultSwitchBackupStore {
    pub fn new(backend: Arc<VaultBackend>) -> Self {
        Self { backend }
    }
}

impl SwitchBackupStore for VaultSwitchBackupStore {
    fn put_switch_backup(
        &self,
        product: &str,
        snapshot_id: &str,
        plaintext: &[u8],
    ) -> Result<String, String> {
        let r = self
            .backend
            .put_switch_backup(product, snapshot_id, plaintext)
            .map_err(map_store_err)?;
        Ok(r.to_ref_string())
    }

    fn get_switch_backup(&self, cred_ref: &str) -> Result<Vec<u8>, String> {
        let r = CredentialRef::parse(cred_ref).map_err(|e| format!("invalid cred_ref: {e}"))?;
        self.backend
            .get_switch_backup_seal(&r)
            .map_err(map_resolve_err)
    }

    fn delete_switch_backup(&self, cred_ref: &str) -> Result<(), String> {
        let r = CredentialRef::parse(cred_ref).map_err(|e| format!("invalid cred_ref: {e}"))?;
        self.backend
            .delete_switch_backup(&r)
            .map_err(map_delete_err)
    }
}

fn map_store_err(e: StoreError) -> String {
    // 不回传可能含路径细节以外的敏感内容；用 code + 安全短句
    match &e {
        StoreError::VaultDirRequired(_) => "vault_dir_required".into(),
        StoreError::PayloadTooLarge { .. } => "payload_too_large".into(),
        StoreError::BackendUnavailable => "backend_unavailable".into(),
        StoreError::BackendDenied => "backend_denied".into(),
        StoreError::InvalidSecret(_) => "invalid_secret".into(),
        StoreError::KindPlatformMismatch => "kind_platform_mismatch".into(),
        StoreError::ReplaceNotFound => "replace_not_found".into(),
        StoreError::InvalidReplaceRef(_) => "invalid_replace_ref".into(),
        StoreError::Internal(_) => "store_internal".into(),
    }
}

fn map_resolve_err(e: ResolveError) -> String {
    match &e {
        ResolveError::InvalidRef(_) => "invalid_ref".into(),
        ResolveError::NotFound => "not_found".into(),
        ResolveError::CorruptEnvelope(_) => "corrupt_envelope".into(),
        ResolveError::VaultDirRequired(_) => "vault_dir_required".into(),
        ResolveError::BackendUnavailable => "backend_unavailable".into(),
        ResolveError::BackendDenied => "backend_denied".into(),
        ResolveError::Internal(_) => "resolve_internal".into(),
    }
}

fn map_delete_err(e: DeleteError) -> String {
    match &e {
        DeleteError::InvalidRef(_) => "invalid_ref".into(),
        DeleteError::NotFound => "not_found".into(),
        DeleteError::BackupRequired => "backup_required".into(),
        DeleteError::BackupFailed(_) => "backup_failed".into(),
        DeleteError::BackendUnavailable => "backend_unavailable".into(),
        DeleteError::BackendDenied => "backend_denied".into(),
        DeleteError::Internal(_) => "delete_internal".into(),
    }
}
