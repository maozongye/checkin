//! IPC commands — Frontend 壳已对接下列名字（lib/api）。
//!
//! - account_list / checkin_run_all / scheduler_* / monitor_*（本模块）
//! - **真导入**：`credential_store` / `credential_delete`（接 credential-vault）
//! - **仅 meta**：`credential_meta_store`（put/get/delete/list，无 secret；壳层 list 用）
//! - switch_*（见 `switch.rs`，由 Switcher 维护，勿覆盖）
//!
//! 账号列表等仍 mock；签到已接 checkin-core Orchestrator；凭据明文只进钥匙串/Memory，不进 SQLite。
//!
//! `generate_handler!` 必须引用**定义** `#[command]` 的子模块路径
//! （`pub use` 不会带走 `__cmd__*` 伴生项）。

pub mod mock_data;
pub mod state;
pub mod account;
pub mod checkin;
pub mod scheduler;
pub mod monitor;
pub mod credential;
pub mod credential_meta;
pub mod switch;

pub use state::AppState;

use tauri::command;

/// 冒烟用 stub；发版前可删。
#[command]
pub fn greet(name: String) -> String {
    format!("你好，{name} — Buddy 加油站")
}
