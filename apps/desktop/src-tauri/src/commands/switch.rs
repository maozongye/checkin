//! 换号 Tauri commands（对接 Frontend SWITCHER.md）。
//!
//! 多号目标：从账号凭据仓 `resolve(cred://v1/account/...)` 取明文 → Adapter 写官方路径。
//! 备份走 `put_switch_backup`；失败 `get_switch_backup_seal` 回滚。
//! Commit：emit `switch://committed`（不写 `cred://v1/account/...`）。
//!
//! Mock 失败场景（acc-wb-2 / acc-trae-2）仅在 `SWITCHER_MOCK_SCENARIOS=1`
//! 或 debug_assertions 下启用。

use serde::{Deserialize, Serialize};
use std::thread;
use std::time::Duration;
use tauri::{command, AppHandle, Emitter, Manager, State};

use super::mock_data::{mock_accounts, now_ms, Account};
use super::state::AppState;
use crate::switcher::backup::SwitchBackupStore;
use crate::switcher::db::{LoginSnapshotRow, SwitchLogRow};
use crate::switcher::material::blob_from_account_handle;
use crate::switcher::paths::path_hint;
use crate::switcher::{adapter_for, normalize_product, SwitcherState};
use crate::vault::VaultState;

const EVENT_PROGRESS: &str = "switch://progress";
const EVENT_COMMITTED: &str = "switch://committed";
const CURRENT_ACCOUNT_ID: &str = "acc-wb-1";
const ERR_TARGET_CRED_MISSING: &str = "target_credential_missing";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SwitchPrecheckResult {
    pub target_account_id: String,
    pub target_display_name: String,
    pub provider: String,
    pub client_running: bool,
    pub can_proceed: bool,
    pub warnings: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_account_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SwitchConfirmExecResult {
    pub job_id: String,
    pub message_key: String,
    pub ok: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SwitchProgressEvent {
    pub job_id: String,
    pub phase: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub percent: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SwitchRollbackRetryResult {
    pub ok: bool,
    pub message_key: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SwitchCommittedEvent {
    account_id: String,
    product: String,
    snapshot_id: String,
}

fn find_account(account_id: &str) -> Result<Account, String> {
    mock_accounts()
        .into_iter()
        .find(|a| a.id == account_id)
        .ok_or_else(|| "账号不存在".into())
}

fn mock_scenarios_enabled() -> bool {
    if std::env::var("SWITCHER_MOCK_SCENARIOS")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
    {
        return true;
    }
    cfg!(debug_assertions)
}

fn resolve_client_running(product: &str, account_id: &str, force_running: bool) -> bool {
    if force_running {
        return true;
    }
    if crate::switcher::client_running_for_product(product) {
        return true;
    }
    if mock_scenarios_enabled() && account_id == "acc-wb-2" {
        return true;
    }
    false
}

fn new_snapshot_id() -> String {
    format!("snap-{}-{:x}", now_ms(), std::process::id())
}

fn new_job_id() -> String {
    format!("switch-{}", now_ms())
}

/// 从 AppState.credentials 取账号 `cred://v1/account/...`；否则 `target_credential_missing`。
fn lookup_account_credential_ref(app_state: &AppState, account_id: &str) -> Result<String, String> {
    let store = app_state
        .credentials
        .lock()
        .map_err(|_| "credential lock poisoned".to_string())?;
    let meta = store
        .iter()
        .find(|c| c.account_id == account_id)
        .ok_or_else(|| ERR_TARGET_CRED_MISSING.to_string())?;
    if !meta.credential_ref.starts_with("cred://v1/account/") {
        return Err(ERR_TARGET_CRED_MISSING.into());
    }
    Ok(meta.credential_ref.clone())
}

fn has_account_credential_ref(app_state: &AppState, account_id: &str) -> bool {
    lookup_account_credential_ref(app_state, account_id).is_ok()
}

#[command]
pub fn switch_list() -> Result<Vec<Account>, String> {
    Ok(mock_accounts()
        .into_iter()
        .filter(|a| a.status != "disabled")
        .collect())
}

/// 预检：不改盘。无目标账号凭据 → `target_credential_missing`（mock 场景除外）。
#[command]
pub fn switch_precheck(
    account_id: String,
    force_running: Option<bool>,
    product: Option<String>,
    switcher: State<'_, SwitcherState>,
    app_state: State<'_, AppState>,
) -> Result<SwitchPrecheckResult, String> {
    let _ = &switcher;
    let target = find_account(&account_id)?;
    let platform = normalize_product(
        product
            .as_deref()
            .unwrap_or(target.platform.as_str()),
    )?;
    let adapter = adapter_for(&platform)?;
    let detect = adapter.detect_install();
    let client_running =
        resolve_client_running(&platform, &account_id, force_running.unwrap_or(false));

    let mut warnings: Vec<String> = Vec::new();
    if platform == "trae_cn" {
        warnings.push("trae_key_only".into());
    }
    if !detect.hit {
        warnings.push(format!(
            "detect_miss:{}",
            detect.message.as_deref().unwrap_or("unknown")
        ));
    } else {
        warnings.push(format!(
            "detect_hit:{}",
            detect.format.as_deref().unwrap_or("?")
        ));
    }

    let cred_missing = !has_account_credential_ref(&app_state, &account_id);
    // mock 场景保持原行为：不因缺凭据阻断预检 UI
    let block_missing_cred = cred_missing && !mock_scenarios_enabled();
    if cred_missing {
        warnings.push(ERR_TARGET_CRED_MISSING.into());
    }

    let need_relogin = target.status == "expired" || target.status == "error";
    let can_proceed = !client_running && !need_relogin && !block_missing_cred;
    let message_key = if block_missing_cred {
        ERR_TARGET_CRED_MISSING
    } else if need_relogin {
        "need_relogin"
    } else if client_running {
        "client_running"
    } else {
        "precheck_ok"
    };

    Ok(SwitchPrecheckResult {
        target_account_id: target.id,
        target_display_name: target.display_name,
        provider: platform,
        client_running,
        can_proceed,
        warnings,
        message_key: Some(message_key.into()),
        current_account_id: Some(CURRENT_ACCOUNT_ID.into()),
    })
}

/// 确认执行。`product` 缺省时从 mock account.platform 推断。
#[command]
pub fn switch_confirm_exec(
    app: AppHandle,
    account_id: String,
    product: Option<String>,
) -> Result<SwitchConfirmExecResult, String> {
    let target = find_account(&account_id)?;
    let platform = normalize_product(product.as_deref().unwrap_or(target.platform.as_str()))?;
    let adapter = adapter_for(&platform)?;

    if adapter.client_running() {
        return Err("client_running".into());
    }

    let job_id = new_job_id();
    let job_id_clone = job_id.clone();
    let account_id_clone = account_id.clone();
    let platform_clone = platform.clone();

    thread::spawn(move || {
        if mock_scenarios_enabled()
            && (account_id_clone == "acc-trae-2" || account_id_clone == "acc-wb-2")
        {
            orchestrate_switch_mock(&app, &job_id_clone, &account_id_clone);
            return;
        }
        orchestrate_switch_real(&app, &job_id_clone, &account_id_clone, &platform_clone);
    });

    Ok(SwitchConfirmExecResult {
        job_id,
        message_key: "in_progress".into(),
        ok: true,
    })
}

#[command]
pub fn switch_rollback_retry(
    app: AppHandle,
    job_id: String,
) -> Result<SwitchRollbackRetryResult, String> {
    let state = app
        .try_state::<SwitcherState>()
        .ok_or_else(|| "SwitcherState not initialized".to_string())?;

    if mock_scenarios_enabled() && job_id.contains("mock") {
        return Ok(SwitchRollbackRetryResult {
            ok: true,
            message_key: "rolled_back".into(),
        });
    }

    let log = {
        let db = state
            .db
            .lock()
            .map_err(|_| "switcher db lock poisoned".to_string())?;
        db.get_log(&job_id)?
            .ok_or_else(|| "switch_log not found".to_string())?
    };

    let snap_id = log
        .backup_snapshot_id
        .ok_or_else(|| "no backup_snapshot_id on log".to_string())?;
    let snap = {
        let db = state
            .db
            .lock()
            .map_err(|_| "switcher db lock poisoned".to_string())?;
        db.get_snapshot(&snap_id)?
            .ok_or_else(|| "login_snapshot not found".to_string())?
    };

    let plaintext = state
        .backup
        .get_switch_backup(&snap.credential_ref)
        .map_err(|_| "get_switch_backup failed".to_string())?;

    let adapter = adapter_for(&log.product)?;
    let account_meta = snap
        .account_meta_json
        .as_ref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_default();
    let format = match snap.format.as_str() {
        "WbEncryptedInfo" => crate::switcher::LoginFormat::WbEncryptedInfo,
        "TraeStorageKey" => crate::switcher::LoginFormat::TraeStorageKey,
        _ => crate::switcher::LoginFormat::PlainJsonInfo,
    };
    let blob = crate::switcher::LoginStateBlob {
        product: log.product.clone(),
        format,
        source_paths: vec![],
        account_meta,
        plaintext,
    };

    match adapter.import_login_state(&blob) {
        Ok(()) => {
            let _ = state.db.lock().map(|db| {
                db.update_log_status(
                    &job_id,
                    "rolled_back",
                    None,
                    Some("rollback_retry ok"),
                    Some(now_ms()),
                )
            });
            Ok(SwitchRollbackRetryResult {
                ok: true,
                message_key: "rolled_back".into(),
            })
        }
        Err(e) => {
            let _ = state.db.lock().map(|db| {
                db.update_log_status(
                    &job_id,
                    "rollback_failed",
                    Some("rollback"),
                    Some(&e),
                    Some(now_ms()),
                )
            });
            Ok(SwitchRollbackRetryResult {
                ok: false,
                message_key: "rollback_failed".into(),
            })
        }
    }
}

fn emit_progress(app: &AppHandle, evt: SwitchProgressEvent) {
    let _ = app.emit(EVENT_PROGRESS, evt);
}

fn emit_committed(app: &AppHandle, account_id: &str, product: &str, snapshot_id: &str) {
    let _ = app.emit(
        EVENT_COMMITTED,
        SwitchCommittedEvent {
            account_id: account_id.into(),
            product: product.into(),
            snapshot_id: snapshot_id.into(),
        },
    );
}

/// 真路径：R5 → resolve 目标 → backup 当前 → import 目标 → health → commit|rollback。
fn orchestrate_switch_real(app: &AppHandle, job_id: &str, account_id: &str, platform: &str) {
    let Some(state) = app.try_state::<SwitcherState>() else {
        emit_done_key(app, job_id, "failed", "SwitcherState missing");
        return;
    };
    let Some(vault) = app.try_state::<VaultState>() else {
        emit_done_key(app, job_id, "failed", "VaultState missing");
        return;
    };
    let Some(app_state) = app.try_state::<AppState>() else {
        emit_done_key(app, job_id, "failed", "AppState missing");
        return;
    };

    let adapter = match adapter_for(platform) {
        Ok(a) => a,
        Err(e) => {
            emit_done_key(app, job_id, "failed", &e);
            return;
        }
    };

    // R5
    if adapter.client_running() {
        emit_done_key(app, job_id, "client_running", "blocked by R5");
        return;
    }

    let created_at = now_ms();
    if let Ok(db) = state.db.lock() {
        let _ = db.insert_log(&SwitchLogRow {
            id: job_id.into(),
            product: platform.into(),
            from_account_id: Some(CURRENT_ACCOUNT_ID.into()),
            to_account_id: account_id.into(),
            status: "in_progress".into(),
            fail_stage: None,
            backup_snapshot_id: None,
            detail: Some("started".into()),
            created_at,
            finished_at: None,
        });
    }

    // ── 解析目标凭据（account resolve，非 switch_backup）──
    let account_cred_ref = match lookup_account_credential_ref(&app_state, account_id) {
        Ok(r) => r,
        Err(_) => {
            finish_log(
                &state,
                job_id,
                "failed",
                Some("precheck"),
                Some(ERR_TARGET_CRED_MISSING),
            );
            emit_done_key(app, job_id, ERR_TARGET_CRED_MISSING, ERR_TARGET_CRED_MISSING);
            return;
        }
    };

    let target_blob = {
        let handle = match vault.backend.resolve_str(&account_cred_ref) {
            Ok(h) => h,
            Err(credential_vault::ResolveError::NotFound)
            | Err(credential_vault::ResolveError::InvalidRef(_)) => {
                finish_log(
                    &state,
                    job_id,
                    "failed",
                    Some("precheck"),
                    Some(ERR_TARGET_CRED_MISSING),
                );
                emit_done_key(app, job_id, ERR_TARGET_CRED_MISSING, ERR_TARGET_CRED_MISSING);
                return;
            }
            Err(_) => {
                finish_log(
                    &state,
                    job_id,
                    "failed",
                    Some("precheck"),
                    Some(ERR_TARGET_CRED_MISSING),
                );
                emit_done_key(app, job_id, ERR_TARGET_CRED_MISSING, "resolve failed");
                return;
            }
        };
        match blob_from_account_handle(platform, &handle) {
            Ok(b) => b,
            Err(e) => {
                let key = if e == ERR_TARGET_CRED_MISSING {
                    ERR_TARGET_CRED_MISSING
                } else {
                    "failed"
                };
                finish_log(&state, job_id, "failed", Some("precheck"), Some(key));
                emit_done_key(app, job_id, key, key);
                return;
            }
        }
        // handle dropped here — tokens not retained beyond target_blob.plaintext
    };

    // ── Backup 当前官方态 ──
    emit_progress(
        app,
        SwitchProgressEvent {
            job_id: job_id.into(),
            phase: "backup".into(),
            percent: Some(25),
            message_key: Some("in_progress".into()),
            detail: Some("export_login_state".into()),
        },
    );

    let backup_blob = match adapter.export_login_state() {
        Ok(b) => b,
        Err(e) => {
            finish_log(&state, job_id, "failed", Some("backup"), Some(&e));
            emit_done_key(app, job_id, "failed", &e);
            return;
        }
    };

    let snapshot_id = new_snapshot_id();
    let backup_cred_ref = match state.backup.put_switch_backup(
        platform,
        &snapshot_id,
        &backup_blob.plaintext,
    ) {
        Ok(r) => r,
        Err(e) => {
            finish_log(&state, job_id, "failed", Some("backup"), Some(&e));
            emit_done_key(app, job_id, "failed", &e);
            return;
        }
    };

    let meta_json = serde_json::to_string(&backup_blob.account_meta).ok();
    let paths_json = serde_json::to_string(
        &backup_blob
            .source_paths
            .iter()
            .map(|p| path_hint(p))
            .collect::<Vec<_>>(),
    )
    .unwrap_or_else(|_| "[]".into());
    let account_key = backup_blob.account_meta.uid.clone();

    if let Ok(db) = state.db.lock() {
        let _ = db.insert_snapshot(&LoginSnapshotRow {
            snapshot_id: snapshot_id.clone(),
            product: platform.into(),
            account_key,
            format: backup_blob.format.as_str().into(),
            source_paths_json: paths_json,
            credential_ref: backup_cred_ref.clone(),
            account_meta_json: meta_json,
            created_at: now_ms(),
            note: Some("pre_switch_backup".into()),
        });
        let _ = db.update_log_status(job_id, "in_progress", None, Some("backup_ok"), None);
        let _ = db.update_log_backup(job_id, &snapshot_id);
    }

    // ── write：import 目标明文 ──
    emit_progress(
        app,
        SwitchProgressEvent {
            job_id: job_id.into(),
            phase: "write".into(),
            percent: Some(55),
            message_key: Some("in_progress".into()),
            detail: Some("import_target".into()),
        },
    );

    if let Err(e) = adapter.import_login_state(&target_blob) {
        rollback_from_seal(
            app,
            &state,
            adapter.as_ref(),
            job_id,
            platform,
            &backup_cred_ref,
            &backup_blob,
            "write",
            &e,
        );
        return;
    }

    // ── verify ──
    emit_progress(
        app,
        SwitchProgressEvent {
            job_id: job_id.into(),
            phase: "verify".into(),
            percent: Some(80),
            message_key: Some("in_progress".into()),
            detail: Some("health_check".into()),
        },
    );
    if let Err(e) = adapter.health_check() {
        rollback_from_seal(
            app,
            &state,
            adapter.as_ref(),
            job_id,
            platform,
            &backup_cred_ref,
            &backup_blob,
            "verify",
            &e,
        );
        return;
    }

    // ── Commit ──
    finish_log(&state, job_id, "success", None, Some("switched"));
    let _ = state.backup.delete_switch_backup(&backup_cred_ref);

    emit_committed(app, account_id, platform, &snapshot_id);

    emit_progress(
        app,
        SwitchProgressEvent {
            job_id: job_id.into(),
            phase: "done".into(),
            percent: Some(100),
            message_key: Some("success".into()),
            detail: Some("ok".into()),
        },
    );
}

/// 失败回滚：优先 `get_switch_backup_seal`；失败则用内存 backup_blob。
fn rollback_from_seal(
    app: &AppHandle,
    state: &SwitcherState,
    adapter: &dyn crate::switcher::PlatformAdapter,
    job_id: &str,
    platform: &str,
    backup_cred_ref: &str,
    memory_fallback: &crate::switcher::LoginStateBlob,
    fail_stage: &str,
    _err: &str,
) {
    emit_progress(
        app,
        SwitchProgressEvent {
            job_id: job_id.into(),
            phase: "rollback".into(),
            percent: Some(85),
            message_key: Some("in_progress".into()),
            detail: Some(format!("{fail_stage}_rollback")),
        },
    );

    let restore = match state.backup.get_switch_backup(backup_cred_ref) {
        Ok(bytes) => crate::switcher::LoginStateBlob {
            product: platform.into(),
            format: memory_fallback.format,
            source_paths: memory_fallback.source_paths.clone(),
            account_meta: memory_fallback.account_meta.clone(),
            plaintext: bytes,
        },
        Err(_) => memory_fallback.clone(),
    };

    match adapter.import_login_state(&restore) {
        Ok(()) => {
            finish_log(
                state,
                job_id,
                "rolled_back",
                Some(fail_stage),
                Some("restored from switch_backup seal"),
            );
            emit_done_key(app, job_id, "rolled_back", "rolled back");
        }
        Err(_) => {
            finish_log(
                state,
                job_id,
                "rollback_failed",
                Some("rollback"),
                Some("seal restore failed"),
            );
            emit_done_key(app, job_id, "rollback_failed", "rollback failed");
        }
    }
}

fn finish_log(
    state: &SwitcherState,
    job_id: &str,
    status: &str,
    fail_stage: Option<&str>,
    detail: Option<&str>,
) {
    if let Ok(db) = state.db.lock() {
        let _ = db.update_log_status(job_id, status, fail_stage, detail, Some(now_ms()));
    }
}

fn emit_done_key(app: &AppHandle, job_id: &str, message_key: &str, detail: &str) {
    emit_progress(
        app,
        SwitchProgressEvent {
            job_id: job_id.into(),
            phase: "done".into(),
            percent: Some(100),
            message_key: Some(message_key.into()),
            detail: Some(detail.into()),
        },
    );
}

fn orchestrate_switch_mock(app: &AppHandle, job_id: &str, account_id: &str) {
    let fail_hard = account_id == "acc-trae-2";
    let fail_soft = account_id == "acc-wb-2";
    let phases = ["backup", "write", "verify"];

    for (i, phase) in phases.iter().enumerate() {
        thread::sleep(Duration::from_millis(380));
        let percent = (((i + 1) as f32) / ((phases.len() + 1) as f32) * 100.0).round() as u32;
        emit_progress(
            app,
            SwitchProgressEvent {
                job_id: job_id.into(),
                phase: (*phase).into(),
                percent: Some(percent),
                message_key: Some("in_progress".into()),
                detail: Some((*phase).into()),
            },
        );

        if fail_soft && *phase == "write" {
            thread::sleep(Duration::from_millis(300));
            emit_progress(
                app,
                SwitchProgressEvent {
                    job_id: job_id.into(),
                    phase: "rollback".into(),
                    percent: Some(85),
                    message_key: Some("in_progress".into()),
                    detail: Some("write failed, rolling back".into()),
                },
            );
            thread::sleep(Duration::from_millis(350));
            emit_done_key(app, job_id, "rolled_back", "rolled back");
            return;
        }

        if fail_hard && *phase == "verify" {
            thread::sleep(Duration::from_millis(300));
            emit_progress(
                app,
                SwitchProgressEvent {
                    job_id: job_id.into(),
                    phase: "rollback".into(),
                    percent: Some(90),
                    message_key: Some("in_progress".into()),
                    detail: Some("verify failed, rollback attempting".into()),
                },
            );
            thread::sleep(Duration::from_millis(400));
            emit_done_key(app, job_id, "rollback_failed", "rollback failed");
            return;
        }
    }

    thread::sleep(Duration::from_millis(280));
    emit_done_key(app, job_id, "success", "ok");
}
