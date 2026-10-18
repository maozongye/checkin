//! SQLite：monitor_* + 业务只读源 accounts / points_snapshot（不存凭据）。

use super::types::{now_ms, AccountStateRow, AlertConfigRow};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS accounts (
  id TEXT PRIMARY KEY,
  platform TEXT NOT NULL,
  display_name TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'active'
);

CREATE TABLE IF NOT EXISTS points_snapshot (
  id TEXT PRIMARY KEY,
  account_id TEXT NOT NULL,
  balance INTEGER NOT NULL DEFAULT 0,
  streak_days INTEGER NOT NULL DEFAULT 0,
  expires_at INTEGER,
  raw_json TEXT,
  fetched_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS monitor_account_state (
  account_id TEXT PRIMARY KEY,
  display_name TEXT NOT NULL DEFAULT '',
  provider TEXT NOT NULL DEFAULT '',
  points_balance INTEGER NOT NULL DEFAULT 0,
  streak_days INTEGER NOT NULL DEFAULT 0,
  points_expire_at INTEGER,
  session_status TEXT NOT NULL DEFAULT 'unknown',
  session_expires_at INTEGER,
  session_checked_at INTEGER,
  last_checkin_status TEXT NOT NULL DEFAULT 'none',
  last_checkin_at INTEGER,
  last_checkin_message TEXT,
  last_checkin_points_delta INTEGER,
  updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS monitor_alert_config (
  rule_id TEXT PRIMARY KEY,
  enabled INTEGER NOT NULL DEFAULT 0,
  threshold_hours INTEGER,
  threshold_value INTEGER,
  cooldown_hours INTEGER NOT NULL DEFAULT 24
);

CREATE TABLE IF NOT EXISTS monitor_settings (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS monitor_alert_log (
  id TEXT PRIMARY KEY,
  rule_id TEXT NOT NULL,
  account_id TEXT NOT NULL,
  severity TEXT NOT NULL,
  title TEXT NOT NULL,
  body TEXT NOT NULL,
  dedupe_key TEXT NOT NULL,
  fired_at INTEGER NOT NULL,
  acked_at INTEGER
);

CREATE INDEX IF NOT EXISTS idx_points_snapshot_account
  ON points_snapshot(account_id, fetched_at DESC);
CREATE INDEX IF NOT EXISTS idx_alert_log_dedupe
  ON monitor_alert_log(dedupe_key, fired_at DESC);
"#;

pub struct MonitorDb {
    conn: Connection,
}

impl MonitorDb {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, String> {
        if let Some(parent) = path.as_ref().parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("create db dir: {e}"))?;
        }
        let conn = Connection::open(path).map_err(|e| format!("open monitor db: {e}"))?;
        conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")
            .map_err(|e| format!("pragma: {e}"))?;
        let db = Self { conn };
        db.migrate()?;
        db.seed_defaults()?;
        Ok(db)
    }

    #[allow(dead_code)]
    pub fn open_in_memory() -> Result<Self, String> {
        let conn = Connection::open_in_memory().map_err(|e| format!("open memory db: {e}"))?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")
            .map_err(|e| format!("pragma: {e}"))?;
        let db = Self { conn };
        db.migrate()?;
        db.seed_defaults()?;
        Ok(db)
    }

    pub fn migrate(&self) -> Result<(), String> {
        self.conn
            .execute_batch(SCHEMA)
            .map_err(|e| format!("migrate: {e}"))?;
        Ok(())
    }

    fn seed_defaults(&self) -> Result<(), String> {
        let seeds: &[(&str, i64, Option<i64>, Option<i64>, i64)] = &[
            // rule_id, enabled, threshold_hours, threshold_value, cooldown_hours
            ("checkin_fail", 1, None, None, 24),
            ("session_expired", 1, None, None, 24),
            ("session_expiring", 0, Some(48), None, 24),
            ("points_expiring", 0, Some(168), None, 24),
            ("points_low", 0, None, Some(100), 24),
        ];
        for (rule, enabled, th_h, th_v, cd) in seeds {
            self.conn
                .execute(
                    "INSERT OR IGNORE INTO monitor_alert_config
                     (rule_id, enabled, threshold_hours, threshold_value, cooldown_hours)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![rule, enabled, th_h, th_v, cd],
                )
                .map_err(|e| format!("seed alert_config: {e}"))?;
        }
        let settings: &[(&str, &str)] = &[
            ("alerts.enabled", "1"),
            ("health_interval_minutes", "30"),
            ("refresh_throttle_secs", "30"),
        ];
        for (k, v) in settings {
            self.conn
                .execute(
                    "INSERT OR IGNORE INTO monitor_settings (key, value) VALUES (?1, ?2)",
                    params![k, v],
                )
                .map_err(|e| format!("seed settings: {e}"))?;
        }
        Ok(())
    }

    pub fn get_setting(&self, key: &str) -> Result<Option<String>, String> {
        self.conn
            .query_row(
                "SELECT value FROM monitor_settings WHERE key = ?1",
                params![key],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| format!("get_setting: {e}"))
    }

    pub fn setting_i64(&self, key: &str, default: i64) -> i64 {
        self.get_setting(key)
            .ok()
            .flatten()
            .and_then(|v| v.parse().ok())
            .unwrap_or(default)
    }

    pub fn alerts_globally_enabled(&self) -> bool {
        self.get_setting("alerts.enabled")
            .ok()
            .flatten()
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(true)
    }

    pub fn list_alert_configs(&self) -> Result<Vec<AlertConfigRow>, String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT rule_id, enabled, threshold_hours, threshold_value, cooldown_hours
                 FROM monitor_alert_config",
            )
            .map_err(|e| format!("list_alert_configs: {e}"))?;
        let rows = stmt
            .query_map([], |r| {
                Ok(AlertConfigRow {
                    rule_id: r.get(0)?,
                    enabled: r.get::<_, i64>(1)? != 0,
                    threshold_hours: r.get(2)?,
                    threshold_value: r.get(3)?,
                    cooldown_hours: r.get(4)?,
                })
            })
            .map_err(|e| format!("list_alert_configs map: {e}"))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(|e| format!("alert_config row: {e}"))?);
        }
        Ok(out)
    }

    pub fn get_alert_config(&self, rule_id: &str) -> Result<Option<AlertConfigRow>, String> {
        self.conn
            .query_row(
                "SELECT rule_id, enabled, threshold_hours, threshold_value, cooldown_hours
                 FROM monitor_alert_config WHERE rule_id = ?1",
                params![rule_id],
                |r| {
                    Ok(AlertConfigRow {
                        rule_id: r.get(0)?,
                        enabled: r.get::<_, i64>(1)? != 0,
                        threshold_hours: r.get(2)?,
                        threshold_value: r.get(3)?,
                        cooldown_hours: r.get(4)?,
                    })
                },
            )
            .optional()
            .map_err(|e| format!("get_alert_config: {e}"))
    }

    pub fn count_accounts(&self) -> Result<i64, String> {
        self.conn
            .query_row("SELECT COUNT(*) FROM accounts", [], |r| r.get(0))
            .map_err(|e| format!("count accounts: {e}"))
    }

    pub fn count_snapshots(&self) -> Result<i64, String> {
        self.conn
            .query_row("SELECT COUNT(*) FROM points_snapshot", [], |r| r.get(0))
            .map_err(|e| format!("count snapshots: {e}"))
    }

    pub fn seed_dev_accounts_and_snapshots(&self) -> Result<(), String> {
        let now = now_ms();
        // DEV seed — 与旧 mock 同 id，便于联调
        let accounts: &[(&str, &str, &str, &str)] = &[
            ("acc-wb-1", "workbuddy", "WB · 小强", "active"),
            ("acc-wb-2", "workbuddy", "WB · 备用", "active"),
            ("acc-trae-1", "trae_cn", "Trae · 主号", "active"),
            ("acc-trae-2", "trae_cn", "Trae · 需重登", "expired"),
        ];
        for (id, platform, name, status) in accounts {
            self.conn
                .execute(
                    "INSERT OR IGNORE INTO accounts (id, platform, display_name, status)
                     VALUES (?1, ?2, ?3, ?4)",
                    params![id, platform, name, status],
                )
                .map_err(|e| format!("seed account: {e}"))?;
        }

        let snaps: &[(&str, &str, i64, i64, Option<i64>, i64)] = &[
            (
                "snap-wb-1",
                "acc-wb-1",
                1280,
                12,
                Some(now + 30 * 86_400_000),
                now - 3_600_000,
            ),
            (
                "snap-wb-2",
                "acc-wb-2",
                420,
                3,
                Some(now + 10 * 86_400_000),
                now - 7_200_000,
            ),
            (
                "snap-trae-1",
                "acc-trae-1",
                860,
                7,
                Some(now + 20 * 86_400_000),
                now - 1_800_000,
            ),
            (
                "snap-trae-2",
                "acc-trae-2",
                0,
                0,
                None,
                now - 86_400_000,
            ),
        ];
        for (id, aid, bal, streak, exp, fetched) in snaps {
            self.conn
                .execute(
                    "INSERT OR IGNORE INTO points_snapshot
                     (id, account_id, balance, streak_days, expires_at, raw_json, fetched_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![
                        id,
                        aid,
                        bal,
                        streak,
                        exp,
                        Some(r#"{"_dev":true,"note":"DEV seed"}"#),
                        fetched
                    ],
                )
                .map_err(|e| format!("seed snapshot: {e}"))?;
        }

        // 初始 session / last_checkin 写入 state（materialize 会再 upsert 余额）
        // (account_id, session_status, session_expires_at, checkin_status, checkin_at, msg, delta)
        let states: &[(
            &str,
            &str,
            Option<i64>,
            &str,
            Option<i64>,
            Option<&str>,
            Option<i64>,
        )] = &[
            (
                "acc-wb-1",
                "ok",
                Some(now + 14 * 86_400_000),
                "ok",
                Some(now - 3_600_000),
                Some("签到成功 +10"),
                Some(10),
            ),
            (
                "acc-wb-2",
                "expiring",
                Some(now + 2 * 86_400_000),
                "fail",
                Some(now - 86_400_000),
                Some("网络超时（DEV seed）"),
                Some(0),
            ),
            (
                "acc-trae-1",
                "ok",
                Some(now + 21 * 86_400_000),
                "ok",
                Some(now - 8_000_000),
                Some("今日已签"),
                Some(0),
            ),
            (
                "acc-trae-2",
                "expired",
                Some(now - 86_400_000),
                "fail",
                Some(now - 2 * 86_400_000),
                Some("登录态失效，请重新登录"),
                None,
            ),
        ];
        for (aid, sess, sess_exp, ck, ck_at, msg, delta) in states {
            let (name, provider) = match *aid {
                "acc-wb-1" => ("WB · 小强", "workbuddy"),
                "acc-wb-2" => ("WB · 备用", "workbuddy"),
                "acc-trae-1" => ("Trae · 主号", "trae_cn"),
                "acc-trae-2" => ("Trae · 需重登", "trae_cn"),
                _ => ("?", "?"),
            };
            self.conn
                .execute(
                    "INSERT OR REPLACE INTO monitor_account_state (
                       account_id, display_name, provider, points_balance, streak_days,
                       points_expire_at, session_status, session_expires_at, session_checked_at,
                       last_checkin_status, last_checkin_at, last_checkin_message,
                       last_checkin_points_delta, updated_at
                     ) VALUES (?1,?2,?3,0,0,NULL,?4,?5,?6,?7,?8,?9,?10,?11)",
                    params![
                        aid,
                        name,
                        provider,
                        sess,
                        sess_exp,
                        Some(now - 600_000),
                        ck,
                        ck_at,
                        msg,
                        delta,
                        now
                    ],
                )
                .map_err(|e| format!("seed state: {e}"))?;
        }
        Ok(())
    }

    /// 从 points_snapshot(+accounts) 物化余额/连签/到期；保留已有 session/checkin 字段。
    pub fn materialize_from_snapshots(&self) -> Result<usize, String> {
        let now = now_ms();
        let mut stmt = self
            .conn
            .prepare(
                "SELECT a.id, a.platform, a.display_name,
                        COALESCE(p.balance, 0), COALESCE(p.streak_days, 0), p.expires_at
                 FROM accounts a
                 LEFT JOIN points_snapshot p ON p.account_id = a.id
                 AND p.fetched_at = (
                   SELECT MAX(fetched_at) FROM points_snapshot WHERE account_id = a.id
                 )",
            )
            .map_err(|e| format!("materialize prepare: {e}"))?;
        let rows: Vec<(String, String, String, i64, i64, Option<i64>)> = stmt
            .query_map([], |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                ))
            })
            .map_err(|e| format!("materialize map: {e}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("materialize rows: {e}"))?;

        let mut n = 0usize;
        for (id, platform, name, bal, streak, exp) in rows {
            self.conn
                .execute(
                    "INSERT INTO monitor_account_state (
                       account_id, display_name, provider, points_balance, streak_days,
                       points_expire_at, session_status, session_expires_at, session_checked_at,
                       last_checkin_status, last_checkin_at, last_checkin_message,
                       last_checkin_points_delta, updated_at
                     ) VALUES (?1,?2,?3,?4,?5,?6,'unknown',NULL,NULL,'none',NULL,NULL,NULL,?7)
                     ON CONFLICT(account_id) DO UPDATE SET
                       display_name = excluded.display_name,
                       provider = excluded.provider,
                       points_balance = excluded.points_balance,
                       streak_days = excluded.streak_days,
                       points_expire_at = excluded.points_expire_at,
                       updated_at = excluded.updated_at",
                    params![id, name, platform, bal, streak, exp, now],
                )
                .map_err(|e| format!("materialize upsert: {e}"))?;
            n += 1;
        }
        Ok(n)
    }

    pub fn update_session(
        &self,
        account_id: &str,
        status: &str,
        expires_at: Option<i64>,
        checked_at: i64,
    ) -> Result<(), String> {
        self.conn
            .execute(
                "UPDATE monitor_account_state SET
                   session_status = ?1,
                   session_expires_at = COALESCE(?2, session_expires_at),
                   session_checked_at = ?3,
                   updated_at = ?3
                 WHERE account_id = ?4",
                params![status, expires_at, checked_at, account_id],
            )
            .map_err(|e| format!("update_session: {e}"))?;
        Ok(())
    }

    pub fn list_account_states(&self) -> Result<Vec<AccountStateRow>, String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT account_id, display_name, provider, points_balance, streak_days,
                        points_expire_at, session_status, session_expires_at, session_checked_at,
                        last_checkin_status, last_checkin_at, last_checkin_message,
                        last_checkin_points_delta, updated_at
                 FROM monitor_account_state
                 ORDER BY display_name COLLATE NOCASE",
            )
            .map_err(|e| format!("list states: {e}"))?;
        let rows = stmt
            .query_map([], |r| {
                Ok(AccountStateRow {
                    account_id: r.get(0)?,
                    display_name: r.get(1)?,
                    provider: r.get(2)?,
                    points_balance: r.get(3)?,
                    streak_days: r.get(4)?,
                    points_expire_at: r.get(5)?,
                    session_status: r.get(6)?,
                    session_expires_at: r.get(7)?,
                    session_checked_at: r.get(8)?,
                    last_checkin_status: r.get(9)?,
                    last_checkin_at: r.get(10)?,
                    last_checkin_message: r.get(11)?,
                    last_checkin_points_delta: r.get(12)?,
                    updated_at: r.get(13)?,
                })
            })
            .map_err(|e| format!("list states map: {e}"))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(|e| format!("state row: {e}"))?);
        }
        Ok(out)
    }

    #[allow(dead_code)]
    pub fn get_account_state(&self, account_id: &str) -> Result<Option<AccountStateRow>, String> {
        self.conn
            .query_row(
                "SELECT account_id, display_name, provider, points_balance, streak_days,
                        points_expire_at, session_status, session_expires_at, session_checked_at,
                        last_checkin_status, last_checkin_at, last_checkin_message,
                        last_checkin_points_delta, updated_at
                 FROM monitor_account_state WHERE account_id = ?1",
                params![account_id],
                |r| {
                    Ok(AccountStateRow {
                        account_id: r.get(0)?,
                        display_name: r.get(1)?,
                        provider: r.get(2)?,
                        points_balance: r.get(3)?,
                        streak_days: r.get(4)?,
                        points_expire_at: r.get(5)?,
                        session_status: r.get(6)?,
                        session_expires_at: r.get(7)?,
                        session_checked_at: r.get(8)?,
                        last_checkin_status: r.get(9)?,
                        last_checkin_at: r.get(10)?,
                        last_checkin_message: r.get(11)?,
                        last_checkin_points_delta: r.get(12)?,
                        updated_at: r.get(13)?,
                    })
                },
            )
            .optional()
            .map_err(|e| format!("get_account_state: {e}"))
    }

    pub fn recent_alert_by_dedupe(
        &self,
        dedupe_key: &str,
        since_ms: i64,
    ) -> Result<Option<(String, i64)>, String> {
        self.conn
            .query_row(
                "SELECT id, fired_at FROM monitor_alert_log
                 WHERE dedupe_key = ?1 AND fired_at >= ?2
                 ORDER BY fired_at DESC LIMIT 1",
                params![dedupe_key, since_ms],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(|e| format!("recent_alert: {e}"))
    }

    pub fn insert_alert_log(
        &self,
        id: &str,
        rule_id: &str,
        account_id: &str,
        severity: &str,
        title: &str,
        body: &str,
        dedupe_key: &str,
        fired_at: i64,
    ) -> Result<(), String> {
        self.conn
            .execute(
                "INSERT INTO monitor_alert_log
                 (id, rule_id, account_id, severity, title, body, dedupe_key, fired_at, acked_at)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,NULL)",
                params![id, rule_id, account_id, severity, title, body, dedupe_key, fired_at],
            )
            .map_err(|e| format!("insert_alert_log: {e}"))?;
        Ok(())
    }

    pub fn list_recent_alerts(&self, limit: i64) -> Result<Vec<super::types::MonitorAlert>, String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, rule_id, account_id, severity, title, body, dedupe_key, fired_at
                 FROM monitor_alert_log
                 ORDER BY fired_at DESC
                 LIMIT ?1",
            )
            .map_err(|e| format!("list alerts: {e}"))?;
        let rows = stmt
            .query_map(params![limit], |r| {
                let severity: String = r.get(3)?;
                let body: String = r.get(5)?;
                Ok(super::types::MonitorAlert {
                    id: r.get(0)?,
                    rule: r.get(1)?,
                    account_id: r.get(2)?,
                    severity: severity.clone(),
                    title: r.get(4)?,
                    body: body.clone(),
                    at: r.get(7)?,
                    dedupe_key: r.get(6)?,
                    level: Some(severity),
                    message: Some(body),
                })
            })
            .map_err(|e| format!("list alerts map: {e}"))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(|e| format!("alert row: {e}"))?);
        }
        Ok(out)
    }

    #[allow(dead_code)]
    pub fn set_setting(&self, key: &str, value: &str) -> Result<(), String> {
        self.conn
            .execute(
                "INSERT INTO monitor_settings (key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![key, value],
            )
            .map_err(|e| format!("set_setting: {e}"))?;
        Ok(())
    }
}

/// Tauri managed state。
pub struct MonitorState {
    pub db: Mutex<MonitorDb>,
    #[allow(dead_code)]
    pub db_path: PathBuf,
    pub last_refresh_ms: Mutex<i64>,
}

impl MonitorState {
    pub fn open(app_data: &Path) -> Result<Self, String> {
        let db_path = app_data.join("monitor").join("monitor.db");
        let db = MonitorDb::open(&db_path)?;
        // 空库时写入 DEV seed
        if db.count_accounts()? == 0 || db.count_snapshots()? == 0 {
            db.seed_dev_accounts_and_snapshots()?;
            let _ = db.materialize_from_snapshots()?;
        }
        Ok(Self {
            db: Mutex::new(db),
            db_path,
            last_refresh_ms: Mutex::new(0),
        })
    }
}
