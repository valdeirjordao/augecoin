//! AUGECOIN WASM Lite Runtime (V1)
//!
//! A deterministic, sandboxed WebAssembly runtime for AUGECOIN smart contracts.
//! It is intentionally minimal:
//!
//! * Only a fixed set of host functions (the "auge" host API) may be imported.
//! * Execution is fuel-metered (gas). Running out of fuel traps deterministically.
//! * The contract has no access to the filesystem, network, clock, threads,
//!   environment, or any other node internals.
//! * All persistence goes through the host, which forwards it to the
//!   `ContractStateStore`. The contract can never touch RocksDB directly.
//!
//! The runtime is intentionally small and auditable. It is used by the
//! `augecoin-contracts` engine to execute arbitrary stored WASM code. The
//! built-in AUGE20 token is handled natively by the engine for V1 (see the
//! design notes in `augecoin-contracts`), but the same host API and ABI are
//! available for future WASM-based contracts.

use std::collections::HashSet;
use thiserror::Error;
use wasmi::core::Trap;
use wasmi::{
    Caller, Engine, ExternType, Instance, Linker, Module, Store, StoreLimits, StoreLimitsBuilder,
    Value,
};

/// Name of the only module a contract is allowed to import from.
pub const MODULE_NAME: &str = "auge";

/// Where the host writes the call input inside the guest's linear memory.
pub const INPUT_PTR: u32 = 1024;
/// Where the guest writes its call output inside its linear memory.
pub const OUTPUT_PTR: u32 = 4096;
/// Maximum output the guest may write at [`OUTPUT_PTR`].
pub const OUTPUT_MAX: u32 = 64 * 1024;

// Host-call gas costs (per call). Execution instructions are metered separately
// by wasmi's fuel mechanism; these cover the host API surface.
pub const GAS_HOST_CALL: u64 = 50;
pub const GAS_STORAGE_READ: u64 = 100;
pub const GAS_STORAGE_WRITE: u64 = 200;
pub const GAS_STORAGE_DELETE: u64 = 100;
pub const GAS_EMIT: u64 = 100;
pub const MAX_STORAGE_KEY_BYTES: u32 = 1024;
pub const MAX_STORAGE_VALUE_BYTES: u32 = 64 * 1024;
pub const MAX_EVENT_BYTES: u32 = 4 * 1024;
pub const MAX_EVENTS_PER_CALL: usize = 64;

/// Fixed signatures a contract may import. Anything else is rejected.
pub const ALLOWED_IMPORTS: &[&str] = &[
    "get_sender",
    "get_block_height",
    "get_contract_id",
    "storage_read",
    "storage_write",
    "storage_delete",
    "emit_event",
];

/// The only exports a transaction may ever trigger. The consensus path
/// (`augecoin-contracts` engine) only ever calls `instantiate`, `execute`,
/// and `query`; this list is the hard gate that rejects any other export
/// name even if a future refactor tries to dispatch on an arbitrary string.
pub const ALLOWED_EXPORTS: &[&str] = &["instantiate", "execute", "query"];

/// Resource limits for a single contract execution.
#[derive(Debug, Clone, Copy)]
pub struct RuntimeLimits {
    pub max_code_size: usize,
    pub max_memory_pages: u32,
    pub max_fuel: u64,
    pub max_call_depth: u32,
}

impl Default for RuntimeLimits {
    fn default() -> Self {
        Self {
            // 64 KiB of WASM code.
            max_code_size: 64 * 1024,
            // 2 MiB of linear memory (32 pages of 64 KiB).
            max_memory_pages: 32,
            // 100_000 fuel units for a single execution.
            max_fuel: 100_000,
            // V1 forbids nested calls.
            max_call_depth: 1,
        }
    }
}

pub trait HostInterface: Send {
    /// The authenticated sender (account number) of the transaction.
    fn sender(&self) -> u64;
    /// Current block height.
    fn block_height(&self) -> u64;
    /// The contract's id (blake3 hash).
    fn contract_id(&self) -> [u8; 32];
    /// Read a key from the contract's storage (may be `None`).
    fn storage_read(&self, key: &[u8]) -> Option<Vec<u8>>;
    /// Write a key in the contract's storage.
    fn storage_write(&mut self, key: &[u8], value: &[u8]);
    /// Delete a key from the contract's storage.
    fn storage_delete(&mut self, key: &[u8]);
    /// Emit a deterministic event for indexers.
    fn emit_event(&mut self, event: &[u8]);

    /// Recover writes produced during an execution. The default implementation
    /// (used by test/placeholder hosts) returns nothing. A real contract host
    /// overrides this to hand back the key/value pairs the contract mutated, so
    /// the engine can persist them.
    fn take_log(&mut self) -> Vec<(Vec<u8>, Option<Vec<u8>>)> {
        Vec::new()
    }

    /// Returns `true` if a host-storage operation exceeded the configured
    /// storage limit during execution. The engine inspects this after a
    /// (otherwise successful) run to fail the whole execution deterministically.
    fn storage_limit_exceeded(&self) -> bool {
        false
    }
}

pub struct HostState {
    host: Box<dyn HostInterface>,
    events: Vec<Vec<u8>>,
    resource_limits: StoreLimits,
}

/// Placeholder host used only to vacate [`HostState`] after an execution
/// returns the real host to the caller.
struct NoopHost;

impl HostInterface for NoopHost {
    fn sender(&self) -> u64 {
        0
    }
    fn block_height(&self) -> u64 {
        0
    }
    fn contract_id(&self) -> [u8; 32] {
        [0u8; 32]
    }
    fn storage_read(&self, _key: &[u8]) -> Option<Vec<u8>> {
        None
    }
    fn storage_write(&mut self, _key: &[u8], _value: &[u8]) {}
    fn storage_delete(&mut self, _key: &[u8]) {}
    fn emit_event(&mut self, _event: &[u8]) {}
}

/// Outcome of a single contract execution.
pub struct CallOutcome {
    pub output: Vec<u8>,
    pub events: Vec<Vec<u8>>,
    pub host: Box<dyn HostInterface>,
    pub gas_consumed: u64,
}

impl std::fmt::Debug for CallOutcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CallOutcome")
            .field("output_len", &self.output.len())
            .field("events", &self.events.len())
            .field("gas_consumed", &self.gas_consumed)
            .finish()
    }
}

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("invalid wasm: {0}")]
    InvalidWasm(String),
    #[error("wasm code too large: {size} > {max}")]
    CodeTooLarge { size: usize, max: usize },
    #[error("memory too large: {pages} pages > {max}")]
    MemoryTooLarge { pages: u32, max: u32 },
    #[error("invalid import: {0}")]
    InvalidImport(String),
    #[error("execution limit exceeded (out of gas)")]
    OutOfGas,
    #[error("storage limit exceeded")]
    StorageLimitExceeded,
    #[error("contract execution trap: {0}")]
    Trap(String),
    #[error("export not found: {0}")]
    ExportNotFound(String),
    #[error("export not allowed by V1 ABI: {0}")]
    ExportNotAllowed(String),
    #[error("memory access error")]
    MemoryError,
    #[error("return data exceeds maximum allowed size")]
    OutputTooLarge,
}

/// The WASM runtime. Holds a compiled `Engine` and the configured limits.
pub struct Runtime {
    engine: Engine,
    limits: RuntimeLimits,
}

impl Runtime {
    pub fn new(limits: RuntimeLimits) -> Self {
        let mut config = wasmi::Config::default();
        config.consume_fuel(true);
        let engine = Engine::new(&config);
        Self { engine, limits }
    }

    pub fn limits(&self) -> RuntimeLimits {
        self.limits
    }

    /// Validate a WASM module without instantiating it.
    ///
    /// Rejects oversized code, unknown imports, disallowed imports, and
    /// excessive memory. This is the gate used by `StoreCode`.
    pub fn validate(&self, wasm: &[u8]) -> Result<(), RuntimeError> {
        if wasm.len() > self.limits.max_code_size {
            return Err(RuntimeError::CodeTooLarge {
                size: wasm.len(),
                max: self.limits.max_code_size,
            });
        }
        let module = Module::new(&self.engine, wasm)
            .map_err(|e| RuntimeError::InvalidWasm(e.to_string()))?;

        let allowed: HashSet<&str> = ALLOWED_IMPORTS.iter().copied().collect();
        for import in module.imports() {
            if import.module() != MODULE_NAME {
                return Err(RuntimeError::InvalidImport(import.module().to_string()));
            }
            if !allowed.contains(import.name()) {
                return Err(RuntimeError::InvalidImport(import.name().to_string()));
            }
        }

        let mut mem_pages: u32 = 0;
        for export in module.exports() {
            if let ExternType::Memory(m) = export.ty() {
                mem_pages = u32::from(m.initial_pages());
                let maximum = m.maximum_pages().map(u32::from);
                if maximum.is_some_and(|pages| pages > self.limits.max_memory_pages) {
                    return Err(RuntimeError::MemoryTooLarge {
                        pages: maximum.unwrap_or(u32::MAX),
                        max: self.limits.max_memory_pages,
                    });
                }
            }
        }
        if mem_pages > self.limits.max_memory_pages {
            return Err(RuntimeError::MemoryTooLarge {
                pages: mem_pages,
                max: self.limits.max_memory_pages,
            });
        }
        Ok(())
    }

    /// Instantiate a validated module with the supplied host interface.
    pub fn instantiate(
        &self,
        wasm: &[u8],
        host: Box<dyn HostInterface>,
    ) -> Result<InstanceHandle, RuntimeError> {
        self.validate(wasm)?;
        let module = Module::new(&self.engine, wasm)
            .map_err(|e| RuntimeError::InvalidWasm(e.to_string()))?;

        let mut linker: Linker<HostState> = Linker::new(&self.engine);
        let max_fuel = self.limits.max_fuel;

        linker
            .func_wrap(
                MODULE_NAME,
                "get_sender",
                move |mut caller: Caller<HostState>, ptr: i32| -> Result<i32, Trap> {
                    GasMeter::new(&mut caller, max_fuel)
                        .charge(GAS_HOST_CALL)
                        .map_err(|_| Trap::new("out of gas"))?;
                    let sender = caller.data().host.sender();
                    write_memory(&mut caller, ptr as u32, &sender.to_be_bytes())
                        .map_err(|_| Trap::new("memory write error"))?;
                    Ok(0)
                },
            )
            .map_err(|e| RuntimeError::InvalidWasm(e.to_string()))?;

        linker
            .func_wrap(
                MODULE_NAME,
                "get_block_height",
                move |mut caller: Caller<HostState>| -> Result<i64, Trap> {
                    GasMeter::new(&mut caller, max_fuel)
                        .charge(GAS_HOST_CALL)
                        .map_err(|_| Trap::new("out of gas"))?;
                    Ok(caller.data().host.block_height() as i64)
                },
            )
            .map_err(|e| RuntimeError::InvalidWasm(e.to_string()))?;

        linker
            .func_wrap(
                MODULE_NAME,
                "get_contract_id",
                move |mut caller: Caller<HostState>, ptr: i32| -> Result<i32, Trap> {
                    GasMeter::new(&mut caller, max_fuel)
                        .charge(GAS_HOST_CALL)
                        .map_err(|_| Trap::new("out of gas"))?;
                    let id = caller.data().host.contract_id();
                    write_memory(&mut caller, ptr as u32, &id)
                        .map_err(|_| Trap::new("memory write error"))?;
                    Ok(0)
                },
            )
            .map_err(|e| RuntimeError::InvalidWasm(e.to_string()))?;

        linker
            .func_wrap(
                MODULE_NAME,
                "storage_read",
                move |mut caller: Caller<HostState>,
                      key_ptr: i32,
                      key_len: i32,
                      out_ptr: i32,
                      out_max: i32|
                      -> Result<i32, Trap> {
                    GasMeter::new(&mut caller, max_fuel)
                        .charge(GAS_STORAGE_READ)
                        .map_err(|_| Trap::new("out of gas"))?;
                    if key_len < 0 || key_len as u32 > MAX_STORAGE_KEY_BYTES {
                        return Err(Trap::new("storage key too large"));
                    }
                    let key = read_memory(&caller, key_ptr as u32, key_len as u32)
                        .map_err(|_| Trap::new("memory read error"))?;
                    match caller.data().host.storage_read(&key) {
                        Some(v) => {
                            if v.len() as i32 > out_max {
                                return Err(Trap::new("output buffer too small"));
                            }
                            write_memory(&mut caller, out_ptr as u32, &v)
                                .map_err(|_| Trap::new("memory write error"))?;
                            Ok(v.len() as i32)
                        }
                        None => Ok(-1),
                    }
                },
            )
            .map_err(|e| RuntimeError::InvalidWasm(e.to_string()))?;

        linker
            .func_wrap(
                MODULE_NAME,
                "storage_write",
                move |mut caller: Caller<HostState>,
                      key_ptr: i32,
                      key_len: i32,
                      val_ptr: i32,
                      val_len: i32|
                      -> Result<i32, Trap> {
                    GasMeter::new(&mut caller, max_fuel)
                        .charge(GAS_STORAGE_WRITE)
                        .map_err(|_| Trap::new("out of gas"))?;
                    if key_len < 0
                        || val_len < 0
                        || key_len as u32 > MAX_STORAGE_KEY_BYTES
                        || val_len as u32 > MAX_STORAGE_VALUE_BYTES
                    {
                        return Err(Trap::new("storage key or value too large"));
                    }
                    let key = read_memory(&caller, key_ptr as u32, key_len as u32)
                        .map_err(|_| Trap::new("memory read error"))?;
                    let val = read_memory(&caller, val_ptr as u32, val_len as u32)
                        .map_err(|_| Trap::new("memory read error"))?;
                    caller.data_mut().host.storage_write(&key, &val);
                    Ok(0)
                },
            )
            .map_err(|e| RuntimeError::InvalidWasm(e.to_string()))?;

        linker
            .func_wrap(
                MODULE_NAME,
                "storage_delete",
                move |mut caller: Caller<HostState>,
                      key_ptr: i32,
                      key_len: i32|
                      -> Result<i32, Trap> {
                    GasMeter::new(&mut caller, max_fuel)
                        .charge(GAS_STORAGE_DELETE)
                        .map_err(|_| Trap::new("out of gas"))?;
                    if key_len < 0 || key_len as u32 > MAX_STORAGE_KEY_BYTES {
                        return Err(Trap::new("storage key too large"));
                    }
                    let key = read_memory(&caller, key_ptr as u32, key_len as u32)
                        .map_err(|_| Trap::new("memory read error"))?;
                    caller.data_mut().host.storage_delete(&key);
                    Ok(0)
                },
            )
            .map_err(|e| RuntimeError::InvalidWasm(e.to_string()))?;

        linker
            .func_wrap(
                MODULE_NAME,
                "emit_event",
                move |mut caller: Caller<HostState>, ptr: i32, len: i32| -> Result<i32, Trap> {
                    GasMeter::new(&mut caller, max_fuel)
                        .charge(GAS_EMIT)
                        .map_err(|_| Trap::new("out of gas"))?;
                    if len < 0
                        || len as u32 > MAX_EVENT_BYTES
                        || caller.data().events.len() >= MAX_EVENTS_PER_CALL
                    {
                        return Err(Trap::new("event limit exceeded"));
                    }
                    let evt = read_memory(&caller, ptr as u32, len as u32)
                        .map_err(|_| Trap::new("memory read error"))?;
                    caller.data_mut().host.emit_event(&evt);
                    caller.data_mut().events.push(evt);
                    Ok(0)
                },
            )
            .map_err(|e| RuntimeError::InvalidWasm(e.to_string()))?;

        let mut store = Store::new(
            &self.engine,
            HostState {
                host,
                events: Vec::new(),
                resource_limits: StoreLimitsBuilder::new()
                    .memory_size(self.limits.max_memory_pages as usize * 64 * 1024)
                    .memories(1)
                    .build(),
            },
        );
        store.limiter(|state| &mut state.resource_limits);
        store
            .add_fuel(self.limits.max_fuel)
            .map_err(|e| RuntimeError::InvalidWasm(e.to_string()))?;

        let instance = linker
            .instantiate(&mut store, &module)
            .and_then(|pre| pre.start(&mut store))
            .map_err(|e| RuntimeError::InvalidWasm(e.to_string()))?;

        Ok(InstanceHandle {
            instance,
            store,
            partial: None,
        })
    }
}

/// A live instance of a contract, ready to be called.
pub struct InstanceHandle {
    instance: Instance,
    store: Store<HostState>,
    /// Partial outcome captured when an execution traps or is otherwise aborted,
    /// so callers (e.g. the engine's Fase 5 pipeline) can still recover gas
    /// consumed and events emitted before the failure.
    partial: Option<CallOutcome>,
}

impl InstanceHandle {
    /// Recover the partial outcome from a failed execution (trap / out-of-gas /
    /// oversized output). Returns `None` if no failure occurred or it was
    /// already taken.
    pub fn take_partial(&mut self) -> Option<CallOutcome> {
        self.partial.take()
    }
}

impl InstanceHandle {
    /// Hard gate: only the three V1 ABI exports may ever be invoked. Exposed as
    /// `pub` so cross-crate tests can assert that disallowed exports are rejected
    /// (it is a Rust method, not a WASM export, and remains the consensus path's
    /// only way to reach a guest function).
    pub fn call_export(&mut self, export: &str, input: &[u8]) -> Result<CallOutcome, RuntimeError> {
        // Hard gate: only the three V1 ABI exports may ever be invoked. This
        // guarantees that, regardless of any caller, a transaction can never
        // select an arbitrary WASM export.
        if !ALLOWED_EXPORTS.contains(&export) {
            return Err(RuntimeError::ExportNotAllowed(export.to_string()));
        }
        let mem = self
            .instance
            .get_export(&self.store, "memory")
            .and_then(|e| e.into_memory())
            .ok_or(RuntimeError::MemoryError)?;

        mem.write(&mut self.store, INPUT_PTR as usize, input)
            .map_err(|_| RuntimeError::MemoryError)?;

        let func = self
            .instance
            .get_func(&self.store, export)
            .ok_or_else(|| RuntimeError::ExportNotFound(export.to_string()))?;

        let mut result = [Value::I32(0)];
        match func.call(
            &mut self.store,
            &[Value::I32(INPUT_PTR as i32), Value::I32(input.len() as i32)],
            &mut result,
        ) {
            Ok(()) => {}
            Err(trap) => {
                let gas = self.store.fuel_consumed().unwrap_or(0);
                let data = self.store.data_mut();
                let host = std::mem::replace(&mut data.host, Box::new(NoopHost));
                let events = std::mem::take(&mut data.events);
                self.partial = Some(CallOutcome {
                    output: Vec::new(),
                    events,
                    host,
                    gas_consumed: gas,
                });
                let msg = trap.to_string();
                if msg.contains("fuel") || msg.contains("gas") {
                    return Err(RuntimeError::OutOfGas);
                }
                return Err(RuntimeError::Trap(msg));
            }
        }

        let out_len = result[0].i32().unwrap_or(0).max(0) as u32;
        if out_len > OUTPUT_MAX {
            // Recover partial state so the engine can still report gas/events.
            let gas = self.store.fuel_consumed().unwrap_or(0);
            let data = self.store.data_mut();
            let host = std::mem::replace(&mut data.host, Box::new(NoopHost));
            let events = std::mem::take(&mut data.events);
            self.partial = Some(CallOutcome {
                output: Vec::new(),
                events,
                host,
                gas_consumed: gas,
            });
            return Err(RuntimeError::OutputTooLarge);
        }
        let out_len = out_len.min(OUTPUT_MAX);
        let mut output = vec![0u8; out_len as usize];
        mem.read(&self.store, OUTPUT_PTR as usize, &mut output)
            .map_err(|_| RuntimeError::MemoryError)?;

        let gas_consumed = self.store.fuel_consumed().unwrap_or(0);
        let data = self.store.data_mut();
        let host = std::mem::replace(&mut data.host, Box::new(NoopHost));
        let events = std::mem::take(&mut data.events);
        Ok(CallOutcome {
            output,
            events,
            host,
            gas_consumed,
        })
    }

    /// Call `instantiate(input)`.
    pub fn instantiate(&mut self, input: &[u8]) -> Result<CallOutcome, RuntimeError> {
        self.call_export("instantiate", input)
    }

    /// Call `execute(input)`.
    pub fn execute(&mut self, input: &[u8]) -> Result<CallOutcome, RuntimeError> {
        self.call_export("execute", input)
    }

    /// Call `query(input)`.
    pub fn query(&mut self, input: &[u8]) -> Result<CallOutcome, RuntimeError> {
        self.call_export("query", input)
    }

    /// Execute an arbitrary exported function by name with the given WASM
    /// Low-level, **test-only** call of an arbitrary exported function by name.
    ///
    /// This is a testing/debugging escape hatch and is gated behind `#[cfg(test)]`
    /// so it is completely absent from production builds and therefore NOT exposed
    /// to the node or any consensus-facing crate.
    ///
    /// The V1 contract ABI is rigid: the only exports a transaction may ever
    /// trigger are `instantiate`, `execute`, and `query` (see
    /// [`InstanceHandle::instantiate`]/[`execute`]/[`query`]). The consensus
    /// path (`augecoin-contracts` engine) calls only those three and never this
    /// method, so a transaction can never choose an arbitrary export.
    ///
    /// Unlike [`InstanceHandle::execute`] (which follows the fixed AUGE20-style
    /// `execute(input_ptr, input_len)` ABI), this calls a plain WASM function so
    /// generic modules (e.g. `add(a, b)`) can be exercised directly. Fuel is
    /// still consumed during execution because the engine was created with
    /// `consume_fuel(true)`.
    #[cfg(test)]
    pub(crate) fn call(
        &mut self,
        export: &str,
        args: &[Value],
    ) -> Result<Vec<Value>, RuntimeError> {
        let func = self
            .instance
            .get_func(&self.store, export)
            .ok_or_else(|| RuntimeError::ExportNotFound(export.to_string()))?;
        let arity = func.ty(&self.store).results().len();
        let mut results = vec![Value::I32(0); arity];
        func.call(&mut self.store, args, &mut results)
            .map_err(map_execution_err)?;
        Ok(results)
    }
}

/// Map a `wasmi::Error` produced while invoking a guest function into the
/// runtime's typed [`RuntimeError`], treating fuel/gas exhaustion as
/// [`RuntimeError::OutOfGas`].
#[cfg(test)]
fn map_execution_err(e: wasmi::Error) -> RuntimeError {
    let msg = e.to_string();
    if msg.to_lowercase().contains("fuel") || msg.to_lowercase().contains("gas") {
        RuntimeError::OutOfGas
    } else {
        RuntimeError::Trap(msg)
    }
}

/// Deterministic gas accounting abstraction over Wasmi's fuel mechanism.
///
/// This reuses the engine's existing fuel meter (enabled via
/// `Config::consume_fuel(true)`); `charge` simply consumes fuel, `consumed` /
/// `remaining` read it back. Every host API call goes through [`GasMeter`] so
/// gas is always accounted — even when the surrounding execution later fails.
pub struct GasMeter<'a, 'b> {
    caller: &'a mut Caller<'b, HostState>,
    limit: u64,
}

impl<'a, 'b> GasMeter<'a, 'b> {
    pub fn new(caller: &'a mut Caller<'b, HostState>, limit: u64) -> Self {
        Self { caller, limit }
    }

    /// Consume `amount` fuel. Returns [`RuntimeError::OutOfGas`] when exhausted,
    /// which the linker maps into a deterministic trap.
    pub fn charge(&mut self, amount: u64) -> Result<(), RuntimeError> {
        self.caller
            .consume_fuel(amount)
            .map_err(|_| RuntimeError::OutOfGas)?;
        Ok(())
    }

    /// Fuel consumed so far in this execution.
    pub fn consumed(&self) -> u64 {
        self.caller.fuel_consumed().unwrap_or(0)
    }

    /// Fuel still available before exhaustion.
    pub fn remaining(&self) -> u64 {
        self.limit.saturating_sub(self.consumed())
    }
}

fn read_memory(caller: &Caller<HostState>, ptr: u32, len: u32) -> Result<Vec<u8>, Trap> {
    let mem = caller
        .get_export("memory")
        .and_then(|e| e.into_memory())
        .ok_or_else(|| Trap::new("missing memory"))?;
    let mut buf = vec![0u8; len as usize];
    mem.read(caller, ptr as usize, &mut buf)
        .map_err(|_| Trap::new("memory read out of bounds"))?;
    Ok(buf)
}

fn write_memory(caller: &mut Caller<HostState>, ptr: u32, data: &[u8]) -> Result<(), Trap> {
    let mem = caller
        .get_export("memory")
        .and_then(|e| e.into_memory())
        .ok_or_else(|| Trap::new("missing memory"))?;
    mem.write(caller, ptr as usize, data)
        .map_err(|_| Trap::new("memory write out of bounds"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const WAT: &str = r#"
    (module
      (import "auge" "get_sender" (func $get_sender (param i32) (result i32)))
      (import "auge" "storage_write" (func $storage_write (param i32 i32 i32 i32) (result i32)))
      (import "auge" "emit_event" (func $emit_event (param i32 i32) (result i32)))
      (memory 1)
      (export "memory" (memory 0))
      (export "execute" (func $execute))
      (func $execute (param $in_ptr i32) (param $in_len i32) (result i32)
        (drop (call $get_sender (i32.const 0)))
        (drop (call $storage_write (i32.const 0) (i32.const 8) (i32.const 0) (i32.const 8)))
        (drop (call $emit_event (i32.const 0) (i32.const 8)))
        (i32.const 0)
      )
    )
    "#;

    struct MockHost {
        sender: u64,
        writes: Vec<(Vec<u8>, Vec<u8>)>,
        events: Vec<Vec<u8>>,
        reads: std::collections::HashMap<Vec<u8>, Vec<u8>>,
    }

    impl MockHost {
        fn new(sender: u64) -> Self {
            Self {
                sender,
                writes: Vec::new(),
                events: Vec::new(),
                reads: std::collections::HashMap::new(),
            }
        }
    }

    impl HostInterface for MockHost {
        fn sender(&self) -> u64 {
            self.sender
        }
        fn block_height(&self) -> u64 {
            1
        }
        fn contract_id(&self) -> [u8; 32] {
            [7u8; 32]
        }
        fn storage_read(&self, key: &[u8]) -> Option<Vec<u8>> {
            self.reads.get(key).cloned()
        }
        fn storage_write(&mut self, key: &[u8], value: &[u8]) {
            self.writes.push((key.to_vec(), value.to_vec()));
        }
        fn storage_delete(&mut self, _key: &[u8]) {}
        fn emit_event(&mut self, event: &[u8]) {
            self.events.push(event.to_vec());
        }
    }

    fn sender_bytes(s: u64) -> Vec<u8> {
        s.to_be_bytes().to_vec()
    }

    #[test]
    fn validate_rejects_disallowed_import() {
        let wat = r#"
        (module
          (import "env" "exit" (func $exit (param i32)))
          (memory 1)
          (export "memory" (memory 0))
          (func (export "execute") (param i32 i32) (result i32) (i32.const 0))
        )
        "#;
        let wasm = wat::parse_str(wat).unwrap();
        let rt = Runtime::new(RuntimeLimits::default());
        assert!(matches!(
            rt.validate(&wasm),
            Err(RuntimeError::InvalidImport(_))
        ));
    }

    #[test]
    fn validate_rejects_oversized_memory() {
        let wat = r#"
        (module
          (memory 64)
          (export "memory" (memory 0))
          (func (export "execute") (param i32 i32) (result i32) (i32.const 0))
        )
        "#;
        let wasm = wat::parse_str(wat).unwrap();
        let rt = Runtime::new(RuntimeLimits::default());
        assert!(matches!(
            rt.validate(&wasm),
            Err(RuntimeError::MemoryTooLarge { .. })
        ));
    }

    #[test]
    fn validate_rejects_oversized_code() {
        let wasm = vec![0u8; 65 * 1024];
        let rt = Runtime::new(RuntimeLimits::default());
        assert!(matches!(
            rt.validate(&wasm),
            Err(RuntimeError::CodeTooLarge { .. })
        ));
    }

    #[test]
    fn execute_runs_and_emits_events() {
        let wasm = wat::parse_str(WAT).unwrap();
        let rt = Runtime::new(RuntimeLimits::default());
        let host = Box::new(MockHost::new(0x1234));
        let mut handle = rt.instantiate(&wasm, host).unwrap();
        let outcome = handle.execute(&[]).unwrap();

        // The emitted event proves get_sender ran and returned the sender bytes,
        // and that the full host-call path executed.
        let sb = sender_bytes(0x1234);
        assert_eq!(outcome.events, vec![sb]);
        assert!(outcome.gas_consumed > 0);
    }

    #[test]
    fn execute_runs_out_of_gas() {
        let wasm = wat::parse_str(WAT).unwrap();
        let limits = RuntimeLimits {
            max_fuel: 1, // far below the cost of a single host call
            ..Default::default()
        };
        let rt = Runtime::new(limits);
        let host = Box::new(MockHost::new(0x1234));
        let mut handle = rt.instantiate(&wasm, host).unwrap();
        assert!(matches!(handle.execute(&[]), Err(RuntimeError::OutOfGas)));
    }

    #[test]
    fn call_export_rejects_disallowed_exports() {
        let wasm = wat::parse_str(
            r#"
            (module
              (import "auge" "get_sender" (func $get_sender (param i32) (result i32)))
              (import "auge" "emit_event" (func $emit_event (param i32 i32) (result i32)))
              (memory 1)
              (export "memory" (memory 0))
              (export "execute" (func $execute))
              (export "add" (func $add))
              (func $execute (param i32 i32) (result i32) (i32.const 0))
              (func $add (param i32 i32) (result i32) (local.get 0) (local.get 1) (i32.add))
            )
            "#,
        )
        .unwrap();
        let rt = Runtime::new(RuntimeLimits::default());
        let host = Box::new(MockHost::new(0x1234));
        let mut handle = rt.instantiate(&wasm, host).unwrap();
        // Any export outside the V1 ABI is rejected by the hard gate.
        assert!(matches!(
            handle.call_export("add", &[]),
            Err(RuntimeError::ExportNotAllowed(_))
        ));
        // The canonical V1 exports remain callable.
        assert!(handle.execute(&[]).is_ok());
    }
}

/// Functional audit of the AUGECOIN WASM runtime: proves that real WASM modules
/// are actually interpreted by Wasmi, executed deterministically, sandboxed, and
/// gas/metered — without simulating WASM in Rust.
#[cfg(test)]
mod functional_audit {
    use super::*;

    /// Minimal host: unused by the pure computation modules but required by
    /// `instantiate`. For modules that declare host imports, validation rejects
    /// them before they ever reach this host.
    struct TestHost;

    impl HostInterface for TestHost {
        fn sender(&self) -> u64 {
            0
        }
        fn block_height(&self) -> u64 {
            0
        }
        fn contract_id(&self) -> [u8; 32] {
            [0u8; 32]
        }
        fn storage_read(&self, _key: &[u8]) -> Option<Vec<u8>> {
            None
        }
        fn storage_write(&mut self, _key: &[u8], _value: &[u8]) {}
        fn storage_delete(&mut self, _key: &[u8]) {}
        fn emit_event(&mut self, _event: &[u8]) {}
    }

    fn rt() -> Runtime {
        Runtime::new(RuntimeLimits::default())
    }

    /// `wasmi::Value` has no `PartialEq`; compare the I32 results we use here.
    fn as_i32(v: &Value) -> i32 {
        match v {
            Value::I32(x) => *x,
            other => panic!("expected i32, got {:?}", other),
        }
    }

    fn values_eq(a: &[Value], b: &[Value]) -> bool {
        a.len() == b.len() && a.iter().zip(b).all(|(x, y)| as_i32(x) == as_i32(y))
    }

    // 2. Real WASM execution: add(a, b) compiled from WAT and run by Wasmi.
    #[test]
    fn real_wasm_add_executes() {
        let wasm = wat::parse_str(
            r#"(module
                (func (export "add") (param i32 i32) (result i32)
                  local.get 0
                  local.get 1
                  i32.add))"#,
        )
        .unwrap();
        let mut handle = rt().instantiate(&wasm, Box::new(TestHost)).unwrap();
        let res = handle.call("add", &[Value::I32(2), Value::I32(3)]).unwrap();
        assert_eq!(as_i32(&res[0]), 5);

        // A different input to rule out a hardcoded return.
        let res2 = handle
            .call("add", &[Value::I32(-7), Value::I32(10)])
            .unwrap();
        assert_eq!(as_i32(&res2[0]), 3);
    }

    // 2b. identity(x) -> x, another real function executed by Wasmi.
    #[test]
    fn real_wasm_identity_executes() {
        let wasm = wat::parse_str(
            r#"(module
                (func (export "identity") (param i32) (result i32)
                  local.get 0))"#,
        )
        .unwrap();
        let mut handle = rt().instantiate(&wasm, Box::new(TestHost)).unwrap();
        let res = handle.call("identity", &[Value::I32(42)]).unwrap();
        assert_eq!(as_i32(&res[0]), 42);
    }

    // 3. Determinism: same module + same input => identical output, twice.
    #[test]
    fn determinism_same_output_twice() {
        let wasm = wat::parse_str(
            r#"(module
                (func (export "add") (param i32 i32) (result i32)
                  local.get 0
                  local.get 1
                  i32.add))"#,
        )
        .unwrap();
        let mut handle = rt().instantiate(&wasm, Box::new(TestHost)).unwrap();
        let a = handle.call("add", &[Value::I32(7), Value::I32(8)]).unwrap();
        let b = handle.call("add", &[Value::I32(7), Value::I32(8)]).unwrap();
        assert!(values_eq(&a, &b));
        assert_eq!(as_i32(&a[0]), 15);

        // And a freshly instantiated instance from identical bytes matches too.
        let mut handle2 = rt().instantiate(&wasm, Box::new(TestHost)).unwrap();
        let c = handle2
            .call("add", &[Value::I32(7), Value::I32(8)])
            .unwrap();
        assert!(values_eq(&a, &c));
    }

    // 4. Sandbox: forbidden imports (env/exit, wasi fd_write, unknown auge fn)
    //    are rejected at validation time.
    fn assert_import_rejected(wat: &str) {
        let wasm = wat::parse_str(wat).unwrap();
        assert!(matches!(
            rt().validate(&wasm),
            Err(RuntimeError::InvalidImport(_))
        ));
    }

    #[test]
    fn sandbox_rejects_prohibited_imports() {
        assert_import_rejected(
            r#"(module
                (import "env" "exit" (func $exit (param i32)))
                (memory 1)
                (export "memory" (memory 0))
                (func (export "execute") (param i32 i32) (result i32) (i32.const 0)))"#,
        );
        // WASI filesystem/network surface.
        assert_import_rejected(
            r#"(module
                (import "wasi_snapshot_preview1" "fd_write" (func $w (param i32 i32 i32 i32) (result i32)))
                (memory 1)
                (export "memory" (memory 0))
                (func (export "execute") (param i32 i32) (result i32) (i32.const 0)))"#,
        );
        // An "auge" import that is not in the allow-list.
        assert_import_rejected(
            r#"(module
                (import "auge" "launch_missiles" (func $m))
                (memory 1)
                (export "memory" (memory 0))
                (func (export "execute") (param i32 i32) (result i32) (i32.const 0)))"#,
        );
    }

    // 5. Memory limit: a module declaring more memory pages than the configured
    //    maximum is rejected before instantiation.
    #[test]
    fn memory_limit_rejects_oversized_module() {
        let wasm = wat::parse_str(
            r#"(module
                (memory 64)
                (export "memory" (memory 0))
                (func (export "execute") (param i32 i32) (result i32) (i32.const 0)))"#,
        )
        .unwrap();
        assert!(matches!(
            rt().validate(&wasm),
            Err(RuntimeError::MemoryTooLarge { .. })
        ));
    }

    // 5b. Runtime bounded memory: `memory.grow` cannot exceed the module's own
    //     declared maximum, so the contract cannot grab unbounded RAM.
    #[test]
    fn memory_grow_is_bounded() {
        let wasm = wat::parse_str(
            r#"(module
                (memory 1 1)
                (func (export "grow") (param i32) (result i32)
                  local.get 0
                  memory.grow))"#,
        )
        .unwrap();
        let mut handle = rt().instantiate(&wasm, Box::new(TestHost)).unwrap();
        let r = handle.call("grow", &[Value::I32(5)]).unwrap();
        // Growth beyond the declared max (1 page) is denied: returns -1.
        assert_eq!(as_i32(&r[0]), -1);
    }

    // 6. Gas: execution consumes fuel, and exhaustion fails deterministically.
    const EXEC_WAT: &str = r#"(module
        (import "auge" "get_sender" (func $get_sender (param i32) (result i32)))
        (import "auge" "storage_write" (func $storage_write (param i32 i32 i32 i32) (result i32)))
        (import "auge" "emit_event" (func $emit_event (param i32 i32) (result i32)))
        (memory 1)
        (export "memory" (memory 0))
        (export "execute" (func $execute))
        (func $execute (param $in_ptr i32) (param $in_len i32) (result i32)
          (drop (call $get_sender (i32.const 0)))
          (drop (call $storage_write (i32.const 0) (i32.const 8) (i32.const 0) (i32.const 8)))
          (drop (call $emit_event (i32.const 0) (i32.const 8)))
          (i32.const 0)
        )
    )"#;

    #[test]
    fn gas_is_consumed_and_enforced() {
        let wasm = wat::parse_str(EXEC_WAT).unwrap();

        // Default limits: executes and reports positive fuel consumption.
        let mut handle = rt().instantiate(&wasm, Box::new(TestHost)).unwrap();
        let outcome = handle.execute(&[]).unwrap();
        assert!(outcome.gas_consumed > 0);

        // Exhausted fuel => deterministic OutOfGas, no panic.
        let lim = RuntimeLimits {
            max_fuel: 1,
            ..Default::default()
        };
        let mut handle2 = Runtime::new(lim)
            .instantiate(&wasm, Box::new(TestHost))
            .unwrap();
        assert!(matches!(handle2.execute(&[]), Err(RuntimeError::OutOfGas)));

        // Determinism of the gas meter: two runs under the same limits consume
        // the exact same amount.
        let mut h_a = rt().instantiate(&wasm, Box::new(TestHost)).unwrap();
        let mut h_b = rt().instantiate(&wasm, Box::new(TestHost)).unwrap();
        let g_a = h_a.execute(&[]).unwrap().gas_consumed;
        let g_b = h_b.execute(&[]).unwrap().gas_consumed;
        assert_eq!(g_a, g_b);
    }

    // 7. Invalid module: garbage bytes must be rejected with a typed error, not
    //    a panic.
    #[test]
    fn invalid_module_rejected_without_panic() {
        let rt = rt();
        assert!(matches!(
            rt.validate(&[1, 2, 3, 4, 5, 6, 7, 8]),
            Err(RuntimeError::InvalidWasm(_))
        ));
        // Truncated magic header.
        assert!(matches!(
            rt.validate(&[0x00, 0x61, 0x73, 0x6d, 0xff, 0xff, 0xff]),
            Err(RuntimeError::InvalidWasm(_))
        ));
        // Empty input.
        assert!(matches!(
            rt.validate(&[]),
            Err(RuntimeError::InvalidWasm(_))
        ));
    }
}
