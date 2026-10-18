//! PlatformConfig — hosts / ClientID / desktop paths are overridable.
//!
//! Verified-on-Windows defaults (Architect probe) are marked as such;
//! unverified items stay 「候选·未拍板」. Never store tokens in this module.

use serde::{Deserialize, Serialize};

/// Aggregate platform config injected into adapters.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PlatformConfig {
    pub workbuddy: WorkBuddyConfig,
    pub trae: TraeConfig,
    pub desktop: DesktopPaths,
}

/// Relative desktop paths (Windows-first, v1 product scope).
///
/// All fields remain overridable. Defaults = 本机 Windows 已验证（Architect 探测）.
/// Resolve with [`DesktopPaths::resolve_wb_auth`] / [`DesktopPaths::resolve_trae_storage`]
/// using LOCALAPPDATA / APPDATA — never hardcode absolute user paths.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DesktopPaths {
    /// Relative to `%LOCALAPPDATA%` (Windows). 本机已验证默认.
    pub wb_auth_relpath: String,
    /// Relative to `%APPDATA%` / Roaming (Windows). 本机已验证默认.
    pub trae_storage_relpath: String,
    /// Key hint inside Trae storage.json (not a secret). 本机已验证默认.
    pub trae_auth_key_hint: String,
    /// Device id key in Trae storage.json. 本机已验证默认.
    pub trae_device_id_key: String,
}

impl Default for DesktopPaths {
    fn default() -> Self {
        Self {
            // 本机 Windows 已验证默认（仍可配置覆盖）
            wb_auth_relpath: "CodeBuddyExtension/Data/Public/auth/workbuddy-desktop.info"
                .into(),
            // 本机 Windows 已验证默认（仍可配置覆盖）
            trae_storage_relpath: "Trae CN/User/globalStorage/storage.json".into(),
            // 键名提示，非密钥 — 本机已验证默认
            trae_auth_key_hint: "iCubeAuthInfo://icube.cloudide".into(),
            // 本机已验证默认
            trae_device_id_key: "telemetry.devDeviceId".into(),
        }
    }
}

impl DesktopPaths {
    /// `%LOCALAPPDATA%` + wb_auth_relpath. Returns None if LOCALAPPDATA unset.
    pub fn resolve_wb_auth(&self) -> Option<std::path::PathBuf> {
        let base = std::env::var_os("LOCALAPPDATA")?;
        Some(std::path::PathBuf::from(base).join(&self.wb_auth_relpath))
    }

    /// `%APPDATA%` + trae_storage_relpath. Returns None if APPDATA unset.
    pub fn resolve_trae_storage(&self) -> Option<std::path::PathBuf> {
        let base = std::env::var_os("APPDATA")?;
        Some(std::path::PathBuf::from(base).join(&self.trae_storage_relpath))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorkBuddyConfig {
    /// 本机已验证默认：登录态 domain=copilot.tencent.com 命中备选升格为主默认.
    pub base_url: String,
    /// 候选备选（仍可配置）.
    pub alt_base_url: String,
    /// Global host — accounts on this host must Skipped (no checkin).
    /// 候选·未拍板：https://www.workbuddy.ai
    pub global_base_url: String,
    /// 候选·未拍板 path：/v2/billing/meter/daily-checkin
    pub checkin_path: String,
    /// 候选·未拍板 path：/v2/billing/meter/checkin-activity-status
    pub status_path: String,
    /// Token refresh path (Python对照).
    pub refresh_path: String,
    pub force_direct: bool,
}

impl Default for WorkBuddyConfig {
    fn default() -> Self {
        Self {
            // 本机已验证默认（仍可配置）
            base_url: "https://copilot.tencent.com".into(),
            // 候选备选（仍可配置）
            alt_base_url: "https://www.codebuddy.cn".into(),
            // 候选·未拍板
            global_base_url: "https://www.workbuddy.ai".into(),
            // 候选·未拍板
            checkin_path: "/v2/billing/meter/daily-checkin".into(),
            // 候选·未拍板
            status_path: "/v2/billing/meter/checkin-activity-status".into(),
            refresh_path: "/v2/plugin/auth/token/refresh".into(),
            force_direct: true,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TraeConfig {
    /// 候选·未拍板：https://api.trae.cn
    pub ug_host: String,
    /// 候选·未拍板：https://api.trae.com.cn
    pub oauth_host: String,
    /// 候选·未拍板（默认候选）；旧版见 TRAE_CLIENT_ID_LEGACY_CANDIDATE
    pub client_id: String,
    /// 候选·未拍板 path：/trae/api/v2/ug/checkin_credits/claim
    pub claim_path: String,
    /// 候选·未拍板 path：/trae/api/v2/ug/checkin_credits/status
    pub status_path: String,
    pub force_direct: bool,
    /// 候选·未拍板：1
    pub req_source: i32,
}

impl Default for TraeConfig {
    fn default() -> Self {
        Self {
            // 候选·未拍板
            ug_host: "https://api.trae.cn".into(),
            // 候选·未拍板
            oauth_host: "https://api.trae.com.cn".into(),
            // 候选·未拍板 — Architect 代定默认候选，仍配置化
            client_id: "ono9krqynydwx5".into(),
            // 候选·未拍板
            claim_path: "/trae/api/v2/ug/checkin_credits/claim".into(),
            // 候选·未拍板
            status_path: "/trae/api/v2/ug/checkin_credits/status".into(),
            force_direct: true,
            // 候选·未拍板
            req_source: 1,
        }
    }
}

/// 旧版 Trae ClientID 候选（未拍板）
pub const TRAE_CLIENT_ID_LEGACY_CANDIDATE: &str = "en1oxy7wnw8j9n";

/// Product scope notes (v1) — see docs/HANDOFF + README.
pub mod scope {
    /// First release targets WorkBuddy desktop + Trae CN only; Windows first.
    pub const PRODUCTS: &str = "workbuddy_desktop+trae_cn";
    /// Orchestration covers daily-checkin only (no travel / makeup).
    pub const ORCHESTRATION: &str = "daily-checkin-only";
    /// v1 points: record credits_delta; fetch_points best-effort; entitlement optional.
    pub const POINTS_POLICY: &str = "credits_delta_v1_entitlement_optional";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wb_base_default_is_copilot() {
        let c = WorkBuddyConfig::default();
        assert_eq!(c.base_url, "https://copilot.tencent.com");
        assert_eq!(c.alt_base_url, "https://www.codebuddy.cn");
    }

    #[test]
    fn desktop_paths_defaults() {
        let d = DesktopPaths::default();
        assert!(d.wb_auth_relpath.ends_with("workbuddy-desktop.info"));
        assert!(d.trae_storage_relpath.contains("Trae CN"));
        assert_eq!(d.trae_auth_key_hint, "iCubeAuthInfo://icube.cloudide");
        assert_eq!(d.trae_device_id_key, "telemetry.devDeviceId");
    }

    #[test]
    fn trae_client_id_candidate() {
        assert_eq!(TraeConfig::default().client_id, "ono9krqynydwx5");
    }
}
