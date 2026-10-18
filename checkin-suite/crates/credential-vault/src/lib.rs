//! `credential-vault` — local credential store for WorkBuddy / Trae CN.
//!
//! # Boundary
//! - SQLite / logs / IPC responses never see tokens or cipher blobs.
//! - UI passes `secret_json` once via store; afterwards only `credential_ref`.
//! - `resolve` is internal (Rust / Adapter); not a frontend IPC.
//!
//! # Ref format (v0.2.2)
//! - Account: `cred://v1/account/{platform}/{account_key}/{uuid}`
//! - Switch backup: `cred://v1/switch_backup/{product}/{snapshot_id}`
//!
//! # Storage (blob >2560 patch)
//! - Backend holds `wbtcred_ptr_v1` pointers (inline envelope or file+key).
//! - Trae / oversized → `{vault_dir}/blobs/*.cve` (XChaCha20-Poly1305, magic `WBTBLOB1`).
//! - Switcher backups → [`CredentialStore::put_switch_backup`] /
//!   [`CredentialStore::get_switch_backup_seal`] / [`CredentialStore::delete_switch_backup`]
//!   → `{vault_dir}/switch_blobs/{snapshot_id}.cve` (always file pointer; P2-exempt delete).
//! - Use [`CredentialStore::with_vault_dir`] for file-mode; [`CredentialStore::new`] is
//!   Memory/inline-only (Trae / oversized / switch backup will error without vault_dir).
//!
//! # Backend
//! - [`MemoryBackend`] for tests / dry-run (always available).
//! - Windows: [`OsKeychainBackend`] → Credential Manager (CredWrite/CredRead/CredDelete).
//! - macOS Keychain / Linux secret-service: still stub (Unavailable); out of this sprint.

#![deny(unsafe_code)]
// Windows CredMan FFI lives in backend/windows.rs under #![allow(unsafe_code)].

pub mod backup;
pub mod backend;
pub mod blob_file;
pub mod envelope;
pub mod error;
pub mod handle;
pub mod pointer;
pub mod ref_id;
pub mod secret;
pub mod store;
pub mod types;

pub use backend::{CredentialBackend, MemoryBackend, OsKeychainBackend};
pub use error::{
    BackupError, BackendError, DeleteError, KindMismatch, ResolveError, StoreError,
};
pub use handle::CredentialHandle;
pub use pointer::{BACKEND_BLOB_SOFT_MAX, INLINE_MAX, POINTER_FORMAT};
pub use ref_id::{CredentialRef, SERVICE};
pub use secret::{MaterialBytes, SecretString};
pub use store::{CredentialStore, DeleteArgs, DeleteOk, StoreArgs, StoreOk};
pub use types::{
    CredentialMeta, MaterialKind, PlatformId, TraeCnBlobView, TraeCnMaterial,
    WorkBuddyMaterial, WorkBuddyTokensView,
};
pub use envelope::{CredentialEnvelope, SCHEMA_VERSION};
