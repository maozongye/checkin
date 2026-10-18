//! Windows Credential Manager backend (`CredWrite` / `CredRead` / `CredDelete`).
//!
//! TargetName = backend key (e.g. `com.benchforge.wb-trae.credential/v1/account/...`).
//! Type = CRED_TYPE_GENERIC; Persist = CRED_PERSIST_LOCAL_MACHINE.
//!
//! # Limits
//! Credential Manager blob max is `CRED_MAX_CREDENTIAL_BLOB_SIZE` (2560 bytes).
//! Oversized envelopes map to `BackendError::Internal`.

#![allow(unsafe_code)]

use std::ffi::c_void;

use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::{
    ERROR_ACCESS_DENIED, ERROR_INVALID_PARAMETER, ERROR_NOT_FOUND, ERROR_NO_SUCH_LOGON_SESSION,
    WIN32_ERROR,
};
use windows::Win32::Security::Credentials::{
    CredDeleteW, CredFree, CredReadW, CredWriteW, CREDENTIALW, CRED_FLAGS,
    CRED_MAX_CREDENTIAL_BLOB_SIZE, CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC,
};

use crate::backend::CredentialBackend;
use crate::error::BackendError;

/// Real OS backend for Windows Credential Manager.
#[derive(Debug, Default, Clone, Copy)]
pub struct OsKeychainBackend;

impl OsKeychainBackend {
    pub fn new() -> Self {
        Self
    }
}

impl CredentialBackend for OsKeychainBackend {
    fn set(&self, key: &str, value: &[u8]) -> Result<(), BackendError> {
        if value.len() > CRED_MAX_CREDENTIAL_BLOB_SIZE as usize {
            return Err(BackendError::Internal(format!(
                "credential blob {} bytes exceeds CRED_MAX_CREDENTIAL_BLOB_SIZE ({})",
                value.len(),
                CRED_MAX_CREDENTIAL_BLOB_SIZE
            )));
        }
        let blob_len = u32::try_from(value.len()).map_err(|_| {
            BackendError::Internal("credential blob length does not fit in u32".into())
        })?;

        let mut target = to_wide(key);
        let mut user = to_wide("credential-vault");
        // CredWrite reads blob via pointer; keep buffer alive for the call.
        let mut blob = value.to_vec();

        let credential = CREDENTIALW {
            Flags: CRED_FLAGS(0),
            Type: CRED_TYPE_GENERIC,
            TargetName: PWSTR(target.as_mut_ptr()),
            Comment: PWSTR::null(),
            LastWritten: Default::default(),
            CredentialBlobSize: blob_len,
            CredentialBlob: blob.as_mut_ptr(),
            Persist: CRED_PERSIST_LOCAL_MACHINE,
            AttributeCount: 0,
            Attributes: std::ptr::null_mut(),
            TargetAlias: PWSTR::null(),
            UserName: PWSTR(user.as_mut_ptr()),
        };

        // SAFETY: all pointers in `credential` reference live buffers for this call.
        unsafe { CredWriteW(&credential, 0) }.map_err(map_win_error)
    }

    fn get(&self, key: &str) -> Result<Vec<u8>, BackendError> {
        let target = to_wide(key);
        let mut pcred: *mut CREDENTIALW = std::ptr::null_mut();

        // SAFETY: target is NUL-terminated; `pcred` is a valid out-param.
        unsafe {
            CredReadW(
                PCWSTR(target.as_ptr()),
                CRED_TYPE_GENERIC,
                None, // flags = 0
                &mut pcred,
            )
        }
        .map_err(map_win_error)?;

        if pcred.is_null() {
            return Err(BackendError::NotFound);
        }

        let guard = CredGuard(pcred);

        // SAFETY: successful CredReadW returns a valid CREDENTIALW allocation owned by CredFree.
        let cred = unsafe { &*guard.0 };
        let len = cred.CredentialBlobSize as usize;
        if len == 0 {
            return Ok(Vec::new());
        }
        if cred.CredentialBlob.is_null() {
            return Err(BackendError::Internal(
                "CredRead returned non-zero size with null blob".into(),
            ));
        }
        // SAFETY: Credential Manager guarantees blob length matches CredentialBlobSize.
        let bytes = unsafe { std::slice::from_raw_parts(cred.CredentialBlob, len) };
        Ok(bytes.to_vec())
    }

    fn delete(&self, key: &str) -> Result<(), BackendError> {
        let target = to_wide(key);
        // SAFETY: target is NUL-terminated wide string.
        unsafe { CredDeleteW(PCWSTR(target.as_ptr()), CRED_TYPE_GENERIC, None) }
            .map_err(map_win_error)
    }
}

/// RAII wrapper so CredFree always runs.
struct CredGuard(*mut CREDENTIALW);

impl Drop for CredGuard {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: pointer came from CredReadW; freed exactly once.
            unsafe { CredFree(self.0 as *const c_void) };
        }
    }
}

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn map_win_error(err: windows::core::Error) -> BackendError {
    // Win32 APIs surface as HRESULT_FROM_WIN32(code) = 0x8007xxxx.
    let hr = err.code().0 as u32;
    let win32 = if (hr & 0xFFFF_0000) == 0x8007_0000 {
        WIN32_ERROR(hr & 0xFFFF)
    } else {
        WIN32_ERROR(hr)
    };

    match win32 {
        ERROR_NOT_FOUND => BackendError::NotFound,
        ERROR_ACCESS_DENIED | ERROR_NO_SUCH_LOGON_SESSION => BackendError::Denied,
        ERROR_INVALID_PARAMETER => BackendError::Internal(format!(
            "CredMan invalid parameter (hr={:#x})",
            err.code().0
        )),
        other => BackendError::Internal(format!(
            "CredMan error win32={} hr={:#x}: {err}",
            other.0,
            err.code().0
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_not_found() {
        let err = windows::core::Error::from(ERROR_NOT_FOUND);
        assert!(matches!(map_win_error(err), BackendError::NotFound));
    }

    #[test]
    fn map_access_denied() {
        let err = windows::core::Error::from(ERROR_ACCESS_DENIED);
        assert!(matches!(map_win_error(err), BackendError::Denied));
    }
}
