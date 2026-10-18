//! Real credential IPC — `credential_store` / `credential_delete`.
//!
//! Align: `docs/credential-tauri-ipc-draft.md` + blob overflow patch (`with_vault_dir`).
//! **Do not** register `resolve` as a command.

use std::path::PathBuf;

use credential_vault::{DeleteArgs, StoreArgs};
use serde::{Deserialize, Serialize};
use tauri::{command, State};

use crate::vault::{
    map_delete_err, map_store_err, parse_kind, parse_platform, IpcErr, VaultState,
};

#[derive(Debug, Serialize)]
pub struct IpcStoreOk {
    pub credential_ref: String,
    pub kind: String,
    pub schema_version: u32,
}

#[derive(Debug, Serialize)]
pub struct IpcDeleteOk {
    pub deleted: bool,
    pub backup_path: String,
}

#[derive(Debug, Deserialize)]
pub struct IpcStoreArgs {
    pub platform: String,
    pub account_key: String,
    pub kind: String,
    /// One-shot secret JSON; discarded after store (UI must drop its copy).
    pub secret_json: String,
    pub source_hint: Option<String>,
    pub replace_ref: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct IpcDeleteArgs {
    pub credential_ref: String,
    /// P2: encrypted backup password required before delete.
    pub backup_password: String,
    /// Optional; empty/None → `{vault_dir}/backups`.
    pub backup_dir: Option<PathBuf>,
}

#[command]
pub fn credential_store(
    state: State<'_, VaultState>,
    args: IpcStoreArgs,
) -> Result<IpcStoreOk, IpcErr> {
    let platform = parse_platform(&args.platform)?;
    let kind = parse_kind(&args.kind)?;
    let store_args = StoreArgs {
        platform,
        account_key: args.account_key,
        kind,
        secret_json: args.secret_json,
        source_hint: args.source_hint,
        replace_ref: args.replace_ref,
    };
    let ok = state
        .backend
        .store(store_args)
        .map_err(map_store_err)?;
    Ok(IpcStoreOk {
        credential_ref: ok.credential_ref_str(),
        kind: ok.kind.as_str().to_string(),
        schema_version: ok.schema_version,
    })
}

#[command]
pub fn credential_delete(
    state: State<'_, VaultState>,
    args: IpcDeleteArgs,
) -> Result<IpcDeleteOk, IpcErr> {
    let backup_dir = args
        .backup_dir
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| state.vault_dir.join("backups"));

    let del = DeleteArgs {
        credential_ref: args.credential_ref,
        backup_password: Some(args.backup_password),
        backup_dir: Some(backup_dir),
    };
    let ok = state.backend.delete(del).map_err(map_delete_err)?;
    Ok(IpcDeleteOk {
        deleted: ok.deleted,
        backup_path: ok.backup_path.to_string_lossy().into_owned(),
    })
}
