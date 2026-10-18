//! Import desktop login material → `credential-vault` CredentialStore.
//!
//! Desktop paths are **import sources only** (Architect). Never log tokens / cipher plaintext.
//! Live HTTP remains gated elsewhere; this module only stores into vault.

use std::fmt;
use std::path::{Path, PathBuf};

use credential_vault::{
    CredentialBackend, CredentialStore, MaterialKind, PlatformId, StoreArgs, StoreError,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
use zeroize::Zeroize;

use crate::config::DesktopPaths;
use crate::types::CredentialsRef;
use crate::vault_bridge::{resolve_for_checkin, ResolvedSession};

/// Safe report after a successful import (never contains tokens / cipher plaintext).
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImportReport {
    pub platform: String,
    /// wb: uid；trae: path-safe device_id or `"primary"`
    pub account_key: String,
    /// `cred://v1/account/...`
    pub credential_ref: String,
    pub source_path: String,
    /// WorkBuddy domain (safe); Trae leaves None
    pub domain: Option<String>,
    /// Trae: whether device_id was present and stored
    pub device_id_present: bool,
}

impl fmt::Debug for ImportReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ImportReport")
            .field("platform", &self.platform)
            .field("account_key", &self.account_key)
            .field("credential_ref", &self.credential_ref)
            .field("source_path", &self.source_path)
            .field("domain", &self.domain)
            .field("device_id_present", &self.device_id_present)
            .finish()
    }
}

impl ImportReport {
    pub fn as_credentials_ref(&self) -> CredentialsRef {
        CredentialsRef::new(&self.credential_ref)
    }
}

/// Aggregated report for one-shot WB + Trae import (no secrets).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ImportBothReport {
    pub workbuddy: Option<ImportReport>,
    pub trae: Option<ImportReport>,
    /// `Display` of [`ImportError`] — no secrets
    pub workbuddy_error: Option<String>,
    pub trae_error: Option<String>,
}

impl ImportBothReport {
    pub fn any_ok(&self) -> bool {
        self.workbuddy.is_some() || self.trae.is_some()
    }

    pub fn reports(&self) -> Vec<&ImportReport> {
        let mut out = Vec::new();
        if let Some(r) = &self.workbuddy {
            out.push(r);
        }
        if let Some(r) = &self.trae {
            out.push(r);
        }
        out
    }

    pub fn both_ok(&self) -> bool {
        self.workbuddy.is_some() && self.trae.is_some()
    }
}

/// Format import error for reports / IPC (never attach secret material).
///
/// Prefer vault `StoreError::code()` → Chinese copy from
/// `docs/credential-store-error-codes-for-checkin.md`. Shape: `{code}: {中文建议}`.
/// Auth file-mode is landed (`VaultDirRequired` / `PayloadTooLarge`); no parallel
/// `TooLarge` name — enum follows credential-vault.
pub fn format_import_error(err: &ImportError) -> String {
    match err {
        ImportError::Vault(se) => format_by_store_code(se.code()),
        ImportError::VaultDirRequired(_) => {
            format_by_store_code("vault_dir_required")
        }
        ImportError::FileMissing(_) => {
            "file_missing: 本机登录态文件不存在，请确认客户端已登录".into()
        }
        ImportError::Unreadable(_) => {
            "unreadable: 本机登录态文件无法读取".into()
        }
        ImportError::Parse => "invalid_secret: 登录态格式不对，请重新从本机导入".into(),
        ImportError::MissingFields(_) => {
            "invalid_secret: 登录态格式不对，请重新从本机导入".into()
        }
        ImportError::EnvUnresolved(msg) => {
            format!("env_unresolved: {msg}")
        }
        ImportError::Internal(_) => "internal: 导入失败，请重试".into(),
    }
}

fn format_by_store_code(code: &str) -> String {
    let zh = match code {
        "invalid_secret" => "登录态格式不对，请重新从本机导入",
        "replace_not_found" => "原凭据不存在，请改为新建导入",
        "vault_dir_required" => {
            // 开发期括号句（见 Auth 错误码文档）
            "应用数据目录未就绪，请重启应用；若持续出现请反馈（CredentialStore 未注入 vault_dir）"
        }
        "payload_too_large" => {
            // 正常 Trae + with_vault_dir 走 file mode，不应再靠此码当主路径；
            // 仅残余：put_raw 超 soft max / 无法溢出落盘等真实失败。
            "备份内容过大，暂不支持裸写入（需 Switcher 指针化）"
        }
        "backend_unavailable" => "系统凭据保险库不可用",
        "backend_denied" => "未获系统凭据权限，请在系统设置中允许",
        "not_found" => "凭据不存在或已删除",
        "invalid_ref" => "凭据引用无效",
        "backup_required" => "删除前请设置备份口令",
        "corrupt_envelope" => "凭据数据损坏，请重新导入",
        "internal" => "导入失败，请重试",
        other => {
            return format!("{other}: 导入失败，请重试");
        }
    };
    format!("{code}: {zh}")
}

/// Import failures — Display/Debug must never attach token material.
#[derive(Error)]
pub enum ImportError {
    #[error("file missing: {0}")]
    FileMissing(String),
    #[error("unreadable: {0}")]
    Unreadable(String),
    #[error("json parse error")]
    Parse,
    #[error("missing fields: {0}")]
    MissingFields(String),
    #[error("env path unresolved: {0}")]
    EnvUnresolved(String),
    /// Trae 强制 file mode；调用方未 `with_vault_dir` 时早失败（勿当静默成功）。
    #[error("vault_dir required: {0}")]
    VaultDirRequired(String),
    #[error("vault store failed: {0}")]
    Vault(#[from] StoreError),
    #[error("internal: {0}")]
    Internal(String),
}

impl fmt::Debug for ImportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Re-use Display — messages are field-name / path only, no secrets.
        write!(f, "ImportError({self})")
    }
}

/// Import WorkBuddy `workbuddy-desktop.info` at an explicit path into the vault.
pub fn import_workbuddy_info_file<B: CredentialBackend>(
    store: &CredentialStore<B>,
    path: &Path,
    replace_ref: Option<&str>,
) -> Result<ImportReport, ImportError> {
    if !path.exists() {
        return Err(ImportError::FileMissing(path.display().to_string()));
    }
    let mut text = std::fs::read_to_string(path).map_err(|_| {
        ImportError::Unreadable(path.display().to_string())
    })?;
    let result = parse_and_store_workbuddy(store, path, &text, replace_ref);
    // Best-effort scrub of file buffer (String drop alone does not zeroize).
    text.zeroize();
    result
}

/// Import using env-resolved Windows default path (`DesktopPaths::resolve_wb_auth`).
pub fn import_workbuddy_default_path<B: CredentialBackend>(
    store: &CredentialStore<B>,
    paths: &DesktopPaths,
    replace_ref: Option<&str>,
) -> Result<ImportReport, ImportError> {
    let path = paths.resolve_wb_auth().ok_or_else(|| {
        ImportError::EnvUnresolved(
            "LOCALAPPDATA unset; cannot resolve WorkBuddy auth path".into(),
        )
    })?;
    import_workbuddy_info_file(store, &path, replace_ref)
}

/// Import Trae CN `storage.json` (cipher + deviceId; **no decrypt**).
pub fn import_trae_storage_file<B: CredentialBackend>(
    store: &CredentialStore<B>,
    path: &Path,
    paths: &DesktopPaths,
    replace_ref: Option<&str>,
) -> Result<ImportReport, ImportError> {
    // Trae 强制 file mode（Auth blob 补丁）；无 vault_dir 立刻失败，勿静默成功。
    if store.vault_dir().is_none() {
        return Err(ImportError::VaultDirRequired(
            "Trae/超限 payload 需 with_vault_dir（外置 .cve）".into(),
        ));
    }
    if !path.exists() {
        return Err(ImportError::FileMissing(path.display().to_string()));
    }
    let mut text = std::fs::read_to_string(path).map_err(|_| {
        ImportError::Unreadable(path.display().to_string())
    })?;
    let result = parse_and_store_trae(store, path, &text, paths, replace_ref);
    text.zeroize();
    result
}

/// Import using env-resolved Windows default Trae storage path.
pub fn import_trae_default_path<B: CredentialBackend>(
    store: &CredentialStore<B>,
    paths: &DesktopPaths,
    replace_ref: Option<&str>,
) -> Result<ImportReport, ImportError> {
    if store.vault_dir().is_none() {
        return Err(ImportError::VaultDirRequired(
            "Trae/超限 payload 需 with_vault_dir（外置 .cve）".into(),
        ));
    }
    let path = paths.resolve_trae_storage().ok_or_else(|| {
        ImportError::EnvUnresolved("APPDATA unset; cannot resolve Trae storage path".into())
    })?;
    import_trae_storage_file(store, &path, paths, replace_ref)
}

/// One-shot import from optional explicit file paths.
///
/// Missing path / file / env → side error string; does **not** panic or abort the other side.
pub fn import_both_files<B: CredentialBackend>(
    store: &CredentialStore<B>,
    wb_path: Option<&Path>,
    trae_path: Option<&Path>,
    paths: &DesktopPaths,
) -> ImportBothReport {
    import_both_files_with_replace(store, wb_path, trae_path, paths, None, None)
}

/// Like [`import_both_files`] with per-platform `replace_ref`.
pub fn import_both_files_with_replace<B: CredentialBackend>(
    store: &CredentialStore<B>,
    wb_path: Option<&Path>,
    trae_path: Option<&Path>,
    paths: &DesktopPaths,
    wb_replace: Option<&str>,
    trae_replace: Option<&str>,
) -> ImportBothReport {
    let mut report = ImportBothReport::default();

    match wb_path {
        None => {
            report.workbuddy_error = Some("file missing: (no workbuddy path)".into());
        }
        Some(p) => match import_workbuddy_info_file(store, p, wb_replace) {
            Ok(r) => report.workbuddy = Some(r),
            Err(e) => report.workbuddy_error = Some(format_import_error(&e)),
        },
    }

    match trae_path {
        None => {
            report.trae_error = Some("file missing: (no trae path)".into());
        }
        Some(p) => match import_trae_storage_file(store, p, paths, trae_replace) {
            Ok(r) => report.trae = Some(r),
            Err(e) => report.trae_error = Some(format_import_error(&e)),
        },
    }

    report
}

/// One-shot import WB + Trae using [`DesktopPaths`] defaults.
///
/// Resolves env paths then delegates to [`import_both_files`]. Single-side failure
/// does not block the other.
pub fn import_both_defaults<B: CredentialBackend>(
    store: &CredentialStore<B>,
    paths: &DesktopPaths,
) -> ImportBothReport {
    import_both_defaults_with_replace(store, paths, None, None)
}

/// Like [`import_both_defaults`] with per-platform `replace_ref`.
pub fn import_both_defaults_with_replace<B: CredentialBackend>(
    store: &CredentialStore<B>,
    paths: &DesktopPaths,
    wb_replace: Option<&str>,
    trae_replace: Option<&str>,
) -> ImportBothReport {
    let wb = paths.resolve_wb_auth();
    let trae = paths.resolve_trae_storage();
    let mut report = import_both_files_with_replace(
        store,
        wb.as_deref(),
        trae.as_deref(),
        paths,
        wb_replace,
        trae_replace,
    );
    if wb.is_none() && report.workbuddy_error.is_some() {
        report.workbuddy_error = Some(
            "env path unresolved: LOCALAPPDATA unset; cannot resolve WorkBuddy auth path".into(),
        );
    }
    if trae.is_none() && report.trae_error.is_some() {
        report.trae_error = Some(
            "env path unresolved: APPDATA unset; cannot resolve Trae storage path".into(),
        );
    }
    report
}

fn parse_and_store_workbuddy<B: CredentialBackend>(
    store: &CredentialStore<B>,
    path: &Path,
    text: &str,
    replace_ref: Option<&str>,
) -> Result<ImportReport, ImportError> {
    let raw: Value = serde_json::from_str(text).map_err(|_| ImportError::Parse)?;

    let uid = extract_wb_uid(&raw).ok_or_else(|| {
        ImportError::MissingFields("account.uid".into())
    })?;
    if uid.is_empty() || uid.contains('/') {
        return Err(ImportError::MissingFields(
            "account.uid must be non-empty and must not contain '/'".into(),
        ));
    }

    let (mut access, mut refresh, domain, expires_at) = extract_wb_tokens(&raw)?;
    if access.is_empty() {
        return Err(ImportError::MissingFields("accessToken".into()));
    }
    if refresh.is_empty() {
        return Err(ImportError::MissingFields("refreshToken".into()));
    }

    // Build material JSON with vault field names, then scrub locals.
    let material = serde_json::json!({
        "access_token": access.as_str(),
        "refresh_token": refresh.as_str(),
        "domain": domain,
        "expires_at": expires_at,
    });
    let mut secret_json = serde_json::to_string(&material)
        .map_err(|e| ImportError::Internal(format!("serialize material: {e}")))?;
    access.zeroize();
    refresh.zeroize();

    let source_hint = Some(path.display().to_string());
    let ok = store.store(StoreArgs {
        platform: PlatformId::Workbuddy,
        account_key: uid.clone(),
        kind: MaterialKind::WorkBuddyTokens,
        secret_json: std::mem::take(&mut secret_json),
        source_hint,
        replace_ref: replace_ref.map(|s| s.to_string()),
    })?;
    secret_json.zeroize();

    let credential_ref = ok.credential_ref_str();
    Ok(ImportReport {
        platform: "workbuddy".into(),
        account_key: uid,
        credential_ref,
        source_path: path.display().to_string(),
        domain,
        device_id_present: false,
    })
}

fn parse_and_store_trae<B: CredentialBackend>(
    store: &CredentialStore<B>,
    path: &Path,
    text: &str,
    paths: &DesktopPaths,
    replace_ref: Option<&str>,
) -> Result<ImportReport, ImportError> {
    let raw: Value = serde_json::from_str(text).map_err(|_| ImportError::Parse)?;

    let hint = &paths.trae_auth_key_hint;
    let mut cipher = raw
        .get(hint.as_str())
        .and_then(value_as_nonempty_string)
        .or_else(|| find_icube_cipher(&raw))
        .unwrap_or_default();
    if cipher.is_empty() {
        return Err(ImportError::MissingFields(format!(
            "auth key `{hint}` (or *iCubeAuthInfo*)"
        )));
    }

    let device_id_key = &paths.trae_device_id_key;
    let mut device_id = raw
        .get(device_id_key.as_str())
        .and_then(value_as_nonempty_string)
        .unwrap_or_default();
    let device_id_present = !device_id.is_empty();

    let account_key = if device_id_present && is_path_safe_key(&device_id) {
        device_id.clone()
    } else {
        "primary".to_string()
    };

    if !device_id_present {
        cipher.zeroize();
        return Err(ImportError::MissingFields(format!(
            "`{device_id_key}` (device_id required)"
        )));
    }

    // Optional JWT for LiveHttp — best-effort; never decrypt iCube cipher.
    let (mut access, mut refresh) = extract_trae_optional_jwt(&raw, hint.as_str());

    let mut material_map = serde_json::Map::new();
    material_map.insert(
        "icube_auth_info_cipher".into(),
        Value::String(cipher.clone()),
    );
    material_map.insert("device_id".into(), Value::String(device_id.clone()));
    if !access.is_empty() {
        material_map.insert("access_token".into(), Value::String(access.clone()));
    }
    if let Some(ref rt) = refresh {
        if !rt.is_empty() {
            material_map.insert("refresh_token".into(), Value::String(rt.clone()));
        }
    }
    let material = Value::Object(material_map);
    let mut secret_json = serde_json::to_string(&material)
        .map_err(|e| ImportError::Internal(format!("serialize material: {e}")))?;
    cipher.zeroize();
    device_id.zeroize();
    access.zeroize();
    if let Some(ref mut rt) = refresh {
        rt.zeroize();
    }

    let ok = store.store(StoreArgs {
        platform: PlatformId::TraeCn,
        account_key: account_key.clone(),
        kind: MaterialKind::TraeCnICubeBlob,
        secret_json: std::mem::take(&mut secret_json),
        source_hint: Some(path.display().to_string()),
        replace_ref: replace_ref.map(|s| s.to_string()),
    })?;
    secret_json.zeroize();

    Ok(ImportReport {
        platform: "trae_cn".into(),
        account_key,
        credential_ref: ok.credential_ref_str(),
        source_path: path.display().to_string(),
        domain: None,
        device_id_present: true,
    })
}


/// Best-effort Trae JWT extraction from storage.json (no cipher decrypt).
///
/// Keys (access): `access_token` / `accessToken` / `AccessToken`
/// Keys (refresh): `refresh_token` / `refreshToken` / `RefreshToken`
/// Strategy: top-level first, then one level of nested objects that mention
/// those keys. Skips iCube auth cipher keys entirely.
fn extract_trae_optional_jwt(raw: &Value, icube_hint: &str) -> (String, Option<String>) {
    const ACCESS_KEYS: &[&str] = &["access_token", "accessToken", "AccessToken"];
    const REFRESH_KEYS: &[&str] = &["refresh_token", "refreshToken", "RefreshToken"];

    let Value::Object(map) = raw else {
        return (String::new(), None);
    };

    let mut access = pick_nonempty_str_keys(map, ACCESS_KEYS).unwrap_or_default();
    let mut refresh = pick_nonempty_str_keys(map, REFRESH_KEYS);

    if access.is_empty() || refresh.as_ref().map(|s| s.is_empty()).unwrap_or(true) {
        for (k, v) in map {
            if is_icube_auth_key(k, icube_hint) {
                continue;
            }
            let Value::Object(nested) = v else {
                continue;
            };
            let looks_auth = ACCESS_KEYS.iter().any(|ak| nested.contains_key(*ak))
                || REFRESH_KEYS.iter().any(|rk| nested.contains_key(*rk));
            if !looks_auth {
                continue;
            }
            if access.is_empty() {
                if let Some(a) = pick_nonempty_str_keys(nested, ACCESS_KEYS) {
                    access = a;
                }
            }
            if refresh.as_ref().map(|s| s.is_empty()).unwrap_or(true) {
                if let Some(r) = pick_nonempty_str_keys(nested, REFRESH_KEYS) {
                    refresh = Some(r);
                }
            }
            if !access.is_empty() && refresh.as_ref().map(|s| !s.is_empty()).unwrap_or(false) {
                break;
            }
        }
    }

    let refresh = refresh.filter(|s| !s.is_empty());
    (access, refresh)
}

fn pick_nonempty_str_keys(
    map: &serde_json::Map<String, Value>,
    keys: &[&str],
) -> Option<String> {
    for k in keys {
        if let Some(s) = map.get(*k).and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
            return Some(s.to_string());
        }
    }
    None
}

fn is_icube_auth_key(k: &str, hint: &str) -> bool {
    (!hint.is_empty() && k == hint) || k.contains("iCubeAuthInfo")
}

fn extract_wb_uid(raw: &Value) -> Option<String> {
    raw.get("account")
        .and_then(|a| a.get("uid"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .or_else(|| {
            raw.get("uid")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
        })
}

/// Returns (access, refresh, domain, expires_at). Tokens may be empty → caller checks.
fn extract_wb_tokens(
    raw: &Value,
) -> Result<(String, String, Option<String>, Option<i64>), ImportError> {
    let auth = raw.get("auth");

    // Case A: auth is object with accessToken / refreshToken
    if let Some(Value::Object(map)) = auth {
        let access = map
            .get("accessToken")
            .or_else(|| map.get("access_token"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let refresh = map
            .get("refreshToken")
            .or_else(|| map.get("refresh_token"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let domain = map
            .get("domain")
            .and_then(|v| v.as_str())
            .filter(|d| !d.is_empty())
            .map(|d| d.to_string())
            .or_else(|| {
                raw.get("domain")
                    .and_then(|v| v.as_str())
                    .filter(|d| !d.is_empty())
                    .map(|d| d.to_string())
            });
        let expires_at = parse_expires(
            map.get("expiresAt")
                .or_else(|| map.get("expires_at"))
                .or_else(|| raw.get("expiresAt"))
                .or_else(|| raw.get("expires_at")),
        );
        return Ok((access, refresh, domain, expires_at));
    }

    // Case B: auth is plain JWT string (Architect: auth 为 JWT 明文)
    let mut access = String::new();
    if let Some(Value::String(tok)) = auth {
        if !tok.is_empty() {
            access = tok.clone();
        }
    }
    if access.is_empty() {
        access = raw
            .get("accessToken")
            .or_else(|| raw.get("access_token"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
    }
    let refresh = raw
        .get("refreshToken")
        .or_else(|| raw.get("refresh_token"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let domain = raw
        .get("domain")
        .and_then(|v| v.as_str())
        .filter(|d| !d.is_empty())
        .map(|d| d.to_string());
    let expires_at = parse_expires(
        raw.get("expiresAt").or_else(|| raw.get("expires_at")),
    );
    Ok((access, refresh, domain, expires_at))
}

fn parse_expires(v: Option<&Value>) -> Option<i64> {
    match v? {
        Value::Number(n) => n.as_i64().or_else(|| n.as_u64().map(|u| u as i64)),
        Value::String(s) => s.parse::<i64>().ok(),
        _ => None,
    }
}

fn value_as_nonempty_string(v: &Value) -> Option<String> {
    match v {
        Value::String(s) if !s.is_empty() => Some(s.clone()),
        // Object blobs: keep whole JSON string as opaque cipher (still no decrypt)
        Value::Object(_) | Value::Array(_) => {
            let s = serde_json::to_string(v).ok()?;
            if s.is_empty() {
                None
            } else {
                Some(s)
            }
        }
        _ => None,
    }
}

fn find_icube_cipher(raw: &Value) -> Option<String> {
    let Value::Object(map) = raw else {
        return None;
    };
    for (k, v) in map {
        if k.contains("iCubeAuthInfo") {
            return value_as_nonempty_string(v);
        }
    }
    None
}

fn is_path_safe_key(s: &str) -> bool {
    !s.is_empty() && !s.contains('/') && !s.contains('\\')
}

/// Convenience: import then resolve for checkin (MemoryBackend / tests).
pub fn import_and_resolve_workbuddy<B: CredentialBackend>(
    store: &CredentialStore<B>,
    path: &Path,
) -> Result<(ImportReport, ResolvedSession), ImportError> {
    let report = import_workbuddy_info_file(store, path, None)?;
    let session = resolve_for_checkin(store, &report.as_credentials_ref()).map_err(|e| {
        ImportError::Internal(format!("resolve after import: {e}"))
    })?;
    Ok((report, session))
}

/// Resolve default or explicit path for CLI helpers.
pub fn resolve_cli_path(
    platform: &str,
    explicit: Option<&Path>,
    paths: &DesktopPaths,
) -> Result<PathBuf, ImportError> {
    if let Some(p) = explicit {
        return Ok(p.to_path_buf());
    }
    match platform {
        "workbuddy" | "wb" => paths.resolve_wb_auth().ok_or_else(|| {
            ImportError::EnvUnresolved(
                "LOCALAPPDATA unset and --path not given".into(),
            )
        }),
        "trae" | "trae_cn" => paths.resolve_trae_storage().ok_or_else(|| {
            ImportError::EnvUnresolved("APPDATA unset and --path not given".into())
        }),
        other => Err(ImportError::Internal(format!(
            "unknown platform `{other}` (workbuddy|trae_cn)"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use credential_vault::MemoryBackend;
    use tempfile::tempdir;

    const WB_ACCESS: &str = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.WB_ACCESS_FIXTURE.SIG";
    const WB_REFRESH: &str = "wb-refresh-secret-FIXTURE-9f3a";

    fn write_wb_fixture(dir: &Path) -> PathBuf {
        let path = dir.join("workbuddy-desktop.info");
        let body = serde_json::json!({
            "account": { "uid": "uid-wb-42", "nickname": "n" },
            "auth": {
                "accessToken": WB_ACCESS,
                "refreshToken": WB_REFRESH,
                "domain": "copilot.tencent.com",
                "expiresAt": 1893456000000i64
            }
        });
        std::fs::write(&path, body.to_string()).unwrap();
        path
    }

    fn write_trae_fixture(dir: &Path, paths: &DesktopPaths) -> PathBuf {
        let path = dir.join("storage.json");
        let cipher = "ICUBE_CIPHER_FIXTURE_blob_do_not_decrypt_xyz";
        let mut map = serde_json::Map::new();
        map.insert(
            paths.trae_auth_key_hint.clone(),
            Value::String(cipher.into()),
        );
        map.insert(
            paths.trae_device_id_key.clone(),
            Value::String("dev-device-abc-123".into()),
        );
        std::fs::write(&path, Value::Object(map).to_string()).unwrap();
        path
    }

    #[test]
    fn import_wb_then_resolve_ok_and_no_token_leak() {
        let dir = tempdir().unwrap();
        let path = write_wb_fixture(dir.path());
        let store = CredentialStore::new(MemoryBackend::new());

        let report = import_workbuddy_info_file(&store, &path, None).expect("import");
        assert_eq!(report.platform, "workbuddy");
        assert_eq!(report.account_key, "uid-wb-42");
        assert!(report
            .credential_ref
            .starts_with("cred://v1/account/workbuddy/"));
        assert_eq!(report.domain.as_deref(), Some("copilot.tencent.com"));

        let dbg = format!("{report:?}");
        let json = serde_json::to_string(&report).unwrap();
        assert!(!dbg.contains(WB_ACCESS));
        assert!(!dbg.contains(WB_REFRESH));
        assert!(!json.contains(WB_ACCESS));
        assert!(!json.contains(WB_REFRESH));

        let session = resolve_for_checkin(&store, &report.as_credentials_ref()).expect("resolve");
        let sdbg = format!("{session:?}");
        assert!(!sdbg.contains(WB_ACCESS));
        assert!(!sdbg.contains(WB_REFRESH));
        assert!(matches!(session, ResolvedSession::WorkBuddy { .. }));
        drop(session);
    }

    #[test]
    fn import_trae_then_resolve_ref_contains_trae_cn() {
        let dir = tempdir().unwrap();
        let vault = tempdir().unwrap();
        let paths = DesktopPaths::default();
        let path = write_trae_fixture(dir.path(), &paths);
        let store = CredentialStore::with_vault_dir(
            MemoryBackend::new(),
            vault.path().to_path_buf(),
        );

        let report = import_trae_storage_file(&store, &path, &paths, None).expect("import");
        assert_eq!(report.platform, "trae_cn");
        assert!(report.device_id_present);
        assert!(report.credential_ref.contains("trae_cn"));
        assert!(report
            .credential_ref
            .starts_with("cred://v1/account/trae_cn/"));
        // account_key = device_id when path-safe
        assert_eq!(report.account_key, "dev-device-abc-123");

        let dbg = format!("{report:?}");
        let json = serde_json::to_string(&report).unwrap();
        assert!(!dbg.contains("ICUBE_CIPHER_FIXTURE"));
        assert!(!json.contains("ICUBE_CIPHER_FIXTURE"));

        let session = resolve_for_checkin(&store, &report.as_credentials_ref()).expect("resolve");
        match &session {
            ResolvedSession::Trae {
                access_token,
                needs_desktop_session,
                ..
            } => {
                assert!(
                    access_token.is_empty(),
                    "cipher-only fixture must leave access empty"
                );
                assert!(
                    *needs_desktop_session,
                    "cipher-only → needs_desktop_session"
                );
            }
            other => panic!("expected Trae, got {other:?}"),
        }
        assert!(!format!("{session:?}").contains("ICUBE_CIPHER_FIXTURE"));
        drop(session);
    }

    #[test]
    fn import_trae_with_top_level_jwt_resolves_ready() {
        let dir = tempdir().unwrap();
        let vault = tempdir().unwrap();
        let paths = DesktopPaths::default();
        let path = dir.path().join("storage.json");
        let cipher = "ICUBE_CIPHER_JWT_TOP_FIXTURE";
        let access = "trae_jwt_ACCESS_SECRET_top_level";
        let refresh = "trae_jwt_REFRESH_SECRET_top_level";
        let body = serde_json::json!({
            paths.trae_auth_key_hint.clone(): cipher,
            paths.trae_device_id_key.clone(): "dev-jwt-top",
            "access_token": access,
            "refresh_token": refresh,
        });
        std::fs::write(&path, body.to_string()).unwrap();
        let store = CredentialStore::with_vault_dir(
            MemoryBackend::new(),
            vault.path().to_path_buf(),
        );

        let report = import_trae_storage_file(&store, &path, &paths, None).expect("import");
        let dbg = format!("{report:?}");
        let json = serde_json::to_string(&report).unwrap();
        assert!(!dbg.contains(access));
        assert!(!dbg.contains(refresh));
        assert!(!json.contains(access));
        assert!(!json.contains(refresh));
        assert!(!dbg.contains(cipher));
        assert!(!json.contains(cipher));

        let session = resolve_for_checkin(&store, &report.as_credentials_ref()).expect("resolve");
        match &session {
            ResolvedSession::Trae {
                access_token,
                refresh_token,
                needs_desktop_session,
                ..
            } => {
                assert_eq!(access_token, access);
                assert_eq!(refresh_token, refresh);
                assert!(!*needs_desktop_session);
            }
            other => panic!("expected Trae, got {other:?}"),
        }
        let sdbg = format!("{session:?}");
        assert!(!sdbg.contains(access));
        assert!(!sdbg.contains(refresh));
        drop(session);
    }

    #[test]
    fn import_trae_with_nested_access_token_resolves_ready() {
        let dir = tempdir().unwrap();
        let vault = tempdir().unwrap();
        let paths = DesktopPaths::default();
        let path = dir.path().join("storage.json");
        let cipher = "ICUBE_CIPHER_JWT_NESTED_FIXTURE";
        let access = "trae_jwt_ACCESS_SECRET_nested_camel";
        let refresh = "trae_jwt_REFRESH_SECRET_nested_camel";
        let body = serde_json::json!({
            paths.trae_auth_key_hint.clone(): cipher,
            paths.trae_device_id_key.clone(): "dev-jwt-nested",
            "session": {
                "accessToken": access,
                "refreshToken": refresh,
            }
        });
        std::fs::write(&path, body.to_string()).unwrap();
        let store = CredentialStore::with_vault_dir(
            MemoryBackend::new(),
            vault.path().to_path_buf(),
        );

        let report = import_trae_storage_file(&store, &path, &paths, None).expect("import");
        let json = serde_json::to_string(&report).unwrap();
        assert!(!json.contains(access));
        assert!(!json.contains(refresh));

        let session = resolve_for_checkin(&store, &report.as_credentials_ref()).expect("resolve");
        match &session {
            ResolvedSession::Trae {
                access_token,
                needs_desktop_session,
                ..
            } => {
                assert_eq!(access_token, access);
                assert!(!*needs_desktop_session);
            }
            other => panic!("expected Trae, got {other:?}"),
        }
        drop(session);
    }

    #[test]
    fn import_wb_missing_uid() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("wb.info");
        std::fs::write(
            &path,
            r#"{"auth":{"accessToken":"a","refreshToken":"b"}}"#,
        )
        .unwrap();
        let store = CredentialStore::new(MemoryBackend::new());
        let err = import_workbuddy_info_file(&store, &path, None).unwrap_err();
        assert!(matches!(err, ImportError::MissingFields(_)));
        assert!(!format!("{err:?}").contains("accessToken"));
    }

    #[test]
    fn import_wb_file_missing() {
        let store = CredentialStore::new(MemoryBackend::new());
        let err = import_workbuddy_info_file(
            &store,
            Path::new("/tmp/definitely-missing-wb-import-xyz.info"),
            None,
        )
        .unwrap_err();
        assert!(matches!(err, ImportError::FileMissing(_)));
    }

    #[test]
    fn import_wb_auth_plain_jwt_needs_refresh() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("wb.info");
        let body = serde_json::json!({
            "account": { "uid": "u1" },
            "auth": "eyJplain.jwt.only",
            "domain": "copilot.tencent.com"
        });
        std::fs::write(&path, body.to_string()).unwrap();
        let store = CredentialStore::new(MemoryBackend::new());
        let err = import_workbuddy_info_file(&store, &path, None).unwrap_err();
        assert!(matches!(err, ImportError::MissingFields(_)));
    }

    #[test]
    fn import_wb_auth_plain_jwt_with_flat_refresh() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("wb.info");
        let access = "eyJplain.jwt.with.refresh.pair";
        let refresh = "flat-refresh-FIXTURE";
        let body = serde_json::json!({
            "account": { "uid": "u2" },
            "auth": access,
            "refreshToken": refresh,
            "domain": "copilot.tencent.com"
        });
        std::fs::write(&path, body.to_string()).unwrap();
        let store = CredentialStore::new(MemoryBackend::new());
        let report = import_workbuddy_info_file(&store, &path, None).expect("import");
        assert!(report.credential_ref.starts_with("cred://v1/account/workbuddy/"));
        assert!(!format!("{report:?}").contains(access));
        assert!(!format!("{report:?}").contains(refresh));
    }

    #[test]
    fn import_trae_missing_device_id() {
        let dir = tempdir().unwrap();
        let vault = tempdir().unwrap();
        let paths = DesktopPaths::default();
        let path = dir.path().join("storage.json");
        let body = serde_json::json!({
            paths.trae_auth_key_hint.clone(): "cipher-only"
        });
        std::fs::write(&path, body.to_string()).unwrap();
        let store = CredentialStore::with_vault_dir(
            MemoryBackend::new(),
            vault.path().to_path_buf(),
        );
        let err = import_trae_storage_file(&store, &path, &paths, None).unwrap_err();
        assert!(matches!(err, ImportError::MissingFields(_)));
    }

    #[test]
    fn import_error_debug_has_no_fixture_token() {
        let err = ImportError::MissingFields("accessToken".into());
        let s = format!("{err:?}");
        assert!(!s.contains(WB_ACCESS));
        assert!(s.contains("MissingFields") || s.contains("missing fields"));
    }

    #[test]
    fn default_path_env_unresolved_on_linux_box() {
        // Ensure LOCALAPPDATA/APPDATA unset for this assertion
        std::env::remove_var("LOCALAPPDATA");
        std::env::remove_var("APPDATA");
        let store = CredentialStore::new(MemoryBackend::new());
        let paths = DesktopPaths::default();
        let err = import_workbuddy_default_path(&store, &paths, None).unwrap_err();
        assert!(matches!(err, ImportError::EnvUnresolved(_)));
    }

    #[test]
    fn import_both_files_ok_independent() {
        let dir = tempdir().unwrap();
        let vault = tempdir().unwrap();
        let paths = DesktopPaths::default();
        let wb = write_wb_fixture(dir.path());
        let trae = write_trae_fixture(dir.path(), &paths);
        let store = CredentialStore::with_vault_dir(
            MemoryBackend::new(),
            vault.path().to_path_buf(),
        );

        let both = import_both_files(&store, Some(&wb), Some(&trae), &paths);
        assert!(both.any_ok());
        assert!(both.both_ok());
        assert_eq!(both.reports().len(), 2);
        assert!(both.workbuddy_error.is_none());
        assert!(both.trae_error.is_none());
        assert_eq!(both.workbuddy.as_ref().unwrap().account_key, "uid-wb-42");
        assert!(both
            .trae
            .as_ref()
            .unwrap()
            .credential_ref
            .contains("trae_cn"));

        let json = serde_json::to_string(&both).unwrap();
        assert!(!json.contains(WB_ACCESS));
        assert!(!json.contains("ICUBE_CIPHER_FIXTURE"));
    }

    #[test]
    fn import_both_files_wb_ok_trae_missing_does_not_block() {
        let dir = tempdir().unwrap();
        let vault = tempdir().unwrap();
        let paths = DesktopPaths::default();
        let wb = write_wb_fixture(dir.path());
        // vault_dir present so Trae path reaches FileMissing (not early VaultDirRequired)
        let store = CredentialStore::with_vault_dir(
            MemoryBackend::new(),
            vault.path().to_path_buf(),
        );

        let both = import_both_files(
            &store,
            Some(&wb),
            Some(Path::new("/tmp/definitely-missing-trae-storage-xyz.json")),
            &paths,
        );
        assert!(both.workbuddy.is_some());
        assert!(both.trae.is_none());
        assert!(
            both.trae_error.as_ref().unwrap().contains("file_missing")
                || both.trae_error.as_ref().unwrap().contains("file missing"),
            "got: {:?}",
            both.trae_error
        );
        assert!(both.any_ok());
        assert!(!both.both_ok());
        assert_eq!(both.reports().len(), 1);
    }

    #[test]
    fn import_both_files_none_paths_record_errors() {
        let store = CredentialStore::new(MemoryBackend::new());
        let paths = DesktopPaths::default();
        let both = import_both_files(&store, None, None, &paths);
        assert!(!both.any_ok());
        assert!(both.workbuddy_error.is_some());
        assert!(both.trae_error.is_some());
        assert!(both.reports().is_empty());
    }

    #[test]
    fn format_import_error_uses_store_code_payload_too_large() {
        let err = ImportError::Vault(StoreError::PayloadTooLarge {
            size: 4096,
            limit: 2400,
        });
        let msg = format_import_error(&err);
        assert!(msg.starts_with("payload_too_large:"), "{msg}");
        assert!(msg.contains("备份内容过大"), "{msg}");
        assert!(!msg.contains("token"));
        assert!(!msg.contains("cipher"));
        assert!(!msg.contains("file_key"));
    }

    #[test]
    fn format_import_error_legacy_internal_credman_overflow_maps_internal() {
        // Auth 已用 file mode；Internal 字符串启发式不再当主路径。
        // 无专用变体时走 code()=internal。
        let err = ImportError::Vault(StoreError::Internal(
            "credential blob 4096 bytes exceeds CRED_MAX_CREDENTIAL_BLOB_SIZE (2560)".into(),
        ));
        let msg = format_import_error(&err);
        assert!(msg.starts_with("internal:"), "{msg}");
    }

    #[test]
    fn format_import_error_vault_dir_required_variant() {
        let err = ImportError::Vault(StoreError::VaultDirRequired(
            "oversized or Trae payload needs with_vault_dir(app_data)".into(),
        ));
        let msg = format_import_error(&err);
        assert!(msg.starts_with("vault_dir_required:"), "{msg}");
        assert!(msg.contains("vault_dir"), "{msg}");
        assert!(!msg.to_ascii_lowercase().contains("secret"));
    }

    #[test]
    fn format_import_error_import_vault_dir_required() {
        let err = ImportError::VaultDirRequired(
            "Trae/超限 payload 需 with_vault_dir（外置 .cve）".into(),
        );
        let msg = format_import_error(&err);
        assert!(msg.starts_with("vault_dir_required:"), "{msg}");
        assert!(msg.contains("CredentialStore 未注入 vault_dir"), "{msg}");
    }

    #[test]
    fn format_import_error_code_table_smoke() {
        for (err, code) in [
            (
                ImportError::Vault(StoreError::InvalidSecret("x".into())),
                "invalid_secret",
            ),
            (
                ImportError::Vault(StoreError::ReplaceNotFound),
                "replace_not_found",
            ),
            (
                ImportError::Vault(StoreError::BackendUnavailable),
                "backend_unavailable",
            ),
            (
                ImportError::Vault(StoreError::BackendDenied),
                "backend_denied",
            ),
        ] {
            let msg = format_import_error(&err);
            assert!(msg.starts_with(&format!("{code}:")), "{msg}");
        }
    }

    #[test]
    fn import_trae_without_vault_dir_early_vault_dir_required() {
        let dir = tempdir().unwrap();
        let paths = DesktopPaths::default();
        let path = write_trae_fixture(dir.path(), &paths);
        let store = CredentialStore::new(MemoryBackend::new());
        let err = import_trae_storage_file(&store, &path, &paths, None).unwrap_err();
        assert!(
            matches!(err, ImportError::VaultDirRequired(_)),
            "got: {err:?}"
        );
        let msg = format_import_error(&err);
        assert!(msg.starts_with("vault_dir_required:"), "{msg}");
        assert!(msg.contains("vault_dir"), "{msg}");
    }

    #[test]
    fn import_trae_with_vault_dir_ok() {
        let dir = tempdir().unwrap();
        let vault = tempdir().unwrap();
        let paths = DesktopPaths::default();
        let path = write_trae_fixture(dir.path(), &paths);
        let store = CredentialStore::with_vault_dir(
            MemoryBackend::new(),
            vault.path().to_path_buf(),
        );
        let report = import_trae_storage_file(&store, &path, &paths, None).expect("import");
        assert!(report.credential_ref.contains("trae_cn"));
        assert!(report.device_id_present);
    }

    #[test]
    fn import_both_wb_ok_trae_no_vault_dir_not_silent() {
        let dir = tempdir().unwrap();
        let paths = DesktopPaths::default();
        let wb = write_wb_fixture(dir.path());
        let trae = write_trae_fixture(dir.path(), &paths);
        // MemoryBackend without vault_dir — WB inline OK; Trae must fail loudly.
        let store = CredentialStore::new(MemoryBackend::new());
        let both = import_both_files(&store, Some(&wb), Some(&trae), &paths);
        assert!(both.workbuddy.is_some(), "WB should succeed");
        assert!(both.trae.is_none());
        let te = both.trae_error.as_ref().expect("trae_error non-empty");
        assert!(te.starts_with("vault_dir_required:"), "{te}");
        assert!(!te.is_empty());
        assert!(both.any_ok());
        assert!(!both.both_ok());
    }

    #[test]
    fn import_both_defaults_env_unresolved_on_linux() {
        std::env::remove_var("LOCALAPPDATA");
        std::env::remove_var("APPDATA");
        let store = CredentialStore::new(MemoryBackend::new());
        let paths = DesktopPaths::default();
        let both = import_both_defaults(&store, &paths);
        assert!(!both.any_ok());
        assert!(both
            .workbuddy_error
            .as_ref()
            .unwrap()
            .contains("env path unresolved"));
        assert!(both
            .trae_error
            .as_ref()
            .unwrap()
            .contains("env path unresolved"));
    }
}
