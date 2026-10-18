//! checkin-core — WorkBuddy desktop + Trae CN daily checkin (desktop crate).
//!
//! Interface layer locked per docs/01–03, 05, rust-sketch.
//! v1 scope: Windows-first; daily-checkin only; no travel/makeup.
//!
//! **Runtime gate:** default [`RuntimeMode::DryRun`]; LiveHttp requires
//! `allow_live_http = true` (keep off until setup on-machine acceptance).
//!
//! **Credentials:** [`CredentialsRef`] holds `cred://v1/account/...`;
//! LiveHttp resolves via [`vault_bridge`] → `credential_vault::CredentialStore`
//! (no parallel vault trait). DesktopPaths = import source only.

pub mod adapter;
pub mod codes;
pub mod config;
pub mod db;
pub mod events;
pub mod http_client;
pub mod import_desktop;
pub mod orchestrator;
pub mod probe;
pub mod runtime;
pub mod trae;
pub mod types;
pub mod vault_bridge;
pub mod workbuddy;

pub use adapter::PlatformAdapter;
pub use config::{DesktopPaths, PlatformConfig, TraeConfig, WorkBuddyConfig};
pub use db::CheckinDb;
pub use events::{
    CheckinDoneEvent, CheckinEvent, CheckinItemView, CheckinProgressEvent, RunCounts, RunSummary,
    EVENT_DONE, EVENT_PROGRESS,
};
pub use orchestrator::{CheckinOrchestrator, OrchestratorConfig, Trigger};
pub use probe::{probe_at, probe_desktop_defaults, PathProbeSummary};
pub use runtime::{CheckinRuntimeOptions, EffectiveRuntime, RuntimeMode};
pub use types::{
    AccountSpec, AdapterError, CheckinCtx, CheckinOutcome, CredentialsRef, HealthReport,
    OrchestratorError, PointsSnapshot,
};
pub use vault_bridge::{resolve_for_checkin, validate_ref_shape, ResolvedSession, VaultBridgeError};
pub use import_desktop::{
    format_import_error, import_both_defaults, import_both_defaults_with_replace,
    import_both_files, import_both_files_with_replace, import_trae_default_path,
    import_trae_storage_file, import_workbuddy_default_path, import_workbuddy_info_file,
    resolve_cli_path, ImportBothReport, ImportError, ImportReport,
};

pub use trae::TraeAdapter;
pub use workbuddy::WorkBuddyAdapter;
