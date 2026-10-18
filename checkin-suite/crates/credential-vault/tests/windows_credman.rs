//! Windows Credential Manager integration tests.
//! Only compiled/run on Windows (`#[cfg(windows)]`).
//! Writes real CredMan entries under account_key prefix `cv-test-`; cleans up on success.

#![cfg(windows)]

use std::path::PathBuf;

use credential_vault::{
    CredentialStore, DeleteArgs, MaterialKind, OsKeychainBackend, PlatformId, StoreArgs,
};
use uuid::Uuid;

fn wb_secret() -> String {
    serde_json::json!({
        "access_token": "wb-access-JWT-SECRET-VALUE",
        "refresh_token": "wb-refresh-JWT-SECRET-VALUE",
        "domain": "example.com",
        "expires_at": null
    })
    .to_string()
}

fn test_account_key(tag: &str) -> String {
    format!("cv-test-{tag}-{}", Uuid::new_v4())
}

/// store WB tokens → resolve → field match; always attempt delete cleanup.
#[test]
fn credman_workbuddy_store_resolve_delete() {
    let store = CredentialStore::new(OsKeychainBackend::new());
    let account_key = test_account_key("wb");

    let ok = store
        .store(StoreArgs {
            platform: PlatformId::Workbuddy,
            account_key: account_key.clone(),
            kind: MaterialKind::WorkBuddyTokens,
            secret_json: wb_secret(),
            source_hint: Some(".info".into()),
            replace_ref: None,
        })
        .expect("CredMan store");

    let ref_s = ok.credential_ref_str();
    assert!(
        ref_s.starts_with(&format!("cred://v1/account/workbuddy/{account_key}/")),
        "ref={ref_s}"
    );

    let handle = match store.resolve(&ok.credential_ref) {
        Ok(h) => h,
        Err(e) => {
            let _ = cleanup_delete(&store, &ref_s);
            panic!("resolve failed: {e}");
        }
    };
    let view = handle.as_workbuddy_tokens().expect("wb view");
    assert_eq!(view.access_token, "wb-access-JWT-SECRET-VALUE");
    assert_eq!(view.refresh_token, "wb-refresh-JWT-SECRET-VALUE");
    assert_eq!(view.domain, Some("example.com".into()));

    let dir = tempfile::tempdir().unwrap();
    let deleted = store
        .delete(DeleteArgs {
            credential_ref: ref_s.clone(),
            backup_password: Some("cv-test-backup-pass".into()),
            backup_dir: Some(dir.path().to_path_buf()),
        })
        .expect("CredMan delete with backup");
    assert!(deleted.deleted);
    assert!(deleted.backup_path.exists());

    let err = store.resolve_str(&ref_s).unwrap_err();
    assert!(
        matches!(err, credential_vault::ResolveError::NotFound),
        "expected NotFound after delete, got {err}"
    );
}

#[test]
fn credman_backend_roundtrip_raw() {
    use credential_vault::CredentialBackend;
    let backend = OsKeychainBackend::new();
    let key = format!(
        "com.benchforge.wb-trae.credential/v1/account/workbuddy/cv-test-raw-{}/{}",
        Uuid::new_v4(),
        Uuid::new_v4()
    );
    let payload = b"envelope-seal-bytes-for-credman-test";

    backend.set(&key, payload).expect("set");
    let got = match backend.get(&key) {
        Ok(v) => v,
        Err(e) => {
            let _ = backend.delete(&key);
            panic!("get failed: {e}");
        }
    };
    assert_eq!(got, payload);

    backend.delete(&key).expect("delete");
    assert!(matches!(
        backend.get(&key),
        Err(credential_vault::BackendError::NotFound)
    ));
    // second delete → NotFound
    assert!(matches!(
        backend.delete(&key),
        Err(credential_vault::BackendError::NotFound)
    ));
}

fn cleanup_delete(store: &CredentialStore<OsKeychainBackend>, ref_s: &str) {
    let dir = std::env::temp_dir().join("cv-test-cleanup");
    let _ = std::fs::create_dir_all(&dir);
    let _ = store.delete(DeleteArgs {
        credential_ref: ref_s.to_string(),
        backup_password: Some("cv-test-cleanup".into()),
        backup_dir: Some(PathBuf::from(dir)),
    });
}
