//! Monitor IPC / 领域类型（serde camelCase；session 合同枚举）。

use serde::{Deserialize, Serialize};

/// 会话健康：合同仅 `ok|expiring|expired|unknown`（禁止 warn/error/need_relogin）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Ok,
    Expiring,
    Expired,
    Unknown,
}

impl SessionStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Expiring => "expiring",
            Self::Expired => "expired",
            Self::Unknown => "unknown",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "ok" => Self::Ok,
            "expiring" => Self::Expiring,
            "expired" => Self::Expired,
            _ => Self::Unknown,
        }
    }
}

impl std::fmt::Display for SessionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// 最近签到：推荐 `ok|fail|none`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LastCheckinStatus {
    Ok,
    Fail,
    None,
}

impl LastCheckinStatus {
    #[allow(dead_code)]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Fail => "fail",
            Self::None => "none",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "ok" | "success" | "already_done" => Self::Ok,
            "fail" | "failed" | "need_relogin" => Self::Fail,
            _ => Self::None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlertSeverity {
    Info,
    Warn,
    Error,
}

impl AlertSeverity {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Error => "error",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlertRule {
    CheckinFail,
    SessionExpired,
    SessionExpiring,
    PointsExpiring,
    PointsLow,
}

impl AlertRule {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CheckinFail => "checkin_fail",
            Self::SessionExpired => "session_expired",
            Self::SessionExpiring => "session_expiring",
            Self::PointsExpiring => "points_expiring",
            Self::PointsLow => "points_low",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "checkin_fail" => Self::CheckinFail,
            "session_expired" => Self::SessionExpired,
            "session_expiring" => Self::SessionExpiring,
            "points_expiring" => Self::PointsExpiring,
            "points_low" => Self::PointsLow,
            _ => return None,
        })
    }

    pub fn default_severity(self) -> AlertSeverity {
        match self {
            Self::CheckinFail | Self::SessionExpired => AlertSeverity::Error,
            Self::SessionExpiring | Self::PointsExpiring | Self::PointsLow => AlertSeverity::Warn,
        }
    }

    #[allow(dead_code)]
    pub fn all() -> &'static [AlertRule] {
        &[
            Self::CheckinFail,
            Self::SessionExpired,
            Self::SessionExpiring,
            Self::PointsExpiring,
            Self::PointsLow,
        ]
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorSession {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_checked_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorLastCheckin {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub points_delta: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorAccountRow {
    pub account_id: String,
    pub display_name: String,
    pub provider: String,
    pub points_balance: i64,
    pub streak_days: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub points_expire_at: Option<i64>,
    pub session: MonitorSession,
    pub last_checkin: MonitorLastCheckin,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorSummary {
    pub healthy: i64,
    pub warn: i64,
    pub error: i64,
}

/// `monitor:alert` 事件载荷（合同必填 rule / severity / dedupeKey）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorAlert {
    pub id: String,
    pub rule: String,
    pub account_id: String,
    pub severity: String,
    pub title: String,
    pub body: String,
    pub at: i64,
    pub dedupe_key: String,
    /// severity 别名，双写兼容旧 toast
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<String>,
    /// body 别名
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorDashboard {
    pub summary: MonitorSummary,
    pub updated_at: i64,
    pub accounts: Vec<MonitorAccountRow>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alerts: Option<Vec<MonitorAlert>>,
}

#[derive(Debug, Clone)]
pub struct AccountStateRow {
    pub account_id: String,
    pub display_name: String,
    pub provider: String,
    pub points_balance: i64,
    pub streak_days: i64,
    pub points_expire_at: Option<i64>,
    pub session_status: String,
    pub session_expires_at: Option<i64>,
    pub session_checked_at: Option<i64>,
    pub last_checkin_status: String,
    pub last_checkin_at: Option<i64>,
    pub last_checkin_message: Option<String>,
    pub last_checkin_points_delta: Option<i64>,
    pub updated_at: i64,
}

#[derive(Debug, Clone)]
pub struct AlertConfigRow {
    pub rule_id: String,
    pub enabled: bool,
    pub threshold_hours: Option<i64>,
    pub threshold_value: Option<i64>,
    pub cooldown_hours: i64,
}

pub fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Asia/Shanghai 日历日 YYYY-MM-DD（固定 UTC+8，无夏令时）。
pub fn local_date_shanghai(ms: i64) -> String {
    let secs = ms.div_euclid(1000) + 8 * 3600;
    let days = secs.div_euclid(86_400);
    // civil from days since 1970-01-01 (Howard Hinnant algorithm)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

pub fn summarize_session(status: &str, healthy: &mut i64, warn: &mut i64, error: &mut i64) {
    match SessionStatus::parse(status) {
        SessionStatus::Ok => *healthy += 1,
        SessionStatus::Expiring => *warn += 1,
        SessionStatus::Expired => *error += 1,
        SessionStatus::Unknown => *warn += 1,
    }
}
