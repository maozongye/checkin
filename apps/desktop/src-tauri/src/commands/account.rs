//! account_list — mock 账号列表（脱敏）。

use super::mock_data::{mock_accounts, Account};
use tauri::command;

#[command]
pub fn account_list() -> Result<Vec<Account>, String> {
    Ok(mock_accounts())
}
