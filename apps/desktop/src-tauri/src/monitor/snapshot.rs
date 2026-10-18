//! points_snapshot → monitor_account_state 物化。

use super::db::MonitorDb;

/// 确保有可联调数据：空则 DEV seed，再物化。
pub fn ensure_and_materialize(db: &MonitorDb) -> Result<usize, String> {
    if db.count_accounts()? == 0 || db.count_snapshots()? == 0 {
        db.seed_dev_accounts_and_snapshots()?;
    }
    db.materialize_from_snapshots()
}
