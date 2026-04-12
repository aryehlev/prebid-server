//! Instantiated WASM module with the hook entry point.

use wasmtime::{Extern, Instance, Memory, Store, TypedFunc};

use crate::error::WasmError;

/// Per-instance mutable state stored inside the wasmtime [`Store`].
///
/// Currently empty, but threaded through so that host ABI functions can
/// access per-call state (metrics, request context, etc.) in the future.
#[derive(Default)]
pub struct HostState {}

/// A fully instantiated wasm module, ready to handle hook invocations.
pub struct WasmInstance {
    store: Store<HostState>,
    memory: Memory,
    alloc: TypedFunc<i32, i32>,
    dealloc: TypedFunc<(i32, i32), ()>,
    handle_hook: TypedFunc<(i32, i32, i32, i32), i64>,
}

impl WasmInstance {
    /// Bind the exports on `instance` into typed function handles.
    pub fn new(mut store: Store<HostState>, instance: Instance) -> Result<Self, WasmError> {
        let memory = match instance.get_export(&mut store, "memory") {
            Some(Extern::Memory(m)) => m,
            _ => {
                return Err(WasmError::Instantiate(
                    "guest module does not export `memory`".into(),
                ))
            }
        };

        let alloc = instance
            .get_typed_func::<i32, i32>(&mut store, "alloc")
            .map_err(|e| WasmError::Instantiate(format!("export `alloc`: {e}")))?;
        let dealloc = instance
            .get_typed_func::<(i32, i32), ()>(&mut store, "dealloc")
            .map_err(|e| WasmError::Instantiate(format!("export `dealloc`: {e}")))?;
        let handle_hook = instance
            .get_typed_func::<(i32, i32, i32, i32), i64>(&mut store, "handle_hook")
            .map_err(|e| WasmError::Instantiate(format!("export `handle_hook`: {e}")))?;

        Ok(Self {
            store,
            memory,
            alloc,
            dealloc,
            handle_hook,
        })
    }

    /// Invoke the guest `handle_hook` function with `stage_name` and
    /// `payload_json`, returning the bytes the guest produced.
    pub fn call_hook(
        &mut self,
        stage_name: &str,
        payload_json: &[u8],
    ) -> Result<Vec<u8>, WasmError> {
        let stage_bytes = stage_name.as_bytes();

        let stage_ptr = self.write_bytes(stage_bytes)?;
        let payload_ptr = self.write_bytes(payload_json)?;

        let packed = self
            .handle_hook
            .call(
                &mut self.store,
                (
                    stage_ptr,
                    stage_bytes.len() as i32,
                    payload_ptr,
                    payload_json.len() as i32,
                ),
            )
            .map_err(|e| WasmError::Trap(e.to_string()))?;

        // Free the input buffers now that the guest has consumed them.
        self.dealloc
            .call(&mut self.store, (stage_ptr, stage_bytes.len() as i32))
            .map_err(|e| WasmError::Trap(format!("dealloc stage: {e}")))?;
        self.dealloc
            .call(&mut self.store, (payload_ptr, payload_json.len() as i32))
            .map_err(|e| WasmError::Trap(format!("dealloc payload: {e}")))?;

        let (result_ptr, result_len) = unpack(packed);
        let result = self.read_bytes(result_ptr, result_len)?;

        // Free the result buffer owned by the guest.
        if result_len > 0 {
            self.dealloc
                .call(&mut self.store, (result_ptr, result_len))
                .map_err(|e| WasmError::Trap(format!("dealloc result: {e}")))?;
        }

        Ok(result)
    }

    /// Allocate `bytes.len()` bytes inside the guest and copy `bytes` into
    /// the resulting buffer, returning the guest pointer.
    fn write_bytes(&mut self, bytes: &[u8]) -> Result<i32, WasmError> {
        let len = bytes.len() as i32;
        let ptr = self
            .alloc
            .call(&mut self.store, len)
            .map_err(|e| WasmError::Trap(format!("alloc: {e}")))?;
        if ptr < 0 {
            return Err(WasmError::GuestAbort(format!(
                "guest `alloc` returned negative pointer {ptr}"
            )));
        }
        let start = ptr as usize;
        let end = start
            .checked_add(bytes.len())
            .ok_or_else(|| WasmError::MemoryAccess("alloc length overflow".into()))?;
        let mem = self.memory.data_mut(&mut self.store);
        if end > mem.len() {
            return Err(WasmError::MemoryAccess(format!(
                "alloc range {start}..{end} out of bounds (len={})",
                mem.len()
            )));
        }
        mem[start..end].copy_from_slice(bytes);
        Ok(ptr)
    }

    /// Read `len` bytes from the guest starting at `ptr`.
    fn read_bytes(&mut self, ptr: i32, len: i32) -> Result<Vec<u8>, WasmError> {
        if len == 0 {
            return Ok(Vec::new());
        }
        if ptr < 0 || len < 0 {
            return Err(WasmError::MemoryAccess(format!(
                "negative ptr/len: ptr={ptr} len={len}"
            )));
        }
        let start = ptr as usize;
        let end = start
            .checked_add(len as usize)
            .ok_or_else(|| WasmError::MemoryAccess("read length overflow".into()))?;
        let mem = self.memory.data(&self.store);
        let slice = mem.get(start..end).ok_or_else(|| {
            WasmError::MemoryAccess(format!(
                "read range {start}..{end} out of bounds (len={})",
                mem.len()
            ))
        })?;
        Ok(slice.to_vec())
    }
}

/// Decode the packed `(ptr, len)` pair returned by `handle_hook`.
///
/// The upper 32 bits carry the pointer; the lower 32 bits carry the length.
pub fn unpack(packed: i64) -> (i32, i32) {
    let ptr = ((packed as u64) >> 32) as u32 as i32;
    let len = ((packed as u64) & 0xFFFF_FFFF) as u32 as i32;
    (ptr, len)
}

/// Encode a `(ptr, len)` pair into a single `i64`, matching [`unpack`].
///
/// Exposed for tests and for guest-side helpers written in Rust.
pub fn pack(ptr: i32, len: i32) -> i64 {
    (((ptr as u32 as u64) << 32) | (len as u32 as u64)) as i64
}
