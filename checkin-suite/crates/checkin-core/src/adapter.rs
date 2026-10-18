//! PlatformAdapter trait — docs/01-platform-adapter.md

use crate::types::{
    AdapterError, CheckinCtx, CheckinOutcome, HealthReport, PointsSnapshot,
};
use async_trait::async_trait;

#[async_trait]
pub trait PlatformAdapter: Send + Sync {
    /// Stable lowercase id: `"workbuddy"` | `"trae"`.
    fn platform(&self) -> &'static str;

    /// Idempotent checkin. Same `(account_id, local_day[, platform])` must yield
    /// `AlreadyDone` (or platform codes 10001 / 9095), never double-credit.
    async fn checkin(&self, ctx: &CheckinCtx) -> Result<CheckinOutcome, AdapterError>;

    /// Read-only points snapshot (no checkin side effect).
    async fn fetch_points(&self, ctx: &CheckinCtx) -> Result<PointsSnapshot, AdapterError>;

    /// Lightweight auth/host probe; no points mutation.
    async fn health(&self, ctx: &CheckinCtx) -> Result<HealthReport, AdapterError>;
}
