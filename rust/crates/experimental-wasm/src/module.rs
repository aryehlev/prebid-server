//! Compiled WASM module plus a pre-built linker wired with host imports.

use std::sync::Arc;

use wasmtime::{Engine, Linker, Module, Store};

use crate::error::WasmError;
use crate::host_abi;
use crate::instance::{HostState, WasmInstance};

/// A compiled wasm [`Module`] together with a pre-built [`Linker`] that has
/// the host imports from [`crate::host_abi`] wired in.
#[derive(Clone)]
pub struct WasmModule {
    engine: Engine,
    module: Module,
    linker: Arc<Linker<HostState>>,
}

impl WasmModule {
    /// Compile `bytes` against `engine` and build a linker with host
    /// imports attached.
    pub fn compile(engine: &Engine, bytes: &[u8]) -> Result<Self, WasmError> {
        let module = Module::new(engine, bytes)
            .map_err(|e| WasmError::ModuleCompile(e.to_string()))?;
        let mut linker: Linker<HostState> = Linker::new(engine);
        host_abi::register(&mut linker)?;
        Ok(Self {
            engine: engine.clone(),
            module,
            linker: Arc::new(linker),
        })
    }

    /// Instantiate the module into a fresh [`WasmInstance`].
    pub fn instantiate(&self) -> Result<WasmInstance, WasmError> {
        let mut store = Store::new(&self.engine, HostState::default());
        let instance = self
            .linker
            .instantiate(&mut store, &self.module)
            .map_err(|e| WasmError::Instantiate(e.to_string()))?;
        WasmInstance::new(store, instance)
    }

    /// Borrow the compiled module.
    pub fn module(&self) -> &Module {
        &self.module
    }
}
