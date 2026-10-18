//! HTTP checkin runtime gate — default DryRun; LiveHttp requires explicit allow.

use serde::{Deserialize, Serialize};

/// Runtime mode for platform checkin HTTP.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeMode {
    /// Never POST; adapters return Skipped / probe only.
    #[default]
    DryRun,
    /// Real HTTP (still requires `allow_live_http = true`).
    LiveHttp,
}

/// Options passed into `CheckinOrchestrator::run_all`.
///
/// **Safety defaults:** `mode = DryRun`, `allow_live_http = false`.
/// Setup installers must keep LiveHttp off until on-machine acceptance.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckinRuntimeOptions {
    pub mode: RuntimeMode,
    /// Must be explicitly `true` to permit LiveHttp. Default `false`.
    pub allow_live_http: bool,
}

impl Default for CheckinRuntimeOptions {
    fn default() -> Self {
        Self {
            mode: RuntimeMode::DryRun,
            allow_live_http: false,
        }
    }
}

/// Result of merging `dry_run` flag with [`CheckinRuntimeOptions`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EffectiveRuntime {
    /// Final dry-run decision (true = no live HTTP).
    pub dry_run: bool,
    /// True when caller asked LiveHttp but gate forced DryRun.
    pub gated: bool,
    /// Note for `checkin_run.note` / events (never tokens).
    pub note: Option<&'static str>,
}

impl CheckinRuntimeOptions {
    /// Merge explicit `dry_run` parameter with options.
    /// Any request for dry → dry. LiveHttp only when `mode=LiveHttp`
    /// **and** `allow_live_http=true` **and** `dry_run=false`.
    pub fn effective(&self, dry_run: bool) -> EffectiveRuntime {
        if dry_run {
            return EffectiveRuntime {
                dry_run: true,
                gated: false,
                note: Some("dry_run"),
            };
        }
        match self.mode {
            RuntimeMode::DryRun => EffectiveRuntime {
                dry_run: true,
                gated: false,
                note: Some("dry_run"),
            },
            RuntimeMode::LiveHttp => {
                if self.allow_live_http {
                    EffectiveRuntime {
                        dry_run: false,
                        gated: false,
                        note: None,
                    }
                } else {
                    EffectiveRuntime {
                        dry_run: true,
                        gated: true,
                        note: Some("gated"),
                    }
                }
            }
        }
    }

    /// Convenience: default options (always dry).
    pub fn dry_run_only() -> Self {
        Self::default()
    }

    /// LiveHttp intent — still needs `allow_live_http = true` to actually go live.
    pub fn live_http_intent() -> Self {
        Self {
            mode: RuntimeMode::LiveHttp,
            allow_live_http: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_dry() {
        let o = CheckinRuntimeOptions::default();
        let e = o.effective(false);
        assert!(e.dry_run);
        assert!(!e.gated);
        assert_eq!(e.note, Some("dry_run"));
    }

    #[test]
    fn live_without_allow_is_gated() {
        let o = CheckinRuntimeOptions {
            mode: RuntimeMode::LiveHttp,
            allow_live_http: false,
        };
        let e = o.effective(false);
        assert!(e.dry_run);
        assert!(e.gated);
        assert_eq!(e.note, Some("gated"));
    }

    #[test]
    fn live_with_allow_is_live() {
        let o = CheckinRuntimeOptions {
            mode: RuntimeMode::LiveHttp,
            allow_live_http: true,
        };
        let e = o.effective(false);
        assert!(!e.dry_run);
        assert!(!e.gated);
        assert_eq!(e.note, None);
    }

    #[test]
    fn dry_run_flag_overrides_live_allow() {
        let o = CheckinRuntimeOptions {
            mode: RuntimeMode::LiveHttp,
            allow_live_http: true,
        };
        let e = o.effective(true);
        assert!(e.dry_run);
        assert_eq!(e.note, Some("dry_run"));
    }
}
