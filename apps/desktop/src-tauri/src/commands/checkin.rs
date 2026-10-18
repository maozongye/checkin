//! checkin_run_all — 立即返回 jobId；真编排走 checkin-core Orchestrator。
//!
//! `allow_live_http` 门控（默认关）：关 → dry-run；开才 live。
//! 进度 emit `checkin:progress` / `checkin:done`，形状对齐 Frontend camelCase。
//! 凭据仅经已注入的 VaultState resolve（禁 IPC resolve）。

use super::mock_data::{mock_accounts, now_ms};
use super::state::{AppState, CredentialMeta};
use crate::vault::{VaultBackend, VaultState};
use checkin_core::{
    AccountSpec, CheckinDb, CheckinEvent, CheckinOrchestrator, CheckinRuntimeOptions,
    CredentialsRef, OrchestratorConfig, RuntimeMode, Trigger,
};
use credential_vault::CredentialRef;
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{command, AppHandle, Emitter, State};
use tokio::sync::Mutex;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckinRunAllResult {
    pub job_id: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CheckinProgressEvent {
    job_id: String,
    done: usize,
    total: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    current_account: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    status: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CheckinItemResult {
    account_id: String,
    display_name: String,
    platform: String,
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    points_delta: Option<i64>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CheckinDoneEvent {
    job_id: String,
    success: usize,
    failed: usize,
    skipped: usize,
    results: Vec<CheckinItemResult>,
}

/// 关 `allow_live_http` → dry-run；开才 live。显式 `dry_run=true` 仍强制 dry。
pub(crate) fn effective_dry_run(allow_live_http: Option<bool>, dry_run: Option<bool>) -> bool {
    let allow = allow_live_http.unwrap_or(false);
    let want_dry = dry_run.unwrap_or(false);
    !allow || want_dry
}

/// Core outcome / progress status → Frontend snake_case status.
pub(crate) fn map_status_to_frontend(core_status: &str) -> &'static str {
    match core_status {
        "Success" | "success" => "success",
        "AlreadyDone" | "already_done" => "already_done",
        "Skipped" | "skipped" => "skipped",
        "Disabled" | "disabled" => "skipped",
        "Failed" | "failed" => "failed",
        "running" => "running",
        _ => "failed",
    }
}

/// Adapter platform (`workbuddy` | `trae`) → Frontend (`workbuddy` | `trae_cn`).
fn platform_to_frontend(core_platform: &str) -> String {
    match core_platform {
        "trae" => "trae_cn".into(),
        other => other.to_string(),
    }
}

/// Vault / UI platform (`workbuddy` | `trae_cn`) → adapter key.
fn platform_to_adapter(ui_or_vault: &str) -> String {
    match ui_or_vault {
        "trae_cn" | "trae" => "trae".into(),
        other => other.to_string(),
    }
}

/// Infer UI platform from `cred://` or account_id heuristics.
fn infer_ui_platform(credential_ref: &str, account_id: &str) -> Option<String> {
    if let Ok(CredentialRef::Account { platform, .. }) = CredentialRef::parse(credential_ref) {
        return Some(platform.as_str().to_string());
    }
    if account_id.contains("trae") {
        return Some("trae_cn".into());
    }
    if account_id.contains("wb") || account_id.contains("workbuddy") {
        return Some("workbuddy".into());
    }
    // Path segment after cred://v1/account/
    if let Some(rest) = credential_ref.strip_prefix("cred://v1/account/") {
        let plat = rest.split('/').next().unwrap_or("");
        if plat == "workbuddy" || plat == "trae_cn" {
            return Some(plat.to_string());
        }
    }
    None
}

fn matches_platform_filter(ui_platform: &str, filter: &str) -> bool {
    filter == "all" || ui_platform == filter
}

/// Build AccountSpec list from credential meta; dry_run may fall back to mock ids.
fn build_account_specs(
    metas: &[CredentialMeta],
    filter: &str,
    dry: bool,
) -> (Vec<AccountSpec>, HashMap<String, String>) {
    let mut display_names: HashMap<String, String> = HashMap::new();
    let mut specs: Vec<AccountSpec> = Vec::new();

    if !metas.is_empty() {
        for m in metas {
            let ui_plat = match infer_ui_platform(&m.credential_ref, &m.account_id) {
                Some(p) => p,
                None => continue,
            };
            if !matches_platform_filter(&ui_plat, filter) {
                continue;
            }
            display_names.insert(m.account_id.clone(), m.label.clone());
            specs.push(AccountSpec {
                account_id: m.account_id.clone(),
                platform: platform_to_adapter(&ui_plat),
                credentials_ref: CredentialsRef::new(m.credential_ref.clone()),
                enabled: true,
            });
        }
        return (specs, display_names);
    }

    // Empty meta: live → empty run; dry → optional mock fallback (legacy refs, no resolve).
    if dry {
        for acc in mock_accounts()
            .into_iter()
            .filter(|a| a.status == "active")
            .filter(|a| matches_platform_filter(&a.platform, filter))
        {
            display_names.insert(acc.id.clone(), acc.display_name.clone());
            specs.push(AccountSpec {
                account_id: acc.id.clone(),
                platform: platform_to_adapter(&acc.platform),
                credentials_ref: CredentialsRef::legacy("mock", &acc.id),
                enabled: true,
            });
        }
    }

    (specs, display_names)
}

fn checkin_db_path(vault_dir: &std::path::Path) -> PathBuf {
    vault_dir
        .parent()
        .unwrap_or(vault_dir)
        .join("checkin")
        .join("checkin.db")
}

fn open_checkin_db(path: &std::path::Path) -> CheckinDb {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    CheckinDb::open(path).unwrap_or_else(|e| {
        eprintln!("checkin db open {path:?} failed ({e}); using in-memory");
        CheckinDb::open_in_memory().expect("in-memory checkin db")
    })
}

fn emit_core_event(
    app: &AppHandle,
    job_id: &str,
    display_names: &HashMap<String, String>,
    event: CheckinEvent,
) {
    match event {
        CheckinEvent::Progress(p) => {
            let status = map_status_to_frontend(&p.status);
            let done = if p.status == "running" {
                p.index
            } else {
                p.index.saturating_add(1)
            };
            let current = display_names
                .get(&p.account_id)
                .cloned()
                .unwrap_or_else(|| p.account_id.clone());
            let _ = app.emit(
                "checkin:progress",
                CheckinProgressEvent {
                    job_id: job_id.to_string(),
                    done,
                    total: p.total,
                    current_account: Some(current),
                    status: Some(status.into()),
                },
            );
        }
        CheckinEvent::Done(d) => {
            let results: Vec<CheckinItemResult> = d
                .items
                .iter()
                .map(|item| {
                    let display_name = display_names
                        .get(&item.account_id)
                        .cloned()
                        .unwrap_or_else(|| item.account_id.clone());
                    CheckinItemResult {
                        account_id: item.account_id.clone(),
                        display_name,
                        platform: platform_to_frontend(&item.platform),
                        status: map_status_to_frontend(&item.outcome).into(),
                        message: Some(item.message.clone()),
                        points_delta: item.credits_delta,
                    }
                })
                .collect();
            // Align prior mock: success count includes already_done for shell summary.
            let success = (d.counts.success + d.counts.already_done) as usize;
            let failed = d.counts.failed as usize;
            let skipped = (d.counts.skipped + d.counts.disabled) as usize;
            let _ = app.emit(
                "checkin:done",
                CheckinDoneEvent {
                    job_id: job_id.to_string(),
                    success,
                    failed,
                    skipped,
                    results,
                },
            );
        }
    }
}

/// 立即返回 jobId；后台 spawn 真 Orchestrator，emit `checkin:progress` / `checkin:done`。
#[command]
pub fn checkin_run_all(
    app: AppHandle,
    state: State<'_, AppState>,
    vault: State<'_, VaultState>,
    platform: Option<String>,
    allow_live_http: Option<bool>,
    dry_run: Option<bool>,
) -> Result<CheckinRunAllResult, String> {
    let job_id = format!("job-{}", now_ms());
    let filter = platform.unwrap_or_else(|| "all".into());
    let dry = effective_dry_run(allow_live_http, dry_run);
    let allow = allow_live_http.unwrap_or(false) && !dry;

    let metas = state
        .credentials
        .lock()
        .map_err(|_| "credential lock poisoned".to_string())?
        .clone();

    let (accounts, display_names) = build_account_specs(&metas, &filter, dry);

    let options = if dry {
        CheckinRuntimeOptions {
            mode: RuntimeMode::DryRun,
            allow_live_http: false,
        }
    } else {
        CheckinRuntimeOptions {
            mode: RuntimeMode::LiveHttp,
            allow_live_http: true,
        }
    };

    let db_path = checkin_db_path(&vault.vault_dir);
    let backend = vault.backend.clone();
    let job_id_spawn = job_id.clone();
    // Capture allow only to document live path; options already encode gate.
    let _allow_live = allow;

    tauri::async_runtime::spawn(async move {
        let db = Arc::new(Mutex::new(open_checkin_db(&db_path)));
        let orch = CheckinOrchestrator::with_default_adapters(OrchestratorConfig::default());

        let app_emit = app.clone();
        let jid = job_id_spawn.clone();
        let names = display_names.clone();
        let emit = move |ev: CheckinEvent| {
            emit_core_event(&app_emit, &jid, &names, ev);
        };

        let result = match backend.as_ref() {
            VaultBackend::Os(store) => {
                let vault_arg = if dry { None } else { Some(store) };
                orch.run_all(
                    db,
                    &accounts,
                    Trigger::Manual,
                    dry,
                    options,
                    vault_arg,
                    emit,
                )
                .await
            }
            VaultBackend::Mem(store) => {
                let vault_arg = if dry { None } else { Some(store) };
                orch.run_all(
                    db,
                    &accounts,
                    Trigger::Manual,
                    dry,
                    options,
                    vault_arg,
                    emit,
                )
                .await
            }
        };

        if let Err(e) = result {
            eprintln!("checkin orchestrator error: {e}");
            // Ensure Frontend leaves "running" even on fatal orchestrator error.
            let _ = app.emit(
                "checkin:done",
                CheckinDoneEvent {
                    job_id: job_id_spawn,
                    success: 0,
                    failed: 1,
                    skipped: 0,
                    results: vec![CheckinItemResult {
                        account_id: "_orchestrator".into(),
                        display_name: "编排".into(),
                        platform: "workbuddy".into(),
                        status: "failed".into(),
                        message: Some(e.to_string()),
                        points_delta: None,
                    }],
                },
            );
        }
    });

    Ok(CheckinRunAllResult { job_id })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effective_dry_run_defaults_closed() {
        assert!(effective_dry_run(None, None));
        assert!(effective_dry_run(Some(false), None));
        assert!(effective_dry_run(Some(false), Some(false)));
        assert!(effective_dry_run(Some(true), Some(true)));
        assert!(!effective_dry_run(Some(true), Some(false)));
        assert!(!effective_dry_run(Some(true), None));
    }

    #[test]
    fn map_status_covers_core_outcomes() {
        assert_eq!(map_status_to_frontend("Success"), "success");
        assert_eq!(map_status_to_frontend("AlreadyDone"), "already_done");
        assert_eq!(map_status_to_frontend("Skipped"), "skipped");
        assert_eq!(map_status_to_frontend("Disabled"), "skipped");
        assert_eq!(map_status_to_frontend("Failed"), "failed");
        assert_eq!(map_status_to_frontend("running"), "running");
    }

    #[test]
    fn platform_adapter_roundtrip() {
        assert_eq!(platform_to_adapter("trae_cn"), "trae");
        assert_eq!(platform_to_frontend("trae"), "trae_cn");
        assert_eq!(platform_to_adapter("workbuddy"), "workbuddy");
        assert_eq!(platform_to_frontend("workbuddy"), "workbuddy");
    }

    #[test]
    fn empty_meta_live_yields_no_accounts() {
        let (specs, _) = build_account_specs(&[], "all", false);
        assert!(specs.is_empty());
    }

    #[test]
    fn empty_meta_dry_uses_mock_fallback() {
        let (specs, names) = build_account_specs(&[], "workbuddy", true);
        assert!(!specs.is_empty());
        assert!(specs.iter().all(|s| s.platform == "workbuddy"));
        assert!(!names.is_empty());
    }

    #[test]
    fn meta_filters_and_maps_trae() {
        let metas = vec![CredentialMeta {
            credential_ref: "cred://v1/account/trae_cn/u1/aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee"
                .into(),
            label: "Trae · 主号".into(),
            account_id: "acc-trae-1".into(),
        }];
        let (specs, names) = build_account_specs(&metas, "trae_cn", false);
        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0].platform, "trae");
        assert_eq!(names.get("acc-trae-1").map(String::as_str), Some("Trae · 主号"));
    }
}
