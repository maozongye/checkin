//! 健康探活节奏（低资源）。v1：间隔来自 settings，由 refresh 路径驱动；无重型常驻定时器。

use super::db::MonitorDb;

pub const DEFAULT_HEALTH_INTERVAL_MINUTES: i64 = 30;

pub fn health_interval_minutes(db: &MonitorDb) -> i64 {
    let v = db.setting_i64("health_interval_minutes", DEFAULT_HEALTH_INTERVAL_MINUTES);
    if v < 1 {
        DEFAULT_HEALTH_INTERVAL_MINUTES
    } else {
        v
    }
}

pub fn refresh_throttle_secs(db: &MonitorDb) -> i64 {
    let v = db.setting_i64("refresh_throttle_secs", 30);
    if v < 0 {
        30
    } else {
        v
    }
}

/// 距上次探活是否已超过间隔（单位 ms）。
pub fn health_due(last_checked_at: Option<i64>, now_ms: i64, interval_minutes: i64) -> bool {
    let Some(last) = last_checked_at else {
        return true;
    };
    now_ms.saturating_sub(last) >= interval_minutes.saturating_mul(60_000)
}
