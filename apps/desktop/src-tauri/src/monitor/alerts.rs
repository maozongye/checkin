//! 告警评估、dedupe_key、emit monitor:alert。

use super::db::MonitorDb;
use super::types::{
    local_date_shanghai, now_ms, AlertRule, AlertSeverity, LastCheckinStatus, MonitorAlert,
    SessionStatus,
};
use tauri::{AppHandle, Emitter};

pub const EVENT_ALERT: &str = "monitor:alert";

/// dedupe_key = `{rule}:{account_id}:{bucket}`
///
/// | rule | bucket |
/// |------|--------|
/// | checkin_fail | 签到日（Asia/Shanghai YYYY-MM-DD；无 at 则用今日） |
/// | session_expired | 固定 `expired` |
/// | session_expiring | 会话到期日（Shanghai） |
/// | points_expiring | 积分到期日（Shanghai） |
/// | points_low | `lte{threshold}` |
pub fn make_dedupe_key(rule: AlertRule, account_id: &str, bucket: &str) -> String {
    format!("{}:{}:{}", rule.as_str(), account_id, bucket)
}

fn bucket_for(
    rule: AlertRule,
    account_id: &str,
    row: &super::types::AccountStateRow,
    cfg: &super::types::AlertConfigRow,
    now: i64,
) -> Option<String> {
    let _ = account_id;
    match rule {
        AlertRule::CheckinFail => {
            let day = row
                .last_checkin_at
                .map(local_date_shanghai)
                .unwrap_or_else(|| local_date_shanghai(now));
            Some(day)
        }
        AlertRule::SessionExpired => Some("expired".into()),
        AlertRule::SessionExpiring => row
            .session_expires_at
            .map(local_date_shanghai)
            .or_else(|| Some(local_date_shanghai(now))),
        AlertRule::PointsExpiring => row
            .points_expire_at
            .map(local_date_shanghai)
            .or_else(|| Some(local_date_shanghai(now))),
        AlertRule::PointsLow => {
            let thr = cfg.threshold_value.unwrap_or(100);
            Some(format!("lte{thr}"))
        }
    }
}

fn should_fire(rule: AlertRule, row: &super::types::AccountStateRow, cfg: &super::types::AlertConfigRow, now: i64) -> bool {
    match rule {
        AlertRule::CheckinFail => {
            LastCheckinStatus::parse(&row.last_checkin_status) == LastCheckinStatus::Fail
        }
        AlertRule::SessionExpired => {
            SessionStatus::parse(&row.session_status) == SessionStatus::Expired
        }
        AlertRule::SessionExpiring => {
            if SessionStatus::parse(&row.session_status) != SessionStatus::Expiring {
                return false;
            }
            true
        }
        AlertRule::PointsExpiring => {
            let Some(exp) = row.points_expire_at else {
                return false;
            };
            let hours = cfg.threshold_hours.unwrap_or(168);
            let remain = exp - now;
            remain > 0 && remain <= hours.saturating_mul(3_600_000)
        }
        AlertRule::PointsLow => {
            let thr = cfg.threshold_value.unwrap_or(100);
            row.points_balance <= thr
        }
    }
}

fn title_body(rule: AlertRule, row: &super::types::AccountStateRow) -> (String, String) {
    let name = &row.display_name;
    match rule {
        AlertRule::CheckinFail => (
            format!("{name}：签到失败"),
            row.last_checkin_message
                .clone()
                .unwrap_or_else(|| "最近一次签到失败".into()),
        ),
        AlertRule::SessionExpired => (
            format!("{name}：登录态已失效"),
            "会话已过期，请重新登录".into(),
        ),
        AlertRule::SessionExpiring => (
            format!("{name}：登录态即将过期"),
            "会话将在阈值内过期，请留意续期".into(),
        ),
        AlertRule::PointsExpiring => (
            format!("{name}：积分即将过期"),
            "积分到期临近，请及时使用".into(),
        ),
        AlertRule::PointsLow => (
            format!("{name}：积分偏低"),
            format!("当前余额 {}", row.points_balance),
        ),
    }
}

/// 评估全部账号；返回新触发的告警（已写 log）；调用方负责 emit。
pub fn evaluate(db: &MonitorDb) -> Result<Vec<MonitorAlert>, String> {
    if !db.alerts_globally_enabled() {
        return Ok(Vec::new());
    }
    let configs = db.list_alert_configs()?;
    let states = db.list_account_states()?;
    let now = now_ms();
    let mut fired = Vec::new();

    for row in &states {
        for cfg in &configs {
            if !cfg.enabled {
                continue;
            }
            let Some(rule) = AlertRule::parse(&cfg.rule_id) else {
                continue;
            };
            if !should_fire(rule, row, cfg, now) {
                continue;
            }
            let Some(bucket) = bucket_for(rule, &row.account_id, row, cfg, now) else {
                continue;
            };
            let dedupe_key = make_dedupe_key(rule, &row.account_id, &bucket);
            let cooldown_ms = cfg.cooldown_hours.saturating_mul(3_600_000);
            let since = now.saturating_sub(cooldown_ms);
            if db.recent_alert_by_dedupe(&dedupe_key, since)?.is_some() {
                continue;
            }
            let severity = rule.default_severity();
            let (title, body) = title_body(rule, row);
            let id = format!("alert-{}-{}-{}", rule.as_str(), row.account_id, now);
            db.insert_alert_log(
                &id,
                rule.as_str(),
                &row.account_id,
                severity.as_str(),
                &title,
                &body,
                &dedupe_key,
                now,
            )?;
            fired.push(MonitorAlert {
                id,
                rule: rule.as_str().into(),
                account_id: row.account_id.clone(),
                severity: severity.as_str().into(),
                title: title.clone(),
                body: body.clone(),
                at: now,
                dedupe_key,
                level: Some(severity.as_str().into()),
                message: Some(body),
            });
        }
    }
    Ok(fired)
}

pub fn emit_alerts(app: &AppHandle, alerts: &[MonitorAlert]) {
    for a in alerts {
        let _ = app.emit(EVENT_ALERT, a);
    }
}

#[allow(dead_code)]
pub fn severity_for_rule(rule: AlertRule) -> AlertSeverity {
    rule.default_severity()
}
