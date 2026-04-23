//! # experimental-wasm
//!
//! **EXPERIMENTAL**: WASM plugin runtime for Prebid hooks using
//! [`wasmtime`](https://docs.rs/wasmtime).
//!
//! This crate is an experimental proof of concept and is **not** ready for
//! production use. The API, ABI, and runtime semantics may change at any time
//! without notice.
//!
//! ## Guest ABI
//!
//! A guest module is expected to export the following functions:
//!
//! - `alloc(size: i32) -> i32`
//! - `dealloc(ptr: i32, size: i32)`
//! - `handle_hook(stage_ptr: i32, stage_len: i32, payload_ptr: i32, payload_len: i32) -> i64`
//!
//! The `i64` return value of `handle_hook` is a packed `(ptr, len)` pair
//! where the upper 32 bits are the result pointer and the lower 32 bits are
//! the result length.
//!
//! The host provides the following imports (in the `env` module):
//!
//! - `log_info(ptr: i32, len: i32)`
//! - `log_warn(ptr: i32, len: i32)`
//! - `now_millis() -> i64`
//! - `http_get(url_ptr: i32, url_len: i32) -> i64` (stub, always returns `0`)

pub mod error;
pub mod host_abi;
pub mod hook_adapter;
pub mod instance;
pub mod module;
pub mod runtime;

pub use error::WasmError;
pub use hook_adapter::WasmHookAdapter;
pub use instance::WasmInstance;
pub use module::WasmModule;
pub use runtime::WasmRuntime;

#[cfg(test)]
mod tests;
