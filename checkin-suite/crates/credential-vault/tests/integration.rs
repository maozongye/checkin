//! Integration tests with MemoryBackend.

use std::path::PathBuf;

use credential_vault::{
    CredentialBackend, CredentialEnvelope, CredentialRef, CredentialStore, DeleteArgs,
    MaterialKind, MemoryBackend, PlatformId, SecretString, StoreArgs, StoreError,
    BACKEND_BLOB_SOFT_MAX, INLINE_MAX, SERVICE,
};

fn wb_secret() -> String {
    serde_json::json!({
        "access_token": "wb-access-JWT-SECRET-VALUE",
        "refresh_token": "wb-refresh-JWT-SECRET-VALUE",
        "domain": "example.com",
        "expires_at": null
    })
    .to_string()
}

fn trae_secret() -> String {
    serde_json::json!({
        "icube_auth_info_cipher": "iCubeAuthInfo://icube.cloudide/CIPHERBLOB",
        "device_id": "device-abc-123"
    })
    .to_string()
}

fn trae_secret_large() -> String {
    let cipher = format!(
        "iCubeAuthInfo://icube.cloudide/{}",
        "X".repeat(3200)
    );
    assert!(cipher.len() > 3000);
    serde_json::json!({
        "icube_auth_info_cipher": cipher,
        "device_id": "device-large-999"
    })
    .to_string()
}

#[test]
fn workbuddy_import_resolve_tokens_and_debug_redacted() {
    let store = CredentialStore::new(MemoryBackend::new());
    let ok = store
        .store(StoreArgs {
            platform: PlatformId::Workbuddy,
            account_key: "uid-001".into(),
            kind: MaterialKind::WorkBuddyTokens,
            secret_json: wb_secret(),
            source_hint: Some(".info".into()),
            replace_ref: None,
        })
        .expect("store");

    let ref_s = ok.credential_ref_str();
    assert!(
        ref_s.starts_with("cred://v1/account/workbuddy/uid-001/"),
        "ref={ref_s}"
    );
    assert!(!ref_s.contains("JWT-SECRET"));

    let backend_key = ok.credential_ref.to_backend_key();
    assert!(backend_key.starts_with(&format!("{SERVICE}/v1/account/workbuddy/")));

    // Backend holds pointer JSON (inline), not raw envelope
    let raw = store.backend().get(&backend_key).unwrap();
    let v: serde_json::Value = serde_json::from_slice(&raw).unwrap();
    assert_eq!(v["format"], "wbtcred_ptr_v1");
    assert_eq!(v["mode"], "inline");
    assert!(raw.len() < 512 || raw.len() < INLINE_MAX + 200);

    let handle = store.resolve(&ok.credential_ref).expect("resolve");
    let view = handle.as_workbuddy_tokens().expect("wb view");
    assert_eq!(view.access_token, "wb-access-JWT-SECRET-VALUE");
    assert_eq!(view.refresh_token, "wb-refresh-JWT-SECRET-VALUE");
    assert_eq!(view.domain, Some("example.com"));

    let dbg = format!("{:?}", handle);
    assert_eq!(dbg, "CredentialHandle([REDACTED])");
    assert!(!dbg.contains("JWT-SECRET"));
    assert!(!dbg.contains("wb-access"));

    assert!(handle.as_trae_cn_blob().is_err());
}

#[test]
fn trae_import_resolve_and_missing_device_id_fails() {
    let vault = tempfile::tempdir().unwrap();
    let store = CredentialStore::with_vault_dir(MemoryBackend::new(), vault.path().to_path_buf());
    let ok = store
        .store(StoreArgs {
            platform: PlatformId::TraeCn,
            account_key: "trae-user".into(),
            kind: MaterialKind::TraeCnICubeBlob,
            secret_json: trae_secret(),
            source_hint: Some("storage.json".into()),
            replace_ref: None,
        })
        .expect("store");

    assert!(ok
        .credential_ref_str()
        .starts_with("cred://v1/account/trae_cn/trae-user/"));

    // Trae always file mode
    let raw = store
        .backend()
        .get(&ok.credential_ref.to_backend_key())
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&raw).unwrap();
    assert_eq!(v["mode"], "file");
    assert!(v["rel_path"].as_str().unwrap().starts_with("blobs/"));
    assert!(raw.len() < 512, "pointer size={}", raw.len());

    let handle = store.resolve(&ok.credential_ref).unwrap();
    let view = handle.as_trae_cn_blob().unwrap();
    assert_eq!(
        view.icube_auth_info_cipher,
        "iCubeAuthInfo://icube.cloudide/CIPHERBLOB"
    );
    assert_eq!(view.device_id, "device-abc-123");

    let bad = store.store(StoreArgs {
        platform: PlatformId::TraeCn,
        account_key: "trae-user-2".into(),
        kind: MaterialKind::TraeCnICubeBlob,
        secret_json: r#"{"icube_auth_info_cipher":"only-cipher"}"#.into(),
        source_hint: None,
        replace_ref: None,
    });
    assert!(bad.is_err(), "must reject missing device_id");
    let err = bad.unwrap_err().to_string();
    assert!(err.contains("device_id"), "{err}");
}

#[test]
fn delete_requires_password_then_backup_and_restore() {
    let vault = tempfile::tempdir().unwrap();
    let store = CredentialStore::with_vault_dir(MemoryBackend::new(), vault.path().to_path_buf());
    let ok = store
        .store(StoreArgs {
            platform: PlatformId::Workbuddy,
            account_key: "del-user".into(),
            kind: MaterialKind::WorkBuddyTokens,
            secret_json: wb_secret(),
            source_hint: None,
            replace_ref: None,
        })
        .unwrap();

    let ref_s = ok.credential_ref_str();

    let err = store
        .delete(DeleteArgs {
            credential_ref: ref_s.clone(),
            backup_password: None,
            backup_dir: Some(PathBuf::from("/tmp")),
        })
        .unwrap_err();
    assert!(matches!(
        err,
        credential_vault::DeleteError::BackupRequired
    ));

    let dir = tempfile::tempdir().unwrap();
    let deleted = store
        .delete(DeleteArgs {
            credential_ref: ref_s.clone(),
            backup_password: Some("backup-pass-强".into()),
            backup_dir: Some(dir.path().to_path_buf()),
        })
        .expect("delete with backup");
    assert!(deleted.deleted);
    assert!(deleted.backup_path.exists());
    assert!(
        deleted
            .backup_path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .ends_with(".cred.bak")
    );

    assert!(store.resolve_str(&ref_s).is_err());

    let pw = SecretString::new("backup-pass-强");
    let restored = store
        .restore_backup(&deleted.backup_path, &pw, None)
        .expect("restore");
    assert!(matches!(restored, CredentialRef::Account { .. }));
    let h = store.resolve(&restored).unwrap();
    let v = h.as_workbuddy_tokens().unwrap();
    assert_eq!(v.access_token, "wb-access-JWT-SECRET-VALUE");
}

#[test]
fn kind_platform_mismatch_rejected() {
    let store = CredentialStore::new(MemoryBackend::new());
    let err = store
        .store(StoreArgs {
            platform: PlatformId::Workbuddy,
            account_key: "x".into(),
            kind: MaterialKind::TraeCnICubeBlob,
            secret_json: trae_secret(),
            source_hint: None,
            replace_ref: None,
        })
        .unwrap_err();
    assert!(matches!(
        err,
        credential_vault::StoreError::KindPlatformMismatch
    ));
}

#[test]
fn switch_backup_ref_parse_and_backend_key() {
    let r = CredentialRef::switch_backup("workbuddy", "snap-42");
    assert_eq!(
        r.to_ref_string(),
        "cred://v1/switch_backup/workbuddy/snap-42"
    );
    assert_eq!(
        r.to_backend_key(),
        format!("{SERVICE}/v1/switch_backup/workbuddy/snap-42")
    );
    let store = CredentialStore::new(MemoryBackend::new());
    store.put_raw(&r, b"{\"test\":true}").unwrap();
    let account = CredentialRef::account(
        PlatformId::Workbuddy,
        "snap-42",
        uuid::Uuid::nil(),
    );
    assert_ne!(r.to_backend_key(), account.to_backend_key());
}

#[test]
fn replace_ref_overwrites_same_uuid() {
    let store = CredentialStore::new(MemoryBackend::new());
    let first = store
        .store(StoreArgs {
            platform: PlatformId::Workbuddy,
            account_key: "u".into(),
            kind: MaterialKind::WorkBuddyTokens,
            secret_json: wb_secret(),
            source_hint: None,
            replace_ref: None,
        })
        .unwrap();
    let ref1 = first.credential_ref_str();

    let secret2 = serde_json::json!({
        "access_token": "NEW-ACCESS",
        "refresh_token": "NEW-REFRESH"
    });
    let second = store
        .store(StoreArgs {
            platform: PlatformId::Workbuddy,
            account_key: "u".into(),
            kind: MaterialKind::WorkBuddyTokens,
            secret_json: secret2.to_string(),
            source_hint: None,
            replace_ref: Some(ref1.clone()),
        })
        .unwrap();
    assert_eq!(second.credential_ref_str(), ref1);
    let handle = store.resolve_str(&ref1).unwrap();
    let v = handle.as_workbuddy_tokens().unwrap();
    assert_eq!(v.access_token, "NEW-ACCESS");
    let _ = secret2;
}

// ─── blob >2560 hybrid storage ────────────────────────────────────────────

#[test]
fn small_workbuddy_stores_inline_and_resolves() {
    let store = CredentialStore::new(MemoryBackend::new());
    let ok = store
        .store(StoreArgs {
            platform: PlatformId::Workbuddy,
            account_key: "inline-wb".into(),
            kind: MaterialKind::WorkBuddyTokens,
            secret_json: wb_secret(),
            source_hint: None,
            replace_ref: None,
        })
        .unwrap();
    let raw = store
        .backend()
        .get(&ok.credential_ref.to_backend_key())
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&raw).unwrap();
    assert_eq!(v["mode"], "inline");
    assert!(raw.len() < 512, "inline pointer len={}", raw.len());
    let h = store.resolve(&ok.credential_ref).unwrap();
    assert_eq!(
        h.as_workbuddy_tokens().unwrap().access_token,
        "wb-access-JWT-SECRET-VALUE"
    );
}

#[test]
fn large_trae_file_mode_resolve_and_delete_cleans_cve() {
    let vault = tempfile::tempdir().unwrap();
    let store = CredentialStore::with_vault_dir(MemoryBackend::new(), vault.path().to_path_buf());

    let secret = trae_secret_large();
    let ok = store
        .store(StoreArgs {
            platform: PlatformId::TraeCn,
            account_key: "trae-large".into(),
            kind: MaterialKind::TraeCnICubeBlob,
            secret_json: secret.clone(),
            source_hint: Some("storage.json".into()),
            replace_ref: None,
        })
        .expect("store large trae");

    let raw = store
        .backend()
        .get(&ok.credential_ref.to_backend_key())
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&raw).unwrap();
    assert_eq!(v["format"], "wbtcred_ptr_v1");
    assert_eq!(v["mode"], "file");
    assert!(raw.len() < 512, "file pointer len={}", raw.len());

    let rel = v["rel_path"].as_str().unwrap().to_string();
    let cve_path = vault.path().join(&rel);
    assert!(cve_path.exists(), "missing {}", cve_path.display());
    let magic = std::fs::read(&cve_path).unwrap();
    assert_eq!(&magic[..8], b"WBTBLOB1");

    let handle = store.resolve(&ok.credential_ref).unwrap();
    let view = handle.as_trae_cn_blob().unwrap();
    let expected: serde_json::Value = serde_json::from_str(&secret).unwrap();
    assert_eq!(
        view.icube_auth_info_cipher,
        expected["icube_auth_info_cipher"].as_str().unwrap()
    );
    assert_eq!(view.device_id, "device-large-999");

    let bak_dir = tempfile::tempdir().unwrap();
    let deleted = store
        .delete(DeleteArgs {
            credential_ref: ok.credential_ref_str(),
            backup_password: Some("pw".into()),
            backup_dir: Some(bak_dir.path().to_path_buf()),
        })
        .expect("delete");
    assert!(deleted.deleted);
    assert!(
        !cve_path.exists(),
        ".cve should be removed after delete"
    );
    assert!(store.resolve(&ok.credential_ref).is_err());
}

#[test]
fn trae_without_vault_dir_fails() {
    let store = CredentialStore::new(MemoryBackend::new());
    let err = store
        .store(StoreArgs {
            platform: PlatformId::TraeCn,
            account_key: "no-vault".into(),
            kind: MaterialKind::TraeCnICubeBlob,
            secret_json: trae_secret(),
            source_hint: None,
            replace_ref: None,
        })
        .unwrap_err();
    assert_eq!(err.code(), "vault_dir_required");
    let msg = err.to_string();
    assert!(
        msg.contains("vault_dir required") || msg.to_lowercase().contains("with_vault_dir"),
        "expected vault_dir error, got: {msg}"
    );
}

#[test]
fn legacy_raw_envelope_still_resolves() {
    let store = CredentialStore::new(MemoryBackend::new());
    // Build a real envelope and plant raw seal bytes (pre-pointer era).
    let material = serde_json::json!({
        "access_token": "LEGACY-ACCESS",
        "refresh_token": "LEGACY-REFRESH"
    });
    let meta = credential_vault::CredentialMeta {
        platform: PlatformId::Workbuddy,
        account_key: "legacy".into(),
        imported_at: 1,
        source_hint: None,
    };
    let env = CredentialEnvelope::new(
        MaterialKind::WorkBuddyTokens,
        credential_vault::MaterialBytes::new(material.to_string().into_bytes()),
        meta,
    );
    let seal = env.seal_to_bytes().unwrap();
    let cred_ref = CredentialRef::account(PlatformId::Workbuddy, "legacy", uuid::Uuid::new_v4());
    store
        .backend()
        .set(&cred_ref.to_backend_key(), &seal)
        .unwrap();

    let handle = store.resolve(&cred_ref).expect("legacy resolve");
    let v = handle.as_workbuddy_tokens().unwrap();
    assert_eq!(v.access_token, "LEGACY-ACCESS");
    assert_eq!(v.refresh_token, "LEGACY-REFRESH");
}


// ─── Switcher large-blob contract ─────────────────────────────────────────

#[test]
fn switch_backup_large_put_get_delete_roundtrip() {
    let vault = tempfile::tempdir().unwrap();
    let store = CredentialStore::with_vault_dir(MemoryBackend::new(), vault.path().to_path_buf());

    let plaintext = vec![0xABu8; BACKEND_BLOB_SOFT_MAX + 500]; // >2400
    assert!(plaintext.len() > 2400);

    let cred_ref = store
        .put_switch_backup("workbuddy", "snap-large-001", &plaintext)
        .expect("put_switch_backup");
    assert_eq!(
        cred_ref.to_ref_string(),
        "cred://v1/switch_backup/workbuddy/snap-large-001"
    );

    // Backend holds file pointer, not bare plaintext
    let raw = store.backend().get(&cred_ref.to_backend_key()).unwrap();
    let v: serde_json::Value = serde_json::from_slice(&raw).unwrap();
    assert_eq!(v["format"], "wbtcred_ptr_v1");
    assert_eq!(v["mode"], "file");
    let rel = v["rel_path"].as_str().unwrap().to_string();
    assert_eq!(rel, "switch_blobs/snap-large-001.cve");
    assert!(raw.len() < 512, "pointer len={}", raw.len());

    let cve = vault.path().join(&rel);
    assert!(cve.exists());
    let magic = std::fs::read(&cve).unwrap();
    assert_eq!(&magic[..8], b"WBTBLOB1");

    let got = store
        .get_switch_backup_seal(&cred_ref)
        .expect("get_switch_backup_seal");
    assert_eq!(got, plaintext);

    store.delete_switch_backup(&cred_ref).expect("delete");
    assert!(store.backend().is_empty());
    assert!(!cve.exists(), ".cve should be gone after delete");
    assert!(store.get_switch_backup_seal(&cred_ref).is_err());
}

#[test]
fn switch_backup_without_vault_dir_fails() {
    let store = CredentialStore::new(MemoryBackend::new());
    let err = store
        .put_switch_backup("workbuddy", "snap-x", b"tiny")
        .unwrap_err();
    assert!(matches!(err, StoreError::VaultDirRequired(_)));
    assert_eq!(err.code(), "vault_dir_required");
}

#[test]
fn put_raw_still_rejects_payload_too_large() {
    let store = CredentialStore::new(MemoryBackend::new());
    let r = CredentialRef::switch_backup("workbuddy", "snap-raw-big");
    let big = vec![0u8; BACKEND_BLOB_SOFT_MAX + 1];
    let err = store.put_raw(&r, &big).unwrap_err();
    assert_eq!(err.code(), "payload_too_large");
    match err {
        StoreError::PayloadTooLarge { size, limit } => {
            assert_eq!(size, BACKEND_BLOB_SOFT_MAX + 1);
            assert_eq!(limit, BACKEND_BLOB_SOFT_MAX);
        }
        other => panic!("expected PayloadTooLarge, got {other:?}"),
    }
}

#[test]
fn put_raw_small_still_works_and_get_switch_reads_legacy() {
    let store = CredentialStore::new(MemoryBackend::new());
    let r = CredentialRef::switch_backup("workbuddy", "snap-legacy-small");
    let sealed = br#"{"legacy":true,"tokens":"abc"}"#;
    store.put_raw(&r, sealed).expect("small put_raw");
    let got = store.get_switch_backup_seal(&r).expect("legacy get");
    assert_eq!(got, sealed);
}

#[test]
fn switch_backup_rejects_unsafe_snapshot_id() {
    let vault = tempfile::tempdir().unwrap();
    let store = CredentialStore::with_vault_dir(MemoryBackend::new(), vault.path().to_path_buf());
    for bad in ["../escape", "a/b", "a\\b", ""] {
        let err = store
            .put_switch_backup("workbuddy", bad, b"x")
            .expect_err(&format!("should reject {bad:?}"));
        assert!(
            matches!(err, StoreError::Internal(_)),
            "bad={bad:?} err={err:?}"
        );
    }
}

#[test]
fn switch_backup_get_delete_reject_account_ref() {
    let vault = tempfile::tempdir().unwrap();
    let store = CredentialStore::with_vault_dir(MemoryBackend::new(), vault.path().to_path_buf());
    let account = CredentialRef::account(PlatformId::Workbuddy, "u", uuid::Uuid::nil());

    let get_err = store.get_switch_backup_seal(&account).unwrap_err();
    assert!(matches!(
        get_err,
        credential_vault::ResolveError::InvalidRef(_)
    ));

    let del_err = store.delete_switch_backup(&account).unwrap_err();
    assert!(matches!(
        del_err,
        credential_vault::DeleteError::InvalidRef(_)
    ));
}

#[test]
fn switch_backup_overwrite_same_ref() {
    let vault = tempfile::tempdir().unwrap();
    let store = CredentialStore::with_vault_dir(MemoryBackend::new(), vault.path().to_path_buf());
    let r1 = store
        .put_switch_backup("workbuddy", "snap-ov", b"version-one")
        .unwrap();
    let r2 = store
        .put_switch_backup("workbuddy", "snap-ov", b"version-two-longer")
        .unwrap();
    assert_eq!(r1, r2);
    let got = store.get_switch_backup_seal(&r2).unwrap();
    assert_eq!(got, b"version-two-longer");
    let cve = vault.path().join("switch_blobs/snap-ov.cve");
    assert!(cve.exists());
}
