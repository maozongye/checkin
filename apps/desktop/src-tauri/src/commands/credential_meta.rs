//! credential_meta_store — put/get/delete/list **仅 meta**（无 secret）。
//!
//! 真导入请用 `credential_store` / `credential_delete`（见 `credential.rs`）。
//! Wire 与 Frontend 对齐：`invoke("credential_meta_store", { args: { ... } })`，字段 snake_case。

use super::mock_data::now_ms;
use super::state::{AppState, CredentialMeta};
use serde::{Deserialize, Serialize};
use tauri::{command, State};

#[derive(Debug, Deserialize)]
pub struct IpcMetaArgs {
    pub action: String,
    pub account_id: Option<String>,
    pub credential_ref: Option<String>,
    pub label: Option<String>,
}

#[derive(Clone, Serialize)]
pub struct CredentialMetaStoreResult {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ok: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credential_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<Vec<CredentialMeta>>,
}

/// action: put | get | delete | list。secret 字段未声明，IPC 丢弃。
#[command]
pub fn credential_meta_store(
    state: State<'_, AppState>,
    args: IpcMetaArgs,
) -> Result<CredentialMetaStoreResult, String> {
    let IpcMetaArgs {
        action,
        account_id,
        credential_ref,
        label,
    } = args;

    let mut store = state
        .credentials
        .lock()
        .map_err(|_| "credential lock poisoned".to_string())?;

    match action.as_str() {
        "put" => {
            let aid = account_id.ok_or_else(|| "put 需要 account_id".to_string())?;
            let lbl = label.unwrap_or_else(|| format!("cred-{aid}"));
            let cref = credential_ref.unwrap_or_else(|| format!("cref-{}-{}", aid, now_ms()));
            store.retain(|c| c.account_id != aid && c.credential_ref != cref);
            store.push(CredentialMeta {
                credential_ref: cref.clone(),
                label: lbl.clone(),
                account_id: aid.clone(),
            });
            Ok(CredentialMetaStoreResult {
                ok: Some(true),
                credential_ref: Some(cref),
                label: Some(lbl),
                account_id: Some(aid),
                items: None,
            })
        }
        "get" => {
            let found = store.iter().find(|c| {
                credential_ref
                    .as_ref()
                    .map(|r| &c.credential_ref == r)
                    .unwrap_or(false)
                    || account_id
                        .as_ref()
                        .map(|a| &c.account_id == a)
                        .unwrap_or(false)
            });
            match found {
                Some(m) => Ok(CredentialMetaStoreResult {
                    ok: Some(true),
                    credential_ref: Some(m.credential_ref.clone()),
                    label: Some(m.label.clone()),
                    account_id: Some(m.account_id.clone()),
                    items: None,
                }),
                None => Err("credential not found".into()),
            }
        }
        "delete" => {
            let before = store.len();
            store.retain(|c| {
                let by_ref = credential_ref
                    .as_ref()
                    .map(|r| &c.credential_ref == r)
                    .unwrap_or(false);
                let by_acc = account_id
                    .as_ref()
                    .map(|a| &c.account_id == a)
                    .unwrap_or(false);
                !(by_ref || by_acc)
            });
            Ok(CredentialMetaStoreResult {
                ok: Some(store.len() < before),
                credential_ref,
                label: None,
                account_id,
                items: None,
            })
        }
        "list" => Ok(CredentialMetaStoreResult {
            ok: Some(true),
            credential_ref: None,
            label: None,
            account_id: None,
            items: Some(store.clone()),
        }),
        other => Err(format!("unknown credential_meta_store action: {other}")),
    }
}
