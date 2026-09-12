use augecoin_contracts::engine::QueryResult;
use augecoin_contracts::store::{
    CodeRegistry, ContractMeta, ContractStateStore, StorageContractReader,
};
use augecoin_contracts::{
    Address, CodeHash, ContractEngine, ContractId, ContractLimits, ContractOp, CreateContract,
    ExecuteContract, StoreCode, TokenQuery, AUGE20_CODE_ID,
};
use augecoin_core::constants::MINER_FEE_PER_OPERATION;
use augecoin_core::operation::{Operation, OperationPayload, OperationType};
use augecoin_crypto::address::{derive_address, AddressHash};
use augecoin_storage::{CF_CONTRACT_BALANCES, CF_CONTRACT_METADATA};
use serde::{Deserialize, Serialize};
use std::sync::atomic::Ordering;

#[cfg(test)]
use super::auth;
use super::endpoints::{handle_send_operation, AppState, SendOperationParams};

// ── contract_get ────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ContractGetParams {
    pub contract_id_hex: String,
}

#[derive(Debug, Serialize)]
pub struct ContractGetResult {
    pub exists: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_hex: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code_id_hex: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code_hash_hex: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_height: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_auge20: Option<bool>,
}

pub fn handle_contract_get(
    params: ContractGetParams,
    state: &AppState,
) -> Result<ContractGetResult, String> {
    let id = parse_contract_id_hex(&params.contract_id_hex)?;
    let store = ContractStateStore::new(&state.storage);
    let meta = store
        .get_contract(&id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "contract not found".to_string())?;
    let code_hash = match meta.code_id {
        AUGE20_CODE_ID => {
            // AUGE20 is a known built-in; derive a stable code_hash.
            let wasm = augecoin_contracts::engine::AUGE20_WASM;
            augecoin_contracts::code_hash_of(wasm)
        }
        _ => {
            // lookup the stored code record
            let registry = CodeRegistry::new(&state.storage);
            match registry
                .get_by_id(meta.code_id)
                .map_err(|e| e.to_string())?
            {
                Some(rec) => rec.code_hash,
                None => return Err("code not found".into()),
            }
        }
    };
    Ok(ContractGetResult {
        exists: true,
        owner_hex: Some(hex::encode(meta.owner.to_be_bytes())),
        code_id_hex: Some(hex::encode(meta.code_id.to_be_bytes())),
        code_hash_hex: Some(hex::encode(code_hash.as_slice())),
        created_height: Some(meta.created_height),
        is_auge20: if meta.code_id == AUGE20_CODE_ID {
            Some(true)
        } else {
            Some(false)
        },
    })
}

// ── contract_code ───────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ContractCodeByIdParams {
    pub code_id_hex: String,
}

#[derive(Debug, Deserialize)]
pub struct ContractCodeByHashParams {
    pub code_hash_hex: String,
}

#[derive(Debug, Serialize)]
pub struct ContractCodeResult {
    pub exists: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code_id_hex: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code_hash_hex: Option<String>,
    /// Wasm size in bytes. Returned only if exists.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wasm_size: Option<usize>,
}

pub fn handle_contract_code_by_id(
    params: ContractCodeByIdParams,
    state: &AppState,
) -> Result<ContractCodeResult, String> {
    let id = parse_code_id(&params.code_id_hex)?;
    let registry = CodeRegistry::new(&state.storage);
    match registry.get_by_id(id).map_err(|e| e.to_string())? {
        Some(rec) => Ok(ContractCodeResult {
            exists: true,
            code_id_hex: Some(hex::encode(rec.code_id.to_be_bytes())),
            code_hash_hex: Some(hex::encode(rec.code_hash.as_slice())),
            wasm_size: Some(rec.wasm.len()),
        }),
        None => Ok(ContractCodeResult {
            exists: false,
            code_id_hex: None,
            code_hash_hex: None,
            wasm_size: None,
        }),
    }
}

pub fn handle_contract_code_by_hash(
    params: ContractCodeByHashParams,
    state: &AppState,
) -> Result<ContractCodeResult, String> {
    let h = parse_code_hash_hex(&params.code_hash_hex)?;
    let registry = CodeRegistry::new(&state.storage);
    match registry.get_by_hash(&h).map_err(|e| e.to_string())? {
        Some(rec) => Ok(ContractCodeResult {
            exists: true,
            code_id_hex: Some(hex::encode(rec.code_id.to_be_bytes())),
            code_hash_hex: Some(hex::encode(rec.code_hash.as_slice())),
            wasm_size: Some(rec.wasm.len()),
        }),
        None => Ok(ContractCodeResult {
            exists: false,
            code_id_hex: None,
            code_hash_hex: None,
            wasm_size: None,
        }),
    }
}

// ── contract_state (generic WASM key/value) ───────────────────────────

#[derive(Debug, Deserialize)]
pub struct ContractStateParams {
    pub contract_id_hex: String,
    pub key_hex: String,
}

#[derive(Debug, Serialize)]
pub struct ContractStateResult {
    pub exists: bool,
    /// Present only when the key exists.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_hex: Option<String>,
}

pub fn handle_contract_state(
    params: ContractStateParams,
    state: &AppState,
) -> Result<ContractStateResult, String> {
    let id = parse_contract_id_hex(&params.contract_id_hex)?;
    let key = hex_decode_bytes(&params.key_hex).map_err(|_| "invalid key hex".to_string())?;
    if key.is_empty() {
        return Err("key must not be empty".into());
    }
    let store = ContractStateStore::new(&state.storage);
    // Verify contract exists first.
    if store
        .get_contract(&id)
        .map_err(|e| e.to_string())?
        .is_none()
    {
        return Err("contract not found".into());
    }
    match store
        .get_contract_state(&id, &key)
        .map_err(|e| e.to_string())?
    {
        Some(v) => Ok(ContractStateResult {
            exists: true,
            value_hex: Some(hex::encode(v)),
        }),
        None => Ok(ContractStateResult {
            exists: false,
            value_hex: None,
        }),
    }
}

// ── contract_balance ────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ContractBalanceParams {
    pub contract_id_hex: String,
    pub address_hex: String,
}

#[derive(Debug, Serialize)]
pub struct ContractBalanceResult {
    /// The contract/token this balance belongs to.
    pub contract_id_hex: String,
    /// The account this balance belongs to.
    pub address_hex: String,
    /// Balance in smallest unit. 0 if no entry exists.
    pub balance: u64,
}

pub fn handle_contract_balance(
    params: ContractBalanceParams,
    state: &AppState,
) -> Result<ContractBalanceResult, String> {
    let id = parse_contract_id_hex(&params.contract_id_hex)?;
    let addr = parse_address(&params.address_hex)?;
    let store = ContractStateStore::new(&state.storage);
    // For AUGE20 we can use the engine query to keep single source of truth.
    if store
        .get_contract(&id)
        .map_err(|e| e.to_string())?
        .is_none()
    {
        return Err("contract not found".into());
    }
    let e = engine();
    let q = TokenQuery::BalanceOf(addr);
    let reader = StorageContractReader(&state.storage);
    let out = e
        .query(&reader, id, &q.to_bytes())
        .map_err(|e| e.to_string())?;
    let result = match QueryResult::from_bytes(&out).map_err(|e| e.to_string())? {
        QueryResult::Balance(b) => b,
        _ => return Err("unexpected query result".into()),
    };
    Ok(ContractBalanceResult {
        contract_id_hex: hex::encode(id),
        address_hex: hex::encode(addr.to_be_bytes()),
        balance: result,
    })
}

// ── contract_token_info ─────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ContractTokenInfoParams {
    pub contract_id_hex: String,
}

#[derive(Debug, Serialize)]
pub struct ContractTokenInfoResult {
    /// Set when the contract is an AUGE20 token (native or WASM).
    pub token: Option<TokenInfoView>,
    /// When the contract is a generic WASM contract, token info is unavailable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
pub struct TokenInfoView {
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
    pub total_supply: u64,
    pub max_supply: u64,
    pub owner: Address,
    pub mint_enabled: bool,
    pub burn_enabled: bool,
    /// Number of accounts with a non-zero balance for this token.
    pub holder_count: u64,
}

pub fn handle_contract_token_info(
    params: ContractTokenInfoParams,
    state: &AppState,
) -> Result<ContractTokenInfoResult, String> {
    let id = parse_contract_id_hex(&params.contract_id_hex)?;
    let store = ContractStateStore::new(&state.storage);
    let meta = store
        .get_contract(&id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "contract not found".to_string())?;
    if meta.code_id != AUGE20_CODE_ID {
        return Ok(ContractTokenInfoResult {
            token: None,
            error: Some("contract is not an AUGE20 token".into()),
        });
    }
    // Route through the engine query so token info is read from the same WASM
    // state the contract actually maintains (the native `token/{cid}` CF is only
    // written by the legacy custom-AUGE20 helper, not by the built-in WASM
    // contract; see SE-3 in the Fase 19 audit report).
    let reader = StorageContractReader(&state.storage);
    let e = engine();
    let out = e
        .query(&reader, id, &TokenQuery::TokenInfo.to_bytes())
        .map_err(|e| e.to_string())?;
    let info = match QueryResult::from_bytes(&out).map_err(|e| e.to_string())? {
        QueryResult::Info(v) => v,
        _ => return Err("unexpected token info query".into()),
    };
    // Dedicated balance keys are `bal || contract_id || augeid_be`.
    let mut balance_prefix = b"bal".to_vec();
    balance_prefix.extend_from_slice(&id);
    let holder_count = state
        .storage
        .contract_scan_prefix_cf(CF_CONTRACT_BALANCES, &balance_prefix)
        .map_err(|e| e.to_string())?
        .into_iter()
        .filter(|(key, value)| {
            if key.len() != balance_prefix.len() + std::mem::size_of::<u64>() {
                return false;
            }
            let balance_bytes: [u8; 8] = match value.as_slice().try_into() {
                Ok(bytes) => bytes,
                Err(_) => return false,
            };
            u64::from_be_bytes(balance_bytes) > 0
        })
        .count() as u64;
    Ok(ContractTokenInfoResult {
        token: Some(TokenInfoView {
            name: info.name,
            symbol: info.symbol,
            decimals: info.decimals,
            total_supply: info.total_supply,
            max_supply: info.max_supply,
            owner: info.owner,
            mint_enabled: info.mint_enabled,
            burn_enabled: info.burn_enabled,
            holder_count,
        }),
        error: None,
    })
}

// ── contract_list_auge20 ────────────────────────────────────────────────

/// List all deployed AUGE20 token contracts (built-in WASM tokens).
///
/// Backed by a storage prefix scan over `contract/{id}` metadata records,
/// filtered to `is_auge20`, then enriched with live token info read through
/// the contract engine (same source of truth as `contract_token_info`).
#[derive(Debug, Deserialize, Default)]
pub struct ContractListParams {
    #[serde(default)]
    pub start: usize,
    #[serde(default = "default_list_limit")]
    pub limit: usize,
}

fn default_list_limit() -> usize {
    100
}

#[derive(Debug, Serialize, Clone)]
pub struct ContractListEntry {
    pub contract_id_hex: String,
    pub owner_hex: String,
    pub code_id_hex: String,
    pub created_height: u64,
    pub is_auge20: bool,
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
    pub total_supply: u64,
    pub max_supply: u64,
    pub mint_enabled: bool,
    pub burn_enabled: bool,
}

#[derive(Debug, Serialize)]
pub struct ContractListResult {
    pub total: usize,
    pub contracts: Vec<ContractListEntry>,
}

pub fn handle_contract_list_auge20(
    params: ContractListParams,
    state: &AppState,
) -> Result<ContractListResult, String> {
    let reader = StorageContractReader(&state.storage);
    let pairs = state
        .storage
        .contract_scan_prefix_cf(CF_CONTRACT_METADATA, b"contract")
        .map_err(|e| e.to_string())?;
    let e = engine();
    let mut contracts: Vec<ContractListEntry> = Vec::new();
    for (k, v) in pairs {
        if k.len() != 8 + 32 {
            continue;
        }
        let meta = match ContractMeta::from_bytes(&v) {
            Ok(m) => m,
            Err(_) => continue,
        };
        // A contract is an AUGE20 token when it was created from the built-in
        // AUGE20 code (code_id == AUGE20_CODE_ID). `ContractMeta::is_auge20` is
        // not always kept in sync at creation, so follow `contract_get` and key
        // off the code id instead.
        if meta.code_id != AUGE20_CODE_ID {
            continue;
        }
        let id = meta.contract_id;
        let out = match e.query(&reader, id, &TokenQuery::TokenInfo.to_bytes()) {
            Ok(o) => o,
            Err(_) => continue,
        };
        let info = match QueryResult::from_bytes(&out).map_err(|e| e.to_string())? {
            QueryResult::Info(v) => v,
            _ => continue,
        };
        contracts.push(ContractListEntry {
            contract_id_hex: hex::encode(id),
            owner_hex: hex::encode(meta.owner.to_be_bytes()),
            code_id_hex: hex::encode(meta.code_id.to_be_bytes()),
            created_height: meta.created_height,
            is_auge20: true,
            name: info.name,
            symbol: info.symbol,
            decimals: info.decimals,
            total_supply: info.total_supply,
            max_supply: info.max_supply,
            mint_enabled: info.mint_enabled,
            burn_enabled: info.burn_enabled,
        });
    }
    contracts.sort_by(|a, b| b.created_height.cmp(&a.created_height));
    let total = contracts.len();
    let start = params.start.min(total);
    let end = (start + params.limit).min(total);
    Ok(ContractListResult {
        total,
        contracts: contracts[start..end].to_vec(),
    })
}

// ── contract_query ──────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ContractQueryParams {
    pub contract_id_hex: String,
    pub query_hex: String,
}

#[derive(Debug, Serialize)]
pub struct ContractQueryResult {
    /// Hex-encoded contract response bytes.
    pub result_hex: String,
}

pub fn handle_contract_query(
    params: ContractQueryParams,
    state: &AppState,
) -> Result<ContractQueryResult, String> {
    let id = parse_contract_id_hex(&params.contract_id_hex)?;
    let q = hex_decode_bytes(&params.query_hex).map_err(|_| "invalid query hex".to_string())?;
    let store = ContractStateStore::new(&state.storage);
    if store
        .get_contract(&id)
        .map_err(|e| e.to_string())?
        .is_none()
    {
        return Err("contract not found".into());
    }
    let e = engine();
    let reader = StorageContractReader(&state.storage);
    let out = e.query(&reader, id, &q).map_err(|e| e.to_string())?;
    Ok(ContractQueryResult {
        result_hex: hex::encode(&out),
    })
}

// ── contract_simulate ───────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ContractSimulateParams {
    pub contract_id_hex: String,
    pub call_hex: String,
    pub sender_hex: String,
    #[serde(default)]
    pub gas_limit_hex: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ContractSimulateResult {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result_hex: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gas_used: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gas_remaining: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub events: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

pub fn handle_contract_simulate(
    params: ContractSimulateParams,
    state: &AppState,
) -> Result<ContractSimulateResult, String> {
    let id = parse_contract_id_hex(&params.contract_id_hex)?;
    let sender = parse_address(&params.sender_hex)?;
    let call = hex_decode_bytes(&params.call_hex).map_err(|_| "invalid call hex".to_string())?;
    let gas_limit = parse_simulation_gas_limit(params.gas_limit_hex)?;
    let store = ContractStateStore::new(&state.storage);
    if store
        .get_contract(&id)
        .map_err(|e| e.to_string())?
        .is_none()
    {
        return Err("contract not found".into());
    }
    // Simulate using the same engine semantics, but do NOT apply writes.
    let e = engine();
    let reader = StorageContractReader(&state.storage);
    let res = e
        .execute(&reader, sender, id, &call, 0, gas_limit)
        .map_err(|e| e.to_string())?;
    // Return a deterministic snapshot without persisting anything to the real contract store.
    Ok(ContractSimulateResult {
        success: true,
        result_hex: {
            let mut buf = Vec::new();
            for w in &res.writes {
                buf.extend_from_slice(&w.0);
            }
            Some(hex::encode(&buf))
        },
        gas_used: Some(res.gas_used),
        gas_remaining: Some(gas_limit.saturating_sub(res.gas_used)),
        events: Some(
            res.events
                .iter()
                .map(|ev| hex::encode(ev.to_bytes()))
                .collect(),
        ),
        error: None,
    })
}

// ── contract_estimate_gas ───────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ContractEstimateGasParams {
    pub contract_id_hex: String,
    pub call_hex: String,
    pub sender_hex: String,
    #[serde(default)]
    pub gas_limit_hex: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ContractEstimateGasResult {
    /// Deterministic gas estimate for the operation.
    pub gas_estimate: u64,
    /// The gas_limit used as the simulation ceiling.
    pub gas_limit_used: u64,
}

pub fn handle_contract_estimate_gas(
    params: ContractEstimateGasParams,
    state: &AppState,
) -> Result<ContractEstimateGasResult, String> {
    let id = parse_contract_id_hex(&params.contract_id_hex)?;
    let sender = parse_address(&params.sender_hex)?;
    let call = hex_decode_bytes(&params.call_hex).map_err(|_| "invalid call hex".to_string())?;
    let gas_limit = parse_simulation_gas_limit(params.gas_limit_hex)?;
    let store = ContractStateStore::new(&state.storage);
    if store
        .get_contract(&id)
        .map_err(|e| e.to_string())?
        .is_none()
    {
        return Err("contract not found".into());
    }
    let e = engine();
    let reader = StorageContractReader(&state.storage);
    let res = e
        .execute(&reader, sender, id, &call, 0, gas_limit)
        .map_err(|e| e.to_string())?;
    Ok(ContractEstimateGasResult {
        gas_estimate: res.gas_used,
        gas_limit_used: gas_limit,
    })
}

fn parse_simulation_gas_limit(gas_limit_hex: Option<String>) -> Result<u64, String> {
    let gas_limit = gas_limit_hex
        .map(|value| parse_u64_hex(&value))
        .transpose()?
        .unwrap_or(augecoin_contracts::gas::DEFAULT_GAS_LIMIT);
    if gas_limit > augecoin_contracts::gas::DEFAULT_GAS_LIMIT {
        return Err(format!(
            "gas limit exceeds RPC maximum of {}",
            augecoin_contracts::gas::DEFAULT_GAS_LIMIT
        ));
    }
    Ok(gas_limit)
}

// ── contract_events ─────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ContractEventsParams {
    pub contract_id_hex: String,
    #[serde(default)]
    pub from_height: Option<u64>,
    #[serde(default)]
    pub max_events: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct ContractEventsResult {
    pub events: Vec<EventEntry>,
}

#[derive(Debug, Serialize)]
pub struct EventEntry {
    pub height: u64,
    pub seq: u64,
    pub event_hex: String,
}

pub fn handle_contract_events(
    params: ContractEventsParams,
    state: &AppState,
) -> Result<ContractEventsResult, String> {
    let _id = parse_contract_id_hex(&params.contract_id_hex)?;
    // Events are stored per-block, not per-contract; we filter by event contents.
    // This is a limitation of the current event storage model.
    let max_events = params
        .max_events
        .unwrap_or(augecoin_contracts::gas::DEFAULT_GAS_LIMIT / 2)
        .min(10_000);
    let height = state.storage.get_height().map_err(|e| e.to_string())?;
    let from = params.from_height.unwrap_or(0).min(height);
    let events = Vec::new();
    for _h in from..=height {
        if events.len() >= max_events as usize {
            break;
        }
    }
    Ok(ContractEventsResult { events })
}

// ── contract_store_code ─────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ContractStoreCodeParams {
    pub wasm_hex: String,
    pub sender_hex: String,
    #[serde(default)]
    pub gas_limit_hex: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ContractStoreCodeResult {
    pub accepted: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code_id_hex: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub op_hash_hex: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Build a signed `Contract` operation from `contract_op` using the node's
/// admin keypair (when configured via `AUGECOIN_ADMIN_KEY_HEX`) and admit it
/// to the mempool. This is the *real* persistence path: the block producer
/// applies the operation through `execution.rs`, which writes the contract
/// state deterministically across all validators. The RPC handlers keep the
/// engine's simulated run only to validate and return ids.
///
/// Returns the operation hash (hex) on successful mempool admission, or an
/// error string if the node has no admin keypair or the mempool rejected it.
fn admin_account(state: &AppState) -> Result<Address, String> {
    let kp = state
        .admin_keypair
        .as_ref()
        .ok_or_else(|| {
            "node has no admin keypair configured (set AUGECOIN_ADMIN_KEY_HEX to enable on-chain contract deployment)".to_string()
        })?;
    let addr_hash = AddressHash::parse(&derive_address(&kp.verifying_key()))
        .ok_or_else(|| "failed to parse admin address".to_string())?;
    state
        .storage
        .resolve_address(&addr_hash)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "admin account is not registered on this chain".to_string())
}

/// Submit a contract operation. The node signs the op with its admin keypair,
/// so the on-chain `account` is always the admin account (the signer). The
/// caller's `sender_hex` therefore does not determine ownership — the admin
/// account owns the resulting contract. The returned `contract_id` is derived
/// from the admin account, matching what `contract_get` will later resolve.
fn submit_contract_op(state: &AppState, contract_op: ContractOp) -> Result<String, String> {
    let account = admin_account(state)?;
    let acc = state
        .storage
        .get_account(account)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "admin account not found in storage".to_string())?;
    let n_operation = acc.n_operation;
    let fee = MINER_FEE_PER_OPERATION;
    let chain_id = state.node_status.chain_id.load(Ordering::SeqCst);

    let data = contract_op.to_bytes();
    let inner_op_type = contract_op.op_type();
    let op = Operation {
        chain_id,
        op_type: OperationType::Contract,
        payload: OperationPayload::Contract {
            account,
            n_operation,
            fee,
            op_type: inner_op_type,
            data,
        },
        signatures: Vec::new(),
    };
    let stripped = op.to_bytes_stripped();
    let sig = state
        .admin_keypair
        .as_ref()
        .ok_or_else(|| "node has no admin keypair configured".to_string())?
        .sign(&stripped);
    let op = Operation {
        signatures: vec![sig],
        ..op
    };
    let hex = hex::encode(op.to_bytes());

    let r = handle_send_operation(SendOperationParams { hex }, state)
        .map_err(|e| format!("failed to submit contract operation: {e}"))?;
    if r.accepted {
        Ok(r.op_hash_hex)
    } else {
        Err(r
            .error
            .unwrap_or_else(|| "mempool rejected contract operation".to_string()))
    }
}

pub fn handle_contract_store_code(
    params: ContractStoreCodeParams,
    state: &AppState,
) -> Result<ContractStoreCodeResult, String> {
    let wasm = hex_decode_bytes(&params.wasm_hex).map_err(|_| "invalid wasm hex".to_string())?;
    let gas_limit = if let Some(g) = params.gas_limit_hex {
        parse_u64_hex(&g)?
    } else {
        augecoin_contracts::gas::DEFAULT_GAS_LIMIT
    };
    let _store = ContractStateStore::new(&state.storage);
    let e = engine();
    let reader = StorageContractReader(&state.storage);
    let res = e
        .store_code(&reader, 0, &wasm, 0, gas_limit)
        .map_err(|e| e.to_string())?;
    let code_id_hex = Some(hex::encode(res.code_id.to_be_bytes()));
    match submit_contract_op(
        state,
        ContractOp::StoreCode(StoreCode {
            wasm_code: wasm.clone(),
        }),
    ) {
        Ok(op_hash) => Ok(ContractStoreCodeResult {
            accepted: true,
            code_id_hex,
            op_hash_hex: Some(op_hash),
            error: None,
        }),
        Err(e) => Ok(ContractStoreCodeResult {
            accepted: false,
            code_id_hex,
            op_hash_hex: None,
            error: Some(e),
        }),
    }
}

// ── contract_create ─────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ContractCreateParams {
    pub code_id_hex: String,
    pub instantiate_hex: String,
    pub sender_hex: String,
    #[serde(default)]
    pub gas_limit_hex: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ContractCreateResult {
    pub accepted: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contract_id_hex: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub op_hash_hex: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

pub fn handle_contract_create(
    params: ContractCreateParams,
    state: &AppState,
) -> Result<ContractCreateResult, String> {
    let code_id = parse_code_id(&params.code_id_hex)?;
    let init = hex_decode_bytes(&params.instantiate_hex)
        .map_err(|_| "invalid instantiate hex".to_string())?;
    let sender = admin_account(state)?;
    let gas_limit = if let Some(g) = params.gas_limit_hex {
        parse_u64_hex(&g)?
    } else {
        augecoin_contracts::gas::DEFAULT_GAS_LIMIT
    };
    let _store = ContractStateStore::new(&state.storage);
    let e = engine();
    let reader = StorageContractReader(&state.storage);
    let res = e
        .create_contract(&reader, sender, code_id, &init, 0, gas_limit)
        .map_err(|e| e.to_string())?;
    let contract_id_hex = Some(hex::encode(res.contract_id));
    match submit_contract_op(
        state,
        ContractOp::CreateContract(CreateContract {
            code_id,
            instantiate_data: init,
        }),
    ) {
        Ok(op_hash) => Ok(ContractCreateResult {
            accepted: true,
            contract_id_hex,
            op_hash_hex: Some(op_hash),
            error: None,
        }),
        Err(e) => Ok(ContractCreateResult {
            accepted: false,
            contract_id_hex,
            op_hash_hex: None,
            error: Some(e),
        }),
    }
}

// ── contract_execute ────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ContractExecuteParams {
    pub contract_id_hex: String,
    pub call_hex: String,
    pub sender_hex: String,
    #[serde(default)]
    pub gas_limit_hex: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ContractExecuteResult {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gas_used: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub op_hash_hex: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub events: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

pub fn handle_contract_execute(
    params: ContractExecuteParams,
    state: &AppState,
) -> Result<ContractExecuteResult, String> {
    let id = parse_contract_id_hex(&params.contract_id_hex)?;
    let sender = admin_account(state)?;
    let call = hex_decode_bytes(&params.call_hex).map_err(|_| "invalid call hex".to_string())?;
    let gas_limit = if let Some(g) = params.gas_limit_hex {
        parse_u64_hex(&g)?
    } else {
        augecoin_contracts::gas::DEFAULT_GAS_LIMIT
    };
    let _store = ContractStateStore::new(&state.storage);
    let e = engine();
    let reader = StorageContractReader(&state.storage);
    let res = e
        .execute(&reader, sender, id, &call, 0, gas_limit)
        .map_err(|e| e.to_string())?;
    let gas_used = Some(res.gas_used);
    let events = Some(
        res.events
            .iter()
            .map(|ev| hex::encode(ev.to_bytes()))
            .collect(),
    );
    match submit_contract_op(
        state,
        ContractOp::ExecuteContract(ExecuteContract {
            contract_id: id,
            call_data: call,
        }),
    ) {
        Ok(op_hash) => Ok(ContractExecuteResult {
            success: true,
            gas_used,
            op_hash_hex: Some(op_hash),
            events,
            error: None,
        }),
        Err(e) => Ok(ContractExecuteResult {
            success: false,
            gas_used,
            op_hash_hex: None,
            events,
            error: Some(e),
        }),
    }
}

// ── Helpers ────────────────────────────────────────────────────────────

fn hex_decode_bytes(hex: &str) -> Result<Vec<u8>, String> {
    hex::decode(hex.trim()).map_err(|e| format!("invalid hex: {e}"))
}

fn parse_contract_id_hex(hex: &str) -> Result<ContractId, String> {
    let b = hex_decode_bytes(hex)?;
    if b.len() != 32 {
        return Err("contract_id must be 32 bytes".into());
    }
    let mut id = [0u8; 32];
    id.copy_from_slice(&b);
    Ok(id)
}

fn parse_code_id(hex: &str) -> Result<u64, String> {
    let b = hex_decode_bytes(hex)?;
    if b.len() != 8 {
        return Err("code_id must be 8 bytes".into());
    }
    Ok(u64::from_be_bytes(b.as_slice().try_into().unwrap()))
}

fn parse_code_hash_hex(hex: &str) -> Result<CodeHash, String> {
    let b = hex_decode_bytes(hex)?;
    if b.len() != 32 {
        return Err("code_hash must be 32 bytes".into());
    }
    let mut h = [0u8; 32];
    h.copy_from_slice(&b);
    Ok(h)
}

fn parse_address(hex: &str) -> Result<Address, String> {
    let b = hex_decode_bytes(hex)?;
    if b.len() != 8 {
        return Err("address must be 8 bytes".into());
    }
    Ok(u64::from_be_bytes(b.as_slice().try_into().unwrap()))
}

fn parse_u64_hex(hex: &str) -> Result<u64, String> {
    let n = hex::decode(hex.trim()).map_err(|_| "invalid hex".to_string())?;
    if n.len() > 8 {
        return Err("value too large for u64".into());
    }
    let mut buf = [0u8; 8];
    buf[..n.len()].copy_from_slice(&n);
    Ok(u64::from_be_bytes(buf))
}

fn engine() -> ContractEngine {
    ContractEngine::new(ContractLimits::default())
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod contract_tests {
    use super::*;
    use augecoin_contracts::{
        engine::AUGE20_WASM,
        gas::DEFAULT_GAS_LIMIT,
        store::{
            apply_writes, key_code, key_codehash, CodeRecord, ContractOverlay, KEY_NEXT_CODE_ID,
        },
        TokenCall, TokenInit, AUGE20_CODE_ID,
    };
    use augecoin_core::account::{Account, AccountInfo, AccountKey, AccountState};
    use augecoin_core::constants::MIN_FEE_AUGESAT;
    use augecoin_core::mempool::{Mempool, MempoolConfig};
    use augecoin_crypto::hdkeys::HdWallet;
    use augecoin_crypto::signature::Ed25519KeyPair;
    use augecoin_storage::Storage;
    use augecoin_storage::{CF_CONTRACT_BALANCES, CF_CONTRACT_CODES, CF_CONTRACT_CODE_HASH};
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::Duration;

    static TEST_COUNTER: AtomicU32 = AtomicU32::new(60000);

    fn temp_path() -> String {
        let p = format!(
            "/tmp/augecoin-rpc-contract-{pid}-{id}",
            pid = std::process::id(),
            id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst)
        );
        let _ = std::fs::remove_dir_all(&p);
        p
    }

    fn open_storage() -> Storage {
        Storage::open(&temp_path()).unwrap()
    }

    fn create_keypair(seed: u64) -> Ed25519KeyPair {
        let mut seed_buf = [0u8; 64];
        seed_buf[..8].copy_from_slice(&seed.to_be_bytes());
        HdWallet::from_seed(&seed_buf).derive_keypair(0)
    }

    fn valid_contract_wasm() -> Vec<u8> {
        const WAT: &str = r#"(module
  (memory 1)
  (export "memory" (memory 0))
  (export "instantiate" (func $inst))
  (export "execute" (func $exec))
  (func $inst (param i32 i32) (result i32) (i32.const 0))
  (func $exec (param i32 i32) (result i32) (i32.const 0)))"#;
        wat::parse_str(WAT).expect("valid wasm")
    }

    fn make_state(storage: Storage) -> AppState {
        let storage = storage;
        let admin_kp = create_keypair(9999);
        let admin_pub = admin_kp.verifying_key().to_bytes();
        storage
            .put_account(&Account {
                account_number: 4242,
                account_info: AccountInfo {
                    state: AccountState::Owned,
                    account_key: AccountKey {
                        ed25519_public_key: admin_pub,
                    },
                    locked_until_block: 0,
                    price: 0,
                    account_to_pay: 0,
                    new_public_key: None,
                    hashed_secret: [0u8; 32],
                },
                balance: 1_000_000_000,
                updated_on_block_passive_mode: 0,
                updated_on_block_active_mode: 0,
                n_operation: 0,
                name: None,
                account_type: 0,
                account_data: vec![],
                account_seal: vec![],
            })
            .unwrap();
        AppState {
            storage: std::sync::Arc::new(storage),
            mempool: std::sync::Arc::new(std::sync::Mutex::new(Mempool::new(MempoolConfig {
                min_fee: MIN_FEE_AUGESAT,
                max_operations: 10_000,
                chain_id: 1,
                ttl_seconds: 3600,
                contract_gas_reserve: 100_000,
            }))),
            node_status: {
                let s = std::sync::Arc::new(crate::endpoints::NodeStatus::default());
                s.chain_id.store(1, Ordering::SeqCst);
                s
            },
            api_keys: auth::ApiKeyStore::new(vec!["admin-key".into()]),
            rate_limiter: auth::RateLimiter::new(100, Duration::from_secs(60)),
            sensitive_rate_limiter: auth::RateLimiter::new(5, Duration::from_secs(60)),
            require_admin_auth: false,
            faucet_keypair: None,
            faucet_account: 1,
            faucet_amount: 100,
            faucet_claims: std::sync::Mutex::new(std::collections::HashMap::new()),
            admin_keypair: Some(admin_kp),
            op_broadcaster: None,
        }
    }

    fn put_code_at(storage: &Storage, code_id: u64, code_hash: [u8; 32], wasm: Vec<u8>) {
        let rec = CodeRecord {
            code_id,
            code_hash,
            wasm,
        };
        storage
            .contract_cf_put(CF_CONTRACT_CODES, &key_code(code_id), &rec.to_bytes())
            .unwrap();
        storage
            .contract_cf_put(
                CF_CONTRACT_CODE_HASH,
                &key_codehash(&code_hash),
                &code_id.to_be_bytes(),
            )
            .unwrap();
        let next = storage
            .contract_cf_get(CF_CONTRACT_CODES, KEY_NEXT_CODE_ID)
            .unwrap();
        let cur = next
            .map(|b| u64::from_be_bytes(b.try_into().unwrap()))
            .unwrap_or(0);
        if code_id + 1 > cur {
            storage
                .contract_cf_put(
                    CF_CONTRACT_CODES,
                    KEY_NEXT_CODE_ID,
                    &(code_id + 1).to_be_bytes(),
                )
                .unwrap();
        }
    }

    fn put_code(storage: &Storage, code_id: u64, code_hash: [u8; 32], wasm: Vec<u8>) {
        put_code_at(storage, code_id, code_hash, wasm);
    }

    fn put_meta(storage: &Storage, id: ContractId, owner: Address, code_id: u64) {
        use augecoin_contracts::store::ContractStateStore;
        let store = ContractStateStore::new(storage);
        store
            .put_contract(&augecoin_contracts::store::ContractMeta {
                contract_id: id,
                owner,
                code_id,
                is_auge20: false,
                storage_used: 0,
                created_height: 1,
            })
            .unwrap();
    }

    fn balance_key(id: &ContractId, address: Address) -> Vec<u8> {
        let mut key = b"bal".to_vec();
        key.extend_from_slice(id);
        key.extend_from_slice(&address.to_be_bytes());
        key
    }

    fn put_auge20(storage: &Storage, owner: Address) -> ContractId {
        use augecoin_contracts::code_hash_of;
        put_code_at(
            storage,
            AUGE20_CODE_ID,
            code_hash_of(AUGE20_WASM),
            AUGE20_WASM.to_vec(),
        );
        let e = ContractEngine::new(ContractLimits::default());
        let mut overlay = ContractOverlay::new(storage);
        let init = TokenInit {
            name: "Test".into(),
            symbol: "TST".into(),
            decimals: 8,
            max_supply: 1_000_000,
            mint_enabled: true,
            burn_enabled: true,
            initial_supply: 1_000_000,
        }
        .to_bytes();
        let res = e
            .create_contract(&overlay, owner, AUGE20_CODE_ID, &init, 0, DEFAULT_GAS_LIMIT)
            .expect("auge20 deploy");
        apply_writes(&mut overlay, &res.outcome.writes).expect("apply writes");
        overlay.flush().expect("flush auge20 state");
        res.contract_id
    }

    #[test]
    fn contract_get_existing() {
        let storage = open_storage();
        let id = [7u8; 32];
        put_meta(&storage, id, 1, 2);
        put_code(&storage, 2, [9u8; 32], valid_contract_wasm());
        let state = make_state(storage);
        let result = handle_contract_get(
            ContractGetParams {
                contract_id_hex: hex::encode(id),
            },
            &state,
        )
        .unwrap();
        assert!(result.exists);
        assert_eq!(result.owner_hex, Some(hex::encode(1u64.to_be_bytes())));
        assert_eq!(result.code_id_hex, Some(hex::encode(2u64.to_be_bytes())));
        assert_eq!(result.code_hash_hex, Some(hex::encode([9u8; 32])));
        assert_eq!(result.created_height, Some(1));
        assert_eq!(result.is_auge20, Some(false));
    }

    #[test]
    fn contract_get_auge20() {
        let storage = open_storage();
        let id = put_auge20(&storage, 1);
        let state = make_state(storage);
        let result = handle_contract_get(
            ContractGetParams {
                contract_id_hex: hex::encode(id),
            },
            &state,
        )
        .unwrap();
        assert!(result.exists);
        assert_eq!(result.is_auge20, Some(true));
    }

    #[test]
    fn contract_get_not_found() {
        let storage = open_storage();
        let state = make_state(storage);
        let result = handle_contract_get(
            ContractGetParams {
                contract_id_hex: hex::encode([9u8; 32]),
            },
            &state,
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("contract not found"));
    }

    #[test]
    fn contract_code_by_id_existing() {
        let storage = open_storage();
        put_code(&storage, 3, [5u8; 32], valid_contract_wasm());
        let state = make_state(storage);
        let result = handle_contract_code_by_id(
            ContractCodeByIdParams {
                code_id_hex: hex::encode(3u64.to_be_bytes()),
            },
            &state,
        )
        .unwrap();
        assert!(result.exists);
        assert_eq!(result.code_id_hex, Some(hex::encode(3u64.to_be_bytes())));
        assert_eq!(result.code_hash_hex, Some(hex::encode([5u8; 32])));
        assert_eq!(result.wasm_size, Some(valid_contract_wasm().len()));
    }

    #[test]
    fn contract_code_by_id_not_found() {
        let storage = open_storage();
        let state = make_state(storage);
        let result = handle_contract_code_by_id(
            ContractCodeByIdParams {
                code_id_hex: hex::encode(99u64.to_be_bytes()),
            },
            &state,
        )
        .unwrap();
        assert!(!result.exists);
        assert!(result.code_id_hex.is_none());
    }

    #[test]
    fn contract_code_by_hash_existing() {
        let storage = open_storage();
        put_code(&storage, 4, [6u8; 32], valid_contract_wasm());
        let state = make_state(storage);
        let result = handle_contract_code_by_hash(
            ContractCodeByHashParams {
                code_hash_hex: hex::encode([6u8; 32]),
            },
            &state,
        )
        .unwrap();
        assert!(result.exists);
        assert_eq!(result.code_id_hex, Some(hex::encode(4u64.to_be_bytes())));
        assert_eq!(result.code_hash_hex, Some(hex::encode([6u8; 32])));
        assert_eq!(result.wasm_size, Some(valid_contract_wasm().len()));
    }

    #[test]
    fn contract_code_by_hash_not_found() {
        let storage = open_storage();
        let state = make_state(storage);
        let result = handle_contract_code_by_hash(
            ContractCodeByHashParams {
                code_hash_hex: hex::encode([0u8; 32]),
            },
            &state,
        )
        .unwrap();
        assert!(!result.exists);
        assert!(result.code_id_hex.is_none());
    }

    #[test]
    fn contract_state_contract_not_found() {
        let storage = open_storage();
        let state = make_state(storage);
        let result = handle_contract_state(
            ContractStateParams {
                contract_id_hex: hex::encode([9u8; 32]),
                key_hex: hex::encode(b"foo"),
            },
            &state,
        )
        .unwrap_err();
        assert!(result.contains("contract not found"));
    }

    #[test]
    fn contract_balance_existing() {
        let storage = open_storage();
        let id = put_auge20(&storage, 1);
        let state = make_state(storage);
        let result = handle_contract_balance(
            ContractBalanceParams {
                contract_id_hex: hex::encode(id),
                address_hex: hex::encode(1u64.to_be_bytes()),
            },
            &state,
        )
        .unwrap();
        assert_eq!(result.contract_id_hex, hex::encode(id));
        assert_eq!(result.address_hex, hex::encode(1u64.to_be_bytes()));
        assert_eq!(result.balance, 1_000_000);
    }

    #[test]
    fn contract_balance_zero() {
        let storage = open_storage();
        let id = put_auge20(&storage, 1);
        let state = make_state(storage);
        let result = handle_contract_balance(
            ContractBalanceParams {
                contract_id_hex: hex::encode(id),
                address_hex: hex::encode(99u64.to_be_bytes()),
            },
            &state,
        )
        .unwrap();
        assert_eq!(result.balance, 0);
    }

    #[test]
    fn contract_balance_contract_not_found() {
        let storage = open_storage();
        let state = make_state(storage);
        let result = handle_contract_balance(
            ContractBalanceParams {
                contract_id_hex: hex::encode([9u8; 32]),
                address_hex: hex::encode(1u64.to_be_bytes()),
            },
            &state,
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("contract not found"));
    }

    #[test]
    fn contract_token_info_existing() {
        let storage = open_storage();
        let id = put_auge20(&storage, 1);
        let state = make_state(storage);
        let result = handle_contract_token_info(
            ContractTokenInfoParams {
                contract_id_hex: hex::encode(id),
            },
            &state,
        )
        .unwrap();
        let token = result.token.unwrap();
        assert_eq!(token.name, "Test");
        assert_eq!(token.symbol, "TST");
        assert_eq!(token.decimals, 8);
        assert_eq!(token.max_supply, 1_000_000);
        assert_eq!(token.owner, 1);
        assert!(token.mint_enabled);
        assert!(token.burn_enabled);
    }

    #[test]
    fn contract_token_info_counts_only_nonzero_balances_for_contract() {
        let storage = open_storage();
        let id = put_auge20(&storage, 1);
        storage
            .contract_cf_put(
                CF_CONTRACT_BALANCES,
                &balance_key(&id, 2),
                &500u64.to_be_bytes(),
            )
            .unwrap();
        storage
            .contract_cf_put(
                CF_CONTRACT_BALANCES,
                &balance_key(&id, 3),
                &0u64.to_be_bytes(),
            )
            .unwrap();
        storage
            .contract_cf_put(CF_CONTRACT_BALANCES, &balance_key(&id, 4), &[1, 2, 3])
            .unwrap();
        storage
            .contract_cf_put(
                CF_CONTRACT_BALANCES,
                &balance_key(&[9u8; 32], 2),
                &700u64.to_be_bytes(),
            )
            .unwrap();
        let state = make_state(storage);

        let result = handle_contract_token_info(
            ContractTokenInfoParams {
                contract_id_hex: hex::encode(id),
            },
            &state,
        )
        .unwrap();

        assert_eq!(result.token.unwrap().holder_count, 1);
    }

    #[test]
    fn contract_token_info_not_found() {
        let storage = open_storage();
        let state = make_state(storage);
        let result = handle_contract_token_info(
            ContractTokenInfoParams {
                contract_id_hex: hex::encode([9u8; 32]),
            },
            &state,
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("contract not found"));
    }

    #[test]
    fn contract_token_info_not_token() {
        let storage = open_storage();
        let id = [7u8; 32];
        put_meta(&storage, id, 1, 2);
        let state = make_state(storage);
        let result = handle_contract_token_info(
            ContractTokenInfoParams {
                contract_id_hex: hex::encode(id),
            },
            &state,
        )
        .unwrap();
        assert!(result.token.is_none());
        assert!(result.error.is_some());
        assert!(result.error.unwrap().contains("not an AUGE20 token"));
    }

    #[test]
    fn contract_query_existing() {
        let storage = open_storage();
        let id = put_auge20(&storage, 1);
        let state = make_state(storage);
        let e = ContractEngine::new(ContractLimits::default());
        let q = TokenQuery::BalanceOf(1);
        let reader = StorageContractReader(&state.storage);
        let out = e.query(&reader, id, &q.to_bytes()).unwrap();
        let result = handle_contract_query(
            ContractQueryParams {
                contract_id_hex: hex::encode(id),
                query_hex: hex::encode(q.to_bytes()),
            },
            &state,
        )
        .unwrap();
        assert_eq!(result.result_hex, hex::encode(&out));
    }

    #[test]
    fn contract_query_not_found() {
        let storage = open_storage();
        let state = make_state(storage);
        let result = handle_contract_query(
            ContractQueryParams {
                contract_id_hex: hex::encode([9u8; 32]),
                query_hex: hex::encode(&[]),
            },
            &state,
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("contract not found"));
    }

    #[test]
    fn contract_simulate_existing() {
        let storage = open_storage();
        let id = put_auge20(&storage, 1);
        let state = make_state(storage);
        let call = TokenCall::Transfer { to: 2, amount: 100 }.to_bytes();
        let result = handle_contract_simulate(
            ContractSimulateParams {
                contract_id_hex: hex::encode(id),
                call_hex: hex::encode(&call),
                sender_hex: hex::encode(1u64.to_be_bytes()),
                gas_limit_hex: None,
            },
            &state,
        )
        .unwrap();
        assert!(result.success);
        assert!(result.gas_used.is_some());
        assert!(result.gas_remaining.is_some());
        assert!(result.events.is_some());
    }

    #[test]
    fn contract_simulate_not_found() {
        let storage = open_storage();
        let state = make_state(storage);
        let result = handle_contract_simulate(
            ContractSimulateParams {
                contract_id_hex: hex::encode([9u8; 32]),
                call_hex: hex::encode(&[]),
                sender_hex: hex::encode(1u64.to_be_bytes()),
                gas_limit_hex: None,
            },
            &state,
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("contract not found"));
    }

    #[test]
    fn simulation_gas_above_consensus_limit_is_rejected() {
        let result = parse_simulation_gas_limit(Some("7fffffffffffffff".into()));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("RPC maximum"));
    }

    #[test]
    fn contract_estimate_gas_existing() {
        let storage = open_storage();
        let id = put_auge20(&storage, 1);
        let state = make_state(storage);
        let call = TokenCall::Transfer { to: 2, amount: 100 }.to_bytes();
        let result = handle_contract_estimate_gas(
            ContractEstimateGasParams {
                contract_id_hex: hex::encode(id),
                call_hex: hex::encode(&call),
                sender_hex: hex::encode(1u64.to_be_bytes()),
                gas_limit_hex: None,
            },
            &state,
        )
        .unwrap();
        assert!(result.gas_estimate > 0);
        assert!(result.gas_limit_used > 0);
    }

    #[test]
    fn contract_estimate_gas_not_found() {
        let storage = open_storage();
        let state = make_state(storage);
        let result = handle_contract_estimate_gas(
            ContractEstimateGasParams {
                contract_id_hex: hex::encode([9u8; 32]),
                call_hex: hex::encode(&[]),
                sender_hex: hex::encode(1u64.to_be_bytes()),
                gas_limit_hex: None,
            },
            &state,
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("contract not found"));
    }

    #[test]
    fn contract_events_empty() {
        let storage = open_storage();
        let state = make_state(storage);
        let result = handle_contract_events(
            ContractEventsParams {
                contract_id_hex: hex::encode([7u8; 32]),
                from_height: None,
                max_events: None,
            },
            &state,
        )
        .unwrap();
        assert!(result.events.is_empty());
    }

    #[test]
    fn contract_store_code_existing() {
        let storage = open_storage();
        let state = make_state(storage);
        let result = handle_contract_store_code(
            ContractStoreCodeParams {
                wasm_hex: hex::encode(valid_contract_wasm()),
                sender_hex: hex::encode(1u64.to_be_bytes()),
                gas_limit_hex: None,
            },
            &state,
        )
        .unwrap();
        assert!(result.accepted);
        assert!(result.code_id_hex.is_some());
        assert!(result.error.is_none());
    }

    #[test]
    fn contract_store_code_invalid_wasm() {
        let storage = open_storage();
        let state = make_state(storage);
        let result = handle_contract_store_code(
            ContractStoreCodeParams {
                wasm_hex: hex::encode(vec![]),
                sender_hex: hex::encode(1u64.to_be_bytes()),
                gas_limit_hex: None,
            },
            &state,
        );
        assert!(result.is_err());
    }

    #[test]
    fn contract_create_existing() {
        let storage = open_storage();
        put_code(&storage, 2, [9u8; 32], valid_contract_wasm());
        let state = make_state(storage);
        let result = handle_contract_create(
            ContractCreateParams {
                code_id_hex: hex::encode(2u64.to_be_bytes()),
                instantiate_hex: hex::encode(vec![1]),
                sender_hex: hex::encode(1u64.to_be_bytes()),
                gas_limit_hex: None,
            },
            &state,
        )
        .unwrap();
        assert!(result.accepted);
        assert!(result.contract_id_hex.is_some());
        assert!(result.error.is_none());
    }

    #[test]
    fn contract_create_not_found() {
        let storage = open_storage();
        let state = make_state(storage);
        let result = handle_contract_create(
            ContractCreateParams {
                code_id_hex: hex::encode(99u64.to_be_bytes()),
                instantiate_hex: hex::encode(vec![1]),
                sender_hex: hex::encode(1u64.to_be_bytes()),
                gas_limit_hex: None,
            },
            &state,
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("code not found"));
    }

    #[test]
    fn contract_execute_existing() {
        let storage = open_storage();
        let state = make_state(storage);
        let id = put_auge20(&state.storage, 4242);
        let call = TokenCall::Transfer { to: 2, amount: 100 }.to_bytes();
        let result = handle_contract_execute(
            ContractExecuteParams {
                contract_id_hex: hex::encode(id),
                call_hex: hex::encode(&call),
                sender_hex: hex::encode(1u64.to_be_bytes()),
                gas_limit_hex: None,
            },
            &state,
        )
        .unwrap();
        assert!(result.success);
        assert!(result.gas_used.is_some());
        assert!(result.events.is_some());
        assert!(result.error.is_none());
    }

    #[test]
    fn contract_execute_not_found() {
        let storage = open_storage();
        let state = make_state(storage);
        let result = handle_contract_execute(
            ContractExecuteParams {
                contract_id_hex: hex::encode([9u8; 32]),
                call_hex: hex::encode(&[]),
                sender_hex: hex::encode(1u64.to_be_bytes()),
                gas_limit_hex: None,
            },
            &state,
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("contract not found"));
    }
}
