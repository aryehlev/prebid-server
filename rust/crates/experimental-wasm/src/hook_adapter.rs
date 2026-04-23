//! Async adapter that runs [`WasmInstance::call_hook`] on a blocking task.
//!
//! WASM execution is CPU-bound and synchronous, so we intentionally hop onto
//! a dedicated blocking pool thread rather than blocking a tokio worker.

use std::sync::{Arc, Mutex};

use crate::error::WasmError;
use crate::instance::WasmInstance;

/// Async-friendly wrapper around a [`WasmInstance`].
///
/// The instance is wrapped in an [`Arc<Mutex<...>>`] because wasmtime stores
/// are single-threaded and must not be entered concurrently.
#[derive(Clone)]
pub struct WasmHookAdapter {
    stage: String,
    instance: Arc<Mutex<WasmInstance>>,
}

impl WasmHookAdapter {
    /// Construct a new adapter for `stage` backed by `instance`.
    pub fn new(stage: impl Into<String>, instance: WasmInstance) -> Self {
        Self {
            stage: stage.into(),
            instance: Arc::new(Mutex::new(instance)),
        }
    }

    /// Invoke the guest hook asynchronously.
    pub async fn handle(&self, payload_json: Vec<u8>) -> Result<Vec<u8>, WasmError> {
        let stage = self.stage.clone();
        let instance = self.instance.clone();

        tokio::task::spawn_blocking(move || {
            let mut guard = instance.lock().map_err(|e| {
                WasmError::GuestAbort(format!("wasm instance mutex poisoned: {e}"))
            })?;
            guard.call_hook(&stage, &payload_json)
        })
        .await
        .map_err(|e| WasmError::Io(format!("spawn_blocking join error: {e}")))?
    }
}
