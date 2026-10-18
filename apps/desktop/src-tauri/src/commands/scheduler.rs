//! scheduler_get / scheduler_set — 内存态。

use super::mock_data::SchedulerConfig;
use super::state::AppState;
use tauri::{State, command};

#[command]
pub fn scheduler_get(state: State<'_, AppState>) -> Result<SchedulerConfig, String> {
    let guard = state
        .scheduler
        .lock()
        .map_err(|_| "scheduler lock poisoned".to_string())?;
    Ok(guard.clone())
}

#[command]
pub fn scheduler_set(
    state: State<'_, AppState>,
    config: SchedulerConfig,
) -> Result<SchedulerConfig, String> {
    let mut guard = state
        .scheduler
        .lock()
        .map_err(|_| "scheduler lock poisoned".to_string())?;
    *guard = config.clone();
    Ok(config)
}
