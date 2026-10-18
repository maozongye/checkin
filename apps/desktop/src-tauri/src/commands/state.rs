//! 进程内共享状态（scheduler + 可选 credential **meta**；无 secret）。
//! 真 vault：`CredentialStore::with_vault_dir` 由 Auth 在 `vault::VaultState` 注入。

use super::mock_data::SchedulerConfig;
use serde::{Deserialize, Serialize};
use std::sync::Mutex;

/// 仅 meta（credential_meta_store）；不含 secret。
#[derive(Clone, Serialize, Deserialize)]
pub struct CredentialMeta {
    pub credential_ref: String,
    pub label: String,
    pub account_id: String,
}

pub struct AppState {
    pub scheduler: Mutex<SchedulerConfig>,
    /// 旧壳 list 用；真导入走 `credential_store`，不写此表
    pub credentials: Mutex<Vec<CredentialMeta>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            scheduler: Mutex::new(SchedulerConfig::default()),
            credentials: Mutex::new(Vec::new()),
        }
    }
}
