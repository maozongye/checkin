//! WorkBuddyAdapter — dry_run + LiveHttp daily-checkin (Python对照).
//! v1 scope: daily-checkin only (no travel / makeup).
//! LiveHttp uses vault-resolved session from CheckinCtx.

use crate::adapter::PlatformAdapter;
use crate::codes::{self, WB_ALREADY, WB_OK, WB_SESSION_INVALID};
use crate::config::{DesktopPaths, WorkBuddyConfig};
use crate::http_client::{self, data_object, extract_code, pick_int, post_json};
use crate::probe;
use crate::types::{AdapterError, CheckinCtx, CheckinOutcome, HealthReport, PointsSnapshot};
use crate::vault_bridge::ResolvedSession;
use async_trait::async_trait;
use serde_json::{json, Value};

const GLOBAL_HOSTS: &[&str] = &["www.workbuddy.ai", "workbuddy.ai"];

pub struct WorkBuddyAdapter {
    pub config: WorkBuddyConfig,
    pub desktop: DesktopPaths,
}

impl WorkBuddyAdapter {
    pub fn new(config: WorkBuddyConfig, desktop: DesktopPaths) -> Self {
        Self { config, desktop }
    }

    pub fn with_defaults() -> Self {
        Self::new(WorkBuddyConfig::default(), DesktopPaths::default())
    }

    /// Resolve base URL from session domain / config. `None` = Global → Skipped.
    pub fn resolve_base(domain: Option<&str>, config: &WorkBuddyConfig) -> Option<String> {
        let domain = domain.unwrap_or("").trim().to_lowercase();
        if !domain.is_empty() {
            let host = domain
                .trim_start_matches("https://")
                .trim_start_matches("http://")
                .split('/')
                .next()
                .unwrap_or("");
            if GLOBAL_HOSTS.iter().any(|g| *g == host) {
                return None;
            }
            if host.contains("copilot.tencent.com") {
                return Some("https://copilot.tencent.com".into());
            }
            if host.contains("codebuddy.cn") {
                return Some("https://www.codebuddy.cn".into());
            }
            if domain.starts_with("http") {
                return Some(domain.trim_end_matches('/').to_string());
            }
        }
        Some(config.base_url.trim_end_matches('/').to_string())
    }
}

#[async_trait]
impl PlatformAdapter for WorkBuddyAdapter {
    fn platform(&self) -> &'static str {
        "workbuddy"
    }

    async fn checkin(&self, ctx: &CheckinCtx) -> Result<CheckinOutcome, AdapterError> {
        if ctx.dry_run {
            let _ = self
                .desktop
                .resolve_wb_auth()
                .map(|p| probe::probe_workbuddy_file(&p));
            return Ok(CheckinOutcome::Skipped {
                reason: "dry_run".into(),
            });
        }

        let session = match &ctx.session {
            Some(s) => s.as_ref(),
            None => {
                return Ok(CheckinOutcome::Failed {
                    code: None,
                    message: "vault_session_required".into(),
                    retryable: false,
                });
            }
        };

        let (mut access_token, refresh_token, domain, account_key) = match session {
            ResolvedSession::WorkBuddy {
                access_token,
                refresh_token,
                domain,
                account_key,
                ..
            } => (
                access_token.clone(),
                refresh_token.clone(),
                domain.clone(),
                account_key.clone(),
            ),
            _ => {
                return Ok(CheckinOutcome::Failed {
                    code: None,
                    message: "platform_session_mismatch".into(),
                    retryable: false,
                });
            }
        };

        let Some(base) = Self::resolve_base(domain.as_deref(), &self.config) else {
            return Ok(CheckinOutcome::Skipped {
                reason: "Global site skipped".into(),
            });
        };

        if access_token.is_empty() {
            return Ok(CheckinOutcome::Failed {
                code: None,
                message: "missing_access_token".into(),
                retryable: false,
            });
        }

        let client = http_client::build_client(self.config.force_direct)?;
        let uid = account_key;
        let url = format!("{}{}", base, self.config.checkin_path);

        let mut refreshed = false;
        loop {
            let headers = [
                ("Authorization", format!("Bearer {access_token}")),
                ("X-User-Id", uid.clone()),
                ("Content-Type", "application/json".into()),
            ];
            let (http_status, parsed) =
                match post_json(&client, &url, &headers, &json!({})).await {
                    Ok(v) => v,
                    Err(e) => {
                        let msg = e.to_string();
                        let looks_auth = msg.contains("401") || msg.contains("403");
                        if looks_auth
                            && !refreshed
                            && try_refresh(
                                &client,
                                &base,
                                &self.config.refresh_path,
                                &access_token,
                                &refresh_token,
                            )
                            .await
                            .map(|new| {
                                access_token = new;
                            })
                            .is_some()
                        {
                            refreshed = true;
                            continue;
                        }
                        return Ok(CheckinOutcome::Failed {
                            code: None,
                            message: format!("network: {e}"),
                            retryable: true,
                        });
                    }
                };

            let code = extract_code(&parsed).unwrap_or(-1);
            let data = data_object(&parsed);

            if code == WB_SESSION_INVALID {
                return Ok(codes::map_workbuddy_code(
                    code,
                    None,
                    None,
                    None,
                    "会话失效(12153)，账号已标记 disabled",
                ));
            }

            if code == WB_ALREADY || code == WB_OK {
                let streak = pick_int(data, "streak_days", None).map(|v| v as i32);
                let delta = pick_int(data, "today_credit", None)
                    .or_else(|| pick_int(data, "daily_credit", None));
                let balance = pick_int(data, "balance", None);
                let msg = if code == WB_OK {
                    "签到成功"
                } else {
                    "今日已签到，跳过"
                };
                return Ok(codes::map_workbuddy_code(code, delta, streak, balance, msg));
            }

            if (code == 401 || code == 403 || http_status == 401 || http_status == 403)
                && !refreshed
            {
                if let Some(new) = try_refresh(
                    &client,
                    &base,
                    &self.config.refresh_path,
                    &access_token,
                    &refresh_token,
                )
                .await
                {
                    access_token = new;
                    refreshed = true;
                    continue;
                }
                return Ok(CheckinOutcome::Failed {
                    code: Some(code),
                    message: "客户端未登录/Token 过期".into(),
                    retryable: false,
                });
            }

            let msg = parsed
                .get("message")
                .or_else(|| parsed.get("msg"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("业务失败 code={code}"));
            return Ok(codes::map_workbuddy_code(code, None, None, None, msg));
        }
    }

    async fn fetch_points(&self, ctx: &CheckinCtx) -> Result<PointsSnapshot, AdapterError> {
        if ctx.dry_run {
            return Ok(PointsSnapshot {
                balance: None,
                streak_days: None,
                today_checked_in: None,
                raw: json!({ "dry_run": true, "policy": "credits_delta_v1" }),
            });
        }
        // Best-effort: optional status endpoint when session present.
        let session = ctx.session.as_ref().map(|s| s.as_ref());
        let Some(ResolvedSession::WorkBuddy {
            access_token,
            domain,
            account_key,
            ..
        }) = session
        else {
            return Ok(PointsSnapshot {
                balance: None,
                streak_days: None,
                today_checked_in: None,
                raw: json!({ "error": "vault_session_required" }),
            });
        };
        let Some(base) = Self::resolve_base(domain.as_deref(), &self.config) else {
            return Ok(PointsSnapshot {
                balance: None,
                streak_days: None,
                today_checked_in: None,
                raw: json!({ "skipped": "global" }),
            });
        };
        let client = http_client::build_client(self.config.force_direct)?;
        let url = format!("{}{}", base, self.config.status_path);
        let headers = [
            ("Authorization", format!("Bearer {access_token}")),
            ("X-User-Id", account_key.clone()),
            ("Content-Type", "application/json".into()),
        ];
        match post_json(&client, &url, &headers, &json!({})).await {
            Ok((_st, parsed)) => {
                let data = data_object(&parsed);
                Ok(PointsSnapshot {
                    balance: pick_int(data, "balance", None),
                    streak_days: pick_int(data, "streak_days", None).map(|v| v as i32),
                    today_checked_in: data
                        .get("today_checked_in")
                        .and_then(|v| v.as_bool()),
                    raw: parsed,
                })
            }
            Err(_) => Ok(PointsSnapshot {
                balance: None,
                streak_days: None,
                today_checked_in: None,
                raw: json!({ "error": "status_fetch_failed" }),
            }),
        }
    }

    async fn health(&self, ctx: &CheckinCtx) -> Result<HealthReport, AdapterError> {
        let path_probe = if ctx.dry_run {
            match self.desktop.resolve_wb_auth() {
                Some(p) => Some(probe::probe_workbuddy_file(&p)),
                None => Some(probe::PathProbeSummary {
                    platform: "workbuddy".into(),
                    path: format!("%LOCALAPPDATA%/{}", self.desktop.wb_auth_relpath),
                    note: "LOCALAPPDATA_unset".into(),
                    ..Default::default()
                }),
            }
        } else {
            None
        };

        let auth_valid = if ctx.dry_run {
            path_probe
                .as_ref()
                .map(|p| p.has_access_token && p.exists)
                .unwrap_or(false)
        } else {
            matches!(
                ctx.session.as_ref().map(|s| s.as_ref()),
                Some(ResolvedSession::WorkBuddy { access_token, .. }) if !access_token.is_empty()
            )
        };

        Ok(HealthReport {
            ok: true,
            auth_valid,
            message: if ctx.dry_run {
                "dry_run path probe (no tokens in detail; DesktopPaths=import source only)".into()
            } else {
                "live: vault session present for HTTP".into()
            },
            detail: json!({
                "base_url": self.config.base_url,
                "alt_base_url": self.config.alt_base_url,
                "dry_run": ctx.dry_run,
                "wb_auth_relpath": self.desktop.wb_auth_relpath,
                "path_probe": path_probe,
                "scope": "daily-checkin-only",
                "cred_uri_is_cred": ctx.credentials_ref.as_cred_uri().is_some(),
                "has_session": ctx.session.is_some(),
            }),
        })
    }
}

async fn try_refresh(
    client: &reqwest::Client,
    base: &str,
    refresh_path: &str,
    access_token: &str,
    refresh_token: &str,
) -> Option<String> {
    if refresh_token.is_empty() {
        return None;
    }
    let url = format!("{base}{refresh_path}");
    let headers = [
        ("Content-Type", "application/json".into()),
        ("Authorization", format!("Bearer {access_token}")),
    ];
    let body = json!({
        "refreshToken": refresh_token,
        "refresh_token": refresh_token,
    });
    let (_st, parsed) = post_json(client, &url, &headers, &body).await.ok()?;
    let code = extract_code(&parsed);
    if let Some(c) = code {
        if c != WB_OK && c != 0 {
            return None;
        }
    }
    let data = data_object(&parsed);
    pick_str(data, &["accessToken", "access_token"])
        .or_else(|| pick_str(&parsed, &["accessToken", "access_token"]))
}

fn pick_str(v: &Value, keys: &[&str]) -> Option<String> {
    for k in keys {
        if let Some(s) = v.get(*k).and_then(|x| x.as_str()).filter(|s| !s.is_empty()) {
            return Some(s.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::CredentialsRef;
    use std::sync::Arc;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn live_ctx(session: ResolvedSession) -> CheckinCtx {
        CheckinCtx {
            account_id: "wb-1".into(),
            platform: "workbuddy".into(),
            credentials_ref: CredentialsRef::legacy("t", "k"),
            local_day: "2026-10-18".into(),
            timezone: "Asia/Shanghai".into(),
            dry_run: false,
            run_id: "run-1".into(),
            attempt: 1,
            session: Some(Arc::new(session)),
        }
    }

    fn wb_session(domain: &str) -> ResolvedSession {
        ResolvedSession::WorkBuddy {
            access_token: "tok_access_TEST_wb".into(),
            refresh_token: "tok_refresh_TEST_wb".into(),
            domain: Some(domain.into()),
            account_key: "uid-1".into(),
            expires_at: None,
        }
    }

    async fn adapter_at(base: &str) -> WorkBuddyAdapter {
        let mut config = WorkBuddyConfig::default();
        config.base_url = base.to_string();
        config.force_direct = true;
        WorkBuddyAdapter::new(config, DesktopPaths::default())
    }

    #[tokio::test]
    async fn wb_code_0_success() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v2/billing/meter/daily-checkin"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "code": 0,
                "data": { "today_credit": 10, "streak_days": 3, "balance": 100 }
            })))
            .mount(&server)
            .await;

        let adapter = adapter_at(&server.uri()).await;
        // domain empty → use config.base_url (mock)
        let mut session = wb_session("");
        if let ResolvedSession::WorkBuddy { domain, .. } = &mut session {
            *domain = None;
        }
        let out = adapter.checkin(&live_ctx(session)).await.unwrap();
        assert_eq!(out.kind_str(), "Success");
        assert_eq!(out.credits_delta(), Some(10));
        assert_eq!(out.streak_days(), Some(3));
        assert_eq!(out.raw_code(), Some(0));
        assert!(!format!("{out:?}").contains("tok_access_TEST"));
    }

    #[tokio::test]
    async fn wb_code_10001_already_done() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v2/billing/meter/daily-checkin"))
            .respond_with(ResponseTemplate::new(400).set_body_json(json!({
                "code": 10001,
                "message": "already",
                "data": { "streak_days": 2 }
            })))
            .mount(&server)
            .await;

        let adapter = adapter_at(&server.uri()).await;
        let mut session = wb_session("");
        if let ResolvedSession::WorkBuddy { domain, .. } = &mut session {
            *domain = None;
        }
        let out = adapter.checkin(&live_ctx(session)).await.unwrap();
        assert_eq!(out.kind_str(), "AlreadyDone");
        assert_eq!(out.raw_code(), Some(10001));
        assert!(out.credits_delta().is_none());
    }

    #[tokio::test]
    async fn wb_code_12153_disabled() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v2/billing/meter/daily-checkin"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "code": 12153,
                "message": "session invalid"
            })))
            .mount(&server)
            .await;

        let adapter = adapter_at(&server.uri()).await;
        let mut session = wb_session("");
        if let ResolvedSession::WorkBuddy { domain, .. } = &mut session {
            *domain = None;
        }
        let out = adapter.checkin(&live_ctx(session)).await.unwrap();
        assert_eq!(out.kind_str(), "Disabled");
    }

    #[tokio::test]
    async fn dry_run_never_posts() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"code": 0})))
            .expect(0)
            .mount(&server)
            .await;

        let adapter = adapter_at(&server.uri()).await;
        let mut ctx = live_ctx(wb_session(""));
        ctx.dry_run = true;
        ctx.session = None;
        let out = adapter.checkin(&ctx).await.unwrap();
        assert_eq!(out.kind_str(), "Skipped");
        assert_eq!(out.message(), "dry_run");
    }

    #[tokio::test]
    async fn missing_session_fails() {
        let adapter = WorkBuddyAdapter::with_defaults();
        let ctx = CheckinCtx {
            account_id: "x".into(),
            platform: "workbuddy".into(),
            credentials_ref: CredentialsRef::legacy("t", "k"),
            local_day: "2026-10-18".into(),
            timezone: "Asia/Shanghai".into(),
            dry_run: false,
            run_id: "r".into(),
            attempt: 1,
            session: None,
        };
        let out = adapter.checkin(&ctx).await.unwrap();
        assert_eq!(out.message(), "vault_session_required");
    }

    #[test]
    fn resolve_base_global_skips() {
        assert!(WorkBuddyAdapter::resolve_base(Some("workbuddy.ai"), &WorkBuddyConfig::default()).is_none());
        assert_eq!(
            WorkBuddyAdapter::resolve_base(Some("copilot.tencent.com"), &WorkBuddyConfig::default())
                .as_deref(),
            Some("https://copilot.tencent.com")
        );
    }
}
