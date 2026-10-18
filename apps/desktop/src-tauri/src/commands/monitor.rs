//! monitor_get_dashboard / monitor_refresh — SQLite 真路径（薄 IPC）。

use crate::monitor::{self, MonitorDashboard, MonitorState};
use tauri::{AppHandle, State, command};

#[command]
pub fn monitor_get_dashboard(state: State<'_, MonitorState>) -> Result<MonitorDashboard, String> {
    monitor::get_dashboard(&state)
}

#[command]
pub fn monitor_refresh(
    app: AppHandle,
    state: State<'_, MonitorState>,
) -> Result<MonitorDashboard, String> {
    monitor::refresh(&app, &state)
}
