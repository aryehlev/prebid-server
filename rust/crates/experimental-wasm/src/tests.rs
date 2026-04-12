//! Unit tests for the experimental WASM runtime.
//!
//! The guest fixtures are written inline as WAT and compiled at test time
//! via `wat::parse_str`. They implement a minimal bump allocator inside a
//! dedicated linear memory region so we can exercise the full alloc /
//! handle_hook / dealloc dance without pulling in a Rust guest build.

use crate::instance::{pack, unpack};
use crate::runtime::WasmRuntime;

/// A minimal WAT guest that echoes the payload back unchanged.
///
/// Memory layout:
///   - `$bump` (mut i32) at the top holds the next free offset into memory.
///   - `alloc` bumps `$bump` by `size` and returns the previous value.
///   - `dealloc` is a no-op.
///   - `handle_hook` allocates a fresh buffer, copies the payload into it,
///     and returns the packed (ptr, len) pair.
const ECHO_WAT: &str = r#"
(module
  (memory (export "memory") 1)
  (global $bump (mut i32) (i32.const 1024))

  (func (export "alloc") (param $size i32) (result i32)
    (local $ptr i32)
    (local.set $ptr (global.get $bump))
    (global.set $bump (i32.add (global.get $bump) (local.get $size)))
    (local.get $ptr)
  )

  (func (export "dealloc") (param $ptr i32) (param $size i32)
    ;; no-op bump allocator
  )

  (func (export "handle_hook")
        (param $stage_ptr i32) (param $stage_len i32)
        (param $payload_ptr i32) (param $payload_len i32)
        (result i64)
    (local $out i32)
    (local.set $out (call $alloc_inner (local.get $payload_len)))
    (memory.copy
      (local.get $out)
      (local.get $payload_ptr)
      (local.get $payload_len))
    (i64.or
      (i64.shl (i64.extend_i32_u (local.get $out)) (i64.const 32))
      (i64.extend_i32_u (local.get $payload_len)))
  )

  (func $alloc_inner (param $size i32) (result i32)
    (local $ptr i32)
    (local.set $ptr (global.get $bump))
    (global.set $bump (i32.add (global.get $bump) (local.get $size)))
    (local.get $ptr)
  )
)
"#;

/// A WAT guest that imports `env.log_info` and calls it during
/// `handle_hook`. It returns an empty buffer.
const LOGGING_WAT: &str = r#"
(module
  (import "env" "log_info" (func $log_info (param i32 i32)))
  (memory (export "memory") 1)
  (data (i32.const 16) "hello from wasm")
  (global $bump (mut i32) (i32.const 1024))

  (func (export "alloc") (param $size i32) (result i32)
    (local $ptr i32)
    (local.set $ptr (global.get $bump))
    (global.set $bump (i32.add (global.get $bump) (local.get $size)))
    (local.get $ptr)
  )

  (func (export "dealloc") (param $ptr i32) (param $size i32))

  (func (export "handle_hook")
        (param i32) (param i32) (param i32) (param i32)
        (result i64)
    (call $log_info (i32.const 16) (i32.const 15))
    (i64.const 0)
  )
)
"#;

fn compile(wat: &str) -> Vec<u8> {
    wat::parse_str(wat).expect("wat should compile")
}

#[test]
fn pack_unpack_round_trip() {
    let cases = [
        (0, 0),
        (1, 2),
        (1024, 42),
        (0x7FFF_FFFF, 0x1234_5678),
    ];
    for (ptr, len) in cases {
        let packed = pack(ptr, len);
        assert_eq!(unpack(packed), (ptr, len), "failed for ({ptr}, {len})");
    }
}

#[test]
fn runtime_compiles_and_instantiates_echo_module() {
    let runtime = WasmRuntime::new().expect("runtime init");
    let bytes = compile(ECHO_WAT);
    let module = runtime.load_module(&bytes).expect("compile module");
    let _instance = module.instantiate().expect("instantiate");
}

#[test]
fn call_hook_round_trips_json_payload() {
    let runtime = WasmRuntime::new().expect("runtime init");
    let bytes = compile(ECHO_WAT);
    let module = runtime.load_module(&bytes).expect("compile");
    let mut instance = module.instantiate().expect("instantiate");

    let payload = br#"{"bidder":"acme","tmax":500}"#;
    let out = instance
        .call_hook("process_auction_request", payload)
        .expect("call_hook");
    assert_eq!(out, payload);
}

#[test]
fn call_hook_handles_multiple_invocations() {
    let runtime = WasmRuntime::new().expect("runtime init");
    let module = runtime.load_module(&compile(ECHO_WAT)).unwrap();
    let mut instance = module.instantiate().unwrap();

    for i in 0..5 {
        let payload = format!(r#"{{"n":{i}}}"#);
        let out = instance
            .call_hook("stage", payload.as_bytes())
            .expect("call_hook");
        assert_eq!(out, payload.as_bytes());
    }
}

#[test]
fn memory_allocation_advances_bump_pointer() {
    // Use the echo guest and observe that repeated hooks do not clobber
    // earlier buffers. If alloc returned the same address twice the echo
    // would be corrupted.
    let runtime = WasmRuntime::new().unwrap();
    let module = runtime.load_module(&compile(ECHO_WAT)).unwrap();
    let mut instance = module.instantiate().unwrap();

    let a = instance.call_hook("s", b"aaaaaaaa").unwrap();
    let b = instance.call_hook("s", b"bbbbbbbb").unwrap();
    assert_eq!(a, b"aaaaaaaa");
    assert_eq!(b, b"bbbbbbbb");
}

#[test]
fn guest_can_call_host_log_info() {
    let runtime = WasmRuntime::new().unwrap();
    let module = runtime.load_module(&compile(LOGGING_WAT)).unwrap();
    let mut instance = module.instantiate().unwrap();
    let out = instance.call_hook("stage", b"{}").expect("call_hook");
    assert!(out.is_empty());
}

#[tokio::test]
async fn hook_adapter_delegates_to_blocking_task() {
    use crate::hook_adapter::WasmHookAdapter;

    let runtime = WasmRuntime::new().unwrap();
    let module = runtime.load_module(&compile(ECHO_WAT)).unwrap();
    let instance = module.instantiate().unwrap();
    let adapter = WasmHookAdapter::new("process_auction_request", instance);

    let payload = br#"{"hello":"world"}"#.to_vec();
    let out = adapter.handle(payload.clone()).await.expect("handle");
    assert_eq!(out, payload);
}
