//! In-memory backend for tests and dry-run.

use std::collections::HashMap;
use std::sync::Mutex;

use crate::backend::CredentialBackend;
use crate::error::BackendError;

/// Thread-safe HashMap backend. Secrets live only in process memory.
#[derive(Debug, Default)]
pub struct MemoryBackend {
    inner: Mutex<HashMap<String, Vec<u8>>>,
}

impl MemoryBackend {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
        }
    }

    pub fn len(&self) -> usize {
        self.inner.lock().map(|g| g.len()).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl CredentialBackend for MemoryBackend {
    fn set(&self, key: &str, value: &[u8]) -> Result<(), BackendError> {
        let mut g = self
            .inner
            .lock()
            .map_err(|e| BackendError::Internal(format!("lock: {e}")))?;
        g.insert(key.to_string(), value.to_vec());
        Ok(())
    }

    fn get(&self, key: &str) -> Result<Vec<u8>, BackendError> {
        let g = self
            .inner
            .lock()
            .map_err(|e| BackendError::Internal(format!("lock: {e}")))?;
        g.get(key)
            .cloned()
            .ok_or(BackendError::NotFound)
    }

    fn delete(&self, key: &str) -> Result<(), BackendError> {
        let mut g = self
            .inner
            .lock()
            .map_err(|e| BackendError::Internal(format!("lock: {e}")))?;
        if g.remove(key).is_none() {
            return Err(BackendError::NotFound);
        }
        Ok(())
    }
}
