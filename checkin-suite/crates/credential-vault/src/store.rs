//! CredentialStore — store / delete / resolve / backup / switch backup.
//!
//! Backend values are `wbtcred_ptr_v1` pointers (inline or file).
//! Oversized / Trae payloads encrypt to `{vault_dir}/blobs/*.cve`.
//! Switcher large backups → `{vault_dir}/switch_blobs/{snapshot_id}.cve`.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use uuid::Uuid;

use crate::backend::CredentialBackend;
use crate::backup::{read_backup_file, write_backup_file};
use crate::blob_file::{
    decrypt_blob, delete_blob_file, generate_file_key, read_blob_file, sanitize_snapshot_id,
    write_blob_file, write_switch_blob_file,
};
use crate::envelope::{CredentialEnvelope, SCHEMA_VERSION};
use crate::error::{BackupError, DeleteError, ResolveError, StoreError};
use crate::handle::CredentialHandle;
use crate::pointer::{
    decode_backend_value, DecodedBackendValue, PointerBody, StoragePointer,
    BACKEND_BLOB_SOFT_MAX, INLINE_MAX,
};
use crate::ref_id::{parse_ref_delete, CredentialRef};
use crate::secret::{MaterialBytes, SecretString};
use crate::types::{parse_material, CredentialMeta, MaterialKind, PlatformId};

/// Arguments for `credential_store` IPC.
#[derive(Clone, Debug)]
pub struct StoreArgs {
    pub platform: PlatformId,
    pub account_key: String,
    pub kind: MaterialKind,
    /// One-shot secret JSON; caller must discard after success.
    pub secret_json: String,
    pub source_hint: Option<String>,
    /// If set, overwrite same account ref (must be Account matching platform/key).
    pub replace_ref: Option<String>,
}

/// Success payload for store.
#[derive(Clone, Debug)]
pub struct StoreOk {
    pub credential_ref: CredentialRef,
    pub kind: MaterialKind,
    pub schema_version: u32,
}

impl StoreOk {
    pub fn credential_ref_str(&self) -> String {
        self.credential_ref.to_ref_string()
    }
}

/// Arguments for `credential_delete` (P2: force encrypted backup).
#[derive(Clone, Debug)]
pub struct DeleteArgs {
    pub credential_ref: String,
    /// Required: password for `.cred.bak` (P2 / P6).
    pub backup_password: Option<String>,
    /// Required: directory for backups (typically app `backups/`).
    pub backup_dir: Option<PathBuf>,
}

#[derive(Clone, Debug)]
pub struct DeleteOk {
    pub deleted: bool,
    pub backup_path: PathBuf,
}

/// High-level vault API over a CredentialBackend.
pub struct CredentialStore<B: CredentialBackend> {
    backend: B,
    /// App data dir for overflow `.cve` blobs. Required for Trae / oversized.
    vault_dir: Option<PathBuf>,
}

impl<B: CredentialBackend> CredentialStore<B> {
    /// Memory tests / dry-run without file-mode support.
    pub fn new(backend: B) -> Self {
        Self {
            backend,
            vault_dir: None,
        }
    }

    /// Production / file-mode capable store.
    pub fn with_vault_dir(backend: B, vault_dir: PathBuf) -> Self {
        Self {
            backend,
            vault_dir: Some(vault_dir),
        }
    }

    pub fn backend(&self) -> &B {
        &self.backend
    }

    pub fn vault_dir(&self) -> Option<&Path> {
        self.vault_dir.as_deref()
    }

    /// Import secret → Backend pointer → `cred://v1/account/...` ref.
    pub fn store(&self, args: StoreArgs) -> Result<StoreOk, StoreError> {
        if !args.kind.matches_platform(args.platform) {
            return Err(StoreError::KindPlatformMismatch);
        }
        if args.account_key.is_empty() || args.account_key.contains('/') {
            return Err(StoreError::InvalidSecret(
                "account_key must be non-empty and must not contain '/'".into(),
            ));
        }

        let (material_bytes, _) = parse_material(args.kind, &args.secret_json)?;
        let _discard = SecretString::new(args.secret_json);

        let id = if let Some(ref replace) = args.replace_ref {
            let existing =
                CredentialRef::parse(replace).map_err(StoreError::InvalidReplaceRef)?;
            match &existing {
                CredentialRef::Account {
                    platform,
                    account_key,
                    id,
                } => {
                    if *platform != args.platform || account_key != &args.account_key {
                        return Err(StoreError::InvalidReplaceRef(
                            "replace_ref platform/account_key mismatch".into(),
                        ));
                    }
                    let backend_key = existing.to_backend_key();
                    match self.backend.get(&backend_key) {
                        Ok(_) => *id,
                        Err(crate::error::BackendError::NotFound) => {
                            return Err(StoreError::ReplaceNotFound);
                        }
                        Err(e) => return Err(e.into()),
                    }
                }
                CredentialRef::SwitchBackup { .. } => {
                    return Err(StoreError::InvalidReplaceRef(
                        "replace_ref must be an account ref, not switch_backup".into(),
                    ));
                }
            }
        } else {
            Uuid::new_v4()
        };

        // Capture old file path for cleanup after successful replace.
        let old_file_rel = if let Some(ref replace) = args.replace_ref {
            self.peek_file_rel(replace).ok().flatten()
        } else {
            None
        };

        let imported_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);

        let meta = CredentialMeta {
            platform: args.platform,
            account_key: args.account_key.clone(),
            imported_at,
            source_hint: args.source_hint,
        };

        let envelope =
            CredentialEnvelope::new(args.kind, MaterialBytes::new(material_bytes), meta);
        let seal = envelope
            .seal_to_bytes()
            .map_err(StoreError::Internal)?;

        let cred_ref = CredentialRef::account(args.platform, args.account_key, id);
        self.write_pointer(&cred_ref, args.kind, &seal)?;

        // Best-effort: remove previous overflow file if path changed.
        if let (Some(vault), Some(old_rel)) = (self.vault_dir.as_ref(), old_file_rel) {
            let _ = delete_blob_file(vault, &old_rel);
        }

        Ok(StoreOk {
            credential_ref: cred_ref,
            kind: args.kind,
            schema_version: SCHEMA_VERSION,
        })
    }

    /// Delete after mandatory encrypted backup (P2).
    /// Backup contains full envelope seal (not the pointer).
    pub fn delete(&self, args: DeleteArgs) -> Result<DeleteOk, DeleteError> {
        let password = match args.backup_password {
            Some(p) if !p.is_empty() => SecretString::new(p),
            _ => return Err(DeleteError::BackupRequired),
        };
        let backup_dir = match args.backup_dir {
            Some(d) => d,
            None => return Err(DeleteError::BackupRequired),
        };

        let cred_ref = parse_ref_delete(&args.credential_ref)?;
        let key = cred_ref.to_backend_key();
        let backend_value = self.backend.get(&key)?;

        let (envelope_seal, file_rel) = self
            .materialize_envelope_seal(&backend_value)
            .map_err(|e| DeleteError::Internal(e.to_string()))?;

        let backup_path = write_backup_file(
            &backup_dir,
            &cred_ref.uuid_for_backup_filename(),
            &envelope_seal,
            &password,
        )
        .map_err(|e| DeleteError::BackupFailed(e.to_string()))?;

        self.backend.delete(&key)?;

        // Best-effort delete .cve (do not fail the delete if file cleanup fails).
        if let (Some(vault), Some(rel)) = (self.vault_dir.as_ref(), file_rel) {
            let _ = delete_blob_file(vault, &rel);
        }

        Ok(DeleteOk {
            deleted: true,
            backup_path,
        })
    }

    /// Internal resolve — not exposed to frontend IPC.
    pub fn resolve(&self, credential_ref: &CredentialRef) -> Result<CredentialHandle, ResolveError> {
        let bytes = self.backend.get(&credential_ref.to_backend_key())?;
        let (seal, _) = self.materialize_envelope_seal(&bytes)?;
        let envelope = CredentialEnvelope::unseal(&seal)?;
        CredentialHandle::from_envelope(envelope)
    }

    pub fn resolve_str(&self, credential_ref: &str) -> Result<CredentialHandle, ResolveError> {
        let r = CredentialRef::parse(credential_ref).map_err(ResolveError::InvalidRef)?;
        self.resolve(&r)
    }

    /// Export encrypted backup without deleting (full envelope seal).
    pub fn export_encrypted_backup(
        &self,
        credential_ref: &CredentialRef,
        backup_dir: &Path,
        password: &SecretString,
    ) -> Result<PathBuf, BackupError> {
        let backend_value = self
            .backend
            .get(&credential_ref.to_backend_key())
            .map_err(BackupError::Backend)?;
        let (seal, _) = self
            .materialize_envelope_seal(&backend_value)
            .map_err(|e| BackupError::Resolve(e.to_string()))?;
        write_backup_file(
            backup_dir,
            &credential_ref.uuid_for_backup_filename(),
            &seal,
            password,
        )
    }

    /// Restore from `.cred.bak` into Backend as a **new** account ref
    /// (or replace if `replace_ref` given). Uses envelope meta for platform/account_key.
    pub fn restore_backup(
        &self,
        path: &Path,
        password: &SecretString,
        replace_ref: Option<&CredentialRef>,
    ) -> Result<CredentialRef, BackupError> {
        let plaintext = read_backup_file(path, password)?;
        let envelope = CredentialEnvelope::unseal(&plaintext)
            .map_err(|e| BackupError::Resolve(e.to_string()))?;

        let platform = envelope.meta.platform;
        let account_key = envelope.meta.account_key.clone();
        let kind = envelope.kind;

        let old_file_rel = if let Some(r) = replace_ref {
            match self.backend.get(&r.to_backend_key()) {
                Ok(v) => self
                    .materialize_envelope_seal(&v)
                    .ok()
                    .and_then(|(_, rel)| rel),
                Err(_) => None,
            }
        } else {
            None
        };

        let id = if let Some(r) = replace_ref {
            match r {
                CredentialRef::Account {
                    platform: p,
                    account_key: k,
                    id,
                } if *p == platform && *k == account_key => *id,
                CredentialRef::Account { .. } => {
                    return Err(BackupError::Store(
                        "replace_ref platform/account_key mismatch".into(),
                    ));
                }
                CredentialRef::SwitchBackup { .. } => {
                    return Err(BackupError::Store(
                        "replace_ref must be account ref".into(),
                    ));
                }
            }
        } else {
            Uuid::new_v4()
        };

        let seal = envelope
            .seal_to_bytes()
            .map_err(|e| BackupError::Store(e))?;
        let cred_ref = CredentialRef::account(platform, account_key, id);
        self.write_pointer(&cred_ref, kind, &seal)
            .map_err(|e| BackupError::Store(e.to_string()))?;

        if let (Some(vault), Some(old_rel)) = (self.vault_dir.as_ref(), old_file_rel) {
            let _ = delete_blob_file(vault, &old_rel);
        }

        Ok(cred_ref)
    }

    /// Low-level: write raw sealed bytes under a SwitchBackup ref (legacy / small).
    /// Does **not** wrap in pointer. `sealed.len() > BACKEND_BLOB_SOFT_MAX(2400)` →
    /// [`StoreError::PayloadTooLarge`] (never push bare large blobs into CredMan).
    /// Prefer [`Self::put_switch_backup`] for Switcher snapshots.
    pub fn put_raw(&self, credential_ref: &CredentialRef, sealed: &[u8]) -> Result<(), StoreError> {
        if sealed.len() > BACKEND_BLOB_SOFT_MAX {
            return Err(StoreError::PayloadTooLarge {
                size: sealed.len(),
                limit: BACKEND_BLOB_SOFT_MAX,
            });
        }
        self.backend
            .set(&credential_ref.to_backend_key(), sealed)
            .map_err(Into::into)
    }

    /// Switcher large-blob API: always file-mode pointer under `switch_blobs/`.
    ///
    /// - Ref: `cred://v1/switch_backup/{product}/{snapshot_id}`
    /// - Disk: `{vault_dir}/switch_blobs/{snapshot_id}.cve` (WBTBLOB1)
    /// - Plaintext is the official login-state backup bytes (no CredentialEnvelope wrap)
    /// - Requires [`Self::with_vault_dir`]; else [`StoreError::VaultDirRequired`]
    /// - Same ref re-put overwrites Backend pointer; best-effort deletes prior `.cve`
    ///   when the relative path changed
    pub fn put_switch_backup(
        &self,
        product: &str,
        snapshot_id: &str,
        plaintext: &[u8],
    ) -> Result<CredentialRef, StoreError> {
        if product.is_empty() {
            return Err(StoreError::Internal("product empty".into()));
        }
        sanitize_snapshot_id(snapshot_id).map_err(StoreError::Internal)?;

        let vault = self.vault_dir.as_ref().ok_or_else(|| {
            StoreError::VaultDirRequired(
                "put_switch_backup needs with_vault_dir (switch_blobs/)".into(),
            )
        })?;

        let cred_ref = CredentialRef::switch_backup(product, snapshot_id);
        let backend_key = cred_ref.to_backend_key();

        // Capture old file rel before overwrite (same snapshot_id → same path usually).
        let old_file_rel = match self.backend.get(&backend_key) {
            Ok(v) => self
                .materialize_envelope_seal(&v)
                .ok()
                .and_then(|(_, rel)| rel),
            Err(crate::error::BackendError::NotFound) => None,
            Err(e) => return Err(e.into()),
        };

        let key = generate_file_key();
        let (rel_path, _) = write_switch_blob_file(vault, snapshot_id, plaintext, &key)
            .map_err(StoreError::Internal)?;
        let ptr = StoragePointer::file(rel_path.clone(), &key);
        let bytes = ptr.to_bytes()?;
        self.backend.set(&backend_key, &bytes)?;

        if let Some(old_rel) = old_file_rel {
            if old_rel != rel_path {
                let _ = delete_blob_file(vault, &old_rel);
            }
        }

        Ok(cred_ref)
    }

    /// Read Switcher backup plaintext (decrypted `.cve`, or legacy bare `put_raw` bytes).
    ///
    /// Only accepts [`CredentialRef::SwitchBackup`]. Name keeps the draft contract;
    /// semantics = decrypted backup plaintext (not a CredentialEnvelope).
    pub fn get_switch_backup_seal(
        &self,
        credential_ref: &CredentialRef,
    ) -> Result<Vec<u8>, ResolveError> {
        match credential_ref {
            CredentialRef::SwitchBackup { .. } => {}
            CredentialRef::Account { .. } => {
                return Err(ResolveError::InvalidRef(
                    "get_switch_backup_seal requires switch_backup ref".into(),
                ));
            }
        }
        let backend_value = self.backend.get(&credential_ref.to_backend_key())?;
        let (plaintext, _) = self.materialize_envelope_seal(&backend_value)?;
        Ok(plaintext)
    }

    /// Delete Switcher backup: Backend entry + best-effort `.cve`.
    ///
    /// **Exempt from P2** — no `backup_password` required (temporary switch snapshot).
    /// Non-SwitchBackup ref → [`DeleteError::InvalidRef`].
    pub fn delete_switch_backup(
        &self,
        credential_ref: &CredentialRef,
    ) -> Result<(), DeleteError> {
        match credential_ref {
            CredentialRef::SwitchBackup { .. } => {}
            CredentialRef::Account { .. } => {
                return Err(DeleteError::InvalidRef(
                    "delete_switch_backup requires switch_backup ref".into(),
                ));
            }
        }
        let key = credential_ref.to_backend_key();
        let backend_value = self.backend.get(&key)?;
        let file_rel = self
            .materialize_envelope_seal(&backend_value)
            .ok()
            .and_then(|(_, rel)| rel);

        self.backend.delete(&key)?;

        if let (Some(vault), Some(rel)) = (self.vault_dir.as_ref(), file_rel) {
            let _ = delete_blob_file(vault, &rel);
        }
        Ok(())
    }

    // ─── hybrid pointer helpers ───────────────────────────────────────────

    fn needs_file_mode(kind: MaterialKind, seal_len: usize) -> bool {
        kind == MaterialKind::TraeCnICubeBlob || seal_len > INLINE_MAX
    }

    fn write_pointer(
        &self,
        cred_ref: &CredentialRef,
        kind: MaterialKind,
        seal: &[u8],
    ) -> Result<(), StoreError> {
        let use_file = Self::needs_file_mode(kind, seal.len());
        let ptr = if use_file {
            self.make_file_pointer(seal)?
        } else {
            let inline = StoragePointer::inline(seal);
            let bytes = inline.to_bytes()?;
            // CredMan hard limit 2560 — escalate if encoded pointer is still too large.
            if bytes.len() > BACKEND_BLOB_SOFT_MAX {
                self.make_file_pointer(seal)?
            } else {
                self.backend
                    .set(&cred_ref.to_backend_key(), &bytes)?;
                return Ok(());
            }
        };

        let bytes = ptr.to_bytes()?;
        self.backend
            .set(&cred_ref.to_backend_key(), &bytes)
            .map_err(Into::into)
    }

    fn make_file_pointer(&self, seal: &[u8]) -> Result<StoragePointer, StoreError> {
        let vault = self.vault_dir.as_ref().ok_or_else(|| {
            StoreError::VaultDirRequired(
                "oversized or Trae payload needs with_vault_dir(app_data)".into(),
            )
        })?;
        let key = generate_file_key();
        let (rel_path, _) =
            write_blob_file(vault, seal, &key).map_err(StoreError::Internal)?;
        Ok(StoragePointer::file(rel_path, &key))
    }

    /// Resolve Backend blob → (envelope seal bytes, optional file rel_path).
    fn materialize_envelope_seal(
        &self,
        backend_value: &[u8],
    ) -> Result<(Vec<u8>, Option<String>), ResolveError> {
        match decode_backend_value(backend_value)? {
            DecodedBackendValue::LegacyEnvelope(seal) => Ok((seal, None)),
            DecodedBackendValue::Pointer(ptr) => match &ptr.body {
                PointerBody::Inline { .. } => {
                    let seal = ptr.decode_inline_envelope()?;
                    Ok((seal, None))
                }
                PointerBody::File { rel_path, .. } => {
                    let vault = self.vault_dir.as_ref().ok_or_else(|| {
                        ResolveError::VaultDirRequired(
                            "file-mode credential needs with_vault_dir(app_data)".into(),
                        )
                    })?;
                    let (rel, key) = ptr.file_parts()?;
                    let file_bytes = read_blob_file(vault, rel)
                        .map_err(|e| ResolveError::CorruptEnvelope(e))?;
                    let seal = decrypt_blob(&file_bytes, &key)
                        .map_err(|e| ResolveError::CorruptEnvelope(e))?;
                    Ok((seal, Some(rel_path.clone())))
                }
            },
        }
    }

    fn peek_file_rel(&self, ref_str: &str) -> Result<Option<String>, ResolveError> {
        let cred_ref = CredentialRef::parse(ref_str).map_err(ResolveError::InvalidRef)?;
        let bytes = self.backend.get(&cred_ref.to_backend_key())?;
        let (_, rel) = self.materialize_envelope_seal(&bytes)?;
        Ok(rel)
    }
}

/// Re-export for callers that need the threshold.
pub use crate::pointer::INLINE_MAX as STORE_INLINE_MAX;
