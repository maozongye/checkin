//! TraeAdapter — dry_run + LiveHttp Trae CN claim (Python对照).
//! v1 scope: Trae CN daily claim only; pair real telemetry.devDeviceId.
//! LiveHttp requires vault JWT (`access_token`); cipher-only → `trae_jwt_unavailable`.

use crate::adapter::PlatformAdapter;
use crate::codes::{self, TRAE_ALREADY, TRAE_BAD_DEVICE, TRAE_OK, TRAE_RATE_LIMIT};
use crate::config::{DesktopPaths, TraeConfig, TRAE_CLIENT_ID_LEGACY_CANDIDATE};
use crate::http_client::{self, data_object, extract_code, pick_int, post_json};
use crate::probe;
use crate::types::{AdapterError, CheckinCtx, CheckinOutcome, HealthReport, PointsSnapshot};
use crate::vault_bridge::ResolvedSession;
use async_trait::async_trait;
use serde_json::{json, Value};

pub struct TraeAdapter {
    pub config: TraeConfig,
    pub desktop: DesktopPaths,
}

impl TraeAdapter {
    pub fn new(config: TraeConfig, desktop: DesktopPaths) -> Self {
        Self { config, desktop }
    }

    pub fn with_defaults() -> Self {
        Self::new(TraeConfig::default(), DesktopPaths::default())
    }

    fn auth_header(token: &str) -> String {
        if token.starts_with("Cloud-IDE-JWT ") {
            token.to_string()
        } else {
            format!("Cloud-IDE-JWT {token}")
        }
    }
}

#[async_trait]
impl PlatformAdapter for TraeAdapter {
    fn platform(&self) -> &'static str {
        "trae"
    }

    async fn checkin(&self, ctx: &CheckinCtx) -> Result<CheckinOutcome, AdapterError> {
        if ctx.dry_run {
            let _ = self
                .desktop
                .resolve_trae_storage()
                .map(|p| probe::probe_trae_storage_file(&p, &self.desktop));
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

        let (mut access_token, mut refresh_token, device_id, needs_desktop) = match session {
            ResolvedSession::Trae {
                access_token,
                refresh_token,
                device_id,
                needs_desktop_session,
                ..
            } => (
                access_token.clone(),
                refresh_token.clone(),
                device_id.clone(),
                *needs_desktop_session,
            ),
            _ => {
                return Ok(CheckinOutcome::Failed {
                    code: None,
                    message: "platform_session_mismatch".into(),
                    retryable: false,
                });
            }
        };

        if needs_desktop || access_token.is_empty() {
            return Ok(CheckinOutcome::Failed {
                code: None,
                message: "trae_jwt_unavailable".into(),
                retryable: false,
            });
        }

        if device_id.is_empty() {
            return Ok(CheckinOutcome::Failed {
                code: None,
                message: "missing_device_id".into(),
                retryable: false,
            });
        }

        let client = http_client::build_client(self.config.force_direct)?;
        let ug = self.config.ug_host.trim_end_matches('/');
        let url = format!("{ug}{}", self.config.claim_path);
        let body = json!({ "req_source": self.config.req_source });

        let mut refreshed = false;
        let mut client_id = self.config.client_id.clone();

        loop {
            let headers = [
                ("Authorization", Self::auth_header(&access_token)),
                ("Content-Type", "application/json".into()),
                ("x-device-type", "windows".into()),
                ("x-device-id", device_id.clone()),
            ];

            let (http_status, parsed) = match post_json(&client, &url, &headers, &body).await {
                Ok(v) => v,
                Err(e) => {
                    return Ok(CheckinOutcome::Failed {
                        code: None,
                        message: format!("network: {e}"),
                        retryable: true,
                    });
                }
            };

            let code = extract_code(&parsed).unwrap_or(-1);
            let data = data_object(&parsed);

            if code == TRAE_ALREADY {
                let streak = pick_int(data, "streak_days", None)
                    .or_else(|| pick_int(data, "continuous_days", None))
                    .map(|v| v as i32);
                let balance = pick_int(data, "balance", None).or_else(|| pick_int(data, "credits", None));
                return Ok(codes::map_trae_code(
                    code,
                    None,
                    streak,
                    balance,
                    "今日已签到，跳过",
                ));
            }

            if code == TRAE_OK {
                let delta = pick_int(data, "credits", None)
                    .or_else(|| pick_int(data, "credit", None))
                    .or_else(|| pick_int(data, "amount", None))
                    .or_else(|| pick_int(data, "checkin_credits", None));
                let streak = pick_int(data, "streak_days", None)
                    .or_else(|| pick_int(data, "continuous_days", None))
                    .map(|v| v as i32);
                return Ok(codes::map_trae_code(code, delta, streak, None, "签到成功"));
            }

            if code == TRAE_BAD_DEVICE {
                return Ok(codes::map_trae_code(
                    code,
                    None,
                    None,
                    None,
                    "设备号无效(9004)：请使用客户端真实注册过的 x-device-id",
                ));
            }

            if code == TRAE_RATE_LIMIT {
                return Ok(codes::map_trae_code(
                    code,
                    None,
                    None,
                    None,
                    "限流(9074)：请稍后重试或确保强制直连、账号错峰",
                ));
            }

            let auth_fail = matches!(code, 1001 | 1002 | 401 | 403)
                || http_status == 401
                || http_status == 403;
            if auth_fail && !refreshed {
                if let Some((new_a, new_r)) = try_exchange(
                    &client,
                    &self.config,
                    &client_id,
                    &refresh_token,
                )
                .await
                {
                    access_token = new_a;
                    if let Some(r) = new_r {
                        refresh_token = r;
                    }
                    refreshed = true;
                    continue;
                }
                // one legacy ClientID retry path inside try_exchange already;
                // if still fail and client_id isn't legacy, flip once.
                if client_id != TRAE_CLIENT_ID_LEGACY_CANDIDATE {
                    client_id = TRAE_CLIENT_ID_LEGACY_CANDIDATE.to_string();
                    if let Some((new_a, new_r)) = try_exchange(
                        &client,
                        &self.config,
                        &client_id,
                        &refresh_token,
                    )
                    .await
                    {
                        access_token = new_a;
                        if let Some(r) = new_r {
                            refresh_token = r;
                        }
                        refreshed = true;
                        continue;
                    }
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
            return Ok(codes::map_trae_code(code, None, None, None, msg));
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
        Ok(PointsSnapshot {
            balance: None,
            streak_days: None,
            today_checked_in: None,
            raw: json!({ "note": "fetch_points_best_effort_v1" }),
        })
    }

    async fn health(&self, ctx: &CheckinCtx) -> Result<HealthReport, AdapterError> {
        let path_probe = if ctx.dry_run {
            match self.desktop.resolve_trae_storage() {
                Some(p) => Some(probe::probe_trae_storage_file(&p, &self.desktop)),
                None => Some(probe::PathProbeSummary {
                    platform: "trae".into(),
                    path: format!("%APPDATA%/{}", self.desktop.trae_storage_relpath),
                    note: "APPDATA_unset".into(),
                    ..Default::default()
                }),
            }
        } else {
            None
        };

        let auth_valid = if ctx.dry_run {
            path_probe
                .as_ref()
                .map(|p| p.auth_key_present && p.device_id_present && p.exists)
                .unwrap_or(false)
        } else {
            matches!(
                ctx.session.as_ref().map(|s| s.as_ref()),
                Some(ResolvedSession::Trae {
                    access_token,
                    needs_desktop_session,
                    ..
                }) if !access_token.is_empty() && !*needs_desktop_session
            )
        };

        Ok(HealthReport {
            ok: true,
            auth_valid,
            message: if ctx.dry_run {
                "dry_run path probe (no tokens in detail; DesktopPaths=import source only)".into()
            } else if auth_valid {
                "live: vault JWT present for claim".into()
            } else {
                "live: trae_jwt_unavailable (cipher-only vault material)".into()
            },
            detail: json!({
                "ug_host": self.config.ug_host,
                "client_id": self.config.client_id,
                "dry_run": ctx.dry_run,
                "trae_storage_relpath": self.desktop.trae_storage_relpath,
                "trae_device_id_key": self.desktop.trae_device_id_key,
                "path_probe": path_probe,
                "scope": "daily-checkin-only",
                "cred_uri_is_cred": ctx.credentials_ref.as_cred_uri().is_some(),
                "has_session": ctx.session.is_some(),
            }),
        })
    }
}

async fn try_exchange(
    client: &reqwest::Client,
    config: &TraeConfig,
    client_id: &str,
    refresh_token: &str,
) -> Option<(String, Option<String>)> {
    if refresh_token.is_empty() {
        return None;
    }
    let oauth = config.oauth_host.trim_end_matches('/');
    let candidates = [
        format!("{oauth}/cloudide/api/v3/oauth/ExchangeToken"),
        format!("{oauth}/api/v3/oauth/ExchangeToken"),
        format!("{oauth}/trae/api/v3/oauth/ExchangeToken"),
    ];
    let payload = json!({
        "ClientID": client_id,
        "client_id": client_id,
        "RefreshToken": refresh_token,
        "refresh_token": refresh_token,
        "GrantType": "refresh_token",
        "grant_type": "refresh_token",
    });
    let headers = [("Content-Type", "application/json".into())];

    for url in &candidates {
        let Ok((_st, parsed)) = post_json(client, url, &headers, &payload).await else {
            continue;
        };
        let data = data_object(&parsed);
        let token_obj = data
            .get("Token")
            .or_else(|| data.get("token"))
            .filter(|t| t.is_object())
            .unwrap_or(data);
        let new_access = pick_str(
            token_obj,
            &["AccessToken", "accessToken", "access_token"],
        )
        .or_else(|| {
            pick_str(data, &["AccessToken", "accessToken", "access_token"])
        });
        let new_refresh = pick_str(
            token_obj,
            &["RefreshToken", "refreshToken", "refresh_token"],
        )
        .or_else(|| {
            pick_str(data, &["RefreshToken", "refreshToken", "refresh_token"])
        });
        if let Some(a) = new_access {
            return Some((a, new_refresh));
        }
    }
    None
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
            account_id: "trae-1".into(),
            platform: "trae".into(),
            credentials_ref: CredentialsRef::legacy("t", "k"),
            local_day: "2026-10-18".into(),
            timezone: "Asia/Shanghai".into(),
            dry_run: false,
            run_id: "run-1".into(),
            attempt: 1,
            session: Some(Arc::new(session)),
        }
    }

    fn trae_session(with_jwt: bool) -> ResolvedSession {
        ResolvedSession::Trae {
            icube_auth_info_cipher: "CIPHER_x".into(),
            device_id: "dev-1".into(),
            access_token: if with_jwt {
                "jwt_SECRET_trae".into()
            } else {
                String::new()
            },
            refresh_token: if with_jwt {
                "rt_SECRET_trae".into()
            } else {
                String::new()
            },
            account_key: "dev-1".into(),
            needs_desktop_session: !with_jwt,
        }
    }

    async fn adapter_at(ug: &str) -> TraeAdapter {
        let mut config = TraeConfig::default();
        config.ug_host = ug.to_string();
        config.oauth_host = ug.to_string();
        config.force_direct = true;
        TraeAdapter::new(config, DesktopPaths::default())
    }

    #[tokio::test]
    async fn trae_code_0_success() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/trae/api/v2/ug/checkin_credits/claim"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "code": 0,
                "data": { "credits": 5, "streak_days": 4 }
            })))
            .mount(&server)
            .await;

        let adapter = adapter_at(&server.uri()).await;
        let out = adapter.checkin(&live_ctx(trae_session(true))).await.unwrap();
        assert_eq!(out.kind_str(), "Success");
        assert_eq!(out.credits_delta(), Some(5));
        assert_eq!(out.raw_code(), Some(0));
        assert!(!format!("{out:?}").contains("jwt_SECRET"));
    }

    #[tokio::test]
    async fn trae_code_9095_already() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/trae/api/v2/ug/checkin_credits/claim"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "code": 9095,
                "data": { "continuous_days": 2 }
            })))
            .mount(&server)
            .await;

        let adapter = adapter_at(&server.uri()).await;
        let out = adapter.checkin(&live_ctx(trae_session(true))).await.unwrap();
        assert_eq!(out.kind_str(), "AlreadyDone");
        assert_eq!(out.raw_code(), Some(9095));
    }

    #[tokio::test]
    async fn trae_code_9074_retryable() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/trae/api/v2/ug/checkin_credits/claim"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "code": 9074,
                "message": "rate"
            })))
            .mount(&server)
            .await;

        let adapter = adapter_at(&server.uri()).await;
        let out = adapter.checkin(&live_ctx(trae_session(true))).await.unwrap();
        assert_eq!(out.kind_str(), "Failed");
        assert_eq!(out.raw_code(), Some(9074));
        assert_eq!(out.retryable(), Some(true));
    }

    #[tokio::test]
    async fn trae_code_9004_bad_device() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/trae/api/v2/ug/checkin_credits/claim"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "code": 9004
            })))
            .mount(&server)
            .await;

        let adapter = adapter_at(&server.uri()).await;
        let out = adapter.checkin(&live_ctx(trae_session(true))).await.unwrap();
        assert_eq!(out.kind_str(), "Failed");
        assert_eq!(out.raw_code(), Some(9004));
        assert_eq!(out.retryable(), Some(false));
    }

    #[tokio::test]
    async fn trae_without_jwt_unavailable() {
        let adapter = TraeAdapter::with_defaults();
        let out = adapter
            .checkin(&live_ctx(trae_session(false)))
            .await
            .unwrap();
        assert_eq!(out.kind_str(), "Failed");
        assert_eq!(out.message(), "trae_jwt_unavailable");
        assert_eq!(out.retryable(), Some(false));
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
        let mut ctx = live_ctx(trae_session(true));
        ctx.dry_run = true;
        ctx.session = None;
        let out = adapter.checkin(&ctx).await.unwrap();
        assert_eq!(out.message(), "dry_run");
    }
}
