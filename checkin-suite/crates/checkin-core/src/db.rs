//! SQLite open + migrate — schema from docs/03-schema.md / migrations/001_checkin.sql

use crate::types::OrchestratorError;
use rusqlite::{params, Connection};
use std::path::Path;

const MIGRATION_001: &str = include_str!("../migrations/001_checkin.sql");

pub struct CheckinDb {
    conn: Connection,
}

impl CheckinDb {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, OrchestratorError> {
        let conn = Connection::open(path).map_err(|e| OrchestratorError::Db(e.to_string()))?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")
            .map_err(|e| OrchestratorError::Db(e.to_string()))?;
        let db = Self { conn };
        db.migrate()?;
        Ok(db)
    }

    pub fn open_in_memory() -> Result<Self, OrchestratorError> {
        let conn = Connection::open_in_memory().map_err(|e| OrchestratorError::Db(e.to_string()))?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")
            .map_err(|e| OrchestratorError::Db(e.to_string()))?;
        let db = Self { conn };
        db.migrate()?;
        Ok(db)
    }

    pub fn migrate(&self) -> Result<(), OrchestratorError> {
        self.conn
            .execute_batch(MIGRATION_001)
            .map_err(|e| OrchestratorError::Db(format!("migrate: {e}")))?;
        Ok(())
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    pub fn conn_mut(&mut self) -> &mut Connection {
        &mut self.conn
    }

    /// Returns existing terminal outcome for idempotency key, if any.
    pub fn get_terminal_outcome(
        &self,
        account_id: &str,
        local_day: &str,
        platform: &str,
    ) -> Result<Option<String>, OrchestratorError> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT outcome FROM checkin_item
                 WHERE account_id = ?1 AND local_day = ?2 AND platform = ?3
                   AND outcome IN ('Success', 'AlreadyDone')",
            )
            .map_err(|e| OrchestratorError::Db(e.to_string()))?;
        let mut rows = stmt
            .query(params![account_id, local_day, platform])
            .map_err(|e| OrchestratorError::Db(e.to_string()))?;
        if let Some(row) = rows.next().map_err(|e| OrchestratorError::Db(e.to_string()))? {
            let outcome: String = row.get(0).map_err(|e| OrchestratorError::Db(e.to_string()))?;
            Ok(Some(outcome))
        } else {
            Ok(None)
        }
    }

    pub fn insert_run(
        &self,
        id: &str,
        trigger: &str,
        started_at: &str,
        timezone: &str,
        concurrency: i64,
        note: Option<&str>,
    ) -> Result<(), OrchestratorError> {
        self.conn
            .execute(
                "INSERT INTO checkin_run (id, trigger, started_at, finished_at, status, timezone, concurrency, note)
                 VALUES (?1, ?2, ?3, NULL, 'running', ?4, ?5, ?6)",
                params![id, trigger, started_at, timezone, concurrency, note],
            )
            .map_err(|e| OrchestratorError::Db(e.to_string()))?;
        Ok(())
    }

    pub fn finish_run(
        &self,
        id: &str,
        finished_at: &str,
        status: &str,
    ) -> Result<(), OrchestratorError> {
        self.conn
            .execute(
                "UPDATE checkin_run SET finished_at = ?1, status = ?2 WHERE id = ?3",
                params![finished_at, status, id],
            )
            .map_err(|e| OrchestratorError::Db(e.to_string()))?;
        Ok(())
    }

    /// Upsert item. Will not downgrade Success/AlreadyDone unless `force`.
    pub fn upsert_item(
        &self,
        id: &str,
        run_id: &str,
        account_id: &str,
        platform: &str,
        local_day: &str,
        outcome: &str,
        credits_delta: Option<i64>,
        streak_days: Option<i32>,
        balance: Option<i64>,
        message: Option<&str>,
        raw_code: Option<i64>,
        retryable: Option<bool>,
        started_at: &str,
        finished_at: &str,
        force: bool,
    ) -> Result<(), OrchestratorError> {
        if !force {
            if let Some(existing) = self.get_terminal_outcome(account_id, local_day, platform)? {
                if existing == "Success" || existing == "AlreadyDone" {
                    // Keep terminal row; do not overwrite.
                    return Ok(());
                }
            }
        }

        let retryable_i: Option<i64> = retryable.map(|b| if b { 1 } else { 0 });
        self.conn
            .execute(
                "INSERT INTO checkin_item (
                    id, run_id, account_id, platform, local_day, outcome,
                    credits_delta, streak_days, balance, message, raw_code, retryable,
                    started_at, finished_at
                 ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)
                 ON CONFLICT(account_id, local_day, platform) DO UPDATE SET
                    id = excluded.id,
                    run_id = excluded.run_id,
                    outcome = excluded.outcome,
                    credits_delta = excluded.credits_delta,
                    streak_days = excluded.streak_days,
                    balance = excluded.balance,
                    message = excluded.message,
                    raw_code = excluded.raw_code,
                    retryable = excluded.retryable,
                    started_at = excluded.started_at,
                    finished_at = excluded.finished_at
                 WHERE checkin_item.outcome NOT IN ('Success', 'AlreadyDone')
                    OR ?15 = 1",
                params![
                    id,
                    run_id,
                    account_id,
                    platform,
                    local_day,
                    outcome,
                    credits_delta,
                    streak_days,
                    balance,
                    message,
                    raw_code,
                    retryable_i,
                    started_at,
                    finished_at,
                    if force { 1i64 } else { 0i64 },
                ],
            )
            .map_err(|e| OrchestratorError::Db(e.to_string()))?;
        Ok(())
    }

    pub fn list_item_outcomes_for_run(
        &self,
        run_id: &str,
    ) -> Result<Vec<(String, String, String)>, OrchestratorError> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT account_id, platform, outcome FROM checkin_item WHERE run_id = ?1 ORDER BY account_id",
            )
            .map_err(|e| OrchestratorError::Db(e.to_string()))?;
        let rows = stmt
            .query_map(params![run_id], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })
            .map_err(|e| OrchestratorError::Db(e.to_string()))?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| OrchestratorError::Db(e.to_string()))?);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrate_applies() {
        let db = CheckinDb::open_in_memory().expect("open");
        let n: i64 = db
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN ('checkin_run','checkin_item')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 2);
    }
}
