//! 登录态健康探活 stub。不调 Auth；预留 hook。

use super::types::{now_ms, SessionStatus};

/// 默认阈值（小时）；可由 alert_config.session_expiring.threshold_hours 覆盖。
pub const DEFAULT_EXPIRING_HOURS: i64 = 48;

/// 对单账号做会话健康判定。
///
/// - `session_expires_at` 已过 → `expired`
/// - 距到期 < threshold_hours → `expiring`
/// - 否则有到期时间 → `ok`
/// - 无数据 → `unknown`
///
/// // TODO: Auth session probe — 凭据合同落地后在此调用 Auth hook，勿在 Monitor 内读钥匙串。
pub fn check_account(
    _account_id: &str,
    session_expires_at: Option<i64>,
    threshold_hours: i64,
) -> SessionStatus {
    // TODO: Auth session probe
    let Some(expires_at) = session_expires_at else {
        return SessionStatus::Unknown;
    };
    let now = now_ms();
    if expires_at <= now {
        return SessionStatus::Expired;
    }
    let remain_ms = expires_at - now;
    let threshold_ms = threshold_hours.saturating_mul(3_600_000);
    if remain_ms < threshold_ms {
        return SessionStatus::Expiring;
    }
    SessionStatus::Ok
}

/// 批量：读 state 行的 session_expires_at，写回 session_status / session_checked_at。
pub fn check_all(
    db: &super::db::MonitorDb,
    threshold_hours: i64,
) -> Result<usize, String> {
    let states = db.list_account_states()?;
    let now = now_ms();
    let mut n = 0usize;
    for row in states {
        let status = check_account(&row.account_id, row.session_expires_at, threshold_hours);
        db.update_session(
            &row.account_id,
            status.as_str(),
            row.session_expires_at,
            now,
        )?;
        n += 1;
    }
    Ok(n)
}
