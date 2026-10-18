//! credential_ref parse / format (v0.2.2).
//!
//! - Account: `cred://v1/account/{platform}/{account_key}/{uuid}`
//! - Switch backup: `cred://v1/switch_backup/{product}/{snapshot_id}`
//!
//! Backend physical key = `{SERVICE}/` + path after `cred://`

use percent_encoding::{percent_decode_str, utf8_percent_encode, AsciiSet, CONTROLS};

/// Encode only path-breaking / unsafe chars; keep `-._~@+` readable.
const PATH_SEGMENT: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'/')
    .add(b'?')
    .add(b'#')
    .add(b'%')
    .add(b'"')
    .add(b'<')
    .add(b'>')
    .add(b'\\')
    .add(b'^')
    .add(b'`')
    .add(b'{')
    .add(b'|')
    .add(b'}');
use uuid::Uuid;

use crate::error::DeleteError;
use crate::types::PlatformId;

/// OS keychain / Credential Manager service prefix.
pub const SERVICE: &str = "com.benchforge.wb-trae.credential";

const SCHEME_PREFIX: &str = "cred://";
const V1_ACCOUNT: &str = "v1/account/";
const V1_SWITCH_BACKUP: &str = "v1/switch_backup/";

/// Opaque credential reference.
#[derive(Clone, PartialEq, Eq, Hash)]
pub enum CredentialRef {
    /// Runtime account credential (stored in account.credential_ref).
    Account {
        platform: PlatformId,
        account_key: String,
        id: Uuid,
    },
    /// Switcher temporary backup (same Backend; NOT in account.credential_ref).
    SwitchBackup {
        product: String,
        snapshot_id: String,
    },
}

impl std::fmt::Debug for CredentialRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // refs are non-secret identifiers — safe to display
        f.write_str(&self.to_string())
    }
}

impl std::fmt::Display for CredentialRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.to_ref_string())
    }
}

impl CredentialRef {
    pub fn account(platform: PlatformId, account_key: impl Into<String>, id: Uuid) -> Self {
        Self::Account {
            platform,
            account_key: account_key.into(),
            id,
        }
    }

    pub fn switch_backup(product: impl Into<String>, snapshot_id: impl Into<String>) -> Self {
        Self::SwitchBackup {
            product: product.into(),
            snapshot_id: snapshot_id.into(),
        }
    }

    pub fn to_ref_string(&self) -> String {
        match self {
            CredentialRef::Account {
                platform,
                account_key,
                id,
            } => {
                let key = encode_path_seg(account_key);
                format!("cred://v1/account/{platform}/{key}/{id}")
            }
            CredentialRef::SwitchBackup {
                product,
                snapshot_id,
            } => {
                let p = encode_path_seg(product);
                let s = encode_path_seg(snapshot_id);
                format!("cred://v1/switch_backup/{p}/{s}")
            }
        }
    }

    /// Parse from `cred://v1/...` string.
    pub fn parse(s: &str) -> Result<Self, String> {
        let rest = s
            .strip_prefix(SCHEME_PREFIX)
            .ok_or_else(|| "must start with cred://".to_string())?;

        if let Some(path) = rest.strip_prefix(V1_ACCOUNT) {
            let parts: Vec<&str> = path.split('/').collect();
            if parts.len() != 3 {
                return Err(format!(
                    "account ref expects platform/account_key/uuid, got {path:?}"
                ));
            }
            let platform = PlatformId::parse(parts[0])
                .ok_or_else(|| format!("unknown platform: {}", parts[0]))?;
            let account_key = decode_path_seg(parts[1])?;
            if account_key.is_empty() {
                return Err("account_key empty".into());
            }
            let id = Uuid::parse_str(parts[2])
                .map_err(|e| format!("invalid uuid: {e}"))?;
            return Ok(CredentialRef::Account {
                platform,
                account_key,
                id,
            });
        }

        if let Some(path) = rest.strip_prefix(V1_SWITCH_BACKUP) {
            let parts: Vec<&str> = path.split('/').collect();
            if parts.len() != 2 {
                return Err(format!(
                    "switch_backup ref expects product/snapshot_id, got {path:?}"
                ));
            }
            let product = decode_path_seg(parts[0])?;
            let snapshot_id = decode_path_seg(parts[1])?;
            if product.is_empty() || snapshot_id.is_empty() {
                return Err("product/snapshot_id empty".into());
            }
            return Ok(CredentialRef::SwitchBackup {
                product,
                snapshot_id,
            });
        }

        Err(format!(
            "unknown kind after cred:// (expected v1/account/ or v1/switch_backup/): {rest}"
        ))
    }

    /// Backend physical key: `{SERVICE}/` + path after `cred://`.
    pub fn to_backend_key(&self) -> String {
        let ref_s = self.to_ref_string();
        let path = ref_s
            .strip_prefix(SCHEME_PREFIX)
            .expect("ref always has cred://");
        format!("{SERVICE}/{path}")
    }

    pub fn as_account(&self) -> Option<(PlatformId, &str, Uuid)> {
        match self {
            CredentialRef::Account {
                platform,
                account_key,
                id,
            } => Some((*platform, account_key.as_str(), *id)),
            _ => None,
        }
    }

    pub fn uuid_for_backup_filename(&self) -> String {
        match self {
            CredentialRef::Account { id, .. } => id.to_string(),
            CredentialRef::SwitchBackup { snapshot_id, .. } => snapshot_id.clone(),
        }
    }
}

fn encode_path_seg(s: &str) -> String {
    // Encode anything that would break path splitting or be unsafe.
    utf8_percent_encode(s, PATH_SEGMENT).to_string()
}

fn decode_path_seg(s: &str) -> Result<String, String> {
    percent_decode_str(s)
        .decode_utf8()
        .map(|c| c.into_owned())
        .map_err(|e| format!("percent-decode: {e}"))
}

impl TryFrom<&str> for CredentialRef {
    type Error = String;
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        CredentialRef::parse(value)
    }
}

pub(crate) fn parse_ref_delete(s: &str) -> Result<CredentialRef, DeleteError> {
    CredentialRef::parse(s).map_err(DeleteError::InvalidRef)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_account() {
        let id = Uuid::parse_str("aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee").unwrap();
        let r = CredentialRef::account(PlatformId::Workbuddy, "user@x.com", id);
        let s = r.to_ref_string();
        assert!(s.starts_with("cred://v1/account/workbuddy/"));
        assert!(s.contains("aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee"));
        let p = CredentialRef::parse(&s).unwrap();
        assert_eq!(p, r);
        assert_eq!(
            r.to_backend_key(),
            format!(
                "{SERVICE}/v1/account/workbuddy/{}/aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee",
                encode_path_seg("user@x.com")
            )
        );
    }

    #[test]
    fn roundtrip_switch_backup() {
        let r = CredentialRef::switch_backup("workbuddy", "snap-001");
        let s = r.to_ref_string();
        assert_eq!(s, "cred://v1/switch_backup/workbuddy/snap-001");
        assert_eq!(CredentialRef::parse(&s).unwrap(), r);
        assert_eq!(
            r.to_backend_key(),
            format!("{SERVICE}/v1/switch_backup/workbuddy/snap-001")
        );
    }

    #[test]
    fn reject_old_format() {
        let err = CredentialRef::parse("cred://workbuddy/user/aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee")
            .unwrap_err();
        assert!(err.contains("unknown kind") || err.contains("v1/account"));
    }

    #[test]
    fn reject_unknown_kind() {
        assert!(CredentialRef::parse("cred://v1/other/x/y").is_err());
    }
}
