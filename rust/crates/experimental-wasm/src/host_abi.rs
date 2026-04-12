//! Host-provided functions wired into a [`wasmtime::Linker`].
//!
//! All host imports live in the `env` module, mirroring the classic C
//! calling convention. The signatures are documented in the crate-level
//! docs.

use std::time::{SystemTime, UNIX_EPOCH};

use wasmtime::{Caller, Extern, Linker, Memory};

use crate::error::WasmError;
use crate::instance::HostState;

/// Wire all host imports into `linker`.
pub fn register(linker: &mut Linker<HostState>) -> Result<(), WasmError> {
    linker
        .func_wrap("env", "log_info", log_info)
        .map_err(|e| WasmError::Instantiate(format!("link log_info: {e}")))?;
    linker
        .func_wrap("env", "log_warn", log_warn)
        .map_err(|e| WasmError::Instantiate(format!("link log_warn: {e}")))?;
    linker
        .func_wrap("env", "now_millis", now_millis)
        .map_err(|e| WasmError::Instantiate(format!("link now_millis: {e}")))?;
    linker
        .func_wrap("env", "http_get", http_get)
        .map_err(|e| WasmError::Instantiate(format!("link http_get: {e}")))?;
    Ok(())
}

fn caller_memory(caller: &mut Caller<'_, HostState>) -> Option<Memory> {
    match caller.get_export("memory") {
        Some(Extern::Memory(m)) => Some(m),
        _ => None,
    }
}

fn read_string(caller: &mut Caller<'_, HostState>, ptr: i32, len: i32) -> Option<String> {
    let memory = caller_memory(caller)?;
    let data = memory.data(&caller);
    let start = ptr as usize;
    let end = start.checked_add(len as usize)?;
    let slice = data.get(start..end)?;
    std::str::from_utf8(slice).ok().map(|s| s.to_string())
}

fn log_info(mut caller: Caller<'_, HostState>, ptr: i32, len: i32) {
    match read_string(&mut caller, ptr, len) {
        Some(msg) => tracing::info!(target: "experimental_wasm::guest", "{msg}"),
        None => tracing::warn!(
            target: "experimental_wasm::guest",
            "log_info: guest passed invalid UTF-8 or out-of-bounds pointer"
        ),
    }
}

fn log_warn(mut caller: Caller<'_, HostState>, ptr: i32, len: i32) {
    match read_string(&mut caller, ptr, len) {
        Some(msg) => tracing::warn!(target: "experimental_wasm::guest", "{msg}"),
        None => tracing::warn!(
            target: "experimental_wasm::guest",
            "log_warn: guest passed invalid UTF-8 or out-of-bounds pointer"
        ),
    }
}

fn now_millis(_caller: Caller<'_, HostState>) -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Stub: wasm guests are intentionally forbidden from issuing real HTTP.
/// Always returns `0` so guests can detect that the host declined.
fn http_get(_caller: Caller<'_, HostState>, _url_ptr: i32, _url_len: i32) -> i64 {
    0
}
