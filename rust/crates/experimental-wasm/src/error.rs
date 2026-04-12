//! Error types for the experimental WASM runtime.

use thiserror::Error;

/// Errors produced by the experimental WASM runtime.
#[derive(Debug, Error)]
pub enum WasmError {
    #[error("failed to initialize wasmtime engine: {0}")]
    EngineInit(String),

    #[error("failed to compile wasm module: {0}")]
    ModuleCompile(String),

    #[error("failed to instantiate wasm module: {0}")]
    Instantiate(String),

    #[error("wasm memory access error: {0}")]
    MemoryAccess(String),

    #[error("guest aborted: {0}")]
    GuestAbort(String),

    #[error("wasm trap: {0}")]
    Trap(String),

    #[error("invalid UTF-8 data crossing the guest boundary")]
    Utf8,

    #[error("I/O error: {0}")]
    Io(String),
}
