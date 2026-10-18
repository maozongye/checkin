//! 从账号凭据 `CredentialHandle` 构建可 import 的 [`LoginStateBlob`]。
//!
//! - Handle 用毕即 drop；调用方勿把 token 拷进日志 / DB / progress
//! - Trae：**不**把 `device_id` 写进 storage.json（只换认证键）

use std::fs;

use credential_vault::CredentialHandle;
use serde_json::{json, Value};

use super::adapter::{AccountMeta, LoginFormat, LoginStateBlob};
use super::paths::{self, TRAE_AUTH_KEY};

/// 将已 resolve 的账号凭据转为 Adapter 可 import 的 blob。
/// `product` 须已归一化为 `workbuddy` | `trae_cn`。
pub fn blob_from_account_handle(
    product: &str,
    handle: &CredentialHandle,
) -> Result<LoginStateBlob, String> {
    match product {
        "workbuddy" => build_workbuddy_blob(handle),
        "trae_cn" => build_trae_blob(handle),
        other => Err(format!("unsupported product for material: {other}")),
    }
}

fn build_workbuddy_blob(handle: &CredentialHandle) -> Result<LoginStateBlob, String> {
    let tokens = handle
        .as_workbuddy_tokens()
        .map_err(|_| "target_credential_missing".to_string())?;

    let path = paths::workbuddy_auth_path();
    let mut root = if let Some(ref p) = path {
        if p.is_file() {
            match fs::read(p) {
                Ok(bytes) => {
                    // 若现盘是 $wbEncrypted，无法用明文 token 原样 patch → 改写为 PlainJson
                    // （目标来自 vault 明文；合同：format = PlainJsonInfo）
                    if bytes.windows(b"$wbEncrypted".len()).any(|w| w == b"$wbEncrypted") {
                        json!({ "account": {}, "auth": {} })
                    } else {
                        serde_json::from_slice::<Value>(&bytes).unwrap_or_else(|_| {
                            json!({ "account": {}, "auth": {} })
                        })
                    }
                }
                Err(_) => json!({ "account": {}, "auth": {} }),
            }
        } else {
            json!({ "account": {}, "auth": {} })
        }
    } else {
        // 非 Windows：仍构造 blob，import 时 Adapter 会报 path unavailable
        json!({ "account": {}, "auth": {} })
    };

    let obj = root
        .as_object_mut()
        .ok_or_else(|| "wb root must be object".to_string())?;
    // 保留既有 account / accounts / allAccounts；无则补空 account
    if !obj.contains_key("account") {
        obj.insert("account".into(), json!({}));
    }

    let auth = obj.entry("auth".to_string()).or_insert_with(|| json!({}));
    let auth_obj = auth
        .as_object_mut()
        .ok_or_else(|| "wb auth must be object".to_string())?;
    auth_obj.insert(
        "accessToken".into(),
        Value::String(tokens.access_token.to_string()),
    );
    auth_obj.insert(
        "refreshToken".into(),
        Value::String(tokens.refresh_token.to_string()),
    );
    if let Some(d) = tokens.domain {
        auth_obj.insert("domain".into(), Value::String(d.to_string()));
    }
    if let Some(exp) = tokens.expires_at {
        auth_obj.insert("expiresAt".into(), json!(exp));
    }

    let plaintext =
        serde_json::to_vec(&root).map_err(|e| format!("serialize wb target: {e}"))?;
    // 脱敏 meta：只取 domain；uid 若现文件 account 有则 peek（不读 token）
    let mut meta = AccountMeta {
        domain: tokens.domain.map(|s| s.to_string()),
        ..AccountMeta::default()
    };
    if let Some(account) = root.get("account") {
        meta.uid = account
            .get("uid")
            .or_else(|| account.get("userId"))
            .or_else(|| account.get("id"))
            .and_then(|x| x.as_str())
            .map(|s| s.to_string());
        meta.nickname = account
            .get("nickname")
            .or_else(|| account.get("nickName"))
            .or_else(|| account.get("name"))
            .and_then(|x| x.as_str())
            .map(|s| s.to_string());
    }

    let source_paths = path.map(|p| vec![p]).unwrap_or_default();

    Ok(LoginStateBlob {
        product: "workbuddy".into(),
        format: LoginFormat::PlainJsonInfo,
        source_paths,
        account_meta: meta,
        plaintext,
    })
}

fn build_trae_blob(handle: &CredentialHandle) -> Result<LoginStateBlob, String> {
    let view = handle
        .as_trae_cn_blob()
        .map_err(|_| "target_credential_missing".to_string())?;

    // auth_value：密文字符串；**不**把 device_id 写入信封/盘
    let envelope = json!({
        "format": "TraeStorageKey",
        "auth_key": TRAE_AUTH_KEY,
        "auth_value": view.icube_auth_info_cipher,
        // device_id 仅文档：material 有此字段，写盘时 Adapter 只替换认证键，忽略 telemetry.devDeviceId
    });
    let plaintext =
        serde_json::to_vec(&envelope).map_err(|e| format!("serialize trae target: {e}"))?;

    let source_paths = paths::trae_cn_storage_path()
        .map(|p| vec![p])
        .unwrap_or_default();

    Ok(LoginStateBlob {
        product: "trae_cn".into(),
        format: LoginFormat::TraeStorageKey,
        source_paths,
        account_meta: AccountMeta::default(),
        plaintext,
    })
}
