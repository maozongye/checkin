//! Core types — aligned with docs/01-platform-adapter.md & rust-sketch.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

/// Opaque vault credential reference — never put tokens into events/logs.
///
/// Preferred form: full `cred://v1/account/{platform}/{account_key}/{uuid}`.
/// Legacy `store`/`key` transitional constructors still emit a non-cred URI
/// string for dry_run only; LiveHttp requires a parseable `cred://` account ref.
#[derive(Clone, Serialize, Deserialize)]
pub struct CredentialsRef {
    /// Opaque `cred://v1/account/...` (or legacy transitional string).
    pub uri: String,
}

impl std::fmt::Debug for CredentialsRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // URI is non-secret; still avoid dumping unexpected payloads.
        f.debug_struct("CredentialsRef")
            .field("uri", &self.uri)
            .finish()
    }
}

impl CredentialsRef {
    pub fn new(uri: impl Into<String>) -> Self {
        Self { uri: uri.into() }
    }

    /// Legacy transitional ctor (dry_run / tests). Prefer [`Self::from_vault_ref`].
    pub fn legacy(store: impl Into<String>, key: impl Into<String>) -> Self {
        let store = store.into();
        let key = key.into();
        Self {
            uri: format!("legacy://{store}/{key}"),
        }
    }

    pub fn as_str(&self) -> &str {
        &self.uri
    }

    /// `Some` when `uri` is a `cred://...` string (may still be switch_backup).
    pub fn as_cred_uri(&self) -> Option<&str> {
        if self.uri.starts_with("cred://") {
            Some(self.uri.as_str())
        } else {
            None
        }
    }

    pub fn from_vault_ref(r: &credential_vault::CredentialRef) -> Self {
        Self {
            uri: r.to_ref_string(),
        }
    }

    /// Parse as vault [`credential_vault::CredentialRef`].
    pub fn parse_vault_ref(&self) -> Result<credential_vault::CredentialRef, String> {
        credential_vault::CredentialRef::parse(&self.uri)
    }

    /// True when URI looks like a vault account ref (not switch_backup / legacy).
    pub fn is_account_ref(&self) -> bool {
        matches!(
            self.parse_vault_ref(),
            Ok(credential_vault::CredentialRef::Account { .. })
        )
    }
}

impl From<&credential_vault::CredentialRef> for CredentialsRef {
    fn from(r: &credential_vault::CredentialRef) -> Self {
        Self::from_vault_ref(r)
    }
}

impl From<credential_vault::CredentialRef> for CredentialsRef {
    fn from(r: credential_vault::CredentialRef) -> Self {
        Self::from_vault_ref(&r)
    }
}

impl TryFrom<&CredentialsRef> for credential_vault::CredentialRef {
    type Error = String;
    fn try_from(value: &CredentialsRef) -> Result<Self, Self::Error> {
        value.parse_vault_ref()
    }
}

#[derive(Clone, Debug)]
pub struct CheckinCtx {
    pub account_id: String,
    /// `"workbuddy"` | `"trae"`
    pub platform: String,
    pub credentials_ref: CredentialsRef,
    /// YYYY-MM-DD
    pub local_day: String,
    /// default Asia/Shanghai
    pub timezone: String,
    pub dry_run: bool,
    pub run_id: String,
    /// 1-based attempt within this run
    pub attempt: u32,
    /// LiveHttp only: vault-resolved session (orchestrator sets; dry_run leaves None).
    /// Debug of [`crate::vault_bridge::ResolvedSession`] redacts secrets.
    pub session: Option<std::sync::Arc<crate::vault_bridge::ResolvedSession>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "PascalCase")]
pub enum CheckinOutcome {
    Success {
        credits_delta: Option<i64>,
        streak_days: Option<i32>,
        balance: Option<i64>,
        message: String,
        raw_code: Option<i64>,
    },
    AlreadyDone {
        credits_delta: Option<i64>,
        streak_days: Option<i32>,
        balance: Option<i64>,
        message: String,
        raw_code: Option<i64>,
    },
    Skipped {
        reason: String,
    },
    Failed {
        code: Option<i64>,
        message: String,
        retryable: bool,
    },
    Disabled {
        reason: String,
    },
}

impl CheckinOutcome {
    /// PascalCase kind string for DB / events.
    pub fn kind_str(&self) -> &'static str {
        match self {
            Self::Success { .. } => "Success",
            Self::AlreadyDone { .. } => "AlreadyDone",
            Self::Skipped { .. } => "Skipped",
            Self::Failed { .. } => "Failed",
            Self::Disabled { .. } => "Disabled",
        }
    }

    pub fn message(&self) -> String {
        match self {
            Self::Success { message, .. }
            | Self::AlreadyDone { message, .. }
            | Self::Failed { message, .. } => message.clone(),
            Self::Skipped { reason } | Self::Disabled { reason } => reason.clone(),
        }
    }

    pub fn credits_delta(&self) -> Option<i64> {
        match self {
            Self::Success { credits_delta, .. } | Self::AlreadyDone { credits_delta, .. } => {
                *credits_delta
            }
            _ => None,
        }
    }

    pub fn streak_days(&self) -> Option<i32> {
        match self {
            Self::Success { streak_days, .. } | Self::AlreadyDone { streak_days, .. } => {
                *streak_days
            }
            _ => None,
        }
    }

    pub fn balance(&self) -> Option<i64> {
        match self {
            Self::Success { balance, .. } | Self::AlreadyDone { balance, .. } => *balance,
            _ => None,
        }
    }

    pub fn raw_code(&self) -> Option<i64> {
        match self {
            Self::Success { raw_code, .. } | Self::AlreadyDone { raw_code, .. } => *raw_code,
            Self::Failed { code, .. } => *code,
            _ => None,
        }
    }

    pub fn retryable(&self) -> Option<bool> {
        match self {
            Self::Failed { retryable, .. } => Some(*retryable),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PointsSnapshot {
    pub balance: Option<i64>,
    pub streak_days: Option<i32>,
    /// Display only — never use as skip-checkin signal.
    pub today_checked_in: Option<bool>,
    pub raw: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HealthReport {
    pub ok: bool,
    pub auth_valid: bool,
    pub message: String,
    pub detail: Value,
}

#[derive(Clone, Debug, Error)]
pub enum AdapterError {
    #[error("network: {0}")]
    Network(String),
    #[error("auth expired: {0}")]
    AuthExpired(String),
    #[error("auth invalid: {0}")]
    AuthInvalid(String),
    #[error("rate limited: {0}")]
    RateLimited(String),
    #[error("protocol: {0}")]
    Protocol(String),
    #[error("config: {0}")]
    Config(String),
}

#[derive(Clone, Debug, Error)]
pub enum OrchestratorError {
    #[error("db: {0}")]
    Db(String),
    #[error("adapter: {0}")]
    Adapter(#[from] AdapterError),
    #[error("config: {0}")]
    Config(String),
}

/// Account row passed into Orchestrator::run_all (Switcher exports this shape).
#[derive(Clone, Debug)]
pub struct AccountSpec {
    pub account_id: String,
    pub platform: String,
    pub credentials_ref: CredentialsRef,
    pub enabled: bool,
}
