//! Switcher 运行时状态（app_data / DB / VaultSwitchBackupStore）。

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::vault::VaultBackend;

use super::backup::VaultSwitchBackupStore;
use super::db::SwitcherDb;

pub struct SwitcherState {
    pub root: PathBuf,
    pub db: Mutex<SwitcherDb>,
    pub backup: VaultSwitchBackupStore,
}

impl SwitcherState {
    /// `app_data` = 本应用 data 根；DB 在 `{app_data}/switcher/`。
    /// 备份走 vault（`{vault_dir}/switch_blobs/`），不自建 FileStub。
    pub fn open(app_data: &Path, vault: Arc<VaultBackend>) -> Result<Self, String> {
        let root = app_data.join("switcher");
        std::fs::create_dir_all(&root).map_err(|e| format!("mkdir switcher root: {e}"))?;
        let db_path = root.join("switcher.db");
        let db = SwitcherDb::open(&db_path)?;
        let backup = VaultSwitchBackupStore::new(vault);
        Ok(Self {
            root,
            db: Mutex::new(db),
            backup,
        })
    }

    /// 无 Tauri path 时的开发 fallback（仅 DB 路径；vault 仍由调用方注入）。
    pub fn open_fallback_temp(vault: Arc<VaultBackend>) -> Result<Self, String> {
        let base = std::env::temp_dir().join("buddy-gasstation-switcher");
        Self::open(&base, vault)
    }
}
