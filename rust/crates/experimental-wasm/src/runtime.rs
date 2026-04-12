//! Top-level WASM runtime that owns a shared [`wasmtime::Engine`].

use wasmtime::{Config, Engine};

use crate::error::WasmError;
use crate::module::WasmModule;

/// A handle to a shared wasmtime [`Engine`] used to compile and instantiate
/// guest modules.
///
/// Cloning a [`WasmRuntime`] is cheap: the inner engine is reference counted
/// inside wasmtime.
#[derive(Clone)]
pub struct WasmRuntime {
    engine: Engine,
}

impl WasmRuntime {
    /// Create a new runtime with a default configuration.
    pub fn new() -> Result<Self, WasmError> {
        let mut config = Config::new();
        // Async support is not required for the experimental runtime; we
        // drive the guest from a blocking task. Keep the config minimal.
        config.wasm_multi_memory(true);
        config.wasm_bulk_memory(true);
        let engine = Engine::new(&config)
            .map_err(|e| WasmError::EngineInit(e.to_string()))?;
        Ok(Self { engine })
    }

    /// Access the underlying engine.
    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    /// Compile a module from its binary representation.
    ///
    /// The bytes may be either the raw `.wasm` binary or (if wasmtime was
    /// built with the `wat` feature) a textual `.wat` module.
    pub fn load_module(&self, bytes: &[u8]) -> Result<WasmModule, WasmError> {
        WasmModule::compile(&self.engine, bytes)
    }
}
