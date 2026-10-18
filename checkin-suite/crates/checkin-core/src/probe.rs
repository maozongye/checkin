//! Dry-run desktop path probe — existence + structure keys only.
//!
//! **Never** returns or logs token / JWT plaintext. Masked summaries only.

use crate::config::DesktopPaths;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;

/// Masked probe summary for one desktop auth source.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PathProbeSummary {
    pub platform: String,
    /// Absolute path used for this probe (may be a fixture path in tests).
    pub path: String,
    pub exists: bool,
    pub readable: bool,
    pub parse_ok: bool,
    /// WorkBuddy: uid present in account object.
    pub uid_present: bool,
    /// WorkBuddy: domain string (safe to expose; not a secret).
    pub domain: Option<String>,
    /// accessToken / auth JWT field present (value never returned).
    pub has_access_token: bool,
    /// refreshToken field present (value never returned).
    pub has_refresh_token: bool,
    /// Trae: auth key hint found in storage.
    pub auth_key_present: bool,
    /// Trae: device id key present.
    pub device_id_present: bool,
    /// Short human note; must not contain secrets.
    pub note: String,
}

/// Probe WorkBuddy `workbuddy-desktop.info` at an explicit path (fixture-friendly).
pub fn probe_workbuddy_file(path: &Path) -> PathProbeSummary {
    let mut s = PathProbeSummary {
        platform: "workbuddy".into(),
        path: path.display().to_string(),
        note: String::new(),
        ..Default::default()
    };
    if !path.exists() {
        s.note = "file_missing".into();
        return s;
    }
    s.exists = true;
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(_) => {
            s.note = "unreadable".into();
            return s;
        }
    };
    s.readable = true;
    let raw: Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(_) => {
            s.note = "json_parse_error".into();
            return s;
        }
    };
    s.parse_ok = true;

    let account = raw.get("account").cloned().unwrap_or(Value::Null);
    let auth = if let Some(a) = raw.get("auth") {
        a.clone()
    } else if raw.get("accessToken").is_some() || raw.get("access_token").is_some() {
        raw.clone()
    } else {
        Value::Null
    };

    s.uid_present = account
        .get("uid")
        .and_then(|v| v.as_str())
        .map(|u| !u.is_empty())
        .unwrap_or(false)
        || raw
            .get("uid")
            .and_then(|v| v.as_str())
            .map(|u| !u.is_empty())
            .unwrap_or(false);

    s.domain = auth
        .get("domain")
        .and_then(|v| v.as_str())
        .filter(|d| !d.is_empty())
        .map(|d| d.to_string());

    s.has_access_token = json_has_nonempty_str(&auth, &["accessToken", "access_token"])
        || json_has_nonempty_str(&raw, &["accessToken", "access_token"]);
    // Treat non-empty string under auth as JWT-shaped presence without inspecting content.
    // Also: some dumps put JWT under key "auth" as a plain string — detect type only.
    if !s.has_access_token {
        if let Some(Value::String(tok)) = raw.get("auth") {
            if !tok.is_empty() {
                s.has_access_token = true;
            }
        }
    }

    s.has_refresh_token = json_has_nonempty_str(&auth, &["refreshToken", "refresh_token"])
        || json_has_nonempty_str(&raw, &["refreshToken", "refresh_token"]);

    s.note = "ok".into();
    s
}

/// Probe Trae `storage.json` at an explicit path (fixture-friendly).
pub fn probe_trae_storage_file(path: &Path, paths: &DesktopPaths) -> PathProbeSummary {
    let mut s = PathProbeSummary {
        platform: "trae".into(),
        path: path.display().to_string(),
        note: String::new(),
        ..Default::default()
    };
    if !path.exists() {
        s.note = "file_missing".into();
        return s;
    }
    s.exists = true;
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(_) => {
            s.note = "unreadable".into();
            return s;
        }
    };
    s.readable = true;
    let raw: Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(_) => {
            s.note = "json_parse_error".into();
            return s;
        }
    };
    s.parse_ok = true;

    let hint = &paths.trae_auth_key_hint;
    s.auth_key_present = raw.get(hint.as_str()).is_some()
        || find_key_contains(&raw, "iCubeAuthInfo");

    let dev_key = &paths.trae_device_id_key;
    s.device_id_present = raw
        .get(dev_key.as_str())
        .map(|v| match v {
            Value::String(x) => !x.is_empty(),
            Value::Null => false,
            _ => true,
        })
        .unwrap_or(false);

    // Presence of nested token fields under auth blob — never unwrap values into summary.
    if let Some(auth_val) = raw.get(hint.as_str()) {
        s.has_access_token = value_mentions_token_keys(auth_val);
        s.has_refresh_token = value_mentions_refresh_keys(auth_val);
    }

    s.note = "ok".into();
    s
}

/// Probe using env-resolved Windows paths (LOCALAPPDATA / APPDATA).
/// On box/Linux without those env vars, returns missing summaries.
pub fn probe_desktop_defaults(paths: &DesktopPaths) -> Vec<PathProbeSummary> {
    let mut out = Vec::new();
    match paths.resolve_wb_auth() {
        Some(p) => out.push(probe_workbuddy_file(&p)),
        None => out.push(PathProbeSummary {
            platform: "workbuddy".into(),
            path: format!("%LOCALAPPDATA%/{}", paths.wb_auth_relpath),
            note: "LOCALAPPDATA_unset".into(),
            ..Default::default()
        }),
    }
    match paths.resolve_trae_storage() {
        Some(p) => out.push(probe_trae_storage_file(&p, paths)),
        None => out.push(PathProbeSummary {
            platform: "trae".into(),
            path: format!("%APPDATA%/{}", paths.trae_storage_relpath),
            note: "APPDATA_unset".into(),
            ..Default::default()
        }),
    }
    out
}

/// Convenience: resolve + probe; override paths for fixtures.
pub fn probe_at(
    wb_path: Option<&Path>,
    trae_path: Option<&Path>,
    paths: &DesktopPaths,
) -> Vec<PathProbeSummary> {
    let mut out = Vec::new();
    if let Some(p) = wb_path {
        out.push(probe_workbuddy_file(p));
    }
    if let Some(p) = trae_path {
        out.push(probe_trae_storage_file(p, paths));
    }
    out
}

fn json_has_nonempty_str(obj: &Value, keys: &[&str]) -> bool {
    for k in keys {
        if let Some(v) = obj.get(*k) {
            if let Some(s) = v.as_str() {
                if !s.is_empty() {
                    return true;
                }
            }
        }
    }
    false
}

fn find_key_contains(obj: &Value, needle: &str) -> bool {
    match obj {
        Value::Object(map) => map.keys().any(|k| k.contains(needle)),
        _ => false,
    }
}

fn value_mentions_token_keys(v: &Value) -> bool {
    match v {
        Value::Object(map) => {
            map.contains_key("accessToken")
                || map.contains_key("access_token")
                || map.values().any(value_mentions_token_keys)
        }
        Value::String(s) => {
            // Encrypted blobs are often base64 strings — treat non-empty as "present"
            // without classifying as JWT plaintext.
            !s.is_empty()
        }
        Value::Array(arr) => arr.iter().any(value_mentions_token_keys),
        _ => false,
    }
}

fn value_mentions_refresh_keys(v: &Value) -> bool {
    match v {
        Value::Object(map) => {
            map.contains_key("refreshToken")
                || map.contains_key("refresh_token")
                || map.values().any(value_mentions_refresh_keys)
        }
        Value::Array(arr) => arr.iter().any(value_mentions_refresh_keys),
        _ => false,
    }
}

/// Assert a serialized summary never contains JWT-looking substrings from a fixture.
#[cfg(test)]
pub fn summary_json_is_token_free(summary: &PathProbeSummary, forbidden: &[&str]) -> bool {
    let s = serde_json::to_string(summary).unwrap_or_default();
    !forbidden.iter().any(|f| !f.is_empty() && s.contains(f))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn probe_wb_fixture_masks_token() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("workbuddy-desktop.info");
        let secret = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.PAYLOAD.SIG";
        let body = serde_json::json!({
            "account": { "uid": "u123", "nickname": "n" },
            "auth": {
                "accessToken": secret,
                "refreshToken": "refresh-secret-xyz",
                "domain": "copilot.tencent.com"
            }
        });
        std::fs::write(&path, body.to_string()).unwrap();

        let sum = probe_workbuddy_file(&path);
        assert!(sum.exists && sum.parse_ok);
        assert!(sum.uid_present);
        assert_eq!(sum.domain.as_deref(), Some("copilot.tencent.com"));
        assert!(sum.has_access_token);
        assert!(sum.has_refresh_token);
        assert!(summary_json_is_token_free(
            &sum,
            &[secret, "refresh-secret-xyz", "PAYLOAD"]
        ));
    }

    #[test]
    fn probe_wb_auth_as_plain_jwt_string() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("workbuddy-desktop.info");
        let secret = "eyJplain.jwt.token.value.here";
        let body = serde_json::json!({
            "account": { "uid": "u1" },
            "auth": secret,
            "domain": "copilot.tencent.com"
        });
        // domain may be only under nested auth in real files; flat domain optional
        std::fs::write(&path, body.to_string()).unwrap();
        let sum = probe_workbuddy_file(&path);
        assert!(sum.has_access_token);
        assert!(sum.uid_present);
        assert!(summary_json_is_token_free(&sum, &[secret]));
    }

    #[test]
    fn probe_trae_fixture() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("storage.json");
        let paths = DesktopPaths::default();
        let secret = "Cloud-IDE-JWT eyJhbGciOi.secret.sig";
        let mut map = serde_json::Map::new();
        map.insert(
            paths.trae_auth_key_hint.clone(),
            Value::String(secret.into()),
        );
        map.insert(
            paths.trae_device_id_key.clone(),
            Value::String("dev-device-abc".into()),
        );
        std::fs::write(&path, Value::Object(map).to_string()).unwrap();

        let sum = probe_trae_storage_file(&path, &paths);
        assert!(sum.exists && sum.parse_ok);
        assert!(sum.auth_key_present);
        assert!(sum.device_id_present);
        assert!(sum.has_access_token);
        assert!(summary_json_is_token_free(
            &sum,
            &[secret, "eyJhbGciOi", "dev-device-abc"]
        ));
        // device id value must not leak — only presence flag
        let dumped = serde_json::to_string(&sum).unwrap();
        assert!(!dumped.contains("dev-device-abc"));
    }

    #[test]
    fn probe_missing_file() {
        let sum = probe_workbuddy_file(Path::new("/tmp/definitely-missing-wb-auth-xyz.info"));
        assert!(!sum.exists);
        assert_eq!(sum.note, "file_missing");
    }

    #[test]
    fn probe_at_both() {
        let dir = tempdir().unwrap();
        let wb = dir.path().join("wb.info");
        let trae = dir.path().join("storage.json");
        let mut f = std::fs::File::create(&wb).unwrap();
        let wb_secret = "WB_FIXTURE_ACCESS_TOKEN_VALUE_9f3a";
        write!(
            f,
            r#"{{"account":{{"uid":"1"}},"auth":{{"accessToken":"{}","domain":"copilot.tencent.com"}}}}"#,
            wb_secret
        )
        .unwrap();
        let paths = DesktopPaths::default();
        let trae_secret = "TRAE_FIXTURE_ACCESS_TOKEN_VALUE_7c2b";
        let body = serde_json::json!({
            paths.trae_auth_key_hint.clone(): {"accessToken": trae_secret},
            paths.trae_device_id_key.clone(): "DEVICE_ID_FIXTURE_NOT_IN_SUMMARY"
        });
        std::fs::write(&trae, body.to_string()).unwrap();

        let sums = probe_at(Some(&wb), Some(&trae), &paths);
        assert_eq!(sums.len(), 2);
        assert!(sums[0].has_access_token);
        assert!(sums[1].device_id_present);
        assert!(summary_json_is_token_free(
            &sums[0],
            &[wb_secret]
        ));
        assert!(summary_json_is_token_free(
            &sums[1],
            &[trae_secret, "DEVICE_ID_FIXTURE_NOT_IN_SUMMARY"]
        ));
    }

    #[test]
    fn resolve_uses_env() {
        let paths = DesktopPaths::default();
        // On Linux box LOCALAPPDATA may be unset — None is ok.
        let _ = paths.resolve_wb_auth();
        let _ = paths.resolve_trae_storage();
        // With env set, join works
        std::env::set_var("LOCALAPPDATA", "/tmp/fake_local");
        std::env::set_var("APPDATA", "/tmp/fake_roaming");
        let wb = paths.resolve_wb_auth().unwrap();
        assert!(wb.ends_with("workbuddy-desktop.info"));
        let tr = paths.resolve_trae_storage().unwrap();
        assert!(tr.to_string_lossy().contains("Trae CN"));
        std::env::remove_var("LOCALAPPDATA");
        std::env::remove_var("APPDATA");
    }
}
