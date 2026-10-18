//! CheckinOrchestrator — docs/02-orchestrator.md
//! Emits via callback (desktop wires to Tauri); concurrency default 3 + jitter.
//! LiveHttp is gated by [`CheckinRuntimeOptions`]; vault resolve required when live.

use crate::adapter::PlatformAdapter;
use crate::db::CheckinDb;
use crate::events::{
    CheckinDoneEvent, CheckinEvent, CheckinItemView, CheckinProgressEvent, RunCounts, RunSummary,
};
use crate::runtime::{CheckinRuntimeOptions, EffectiveRuntime};
use crate::types::{AccountSpec, CheckinCtx, CheckinOutcome, OrchestratorError};
use crate::vault_bridge::{self, VaultBridgeError};
use chrono::{FixedOffset, Utc};
use credential_vault::{CredentialBackend, CredentialStore};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, Semaphore};
use uuid::Uuid;

#[derive(Clone, Debug)]
pub enum Trigger {
    Manual,
    Schedule { cron_or_at: String },
}

impl Trigger {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::Schedule { .. } => "schedule",
        }
    }

    pub fn cron_or_at(&self) -> Option<&str> {
        match self {
            Self::Schedule { cron_or_at } => Some(cron_or_at.as_str()),
            Self::Manual => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct OrchestratorConfig {
    /// Default 3. Architect: multi-account same egress → concurrency ≤ 3.
    pub concurrency: usize,
    /// Max random delay between account starts (ms); 0 = off. Suggest 200..=2000.
    pub jitter_ms: u64,
    /// Default Asia/Shanghai.
    pub timezone: String,
    /// Extra retries for RateLimited/Network (excluding first attempt). Default 2.
    pub max_retries: u32,
    /// If true, still hit platform when local Success/AlreadyDone exists. Default false.
    pub force_remote: bool,
}

impl Default for OrchestratorConfig {
    fn default() -> Self {
        Self {
            concurrency: 3,
            jitter_ms: 500,
            timezone: "Asia/Shanghai".into(),
            max_retries: 2,
            force_remote: false,
        }
    }
}

pub struct CheckinOrchestrator {
    adapters: HashMap<String, Arc<dyn PlatformAdapter>>,
    pub config: OrchestratorConfig,
}

impl CheckinOrchestrator {
    pub fn new(
        adapters: HashMap<String, Arc<dyn PlatformAdapter>>,
        config: OrchestratorConfig,
    ) -> Self {
        Self { adapters, config }
    }

    /// Build with WorkBuddy + Trae stubs (default PlatformConfig).
    pub fn with_default_adapters(config: OrchestratorConfig) -> Self {
        use crate::trae::TraeAdapter;
        use crate::workbuddy::WorkBuddyAdapter;
        let mut adapters: HashMap<String, Arc<dyn PlatformAdapter>> = HashMap::new();
        adapters.insert(
            "workbuddy".into(),
            Arc::new(WorkBuddyAdapter::with_defaults()),
        );
        adapters.insert("trae".into(), Arc::new(TraeAdapter::with_defaults()));
        Self::new(adapters, config)
    }

    /// One-shot run over `accounts`.
    ///
    /// - `dry_run` + `options` merge via [`CheckinRuntimeOptions::effective`]:
    ///   any dry request → dry; LiveHttp only when mode=LiveHttp **and**
    ///   `allow_live_http=true` **and** `dry_run=false`.
    /// - When effective dry: adapters must not POST; no vault plaintext resolve.
    /// - When effective live: `vault` must be `Some`; resolve account `cred://`
    ///   before adapter call (switch_backup rejected).
    /// - `emit`: `checkin:progress` / `checkin:done` (never tokens).
    /// - Single-account failure never aborts the rest.
    pub async fn run_all<F, B>(
        &self,
        db: Arc<Mutex<CheckinDb>>,
        accounts: &[AccountSpec],
        trigger: Trigger,
        dry_run: bool,
        options: CheckinRuntimeOptions,
        vault: Option<&CredentialStore<B>>,
        mut emit: F,
    ) -> Result<RunSummary, OrchestratorError>
    where
        F: FnMut(CheckinEvent) + Send,
        B: CredentialBackend,
    {
        let effective = options.effective(dry_run);
        let started = now_shanghai();
        let local_day = local_day_shanghai();
        let run_id = Uuid::new_v4().to_string();
        let timezone = self.config.timezone.clone();
        let concurrency = self.config.concurrency.max(1);

        let enabled: Vec<AccountSpec> = accounts
            .iter()
            .filter(|a| a.enabled)
            .cloned()
            .collect();
        let total = enabled.len();

        let note = run_note(&effective, &trigger);

        {
            let dbg = db.lock().await;
            dbg.insert_run(
                &run_id,
                trigger.as_str(),
                &started,
                &timezone,
                concurrency as i64,
                note,
            )?;
        }

        let sem = Arc::new(Semaphore::new(concurrency));
        let results: Arc<Mutex<Vec<(usize, CheckinItemView, CheckinOutcome)>>> =
            Arc::new(Mutex::new(Vec::new()));

        for (index, acc) in enabled.iter().enumerate() {
            emit(CheckinEvent::Progress(CheckinProgressEvent {
                run_id: run_id.clone(),
                account_id: acc.account_id.clone(),
                platform: acc.platform.clone(),
                index,
                total,
                status: "running".into(),
                message: if effective.gated {
                    "starting (gated→dry_run)".into()
                } else if effective.dry_run {
                    "starting (dry_run)".into()
                } else {
                    "starting".into()
                },
            }));
        }

        let mut handles = Vec::new();
        for (index, acc) in enabled.into_iter().enumerate() {
            let permit = sem
                .clone()
                .acquire_owned()
                .await
                .map_err(|e| OrchestratorError::Config(format!("semaphore: {e}")))?;

            let adapters = self.adapters.clone();
            let config = self.config.clone();
            let db = db.clone();
            let run_id = run_id.clone();
            let local_day = local_day.clone();
            let timezone = timezone.clone();
            let results = results.clone();
            let eff_dry = effective.dry_run;
            let gated = effective.gated;

            // Pre-resolve on live path (sync) before spawn; pass Arc session into adapter.
            // dry_run: never resolve plaintext.
            let (pre_resolve_err, live_session): (
                Option<VaultBridgeError>,
                Option<Arc<vault_bridge::ResolvedSession>>,
            ) = if !eff_dry {
                match vault {
                    None => (
                        Some(VaultBridgeError::Resolve(
                            "vault_required_for_live_http".into(),
                        )),
                        None,
                    ),
                    Some(store) => match vault_bridge::resolve_for_checkin(store, &acc.credentials_ref)
                    {
                        Ok(session) => (None, Some(Arc::new(session))),
                        Err(e) => (Some(e), None),
                    },
                }
            } else {
                let err = if acc.credentials_ref.as_cred_uri().is_some() {
                    if let Err(e) = vault_bridge::validate_ref_shape(&acc.credentials_ref) {
                        if matches!(e, VaultBridgeError::SwitchBackupRejected) {
                            Some(e)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                };
                (err, None)
            };

            let handle = tokio::spawn(async move {
                let _permit = permit;

                if config.jitter_ms > 0 {
                    let nanos = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.subsec_nanos() as u64)
                        .unwrap_or(0);
                    let jitter = (nanos.wrapping_mul(31).wrapping_add(index as u64 * 97))
                        % (config.jitter_ms + 1);
                    if jitter > 0 {
                        tokio::time::sleep(std::time::Duration::from_millis(jitter)).await;
                    }
                }

                let item_started = now_shanghai();
                let outcome = if let Some(err) = pre_resolve_err {
                    map_vault_err(err)
                } else {
                    process_one_account(
                        &adapters,
                        &db,
                        &config,
                        &acc,
                        &run_id,
                        &local_day,
                        &timezone,
                        eff_dry,
                        gated,
                        live_session,
                    )
                    .await
                };

                let item_finished = now_shanghai();
                let view = CheckinItemView {
                    account_id: acc.account_id.clone(),
                    platform: acc.platform.clone(),
                    outcome: outcome.kind_str().to_string(),
                    credits_delta: outcome.credits_delta(),
                    streak_days: outcome.streak_days(),
                    balance: outcome.balance(),
                    message: outcome.message(),
                    raw_code: outcome.raw_code(),
                };

                {
                    let dbg = db.lock().await;
                    let item_id = Uuid::new_v4().to_string();
                    let _ = dbg.upsert_item(
                        &item_id,
                        &run_id,
                        &acc.account_id,
                        &acc.platform,
                        &local_day,
                        outcome.kind_str(),
                        outcome.credits_delta(),
                        outcome.streak_days(),
                        outcome.balance(),
                        Some(&outcome.message()),
                        outcome.raw_code(),
                        outcome.retryable(),
                        &item_started,
                        &item_finished,
                        config.force_remote,
                    );
                }

                results.lock().await.push((index, view, outcome));
            });
            handles.push(handle);
        }

        for h in handles {
            let _ = h.await;
        }

        let mut collected = results.lock().await;
        collected.sort_by_key(|(i, _, _)| *i);
        let items: Vec<CheckinItemView> = collected.iter().map(|(_, v, _)| v.clone()).collect();
        let outcomes: Vec<&CheckinOutcome> = collected.iter().map(|(_, _, o)| o).collect();

        for (index, view, _) in collected.iter() {
            emit(CheckinEvent::Progress(CheckinProgressEvent {
                run_id: run_id.clone(),
                account_id: view.account_id.clone(),
                platform: view.platform.clone(),
                index: *index,
                total,
                status: view.outcome.clone(),
                message: view.message.clone(),
            }));
        }

        let counts = tally(&outcomes);
        let status = run_status(&counts);
        let finished = now_shanghai();

        {
            let dbg = db.lock().await;
            dbg.finish_run(&run_id, &finished, &status)?;
        }

        let done = CheckinDoneEvent {
            run_id: run_id.clone(),
            trigger: trigger.as_str().to_string(),
            cron_or_at: trigger.cron_or_at().map(|s| s.to_string()),
            started_at: started.clone(),
            finished_at: finished.clone(),
            timezone: timezone.clone(),
            local_day: local_day.clone(),
            counts: counts.clone(),
            items: items.clone(),
        };
        emit(CheckinEvent::Done(done));

        Ok(RunSummary {
            run_id,
            trigger: trigger.as_str().to_string(),
            local_day,
            timezone,
            started_at: started,
            finished_at: finished,
            status,
            counts,
            items,
        })
    }
}

fn run_note<'a>(effective: &'a EffectiveRuntime, trigger: &'a Trigger) -> Option<&'a str> {
    if effective.gated {
        Some("gated")
    } else if effective.dry_run {
        Some("dry_run")
    } else {
        trigger.cron_or_at()
    }
}

fn map_vault_err(err: VaultBridgeError) -> CheckinOutcome {
    match err {
        VaultBridgeError::NotFound
        | VaultBridgeError::AuthInvalid(_)
        | VaultBridgeError::InvalidRef(_)
        | VaultBridgeError::NotCredUri => CheckinOutcome::Disabled {
            reason: err.to_string(),
        },
        other => CheckinOutcome::Failed {
            code: None,
            message: other.to_string(),
            retryable: false,
        },
    }
}

async fn process_one_account(
    adapters: &HashMap<String, Arc<dyn PlatformAdapter>>,
    db: &Arc<Mutex<CheckinDb>>,
    config: &OrchestratorConfig,
    acc: &AccountSpec,
    run_id: &str,
    local_day: &str,
    timezone: &str,
    dry_run: bool,
    gated: bool,
    session: Option<Arc<vault_bridge::ResolvedSession>>,
) -> CheckinOutcome {
    if !config.force_remote {
        let dbg = db.lock().await;
        if let Ok(Some(_)) = dbg.get_terminal_outcome(&acc.account_id, local_day, &acc.platform) {
            return CheckinOutcome::AlreadyDone {
                credits_delta: None,
                streak_days: None,
                balance: None,
                message: "local idempotent hit".into(),
                raw_code: None,
            };
        }
    }

    let Some(adapter) = adapters.get(&acc.platform) else {
        return CheckinOutcome::Failed {
            code: None,
            message: format!("no adapter for platform {}", acc.platform),
            retryable: false,
        };
    };

    let mut attempt: u32 = 1;
    let max_attempts = 1 + config.max_retries;

    loop {
        let ctx = CheckinCtx {
            account_id: acc.account_id.clone(),
            platform: acc.platform.clone(),
            credentials_ref: acc.credentials_ref.clone(),
            local_day: local_day.to_string(),
            timezone: timezone.to_string(),
            dry_run,
            run_id: run_id.to_string(),
            attempt,
            session: session.clone(),
        };

        match adapter.checkin(&ctx).await {
            Ok(mut outcome) => {
                // Annotate gated dry-run skips.
                if gated {
                    if let CheckinOutcome::Skipped { reason } = &mut outcome {
                        if reason == "dry_run" {
                            *reason = "dry_run_gated".into();
                        }
                    }
                }
                if let CheckinOutcome::Failed {
                    retryable: true, ..
                } = &outcome
                {
                    if attempt < max_attempts {
                        attempt += 1;
                        let backoff_ms = 200u64 * attempt as u64;
                        tokio::time::sleep(std::time::Duration::from_millis(backoff_ms)).await;
                        continue;
                    }
                }
                return outcome;
            }
            Err(e) => {
                let retryable = matches!(
                    e,
                    crate::types::AdapterError::Network(_)
                        | crate::types::AdapterError::RateLimited(_)
                );
                if retryable && attempt < max_attempts {
                    attempt += 1;
                    let backoff_ms = 200u64 * attempt as u64;
                    tokio::time::sleep(std::time::Duration::from_millis(backoff_ms)).await;
                    continue;
                }
                return match e {
                    crate::types::AdapterError::AuthInvalid(msg) => {
                        CheckinOutcome::Disabled { reason: msg }
                    }
                    other => CheckinOutcome::Failed {
                        code: None,
                        message: other.to_string(),
                        retryable: false,
                    },
                };
            }
        }
    }
}

fn tally(outcomes: &[&CheckinOutcome]) -> RunCounts {
    let mut c = RunCounts {
        total: outcomes.len() as u32,
        ..Default::default()
    };
    for o in outcomes {
        match o {
            CheckinOutcome::Success { .. } => c.success += 1,
            CheckinOutcome::AlreadyDone { .. } => c.already_done += 1,
            CheckinOutcome::Skipped { .. } => c.skipped += 1,
            CheckinOutcome::Failed { .. } => c.failed += 1,
            CheckinOutcome::Disabled { .. } => c.disabled += 1,
        }
    }
    c
}

fn run_status(counts: &RunCounts) -> String {
    if counts.total == 0 {
        return "failed".into();
    }
    let okish = counts.success + counts.already_done + counts.skipped;
    let bad = counts.failed + counts.disabled;
    if bad == 0 {
        "done".into()
    } else if okish > 0 {
        "partial".into()
    } else {
        "failed".into()
    }
}

/// Asia/Shanghai (+08:00) wall clock as ISO-8601 with offset.
pub fn now_shanghai() -> String {
    let offset = FixedOffset::east_opt(8 * 3600).expect("+08");
    Utc::now().with_timezone(&offset).to_rfc3339()
}

/// Calendar date YYYY-MM-DD in Asia/Shanghai.
pub fn local_day_shanghai() -> String {
    let offset = FixedOffset::east_opt(8 * 3600).expect("+08");
    Utc::now()
        .with_timezone(&offset)
        .format("%Y-%m-%d")
        .to_string()
}
