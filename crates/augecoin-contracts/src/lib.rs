//! AUGECOIN Smart Contracts — WASM Lite V1
//!
//! This crate implements the deterministic, incremental, low-resource smart
//! contract subsystem described in the design prompt. It is intentionally
//! small and auditable.
//!
//! Design highlights:
//!
//! * **No SafeBox rewrite.** Contract state lives outside the account SafeBox,
//!   in its own column family, and is mutated incrementally (only the affected
//!   keys are written).
//! * **Determinism.** The WASM runtime is sandboxed (see `augecoin-wasm-runtime`),
//!   fuel-metered, and only exposes the fixed `auge` host API. Block context is
//!   supplied by the host, never by a wall clock.
//! * **Simplicity first.** The built-in AUGE20 token is handled natively by the
//!   engine for V1 (faster, simpler, fully deterministic). The WASM runtime is
//!   still the execution mechanism for arbitrary stored code (`StoreCode` +
//!   `CreateContract` + `ExecuteContract` of custom WASM). See `docs/auge20.md`
//!   for the rationale (prompt §32).
//! * **Incremental state.** Every apply call returns a list of writes that the
//!   caller persists only after the operation succeeds, preserving atomicity.

pub mod codec;
pub mod engine;
pub mod state;
pub mod store;
pub mod types;

pub use state::{CodeRegistry, ContractStateStore, ContractStateWrite};
pub use types::{
    CodeHash, ContractInfo, ContractState, CreateContract, ExecuteContract, StoreCode,
    TokenBalance, TokenInfo,
};

pub use engine::ContractEngine;

use crate::codec::{Reader, Writer};
use augecoin_crypto::hash::blake3_512;
use thiserror::Error;

/// Identifier of a registered WASM code blob.
pub type CodeId = u64;

/// Identifier of a deployed contract (blake3 hash).
pub type ContractId = [u8; 32];

/// Address used for token balances. In V1 this is the chain account number.
pub type Address = u64;

/// The built-in AUGE20 token is registered under this code id.
pub const AUGE20_CODE_ID: CodeId = 1;

/// Resource limits, centralized so no magic numbers are scattered around.
#[derive(Debug, Clone, Copy)]
pub struct ContractLimits {
    /// Maximum WASM bytecode size (bytes). 64 KiB.
    pub max_code_size: usize,
    /// Maximum linear memory (pages of 64 KiB). 32 pages = 2 MiB.
    pub max_memory_pages: u32,
    /// Maximum per-contract stored bytes. 64 KiB.
    pub max_storage_bytes: usize,
    /// Maximum execution fuel for a single WASM call.
    pub max_gas: u64,
    /// Maximum call depth (V1 forbids nested calls).
    pub max_call_depth: u32,
}

impl Default for ContractLimits {
    fn default() -> Self {
        Self {
            max_code_size: 64 * 1024,
            max_memory_pages: 32,
            max_storage_bytes: 64 * 1024,
            max_gas: 100_000,
            max_call_depth: 1,
        }
    }
}

/// Deterministic gas accounting model for AUGECOIN smart contracts (Fase 6).
///
/// # Vocabulary
///
/// * **gas_limit** — the total gas budget supplied for a single operation
///   (StoreCode / CreateContract / ExecuteContract). It is the caller-provided
///   ceiling; no execution may report `gas_used > gas_limit`.
/// * **gas_consumed** — the gas actually consumed by an execution. It is the sum
///   of the flat operation base cost (see [`gas`]) plus the WASM fuel consumed by
///   instruction execution *and* every host API call (storage_read / _write /
///   _delete / emit_event / context getters), all metered through Wasmi's fuel.
/// * **gas_remaining** — `gas_limit.saturating_sub(gas_used)`.
/// * **OutOfGas** — a typed [`ContractError::OutOfGas`] produced when the WASM
///   fuel is exhausted. It never panics, never persists partial state (the
///   overlay is discarded), and charges the full `gas_limit` as consumed.
///
/// # Relationship to the economic model
///
/// The network `fee` (charged in the state transition) is a *separate* flat
/// economic fee (anti-spam). On top of it, the state transition now charges the
/// sender for the gas actually consumed by contract execution:
/// `gas_used * GAS_PRICE_AUGESAT` (see [`gas::GAS_PRICE_AUGESAT`]), wired in
/// `augecoin-node/src/execution.rs`. `GAS_PRICE_AUGESAT` is a chain-wide consensus
/// constant, so the charge is deterministic across validators and is aggregated
/// into the block's total fees (credited to the block leader). Because
/// `gas_used` is fully deterministic (Wasmi fuel + host-call costs, writes sorted
/// for stability), this multiplication is consensus-safe. To change the economic
/// cost of contracts, adjust that single constant.
///
/// # Invariant
///
/// For every execution: `gas_used == base_cost + wasm_fuel_consumed` and
/// `gas_used <= gas_limit`. The WASM fuel handed to the runtime is therefore
/// `min(gas_limit.saturating_sub(base_cost), MAX_WASM_FUEL)`, guaranteeing the
/// sum can never exceed `gas_limit`.
pub mod gas {
    /// Flat operation base costs, charged on top of WASM fuel for the
    /// corresponding high-level operation.
    pub const STORE_CODE: u64 = 50_000;
    pub const CREATE_CONTRACT: u64 = 10_000;
    pub const TRANSFER: u64 = 5_000;
    pub const MINT: u64 = 7_500;
    pub const BURN: u64 = 5_000;

    /// Default per-operation gas limit. Used by the state transition when no
    /// explicit per-transaction gas limit is supplied (see RPC Fase 14).
    pub const DEFAULT_GAS_LIMIT: u64 = 100_000;

    /// Chain-wide gas price, in augesat per gas unit.
    ///
    /// This is a *consensus parameter*: every validator must agree on the same
    /// value, otherwise gas accounting would diverge and blocks would not
    /// reproduce. Charging is wired in the state transition
    /// (`augecoin-node/src/execution.rs`): the sender pays
    /// `gas_used * GAS_PRICE_AUGESAT` (in addition to the flat network fee), and
    /// the amount is aggregated into the block's total fees (and thus credited to
    /// the block leader). Because `gas_used` is fully deterministic (Wasmi fuel +
    /// host-call costs, writes sorted for stability in `engine.rs`), multiplying
    /// by this constant is consensus-safe.
    ///
    /// To change the economic cost of contracts, adjust this single constant; no
    /// change to the accounting internals is required.
    pub const GAS_PRICE_AUGESAT: u64 = 1;

    /// Maximum fuel that may be handed to Wasmi in a single execution.
    ///
    /// Wasmi stores fuel internally as `i64`; an arbitrarily large `gas_limit`
    /// is therefore clamped to this ceiling so a huge `gas_limit` can never
    /// overflow the runtime. `gas_used` is reported against the *original*
    /// `gas_limit`, keeping accounting deterministic and bounded.
    pub const MAX_WASM_FUEL: u64 = i64::MAX as u64;

    /// Clamp a caller-supplied `gas_limit` to the value that can actually be
    /// handed to Wasmi as fuel (independent of any base cost).
    pub fn clamp_fuel(gas_limit: u64) -> u64 {
        gas_limit.min(MAX_WASM_FUEL)
    }
}

#[derive(Debug, Error)]
pub enum ContractError {
    #[error("invalid serialization")]
    InvalidSerialization,
    #[error("invalid wasm: {0}")]
    InvalidWasm(String),
    #[error("wasm code too large")]
    WasmTooLarge,
    #[error("invalid import: {0}")]
    InvalidImport(String),
    #[error("execution limit exceeded (out of gas)")]
    ExecutionLimitExceeded,
    #[error("storage limit exceeded")]
    StorageLimitExceeded,
    #[error("out of gas")]
    OutOfGas,
    #[error("contract not found")]
    ContractNotFound,
    #[error("code not found")]
    CodeNotFound,
    #[error("unauthorized")]
    Unauthorized,
    #[error("insufficient balance")]
    InsufficientBalance,
    #[error("max supply exceeded")]
    MaxSupplyExceeded,
    #[error("invalid amount")]
    InvalidAmount,
    #[error("arithmetic overflow")]
    ArithmeticOverflow,
    #[error("invalid decimals")]
    InvalidDecimals,
    #[error("invalid token metadata")]
    InvalidTokenMetadata,
    #[error("invalid address")]
    InvalidAddress,
    #[error("mint disabled")]
    MintDisabled,
    #[error("burn disabled")]
    BurnDisabled,
    #[error("duplicate code")]
    DuplicateCode,
    #[error("duplicate contract")]
    DuplicateContract,
    #[error("contract execution failed: {0}")]
    ExecutionFailed(String),
    #[error("storage error: {0}")]
    Storage(String),
    #[error("invalid execution context")]
    InvalidContext,
    #[error("return data exceeds maximum allowed size")]
    ReturnDataTooLarge,
}

impl From<augecoin_storage::StorageError> for ContractError {
    fn from(e: augecoin_storage::StorageError) -> Self {
        ContractError::Storage(e.to_string())
    }
}

/// Top-level contract operation carried inside `OperationPayload::Contract`.
///
/// The payloads are the canonical typed structs defined in [`crate::types`],
/// ensuring a single source of truth for these structures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContractOp {
    StoreCode(StoreCode),
    CreateContract(CreateContract),
    ExecuteContract(ExecuteContract),
}

impl ContractOp {
    pub fn op_type(&self) -> u8 {
        match self {
            ContractOp::StoreCode(_) => 0,
            ContractOp::CreateContract(_) => 1,
            ContractOp::ExecuteContract(_) => 2,
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.u8(self.op_type());
        match self {
            ContractOp::StoreCode(s) => {
                w.bytes(&s.wasm_code);
            }
            ContractOp::CreateContract(c) => {
                w.u64(c.code_id);
                w.bytes(&c.instantiate_data);
            }
            ContractOp::ExecuteContract(c) => {
                w.fixed(&c.contract_id);
                w.bytes(&c.call_data);
            }
        }
        w.into_vec()
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self, ContractError> {
        let mut r = Reader::new(data);
        let t = r.u8()?;
        let op = match t {
            0 => ContractOp::StoreCode(StoreCode {
                wasm_code: r.bytes()?,
            }),
            1 => ContractOp::CreateContract(CreateContract {
                code_id: r.u64()?,
                instantiate_data: r.bytes()?,
            }),
            2 => ContractOp::ExecuteContract(ExecuteContract {
                contract_id: r.fixed()?,
                call_data: r.bytes()?,
            }),
            _ => return Err(ContractError::InvalidSerialization),
        };
        r.expect_end()?;
        Ok(op)
    }
}

/// AUGE20 instantiation parameters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenInit {
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
    pub initial_supply: u64,
    pub max_supply: u64,
    pub mint_enabled: bool,
    pub burn_enabled: bool,
}

impl TokenInit {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.u8(self.decimals);
        w.bool(self.mint_enabled);
        w.bool(self.burn_enabled);
        w.u64(self.initial_supply);
        w.u64(self.max_supply);
        w.string(&self.name);
        w.string(&self.symbol);
        w.into_vec()
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self, ContractError> {
        let mut r = Reader::new(data);
        let decimals = r.u8()?;
        let mint_enabled = r.bool()?;
        let burn_enabled = r.bool()?;
        let initial_supply = r.u64()?;
        let max_supply = r.u64()?;
        let name = r.string()?;
        let symbol = r.string()?;
        r.expect_end()?;
        Ok(TokenInit {
            name,
            symbol,
            decimals,
            initial_supply,
            max_supply,
            mint_enabled,
            burn_enabled,
        })
    }
}

/// AUGE20 action invoked via `ExecuteContract`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenCall {
    Transfer { to: Address, amount: u64 },
    Mint { to: Address, amount: u64 },
    Burn { amount: u64 },
}

impl TokenCall {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut w = Writer::new();
        match self {
            TokenCall::Transfer { to, amount } => {
                w.u8(0);
                w.u64(*to);
                w.u64(*amount);
            }
            TokenCall::Mint { to, amount } => {
                w.u8(1);
                w.u64(*to);
                w.u64(*amount);
            }
            TokenCall::Burn { amount } => {
                w.u8(2);
                w.u64(*amount);
            }
        }
        w.into_vec()
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self, ContractError> {
        let mut r = Reader::new(data);
        let t = r.u8()?;
        let call = match t {
            0 => {
                let to = r.u64()?;
                let amount = r.u64()?;
                TokenCall::Transfer { to, amount }
            }
            1 => {
                let to = r.u64()?;
                let amount = r.u64()?;
                TokenCall::Mint { to, amount }
            }
            2 => {
                let amount = r.u64()?;
                TokenCall::Burn { amount }
            }
            _ => return Err(ContractError::InvalidSerialization),
        };
        r.expect_end()?;
        Ok(call)
    }
}

/// Read-only query against a contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenQuery {
    BalanceOf(Address),
    TotalSupply,
    TokenInfo,
}

impl TokenQuery {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut w = Writer::new();
        match self {
            TokenQuery::BalanceOf(a) => {
                w.u8(0);
                w.u64(*a);
            }
            TokenQuery::TotalSupply => {
                w.u8(1);
            }
            TokenQuery::TokenInfo => {
                w.u8(2);
            }
        }
        w.into_vec()
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self, ContractError> {
        let mut r = Reader::new(data);
        let t = r.u8()?;
        let q = match t {
            0 => TokenQuery::BalanceOf(r.u64()?),
            1 => TokenQuery::TotalSupply,
            2 => TokenQuery::TokenInfo,
            _ => return Err(ContractError::InvalidSerialization),
        };
        r.expect_end()?;
        Ok(q)
    }
}

/// Deterministic contract event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContractEvent {
    CodeStored {
        code_id: CodeId,
        code_hash: [u8; 32],
    },
    ContractCreated {
        contract_id: ContractId,
        owner: Address,
        code_id: CodeId,
    },
    TokenTransferred {
        contract_id: ContractId,
        from: Address,
        to: Address,
        amount: u64,
    },
    TokenMinted {
        contract_id: ContractId,
        to: Address,
        amount: u64,
    },
    TokenBurned {
        contract_id: ContractId,
        from: Address,
        amount: u64,
    },
}

impl ContractEvent {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut w = Writer::new();
        match self {
            ContractEvent::CodeStored { code_id, code_hash } => {
                w.u8(0);
                w.u64(*code_id);
                w.fixed(code_hash);
            }
            ContractEvent::ContractCreated {
                contract_id,
                owner,
                code_id,
            } => {
                w.u8(1);
                w.fixed(contract_id);
                w.u64(*owner);
                w.u64(*code_id);
            }
            ContractEvent::TokenTransferred {
                contract_id,
                from,
                to,
                amount,
            } => {
                w.u8(2);
                w.fixed(contract_id);
                w.u64(*from);
                w.u64(*to);
                w.u64(*amount);
            }
            ContractEvent::TokenMinted {
                contract_id,
                to,
                amount,
            } => {
                w.u8(3);
                w.fixed(contract_id);
                w.u64(*to);
                w.u64(*amount);
            }
            ContractEvent::TokenBurned {
                contract_id,
                from,
                amount,
            } => {
                w.u8(4);
                w.fixed(contract_id);
                w.u64(*from);
                w.u64(*amount);
            }
        }
        w.into_vec()
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self, ContractError> {
        let mut r = Reader::new(data);
        let t = r.u8()?;
        let ev = match t {
            0 => {
                let code_id = r.u64()?;
                let code_hash = r.fixed()?;
                ContractEvent::CodeStored { code_id, code_hash }
            }
            1 => {
                let contract_id = r.fixed()?;
                let owner = r.u64()?;
                let code_id = r.u64()?;
                ContractEvent::ContractCreated {
                    contract_id,
                    owner,
                    code_id,
                }
            }
            2 => {
                let contract_id = r.fixed()?;
                let from = r.u64()?;
                let to = r.u64()?;
                let amount = r.u64()?;
                ContractEvent::TokenTransferred {
                    contract_id,
                    from,
                    to,
                    amount,
                }
            }
            3 => {
                let contract_id = r.fixed()?;
                let to = r.u64()?;
                let amount = r.u64()?;
                ContractEvent::TokenMinted {
                    contract_id,
                    to,
                    amount,
                }
            }
            4 => {
                let contract_id = r.fixed()?;
                let from = r.u64()?;
                let amount = r.u64()?;
                ContractEvent::TokenBurned {
                    contract_id,
                    from,
                    amount,
                }
            }
            _ => return Err(ContractError::InvalidSerialization),
        };
        r.expect_end()?;
        Ok(ev)
    }
}

/// A write produced by an apply call. `None` value means deletion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteOp(pub Vec<u8>, pub Option<Vec<u8>>);

/// Result of applying a contract operation. The caller persists `writes`
/// (and `events`) only after the whole operation succeeds.
#[derive(Debug, Default)]
pub struct ApplyOutcome {
    pub gas_used: u64,
    pub events: Vec<ContractEvent>,
    pub writes: Vec<WriteOp>,
}

impl ApplyOutcome {
    pub fn write(&mut self, key: Vec<u8>, value: Vec<u8>) {
        self.writes.push(WriteOp(key, Some(value)));
    }
    pub fn delete(&mut self, key: Vec<u8>) {
        self.writes.push(WriteOp(key, None));
    }
}

/// Compute the blake3-based code hash for a WASM blob.
pub fn code_hash_of(wasm: &[u8]) -> [u8; 32] {
    let full = blake3_512(wasm);
    let mut h = [0u8; 32];
    h.copy_from_slice(&full[..32]);
    h
}

/// Compute a deterministic contract id.
pub fn contract_id_of(owner: Address, code_id: CodeId, init: &[u8]) -> ContractId {
    let mut w = Writer::new();
    w.u64(owner);
    w.u64(code_id);
    w.bytes(init);
    let full = blake3_512(&w.into_vec());
    let mut h = [0u8; 32];
    h.copy_from_slice(&full[..32]);
    h
}
