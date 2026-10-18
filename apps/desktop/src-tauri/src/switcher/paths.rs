//! 官方登录态路径解析（路径合同 v1 · Windows 已验证；非 Windows 返回 None）。

use std::path::PathBuf;

/// WorkBuddy 桌面：`%LOCALAPPDATA%\CodeBuddyExtension\Data\Public\auth\workbuddy-desktop.info`
pub fn workbuddy_auth_path() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        let local = std::env::var_os("LOCALAPPDATA")?;
        Some(
            PathBuf::from(local)
                .join("CodeBuddyExtension")
                .join("Data")
                .join("Public")
                .join("auth")
                .join("workbuddy-desktop.info"),
        )
    }
    #[cfg(not(windows))]
    {
        // 非 Windows：合同为 candidate；不 panic，Detect 报 hit=false
        None
    }
}

/// Trae CN：`%APPDATA%\Trae CN\User\globalStorage\storage.json`
pub fn trae_cn_storage_path() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        let roaming = std::env::var_os("APPDATA")?;
        Some(
            PathBuf::from(roaming)
                .join("Trae CN")
                .join("User")
                .join("globalStorage")
                .join("storage.json"),
        )
    }
    #[cfg(not(windows))]
    {
        None
    }
}

/// Trae CN 认证键（只换此键，不动 deviceId / icube-dc 等）。
pub const TRAE_AUTH_KEY: &str = "iCubeAuthInfo://icube.cloudide";

/// 路径展示用脱敏：只留文件名 + 父级名，避免完整用户目录进日志。
pub fn path_hint(p: &std::path::Path) -> String {
    let file = p
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("?");
    let parent = p
        .parent()
        .and_then(|par| par.file_name())
        .and_then(|s| s.to_str())
        .unwrap_or("?");
    format!("{parent}/{file}")
}
