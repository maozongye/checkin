//! CredentialBackend trait + MemoryBackend + OS keychain backends.

mod memory;

#[cfg(windows)]
mod windows;

pub use memory::MemoryBackend;

use crate::error::BackendError;

/// Low-level key/value backend (OS keychain or in-memory).
pub trait CredentialBackend: Send + Sync {
    fn set(&self, key: &str, value: &[u8]) -> Result<(), BackendError>;
    fn get(&self, key: &str) -> Result<Vec<u8>, BackendError>;
    fn delete(&self, key: &str) -> Result<(), BackendError>;
}

#[cfg(windows)]
pub use windows::OsKeychainBackend;

/// Non-Windows placeholder: returns Unavailable until a platform backend lands.
#[cfg(not(windows))]
#[derive(Debug, Default, Clone, Copy)]
pub struct OsKeychainBackend;

#[cfg(not(windows))]
impl OsKeychainBackend {
    pub fn new() -> Self {
        Self
    }
}

#[cfg(not(windows))]
impl CredentialBackend for OsKeychainBackend {
    fn set(&self, _key: &str, _value: &[u8]) -> Result<(), BackendError> {
        Err(BackendError::Unavailable)
    }
    fn get(&self, _key: &str) -> Result<Vec<u8>, BackendError> {
        Err(BackendError::Unavailable)
    }
    fn delete(&self, _key: &str) -> Result<(), BackendError> {
        Err(BackendError::Unavailable)
    }
}

/// Construct the OS backend for this target.
/// - Windows: real Credential Manager
/// - others: Unavailable stub (use [`MemoryBackend`] for tests / Linux dry-run)
pub fn os_backend() -> OsKeychainBackend {
    OsKeychainBackend::default()
}

/// Deprecated alias kept for handoff compatibility.
#[inline]
pub fn os_backend_stub() -> OsKeychainBackend {
    os_backend()
}
