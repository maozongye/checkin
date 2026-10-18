//! 与前端 `src/lib/api.ts` JS mock 对齐的账号 / 看板数据（同 id：acc-wb-1 等）。

use serde::Serialize;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountPoints {
    pub balance: i64,
    pub streak_days: i64,
    pub fetched_at: i64,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub id: String,
    pub platform: String,
    pub display_name: String,
    pub status: String,
    pub tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub points: Option<AccountPoints>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_uid: Option<String>,
}

pub fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

pub fn mock_accounts() -> Vec<Account> {
    let now = now_ms();
    vec![
        Account {
            id: "acc-wb-1".into(),
            platform: "workbuddy".into(),
            display_name: "WB · 小强".into(),
            status: "active".into(),
            tags: vec!["日常".into()],
            points: Some(AccountPoints {
                balance: 1280,
                streak_days: 12,
                fetched_at: now - 3_600_000,
            }),
            external_uid: Some("wb_1001".into()),
        },
        Account {
            id: "acc-wb-2".into(),
            platform: "workbuddy".into(),
            display_name: "WB · 备用".into(),
            status: "active".into(),
            tags: vec!["备用".into()],
            points: Some(AccountPoints {
                balance: 420,
                streak_days: 3,
                fetched_at: now - 7_200_000,
            }),
            external_uid: None,
        },
        Account {
            id: "acc-trae-1".into(),
            platform: "trae_cn".into(),
            display_name: "Trae · 主号".into(),
            status: "active".into(),
            tags: vec!["IDE".into()],
            points: Some(AccountPoints {
                balance: 860,
                streak_days: 7,
                fetched_at: now - 1_800_000,
            }),
            external_uid: None,
        },
        Account {
            id: "acc-trae-2".into(),
            platform: "trae_cn".into(),
            display_name: "Trae · 需重登".into(),
            status: "expired".into(),
            tags: vec!["告警".into()],
            points: Some(AccountPoints {
                balance: 0,
                streak_days: 0,
                fetched_at: now - 86_400_000,
            }),
            external_uid: None,
        },
    ]
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorSession {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_checked_at: Option<i64>,
}

#[derive(Clone, Serialize)]
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

#[derive(Clone, Serialize)]
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

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorSummary {
    pub healthy: i64,
    pub warn: i64,
    pub error: i64,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorAlert {
    pub id: String,
    pub level: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,
    pub at: i64,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorDashboard {
    pub summary: MonitorSummary,
    pub updated_at: i64,
    pub accounts: Vec<MonitorAccountRow>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alerts: Option<Vec<MonitorAlert>>,
}

pub fn build_dashboard() -> MonitorDashboard {
    let now = now_ms();
    let accounts = vec![
        MonitorAccountRow {
            account_id: "acc-wb-1".into(),
            display_name: "WB · 小强".into(),
            provider: "workbuddy".into(),
            points_balance: 1280,
            streak_days: 12,
            points_expire_at: Some(now + 30 * 86_400_000),
            session: MonitorSession {
                status: "ok".into(),
                expires_at: Some(now + 14 * 86_400_000),
                last_checked_at: Some(now - 600_000),
            },
            last_checkin: MonitorLastCheckin {
                status: "success".into(),
                at: Some(now - 3_600_000),
                message: Some("签到成功 +10".into()),
                points_delta: Some(10),
            },
        },
        MonitorAccountRow {
            account_id: "acc-wb-2".into(),
            display_name: "WB · 备用".into(),
            provider: "workbuddy".into(),
            points_balance: 420,
            streak_days: 3,
            points_expire_at: Some(now + 10 * 86_400_000),
            session: MonitorSession {
                status: "warn".into(),
                expires_at: Some(now + 2 * 86_400_000),
                last_checked_at: Some(now - 1_200_000),
            },
            last_checkin: MonitorLastCheckin {
                status: "failed".into(),
                at: Some(now - 86_400_000),
                message: Some("网络超时（mock）".into()),
                points_delta: Some(0),
            },
        },
        MonitorAccountRow {
            account_id: "acc-trae-1".into(),
            display_name: "Trae · 主号".into(),
            provider: "trae_cn".into(),
            points_balance: 860,
            streak_days: 7,
            points_expire_at: Some(now + 20 * 86_400_000),
            session: MonitorSession {
                status: "ok".into(),
                expires_at: Some(now + 21 * 86_400_000),
                last_checked_at: Some(now - 300_000),
            },
            last_checkin: MonitorLastCheckin {
                status: "already_done".into(),
                at: Some(now - 8_000_000),
                message: Some("今日已签".into()),
                points_delta: Some(0),
            },
        },
        MonitorAccountRow {
            account_id: "acc-trae-2".into(),
            display_name: "Trae · 需重登".into(),
            provider: "trae_cn".into(),
            points_balance: 0,
            streak_days: 0,
            points_expire_at: None,
            session: MonitorSession {
                status: "need_relogin".into(),
                expires_at: Some(now - 86_400_000),
                last_checked_at: Some(now - 3_600_000),
            },
            last_checkin: MonitorLastCheckin {
                status: "need_relogin".into(),
                at: Some(now - 2 * 86_400_000),
                message: Some("登录态失效，请重新登录".into()),
                points_delta: None,
            },
        },
    ];

    let mut healthy = 0i64;
    let mut warn = 0i64;
    let mut error = 0i64;
    for a in &accounts {
        match a.session.status.as_str() {
            "ok" => healthy += 1,
            "warn" => warn += 1,
            _ => error += 1,
        }
    }

    MonitorDashboard {
        summary: MonitorSummary {
            healthy,
            warn,
            error,
        },
        updated_at: now,
        accounts,
        alerts: Some(vec![
            MonitorAlert {
                id: "alert-1".into(),
                level: "error".into(),
                message: "Trae · 需重登：登录态失效".into(),
                account_id: Some("acc-trae-2".into()),
                at: now - 3_600_000,
            },
            MonitorAlert {
                id: "alert-2".into(),
                level: "warn".into(),
                message: "WB · 备用：上次签到失败".into(),
                account_id: Some("acc-wb-2".into()),
                at: now - 86_400_000,
            },
        ]),
    }
}

#[derive(Clone, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SchedulerConfig {
    pub enabled: bool,
    pub cron_or_time: String,
    pub timezone: String,
    #[serde(default)]
    pub platforms_filter: Option<Vec<String>>,
    #[serde(default)]
    pub last_run_at: Option<i64>,
    #[serde(default)]
    pub next_run_at: Option<i64>,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            cron_or_time: "09:00".into(),
            timezone: "Asia/Shanghai".into(),
            platforms_filter: None,
            last_run_at: None,
            next_run_at: None,
        }
    }
}
