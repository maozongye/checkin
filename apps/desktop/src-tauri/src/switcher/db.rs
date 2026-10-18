//! switcher.db：login_snapshot + switch_log（credential_ref 只存 URI，不存 blob）。

use std::path::Path;

use rusqlite::{params, Connection};

#[derive(Debug, Clone)]
pub struct LoginSnapshotRow {
    pub snapshot_id: String,
    pub product: String,
    pub account_key: Option<String>,
    pub format: String,
    pub source_paths_json: String,
    pub credential_ref: String,
    pub account_meta_json: Option<String>,
    pub created_at: i64,
    pub note: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SwitchLogRow {
    pub id: String,
    pub product: String,
    pub from_account_id: Option<String>,
    pub to_account_id: String,
    pub status: String,
    pub fail_stage: Option<String>,
    pub backup_snapshot_id: Option<String>,
    pub detail: Option<String>,
    pub created_at: i64,
    pub finished_at: Option<i64>,
}

pub struct SwitcherDb {
    conn: Connection,
}

impl SwitcherDb {
    pub fn open(db_path: &Path) -> Result<Self, String> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("mkdir db parent: {e}"))?;
        }
        let conn = Connection::open(db_path).map_err(|e| format!("open switcher.db: {e}"))?;
        let db = Self { conn };
        db.migrate()?;
        Ok(db)
    }

    fn migrate(&self) -> Result<(), String> {
        self.conn
            .execute_batch(
                r#"
CREATE TABLE IF NOT EXISTS login_snapshot (
  snapshot_id TEXT PRIMARY KEY,
  product TEXT NOT NULL,
  account_key TEXT,
  format TEXT NOT NULL,
  source_paths_json TEXT NOT NULL,
  credential_ref TEXT NOT NULL,
  account_meta_json TEXT,
  created_at INTEGER NOT NULL,
  note TEXT
);

CREATE TABLE IF NOT EXISTS switch_log (
  id TEXT PRIMARY KEY,
  product TEXT NOT NULL,
  from_account_id TEXT,
  to_account_id TEXT NOT NULL,
  status TEXT NOT NULL,
  fail_stage TEXT,
  backup_snapshot_id TEXT,
  detail TEXT,
  created_at INTEGER NOT NULL,
  finished_at INTEGER
);

CREATE INDEX IF NOT EXISTS idx_switch_log_created ON switch_log(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_login_snapshot_product ON login_snapshot(product, created_at DESC);
"#,
            )
            .map_err(|e| format!("migrate switcher.db: {e}"))?;
        Ok(())
    }

    pub fn insert_snapshot(&self, row: &LoginSnapshotRow) -> Result<(), String> {
        self.conn
            .execute(
                r#"INSERT INTO login_snapshot(
                    snapshot_id, product, account_key, format, source_paths_json,
                    credential_ref, account_meta_json, created_at, note
                ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)"#,
                params![
                    row.snapshot_id,
                    row.product,
                    row.account_key,
                    row.format,
                    row.source_paths_json,
                    row.credential_ref,
                    row.account_meta_json,
                    row.created_at,
                    row.note,
                ],
            )
            .map_err(|e| format!("insert login_snapshot: {e}"))?;
        Ok(())
    }

    pub fn get_snapshot(&self, snapshot_id: &str) -> Result<Option<LoginSnapshotRow>, String> {
        let mut stmt = self
            .conn
            .prepare(
                r#"SELECT snapshot_id, product, account_key, format, source_paths_json,
                          credential_ref, account_meta_json, created_at, note
                   FROM login_snapshot WHERE snapshot_id = ?1"#,
            )
            .map_err(|e| format!("prepare get_snapshot: {e}"))?;
        let mut rows = stmt
            .query(params![snapshot_id])
            .map_err(|e| format!("query get_snapshot: {e}"))?;
        if let Some(r) = rows.next().map_err(|e| format!("row get_snapshot: {e}"))? {
            Ok(Some(LoginSnapshotRow {
                snapshot_id: r.get(0).map_err(|e| e.to_string())?,
                product: r.get(1).map_err(|e| e.to_string())?,
                account_key: r.get(2).map_err(|e| e.to_string())?,
                format: r.get(3).map_err(|e| e.to_string())?,
                source_paths_json: r.get(4).map_err(|e| e.to_string())?,
                credential_ref: r.get(5).map_err(|e| e.to_string())?,
                account_meta_json: r.get(6).map_err(|e| e.to_string())?,
                created_at: r.get(7).map_err(|e| e.to_string())?,
                note: r.get(8).map_err(|e| e.to_string())?,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn insert_log(&self, row: &SwitchLogRow) -> Result<(), String> {
        self.conn
            .execute(
                r#"INSERT INTO switch_log(
                    id, product, from_account_id, to_account_id, status,
                    fail_stage, backup_snapshot_id, detail, created_at, finished_at
                ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)"#,
                params![
                    row.id,
                    row.product,
                    row.from_account_id,
                    row.to_account_id,
                    row.status,
                    row.fail_stage,
                    row.backup_snapshot_id,
                    row.detail,
                    row.created_at,
                    row.finished_at,
                ],
            )
            .map_err(|e| format!("insert switch_log: {e}"))?;
        Ok(())
    }

    pub fn update_log_status(
        &self,
        id: &str,
        status: &str,
        fail_stage: Option<&str>,
        detail: Option<&str>,
        finished_at: Option<i64>,
    ) -> Result<(), String> {
        self.conn
            .execute(
                r#"UPDATE switch_log SET status=?2, fail_stage=?3, detail=?4, finished_at=?5
                   WHERE id=?1"#,
                params![id, status, fail_stage, detail, finished_at],
            )
            .map_err(|e| format!("update switch_log: {e}"))?;
        Ok(())
    }

    pub fn update_log_backup(&self, id: &str, backup_snapshot_id: &str) -> Result<(), String> {
        self.conn
            .execute(
                r#"UPDATE switch_log SET backup_snapshot_id=?2 WHERE id=?1"#,
                params![id, backup_snapshot_id],
            )
            .map_err(|e| format!("update switch_log backup: {e}"))?;
        Ok(())
    }

    pub fn get_log(&self, id: &str) -> Result<Option<SwitchLogRow>, String> {
        let mut stmt = self
            .conn
            .prepare(
                r#"SELECT id, product, from_account_id, to_account_id, status,
                          fail_stage, backup_snapshot_id, detail, created_at, finished_at
                   FROM switch_log WHERE id = ?1"#,
            )
            .map_err(|e| format!("prepare get_log: {e}"))?;
        let mut rows = stmt
            .query(params![id])
            .map_err(|e| format!("query get_log: {e}"))?;
        if let Some(r) = rows.next().map_err(|e| format!("row get_log: {e}"))? {
            Ok(Some(SwitchLogRow {
                id: r.get(0).map_err(|e| e.to_string())?,
                product: r.get(1).map_err(|e| e.to_string())?,
                from_account_id: r.get(2).map_err(|e| e.to_string())?,
                to_account_id: r.get(3).map_err(|e| e.to_string())?,
                status: r.get(4).map_err(|e| e.to_string())?,
                fail_stage: r.get(5).map_err(|e| e.to_string())?,
                backup_snapshot_id: r.get(6).map_err(|e| e.to_string())?,
                detail: r.get(7).map_err(|e| e.to_string())?,
                created_at: r.get(8).map_err(|e| e.to_string())?,
                finished_at: r.get(9).map_err(|e| e.to_string())?,
            }))
        } else {
            Ok(None)
        }
    }
}
