//! PlatformAdapter：Detect / Export / Import / Health / client_running。
//! 返回值与日志 **永不**包含 token / 密文正文。

use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::paths::{self, path_hint, TRAE_AUTH_KEY};
use super::process;

/// 登录态格式（元数据，进 DB / Detect，非机密）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum LoginFormat {
    PlainJsonInfo,
    WbEncryptedInfo,
    TraeStorageKey,
}

impl LoginFormat {
    pub fn as_str(self) -> &'static str {
        match self {
            LoginFormat::PlainJsonInfo => "PlainJsonInfo",
            LoginFormat::WbEncryptedInfo => "WbEncryptedInfo",
            LoginFormat::TraeStorageKey => "TraeStorageKey",
        }
    }
}

/// 脱敏账号元信息（可进 DB / IPC；无 token）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountMeta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nickname: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
}

/// Detect 结果（路径仅 hint，不强制完整用户目录进 UI）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectResult {
    pub product: String,
    pub hit: bool,
    pub paths: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_meta: Option<AccountMeta>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// 内存中的登录态备份字节（调用方负责交给 SwitchBackupStore；勿 println）。
#[derive(Debug, Clone)]
pub struct LoginStateBlob {
    pub product: String,
    pub format: LoginFormat,
    pub source_paths: Vec<PathBuf>,
    pub account_meta: AccountMeta,
    /// 原始备份载荷（WB=整文件；Trae=含 auth 键的信封 JSON）
    pub plaintext: Vec<u8>,
}

pub trait PlatformAdapter: Send + Sync {
    fn product(&self) -> &'static str;
    fn detect_install(&self) -> DetectResult;
    fn export_login_state(&self) -> Result<LoginStateBlob, String>;
    fn import_login_state(&self, blob: &LoginStateBlob) -> Result<(), String>;
    fn health_check(&self) -> Result<(), String>;
    fn client_running(&self) -> bool;
}

pub fn product_adapter(platform: &str) -> Result<Box<dyn PlatformAdapter>, String> {
    match platform {
        "workbuddy" => Ok(Box::new(WorkBuddyAdapter)),
        "trae_cn" => Ok(Box::new(TraeCnAdapter)),
        other => Err(format!("unsupported product: {other}")),
    }
}

// ─── WorkBuddy ───────────────────────────────────────────────

pub struct WorkBuddyAdapter;

impl PlatformAdapter for WorkBuddyAdapter {
    fn product(&self) -> &'static str {
        "workbuddy"
    }

    fn detect_install(&self) -> DetectResult {
        let Some(path) = paths::workbuddy_auth_path() else {
            return DetectResult {
                product: "workbuddy".into(),
                hit: false,
                paths: vec![],
                format: None,
                account_meta: None,
                message: Some("path_candidate_non_windows".into()),
            };
        };
        if !path.is_file() {
            return DetectResult {
                product: "workbuddy".into(),
                hit: false,
                paths: vec![path_hint(&path)],
                format: None,
                account_meta: None,
                message: Some("auth_file_missing".into()),
            };
        }
        match fs::read(&path) {
            Ok(bytes) => {
                let format = detect_wb_format(&bytes);
                let meta = peek_wb_meta(&bytes);
                DetectResult {
                    product: "workbuddy".into(),
                    hit: true,
                    paths: vec![path_hint(&path)],
                    format: Some(format.as_str().into()),
                    account_meta: Some(meta),
                    message: None,
                }
            }
            Err(_) => DetectResult {
                product: "workbuddy".into(),
                hit: false,
                paths: vec![path_hint(&path)],
                format: None,
                account_meta: None,
                message: Some("auth_file_unreadable".into()),
            },
        }
    }

    fn export_login_state(&self) -> Result<LoginStateBlob, String> {
        let path = paths::workbuddy_auth_path()
            .ok_or_else(|| "workbuddy path unavailable on this OS".to_string())?;
        if !path.is_file() {
            return Err("workbuddy auth file missing".into());
        }
        let plaintext = fs::read(&path).map_err(|e| format!("read wb auth: {e}"))?;
        let format = detect_wb_format(&plaintext);
        let account_meta = peek_wb_meta(&plaintext);
        Ok(LoginStateBlob {
            product: "workbuddy".into(),
            format,
            source_paths: vec![path],
            account_meta,
            plaintext,
        })
    }

    fn import_login_state(&self, blob: &LoginStateBlob) -> Result<(), String> {
        if blob.product != "workbuddy" {
            return Err("blob product mismatch for WorkBuddy".into());
        }
        let path = paths::workbuddy_auth_path()
            .ok_or_else(|| "workbuddy path unavailable on this OS".to_string())?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("mkdir wb auth: {e}"))?;
        }
        atomic_write(&path, &blob.plaintext)?;
        Ok(())
    }

    fn health_check(&self) -> Result<(), String> {
        let d = self.detect_install();
        if d.hit {
            Ok(())
        } else {
            Err(d.message.unwrap_or_else(|| "workbuddy health failed".into()))
        }
    }

    fn client_running(&self) -> bool {
        process::client_running_for_product("workbuddy")
    }
}

fn detect_wb_format(bytes: &[u8]) -> LoginFormat {
    // 探测标记，不把正文当日志
    if bytes.windows(b"$wbEncrypted".len()).any(|w| w == b"$wbEncrypted") {
        LoginFormat::WbEncryptedInfo
    } else {
        LoginFormat::PlainJsonInfo
    }
}

/// 只抽 uid / nickname / domain；**永不**读取或返回 token 字段。
fn peek_wb_meta(bytes: &[u8]) -> AccountMeta {
    let Ok(v) = serde_json::from_slice::<Value>(bytes) else {
        return AccountMeta::default();
    };
    let mut meta = AccountMeta::default();
    if let Some(account) = v.get("account") {
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
    if let Some(auth) = v.get("auth") {
        meta.domain = auth
            .get("domain")
            .and_then(|x| x.as_str())
            .map(|s| s.to_string());
    }
    if meta.domain.is_none() {
        meta.domain = v
            .get("domain")
            .and_then(|x| x.as_str())
            .map(|s| s.to_string());
    }
    meta
}

// ─── Trae CN ─────────────────────────────────────────────────

pub struct TraeCnAdapter;

impl PlatformAdapter for TraeCnAdapter {
    fn product(&self) -> &'static str {
        "trae_cn"
    }

    fn detect_install(&self) -> DetectResult {
        let Some(path) = paths::trae_cn_storage_path() else {
            return DetectResult {
                product: "trae_cn".into(),
                hit: false,
                paths: vec![],
                format: None,
                account_meta: None,
                message: Some("path_candidate_non_windows".into()),
            };
        };
        if !path.is_file() {
            return DetectResult {
                product: "trae_cn".into(),
                hit: false,
                paths: vec![path_hint(&path)],
                format: None,
                account_meta: None,
                message: Some("storage_json_missing".into()),
            };
        }
        match fs::read_to_string(&path) {
            Ok(text) => match serde_json::from_str::<Value>(&text) {
                Ok(root) => {
                    let has = root
                        .get(TRAE_AUTH_KEY)
                        .map(|v| !v.is_null() && v != &Value::String(String::new()))
                        .unwrap_or(false);
                    DetectResult {
                        product: "trae_cn".into(),
                        hit: has,
                        paths: vec![path_hint(&path)],
                        format: Some(LoginFormat::TraeStorageKey.as_str().into()),
                        account_meta: None, // 密文不可 peek
                        message: if has {
                            None
                        } else {
                            Some("trae_auth_key_missing_or_empty".into())
                        },
                    }
                }
                Err(_) => DetectResult {
                    product: "trae_cn".into(),
                    hit: false,
                    paths: vec![path_hint(&path)],
                    format: None,
                    account_meta: None,
                    message: Some("storage_json_invalid".into()),
                },
            },
            Err(_) => DetectResult {
                product: "trae_cn".into(),
                hit: false,
                paths: vec![path_hint(&path)],
                format: None,
                account_meta: None,
                message: Some("storage_json_unreadable".into()),
            },
        }
    }

    fn export_login_state(&self) -> Result<LoginStateBlob, String> {
        let path = paths::trae_cn_storage_path()
            .ok_or_else(|| "trae_cn path unavailable on this OS".to_string())?;
        if !path.is_file() {
            return Err("trae storage.json missing".into());
        }
        let text = fs::read_to_string(&path).map_err(|e| format!("read trae storage: {e}"))?;
        let root: Value =
            serde_json::from_str(&text).map_err(|e| format!("parse trae storage: {e}"))?;
        let auth_value = root
            .get(TRAE_AUTH_KEY)
            .cloned()
            .ok_or_else(|| "trae auth key missing".to_string())?;
        if auth_value.is_null() {
            return Err("trae auth key empty".into());
        }
        // 信封：只存认证键值 + 格式标记；整文件不重复塞入以免误写其它键
        let envelope = serde_json::json!({
            "format": "TraeStorageKey",
            "auth_key": TRAE_AUTH_KEY,
            "auth_value": auth_value,
        });
        let plaintext = serde_json::to_vec(&envelope).map_err(|e| format!("serde envelope: {e}"))?;
        Ok(LoginStateBlob {
            product: "trae_cn".into(),
            format: LoginFormat::TraeStorageKey,
            source_paths: vec![path],
            account_meta: AccountMeta::default(),
            plaintext,
        })
    }

    fn import_login_state(&self, blob: &LoginStateBlob) -> Result<(), String> {
        if blob.product != "trae_cn" {
            return Err("blob product mismatch for Trae CN".into());
        }
        let path = paths::trae_cn_storage_path()
            .ok_or_else(|| "trae_cn path unavailable on this OS".to_string())?;
        let envelope: Value = serde_json::from_slice(&blob.plaintext)
            .map_err(|e| format!("parse trae backup envelope: {e}"))?;
        let auth_value = envelope
            .get("auth_value")
            .cloned()
            .ok_or_else(|| "trae backup missing auth_value".to_string())?;
        // $wbEncrypted 类密文：原样写回（auth_value 保持 JSON 值形态）
        let mut root = if path.is_file() {
            let text = fs::read_to_string(&path).map_err(|e| format!("read trae for import: {e}"))?;
            serde_json::from_str::<Value>(&text).map_err(|e| format!("parse trae for import: {e}"))?
        } else {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(|e| format!("mkdir trae: {e}"))?;
            }
            Value::Object(serde_json::Map::new())
        };
        let obj = root
            .as_object_mut()
            .ok_or_else(|| "trae storage.json root must be object".to_string())?;
        obj.insert(TRAE_AUTH_KEY.to_string(), auth_value);
        let out = serde_json::to_vec_pretty(&root).map_err(|e| format!("serialize trae: {e}"))?;
        atomic_write(&path, &out)?;
        Ok(())
    }

    fn health_check(&self) -> Result<(), String> {
        let d = self.detect_install();
        if d.hit {
            Ok(())
        } else {
            Err(d.message.unwrap_or_else(|| "trae_cn health failed".into()))
        }
    }

    fn client_running(&self) -> bool {
        process::client_running_for_product("trae_cn")
    }
}

// ─── 原子写 ──────────────────────────────────────────────────

fn atomic_write(path: &Path, data: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "target path has no parent".to_string())?;
    fs::create_dir_all(parent).map_err(|e| format!("mkdir for atomic write: {e}"))?;
    let tmp = parent.join(format!(
        ".{}.tmp-{}",
        path.file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("switch"),
        std::process::id()
    ));
    {
        let mut f = File::create(&tmp).map_err(|e| format!("create tmp: {e}"))?;
        f.write_all(data).map_err(|e| format!("write tmp: {e}"))?;
        f.sync_all().map_err(|e| format!("fsync tmp: {e}"))?;
    }
    // Windows 上 rename 覆盖已存在目标可能失败 → 先试 replace
    if path.exists() {
        let bak = parent.join(format!(
            ".{}.bak-{}",
            path.file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("switch"),
            std::process::id()
        ));
        let _ = fs::rename(path, &bak);
        match fs::rename(&tmp, path) {
            Ok(()) => {
                let _ = fs::remove_file(&bak);
                Ok(())
            }
            Err(e) => {
                let _ = fs::rename(&bak, path);
                let _ = fs::remove_file(&tmp);
                Err(format!("atomic rename: {e}"))
            }
        }
    } else {
        fs::rename(&tmp, path).map_err(|e| format!("atomic rename: {e}"))
    }
}
