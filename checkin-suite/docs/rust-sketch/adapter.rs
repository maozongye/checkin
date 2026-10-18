//! Checkin PlatformAdapter 草稿（可编译级语法参考，不必在本仓库 cargo build）。
//! 平台常量均为「已知候选 / 未拍板」，以用户确认与桌面端实测为准。

#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;

// ---------------------------------------------------------------------------
// Ctx / Credentials
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct CredentialsRef {
    pub store: String,
    pub key: String,
}

#[derive(Clone, Debug)]
pub struct CheckinCtx {
    pub account_id: String,
    pub platform: String,
    pub credentials_ref: CredentialsRef,
    /// YYYY-MM-DD
    pub local_day: String,
    /// default Asia/Shanghai
    pub timezone: String,
    pub dry_run: bool,
    pub run_id: String,
    pub attempt: u32,
}

// ---------------------------------------------------------------------------
// Outcome / Snapshots
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum CheckinOutcome {
    Success {
        credits_delta: Option<i64>,
        streak_days: Option<i32>,
        balance: Option<i64>,
        message: String,
        raw_code: Option<i64>,
    },
    AlreadyDone {
        credits_delta: Option<i64>,
        streak_days: Option<i32>,
        balance: Option<i64>,
        message: String,
        raw_code: Option<i64>,
    },
    Skipped {
        reason: String,
    },
    Failed {
        code: Option<i64>,
        message: String,
        retryable: bool,
    },
    Disabled {
        reason: String,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PointsSnapshot {
    pub balance: Option<i64>,
    pub streak_days: Option<i32>,
    /// 仅展示，不可作为签到跳过依据
    pub today_checked_in: Option<bool>,
    pub raw: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HealthReport {
    pub ok: bool,
    pub auth_valid: bool,
    pub message: String,
    pub detail: Value,
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Error)]
pub enum AdapterError {
    #[error("network: {0}")]
    Network(String),
    #[error("auth expired: {0}")]
    AuthExpired(String),
    #[error("auth invalid: {0}")]
    AuthInvalid(String),
    #[error("rate limited: {0}")]
    RateLimited(String),
    #[error("protocol: {0}")]
    Protocol(String),
    #[error("config: {0}")]
    Config(String),
}

// ---------------------------------------------------------------------------
// Trait
// ---------------------------------------------------------------------------

#[async_trait::async_trait]
pub trait PlatformAdapter: Send + Sync {
    fn platform(&self) -> &'static str;

    async fn checkin(&self, ctx: &CheckinCtx) -> Result<CheckinOutcome, AdapterError>;

    async fn fetch_points(&self, ctx: &CheckinCtx) -> Result<PointsSnapshot, AdapterError>;

    async fn health(&self, ctx: &CheckinCtx) -> Result<HealthReport, AdapterError>;
}

// ---------------------------------------------------------------------------
// Platform configs — 候选默认，待确认
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorkBuddyConfig {
    /// 本机已验证默认：https://copilot.tencent.com
    pub base_url: String,
    /// 候选备选：https://www.codebuddy.cn
    pub alt_base_url: String,
    pub force_direct: bool,
}

impl Default for WorkBuddyConfig {
    fn default() -> Self {
        Self {
            // 本机已验证默认（仍可配置）
            base_url: "https://copilot.tencent.com".into(),
            // 候选备选
            alt_base_url: "https://www.codebuddy.cn".into(),
            force_direct: true,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TraeConfig {
    /// 已知候选 / 未拍板：https://api.trae.cn
    pub ug_host: String,
    /// 已知候选 / 未拍板：https://api.trae.com.cn
    pub oauth_host: String,
    /// 已知候选 / 未拍板：ono9krqynydwx5（旧版候选 en1oxy7wnw8j9n）
    pub client_id: String,
    pub force_direct: bool,
    /// 待确认
    pub req_source: i32,
}

impl Default for TraeConfig {
    fn default() -> Self {
        Self {
            // 待确认：以用户确认与桌面端实测为准
            ug_host: "https://api.trae.cn".into(),
            oauth_host: "https://api.trae.com.cn".into(),
            client_id: "ono9krqynydwx5".into(),
            force_direct: true,
            req_source: 1,
        }
    }
}

/// Trae 旧版 ClientID 候选（未拍板）
pub const TRAE_CLIENT_ID_LEGACY_CANDIDATE: &str = "en1oxy7wnw8j9n";

// ---------------------------------------------------------------------------
// Business codes（对照实现，待实测复核）
// ---------------------------------------------------------------------------

pub mod codes {
    pub const WB_OK: i64 = 0;
    pub const WB_ALREADY: i64 = 10001;
    pub const WB_SESSION_INVALID: i64 = 12153;

    pub const TRAE_OK: i64 = 0;
    pub const TRAE_ALREADY: i64 = 9095;
    pub const TRAE_RATE_LIMIT: i64 = 9074;
    pub const TRAE_BAD_DEVICE: i64 = 9004;
}

// ---------------------------------------------------------------------------
// Orchestrator sketch
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub enum Trigger {
    Manual,
    Schedule { cron_or_at: String },
}

#[derive(Clone, Debug)]
pub struct OrchestratorConfig {
    pub concurrency: usize,
    pub jitter_ms: u64,
    pub timezone: String,
    pub max_retries: u32,
    pub force_remote: bool,
}

impl Default for OrchestratorConfig {
    fn default() -> Self {
        Self {
            concurrency: 3,
            jitter_ms: 500,
            timezone: "Asia/Shanghai".into(),
            max_retries: 2,
            force_remote: false,
        }
    }
}

#[derive(Clone, Debug, Serialize, Default)]
pub struct RunCounts {
    pub total: u32,
    pub success: u32,
    pub already_done: u32,
    pub skipped: u32,
    pub failed: u32,
    pub disabled: u32,
}

#[derive(Clone, Debug, Serialize)]
pub struct CheckinItemView {
    pub account_id: String,
    pub platform: String,
    pub outcome: String,
    pub credits_delta: Option<i64>,
    pub streak_days: Option<i32>,
    pub balance: Option<i64>,
    pub message: String,
    pub raw_code: Option<i64>,
}

#[derive(Clone, Debug, Serialize)]
pub struct RunSummary {
    pub run_id: String,
    pub trigger: String,
    pub local_day: String,
    pub timezone: String,
    pub started_at: String,
    pub finished_at: String,
    pub status: String,
    pub counts: RunCounts,
    pub items: Vec<CheckinItemView>,
}

pub struct CheckinOrchestrator {
    pub adapters: HashMap<String, Arc<dyn PlatformAdapter>>,
    pub config: OrchestratorConfig,
}

impl CheckinOrchestrator {
    /// 由桌面端填充：拉账号、幂等查表、并发、emit、写库。
    pub async fn run_all(
        &self,
        _trigger: Trigger,
        _platform_filter: Option<&str>,
        _dry_run: bool,
    ) -> Result<RunSummary, AdapterError> {
        Err(AdapterError::Config(
            "orchestrator body lives in desktop crate".into(),
        ))
    }
}

/// 将平台业务码映射为 Outcome 片段（WorkBuddy）
pub fn map_workbuddy_code(
    code: i64,
    credits_delta: Option<i64>,
    streak_days: Option<i32>,
    balance: Option<i64>,
    message: impl Into<String>,
) -> CheckinOutcome {
    let message = message.into();
    match code {
        codes::WB_OK => CheckinOutcome::Success {
            credits_delta,
            streak_days,
            balance,
            message,
            raw_code: Some(code),
        },
        codes::WB_ALREADY => CheckinOutcome::AlreadyDone {
            credits_delta: None,
            streak_days,
            balance,
            message,
            raw_code: Some(code),
        },
        codes::WB_SESSION_INVALID => CheckinOutcome::Disabled {
            reason: message,
        },
        _ => CheckinOutcome::Failed {
            code: Some(code),
            message,
            retryable: false,
        },
    }
}

/// Trae 业务码映射
pub fn map_trae_code(
    code: i64,
    credits_delta: Option<i64>,
    streak_days: Option<i32>,
    balance: Option<i64>,
    message: impl Into<String>,
) -> CheckinOutcome {
    let message = message.into();
    match code {
        codes::TRAE_OK => CheckinOutcome::Success {
            credits_delta,
            streak_days,
            balance,
            message,
            raw_code: Some(code),
        },
        codes::TRAE_ALREADY => CheckinOutcome::AlreadyDone {
            credits_delta: None,
            streak_days,
            balance,
            message,
            raw_code: Some(code),
        },
        codes::TRAE_RATE_LIMIT => CheckinOutcome::Failed {
            code: Some(code),
            message,
            retryable: true,
        },
        codes::TRAE_BAD_DEVICE => CheckinOutcome::Failed {
            code: Some(code),
            message,
            retryable: false,
        },
        1001 | 1002 | 401 | 403 => CheckinOutcome::Failed {
            code: Some(code),
            message,
            retryable: false,
        },
        _ => CheckinOutcome::Failed {
            code: Some(code),
            message,
            retryable: false,
        },
    }
}
