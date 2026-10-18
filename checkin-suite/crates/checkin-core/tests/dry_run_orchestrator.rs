//! Integration: dry_run / gate / vault resolve / LiveHttp mock.

use checkin_core::{
    AccountSpec, CheckinDb, CheckinEvent, CheckinOrchestrator, CheckinRuntimeOptions,
    CredentialsRef, DesktopPaths, OrchestratorConfig, RuntimeMode, TraeAdapter,
    Trigger, WorkBuddyAdapter, WorkBuddyConfig, EVENT_DONE, EVENT_PROGRESS,
};
use checkin_core::adapter::PlatformAdapter;
use credential_vault::{
    CredentialStore, MaterialKind, MemoryBackend, PlatformId, StoreArgs,
};
use serde_json::json;
use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex};
use tokio::sync::Mutex;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn legacy_cred(key: &str) -> CredentialsRef {
    CredentialsRef::legacy("accounts_json", key)
}

#[tokio::test]
async fn dry_run_two_accounts_skipped_and_migrated() {
    let db = Arc::new(Mutex::new(
        CheckinDb::open_in_memory().expect("migrate in-memory"),
    ));

    let orch = CheckinOrchestrator::with_default_adapters(OrchestratorConfig {
        concurrency: 3,
        jitter_ms: 0,
        timezone: "Asia/Shanghai".into(),
        max_retries: 0,
        force_remote: false,
    });

    let accounts = vec![
        AccountSpec {
            account_id: "wb-dry-1".into(),
            platform: "workbuddy".into(),
            credentials_ref: legacy_cred("wb-dry-1"),
            enabled: true,
        },
        AccountSpec {
            account_id: "trae-dry-1".into(),
            platform: "trae".into(),
            credentials_ref: legacy_cred("trae-dry-1"),
            enabled: true,
        },
    ];

    let events: Arc<StdMutex<Vec<String>>> = Arc::new(StdMutex::new(Vec::new()));
    let ev_cap = events.clone();

    let summary = orch
        .run_all(
            db.clone(),
            &accounts,
            Trigger::Manual,
            true, // dry_run
            CheckinRuntimeOptions::default(),
            None::<&CredentialStore<MemoryBackend>>,
            move |e: CheckinEvent| {
                ev_cap.lock().unwrap().push(e.name().to_string());
            },
        )
        .await
        .expect("run_all");

    assert_eq!(summary.counts.total, 2);
    assert_eq!(summary.counts.skipped, 2);
    assert_eq!(summary.counts.failed, 0);
    assert_eq!(summary.status, "done");
    assert!(summary.items.iter().all(|i| i.outcome == "Skipped"));
    assert!(summary.items.iter().all(|i| i.message == "dry_run"));

    let names = events.lock().unwrap().clone();
    assert!(names.iter().any(|n| n == EVENT_PROGRESS));
    assert!(names.iter().filter(|n| *n == EVENT_DONE).count() >= 1);

    let dbg = db.lock().await;
    let rows = dbg
        .list_item_outcomes_for_run(&summary.run_id)
        .expect("list");
    assert_eq!(rows.len(), 2);
    for (_, _, outcome) in &rows {
        assert_eq!(outcome, "Skipped");
    }

    let note: Option<String> = dbg
        .conn()
        .query_row(
            "SELECT note FROM checkin_run WHERE id = ?1",
            [&summary.run_id],
            |r| r.get(0),
        )
        .expect("note");
    assert_eq!(note.as_deref(), Some("dry_run"));
}

#[tokio::test]
async fn default_options_force_dry_even_when_dry_run_false() {
    let db = Arc::new(Mutex::new(CheckinDb::open_in_memory().unwrap()));
    let orch = CheckinOrchestrator::with_default_adapters(OrchestratorConfig {
        jitter_ms: 0,
        max_retries: 0,
        ..Default::default()
    });
    let accounts = vec![AccountSpec {
        account_id: "wb-default".into(),
        platform: "workbuddy".into(),
        credentials_ref: legacy_cred("wb-default"),
        enabled: true,
    }];

    let summary = orch
        .run_all(
            db.clone(),
            &accounts,
            Trigger::Manual,
            false, // caller did not ask dry_run
            CheckinRuntimeOptions::default(), // mode=DryRun, allow=false
            None::<&CredentialStore<MemoryBackend>>,
            |_| {},
        )
        .await
        .unwrap();

    assert_eq!(summary.counts.skipped, 1);
    assert_eq!(summary.items[0].message, "dry_run");
    let note: Option<String> = db
        .lock()
        .await
        .conn()
        .query_row(
            "SELECT note FROM checkin_run WHERE id = ?1",
            [&summary.run_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(note.as_deref(), Some("dry_run"));
}

#[tokio::test]
async fn live_http_without_allow_is_gated_dry() {
    let db = Arc::new(Mutex::new(CheckinDb::open_in_memory().unwrap()));
    let orch = CheckinOrchestrator::with_default_adapters(OrchestratorConfig {
        jitter_ms: 0,
        max_retries: 0,
        ..Default::default()
    });
    let accounts = vec![AccountSpec {
        account_id: "wb-gated".into(),
        platform: "workbuddy".into(),
        credentials_ref: legacy_cred("wb-gated"),
        enabled: true,
    }];

    let options = CheckinRuntimeOptions {
        mode: RuntimeMode::LiveHttp,
        allow_live_http: false,
    };

    let summary = orch
        .run_all(
            db.clone(),
            &accounts,
            Trigger::Manual,
            false,
            options,
            None::<&CredentialStore<MemoryBackend>>,
            |_| {},
        )
        .await
        .unwrap();

    assert_eq!(summary.counts.skipped, 1);
    assert_eq!(summary.items[0].message, "dry_run_gated");
    let note: Option<String> = db
        .lock()
        .await
        .conn()
        .query_row(
            "SELECT note FROM checkin_run WHERE id = ?1",
            [&summary.run_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(note.as_deref(), Some("gated"));
}

/// LiveHttp + allow + vault → real adapter against wiremock (Success).
#[tokio::test]
async fn live_http_with_vault_resolves_then_mock_success() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v2/billing/meter/daily-checkin"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "code": 0,
            "data": { "today_credit": 7, "streak_days": 1, "balance": 70 }
        })))
        .mount(&server)
        .await;

    let store = CredentialStore::new(MemoryBackend::new());
    // domain omitted → adapter uses config.base_url (= mock)
    let secret = r#"{
        "access_token": "tok_access_SECRET_live",
        "refresh_token": "tok_refresh_SECRET_live"
    }"#;
    let ok = store
        .store(StoreArgs {
            platform: PlatformId::Workbuddy,
            account_key: "wb-live".into(),
            kind: MaterialKind::WorkBuddyTokens,
            secret_json: secret.into(),
            source_hint: Some(".info".into()),
            replace_ref: None,
        })
        .unwrap();
    let cred = CredentialsRef::from(&ok.credential_ref);

    let mut wb_cfg = WorkBuddyConfig::default();
    wb_cfg.base_url = server.uri();
    wb_cfg.force_direct = true;
    let mut adapters: HashMap<String, Arc<dyn PlatformAdapter>> = HashMap::new();
    adapters.insert(
        "workbuddy".into(),
        Arc::new(WorkBuddyAdapter::new(wb_cfg, DesktopPaths::default())),
    );
    adapters.insert(
        "trae".into(),
        Arc::new(TraeAdapter::with_defaults()),
    );

    let db = Arc::new(Mutex::new(CheckinDb::open_in_memory().unwrap()));
    let orch = CheckinOrchestrator::new(
        adapters,
        OrchestratorConfig {
            jitter_ms: 0,
            max_retries: 0,
            force_remote: true,
            ..Default::default()
        },
    );
    let accounts = vec![AccountSpec {
        account_id: "wb-live".into(),
        platform: "workbuddy".into(),
        credentials_ref: cred,
        enabled: true,
    }];

    let options = CheckinRuntimeOptions {
        mode: RuntimeMode::LiveHttp,
        allow_live_http: true,
    };

    let summary = orch
        .run_all(
            db,
            &accounts,
            Trigger::Manual,
            false,
            options,
            Some(&store),
            |_| {},
        )
        .await
        .unwrap();

    assert_eq!(summary.counts.total, 1);
    assert_eq!(summary.counts.success, 1);
    assert_eq!(summary.items[0].outcome, "Success");
    assert_eq!(summary.items[0].credits_delta, Some(7));
    assert!(!summary.items[0].message.contains("tok_access_SECRET"));
    assert!(!summary.items[0].message.contains("live_http_not_implemented"));
}

#[tokio::test]
async fn live_http_trae_cipher_only_jwt_unavailable() {
    let dir = tempfile::tempdir().unwrap();
    let store =
        CredentialStore::with_vault_dir(MemoryBackend::new(), dir.path().to_path_buf());
    let secret = r#"{
        "icube_auth_info_cipher": "CIPHER_SECRET_only",
        "device_id": "dev-no-jwt"
    }"#;
    let ok = store
        .store(StoreArgs {
            platform: PlatformId::TraeCn,
            account_key: "dev-no-jwt".into(),
            kind: MaterialKind::TraeCnICubeBlob,
            secret_json: secret.into(),
            source_hint: Some("storage.json".into()),
            replace_ref: None,
        })
        .unwrap();
    let cred = CredentialsRef::from(&ok.credential_ref);

    let db = Arc::new(Mutex::new(CheckinDb::open_in_memory().unwrap()));
    let orch = CheckinOrchestrator::with_default_adapters(OrchestratorConfig {
        jitter_ms: 0,
        max_retries: 0,
        force_remote: true,
        ..Default::default()
    });
    let accounts = vec![AccountSpec {
        account_id: "trae-no-jwt".into(),
        platform: "trae".into(),
        credentials_ref: cred,
        enabled: true,
    }];

    let options = CheckinRuntimeOptions {
        mode: RuntimeMode::LiveHttp,
        allow_live_http: true,
    };

    let summary = orch
        .run_all(
            db,
            &accounts,
            Trigger::Manual,
            false,
            options,
            Some(&store),
            |_| {},
        )
        .await
        .unwrap();

    assert_eq!(summary.counts.failed, 1);
    assert_eq!(summary.items[0].message, "trae_jwt_unavailable");
    assert!(!summary.items[0].message.contains("CIPHER_SECRET"));
}

#[tokio::test]
async fn second_run_same_day_idempotent_already_done_after_success_row() {
    let db = Arc::new(Mutex::new(CheckinDb::open_in_memory().unwrap()));
    let local_day = checkin_core::orchestrator::local_day_shanghai();
    {
        let dbg = db.lock().await;
        dbg.insert_run(
            "seed-run",
            "manual",
            "2026-10-18T00:00:00+08:00",
            "Asia/Shanghai",
            3,
            None,
        )
        .unwrap();
        dbg.upsert_item(
            "seed-item",
            "seed-run",
            "wb-idem",
            "workbuddy",
            &local_day,
            "Success",
            Some(10),
            Some(1),
            Some(10),
            Some("seeded"),
            Some(0),
            None,
            "2026-10-18T00:00:00+08:00",
            "2026-10-18T00:00:01+08:00",
            false,
        )
        .unwrap();
    }

    let orch = CheckinOrchestrator::with_default_adapters(OrchestratorConfig {
        jitter_ms: 0,
        max_retries: 0,
        ..Default::default()
    });

    let accounts = vec![AccountSpec {
        account_id: "wb-idem".into(),
        platform: "workbuddy".into(),
        credentials_ref: legacy_cred("wb-idem"),
        enabled: true,
    }];

    let summary = orch
        .run_all(
            db.clone(),
            &accounts,
            Trigger::Manual,
            true,
            CheckinRuntimeOptions::default(),
            None::<&CredentialStore<MemoryBackend>>,
            |_| {},
        )
        .await
        .unwrap();

    assert_eq!(summary.counts.total, 1);
    assert_eq!(summary.counts.already_done, 1);
    assert_eq!(summary.items[0].outcome, "AlreadyDone");
}
