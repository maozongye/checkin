//! Tauri event payloads — event names are fixed: `checkin:progress` / `checkin:done`.

use serde::{Deserialize, Serialize};

pub const EVENT_PROGRESS: &str = "checkin:progress";
pub const EVENT_DONE: &str = "checkin:done";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CheckinProgressEvent {
    pub run_id: String,
    pub account_id: String,
    pub platform: String,
    /// 0-based index in this run's account list
    pub index: usize,
    pub total: usize,
    /// `running` | `Success` | `AlreadyDone` | `Skipped` | `Failed` | `Disabled`
    pub status: String,
    /// Human summary — never include tokens
    pub message: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct RunCounts {
    pub total: u32,
    pub success: u32,
    pub already_done: u32,
    pub skipped: u32,
    pub failed: u32,
    pub disabled: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CheckinItemView {
    pub account_id: String,
    pub platform: String,
    pub outcome: String,
    pub credits_delta: Option<i64>,
    pub streak_days: Option<i32>,
    pub balance: Option<i64>,
    pub message: String,
    pub raw_code: Option<i64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CheckinDoneEvent {
    pub run_id: String,
    /// `manual` | `schedule`
    pub trigger: String,
    pub cron_or_at: Option<String>,
    pub started_at: String,
    pub finished_at: String,
    pub timezone: String,
    pub local_day: String,
    pub counts: RunCounts,
    pub items: Vec<CheckinItemView>,
}

#[derive(Clone, Debug)]
pub enum CheckinEvent {
    Progress(CheckinProgressEvent),
    Done(CheckinDoneEvent),
}

impl CheckinEvent {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Progress(_) => EVENT_PROGRESS,
            Self::Done(_) => EVENT_DONE,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RunSummary {
    pub run_id: String,
    pub trigger: String,
    pub local_day: String,
    pub timezone: String,
    pub started_at: String,
    pub finished_at: String,
    /// `done` | `partial` | `failed`
    pub status: String,
    pub counts: RunCounts,
    pub items: Vec<CheckinItemView>,
}
