//! Monitor 服务：积分/连签/会话健康、本地告警（不执行签到副作用）。

pub mod alerts;
pub mod db;
pub mod health;
pub mod scheduler;
pub mod snapshot;
pub mod types;

pub use db::MonitorState;
pub use types::MonitorDashboard;

use alerts::{emit_alerts, evaluate};
use db::MonitorDb;
use types::{
    now_ms, summarize_session, MonitorAccountRow, MonitorLastCheckin, MonitorSession,
    MonitorSummary,
};

fn row_to_account(r: types::AccountStateRow) -> MonitorAccountRow {
    MonitorAccountRow {
        account_id: r.account_id,
        display_name: r.display_name,
        provider: r.provider,
        points_balance: r.points_balance,
        streak_days: r.streak_days,
        points_expire_at: r.points_expire_at,
        session: MonitorSession {
            status: r.session_status,
            expires_at: r.session_expires_at,
            last_checked_at: r.session_checked_at,
        },
        last_checkin: MonitorLastCheckin {
            status: r.last_checkin_status,
            at: r.last_checkin_at,
            message: r.last_checkin_message,
            points_delta: r.last_checkin_points_delta,
        },
    }
}

fn build_dashboard_from_db(db: &MonitorDb) -> Result<MonitorDashboard, String> {
    let states = db.list_account_states()?;
    let mut healthy = 0i64;
    let mut warn = 0i64;
    let mut error = 0i64;
    let mut updated_at = 0i64;
    let mut accounts = Vec::with_capacity(states.len());
    for s in states {
        summarize_session(&s.session_status, &mut healthy, &mut warn, &mut error);
        if s.updated_at > updated_at {
            updated_at = s.updated_at;
        }
        accounts.push(row_to_account(s));
    }
    if updated_at == 0 {
        updated_at = now_ms();
    }
    let alerts = db.list_recent_alerts(20).ok();
    Ok(MonitorDashboard {
        summary: MonitorSummary {
            healthy,
            warn,
            error,
        },
        updated_at,
        accounts,
        alerts,
    })
}

/// 纯读：不触网、不写库（除首次空库 seed 外在 State::open 已完成）。
pub fn get_dashboard(state: &MonitorState) -> Result<MonitorDashboard, String> {
    let db = state.db.lock().map_err(|e| format!("db lock: {e}"))?;
    build_dashboard_from_db(&db)
}

/// 刷新：节流 → 物化 snapshot → health stub → alerts → dashboard。
pub fn refresh(
    app: &tauri::AppHandle,
    state: &MonitorState,
) -> Result<MonitorDashboard, String> {
    let throttle_secs;
    let interval_min;
    let expiring_hours;
    {
        let db = state.db.lock().map_err(|e| format!("db lock: {e}"))?;
        throttle_secs = scheduler::refresh_throttle_secs(&db);
        interval_min = scheduler::health_interval_minutes(&db);
        expiring_hours = db
            .get_alert_config("session_expiring")?
            .and_then(|c| c.threshold_hours)
            .unwrap_or(health::DEFAULT_EXPIRING_HOURS);
    }

    let now = now_ms();
    {
        let mut last = state
            .last_refresh_ms
            .lock()
            .map_err(|e| format!("throttle lock: {e}"))?;
        if *last > 0 && now.saturating_sub(*last) < throttle_secs.saturating_mul(1000) {
            // 节流命中：仍返回当前 dashboard，不重复探活/告警
            let db = state.db.lock().map_err(|e| format!("db lock: {e}"))?;
            return build_dashboard_from_db(&db);
        }
        *last = now;
    }

    let fired = {
        let db = state.db.lock().map_err(|e| format!("db lock: {e}"))?;
        snapshot::ensure_and_materialize(&db)?;

        // 低资源：按 settings 间隔决定是否跑 health stub（refresh 路径内）
        let due = {
            let states = db.list_account_states()?;
            let oldest = states
                .iter()
                .filter_map(|s| s.session_checked_at)
                .min();
            scheduler::health_due(oldest, now, interval_min) || states.is_empty()
                || states.iter().any(|s| s.session_checked_at.is_none())
        };
        if due {
            health::check_all(&db, expiring_hours)?;
        }

        evaluate(&db)?
    };

    emit_alerts(app, &fired);

    let db = state.db.lock().map_err(|e| format!("db lock: {e}"))?;
    build_dashboard_from_db(&db)
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_materialize_and_alerts() {
        let db = db::MonitorDb::open_in_memory().expect("db");
        snapshot::ensure_and_materialize(&db).expect("mat");
        let dash = build_dashboard_from_db(&db).expect("dash");
        assert_eq!(dash.accounts.len(), 4);
        assert!(dash.summary.healthy >= 1);
        let fired = alerts::evaluate(&db).expect("eval");
        // defaults: checkin_fail + session_expired enabled
        assert!(
            fired.iter().any(|a| a.rule == "checkin_fail"),
            "expected checkin_fail, got {:?}",
            fired.iter().map(|a| &a.rule).collect::<Vec<_>>()
        );
        assert!(
            fired.iter().any(|a| a.rule == "session_expired"),
            "expected session_expired"
        );
        assert!(fired.iter().all(|a| !a.dedupe_key.is_empty() && !a.severity.is_empty()));
    }

    #[test]
    fn session_health_stub() {
        let now = types::now_ms();
        assert_eq!(
            health::check_account("a", Some(now - 1000), 48).as_str(),
            "expired"
        );
        assert_eq!(
            health::check_account("a", Some(now + 3_600_000), 48).as_str(),
            "expiring"
        );
        assert_eq!(
            health::check_account("a", Some(now + 100 * 3_600_000), 48).as_str(),
            "ok"
        );
        assert_eq!(health::check_account("a", None, 48).as_str(), "unknown");
    }
}
