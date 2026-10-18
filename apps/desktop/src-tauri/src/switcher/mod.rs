//! Switcher 真路径骨架：PlatformAdapter + SQLite + Vault switch_backup + 多号目标 resolve。
//!
//! - 目标明文：账号仓 `resolve(cred://v1/account/...)`（**不是** switch_backup）
//! - 备份：`put_switch_backup` → `{vault_dir}/switch_blobs/{snapshot_id}.cve`
//! - **不**写 `cred://v1/account/...`；Commit 后 emit `switch://committed`
//! - DB / 日志 / progress **永不**含 token / 密文正文

pub mod adapter;
pub mod backup;
pub mod db;
pub mod material;
pub mod paths;
pub mod process;
pub mod state;

pub use adapter::{
    AccountMeta, DetectResult, LoginFormat, LoginStateBlob, PlatformAdapter, TraeCnAdapter,
    WorkBuddyAdapter,
};
pub use backup::{SwitchBackupStore, VaultSwitchBackupStore};
pub use db::SwitcherDb;
pub use process::client_running_for_product;
pub use state::SwitcherState;

use adapter::product_adapter;

/// 按 platform 字符串取 Adapter（workbuddy | trae_cn）。
pub fn adapter_for(platform: &str) -> Result<Box<dyn PlatformAdapter>, String> {
    product_adapter(platform)
}

/// 归一化产品 id：`wb_desktop`|`workbuddy` → workbuddy；`trae_cn` → trae_cn。
pub fn normalize_product(raw: &str) -> Result<String, String> {
    match raw {
        "wb_desktop" | "workbuddy" => Ok("workbuddy".into()),
        "trae_cn" => Ok("trae_cn".into()),
        other => Err(format!("unsupported product: {other}")),
    }
}
