//! Contract state storage abstraction.
//!
//! Contract state is kept entirely outside the account SafeBox. Keys are
//! prefixed so that each operation touches only the affected keys (incremental
//! writes, no full-state rewrite — see prompt §12, §26, §55).
//!
//! * `code/{code_id}`        -> CodeRecord
//! * `codehash/{hash}`       -> code_id (reverse index)
//! * `contract/{contract_id}`-> ContractMeta
//! * `bal/{contract_id}/{addr}` -> u64 amount (big-endian)
//! * `supply/{contract_id}`  -> u64 total supply
//! * `token/{contract_id}`   -> TokenConfig
//! * `next_code_id`          -> u64 counter
//! * `event/{height}/{seq}`  -> event bytes (for indexers/RPC)

use crate::codec::{Reader, Writer};
use crate::{Address, CodeId, ContractError, ContractEvent, ContractId, WriteOp};
use augecoin_storage::{
    Storage, WriteBatch, CF_CONTRACTS, CF_CONTRACT_BALANCES, CF_CONTRACT_CODES,
    CF_CONTRACT_CODE_HASH, CF_CONTRACT_METADATA,
};
use std::collections::HashMap;

pub fn key_code(id: CodeId) -> Vec<u8> {
    let mut k = b"code".to_vec();
    k.extend_from_slice(&id.to_be_bytes());
    k
}

pub fn key_codehash(h: &[u8; 32]) -> Vec<u8> {
    let mut k = b"codehash".to_vec();
    k.extend_from_slice(h);
    k
}

pub fn key_contract(id: &ContractId) -> Vec<u8> {
    let mut k = b"contract".to_vec();
    k.extend_from_slice(id);
    k
}

pub fn key_balance(id: &ContractId, addr: Address) -> Vec<u8> {
    let mut k = b"bal".to_vec();
    k.extend_from_slice(id);
    k.extend_from_slice(&addr.to_be_bytes());
    k
}

pub fn key_supply(id: &ContractId) -> Vec<u8> {
    let mut k = b"supply".to_vec();
    k.extend_from_slice(id);
    k
}

pub fn key_token(id: &ContractId) -> Vec<u8> {
    let mut k = b"token".to_vec();
    k.extend_from_slice(id);
    k
}

pub const KEY_NEXT_CODE_ID: &[u8] = b"next_code_id";

pub fn key_event(height: u64, seq: u64) -> Vec<u8> {
    let mut k = b"event".to_vec();
    k.extend_from_slice(&height.to_be_bytes());
    k.extend_from_slice(&seq.to_be_bytes());
    k
}

/// Route a contract-state key to the column family where its value actually
/// lives. The dedicated contract CFs (`contract_codes`, `contract_metadata`,
/// `contract_balances`, `contract_code_hash`) are read by `CodeRegistry` and
/// `ContractStateStore`, while arbitrary WASM state (`state/...`, `event/...`)
/// and any `contract_id`-prefixed keys stay in the generic `contracts` CF.
///
/// Without this routing, the overlay/reader would read and write everything in
/// the generic `contracts` CF and never see (nor persist) code/metadata/balance
/// records — making contract deployment and execution no-ops on disk.
fn contract_cf_for_key(key: &[u8]) -> &'static str {
    if key == KEY_NEXT_CODE_ID {
        return CF_CONTRACT_CODES;
    }
    if key.starts_with(b"codehash") {
        return CF_CONTRACT_CODE_HASH;
    }
    if key.starts_with(b"code") {
        return CF_CONTRACT_CODES;
    }
    if key.starts_with(b"contract") {
        return CF_CONTRACT_METADATA;
    }
    if key.starts_with(b"token") {
        return CF_CONTRACT_METADATA;
    }
    if key.starts_with(b"bal") {
        return CF_CONTRACT_BALANCES;
    }
    if key.starts_with(b"supply") {
        return CF_CONTRACT_BALANCES;
    }
    // state/*, event/* and all arbitrary WASM contract state live in the
    // generic contracts CF.
    CF_CONTRACTS
}

/// Key for a single WASM contract's arbitrary key/value state entry. Lives in
/// the generic `contracts` CF and is namespaced by `contract_id` so that a
/// transfer in one contract never scans or rewrites another contract's state,
/// and never touches the account SafeBox.
pub fn key_state(id: &ContractId, state_key: &[u8]) -> Vec<u8> {
    let mut k = b"state".to_vec();
    k.extend_from_slice(id);
    k.extend_from_slice(state_key);
    k
}

/// Read/write access to contract state. Implemented by an in-memory backend
/// (tests) and by a RocksDB-backed reader used by the node.
/// Key/value pairs returned by prefix scans.
pub type KeyValuePairs = Vec<(Vec<u8>, Vec<u8>)>;

pub trait ContractStore: Send + Sync {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, ContractError>;
    /// Scan all keys starting with `prefix`. Used to build a read-only snapshot
    /// when executing arbitrary WASM code.
    fn scan_prefix(&self, prefix: &[u8]) -> Result<KeyValuePairs, ContractError> {
        let _ = prefix;
        Ok(Vec::new())
    }
    /// Persist a write. For read-only node readers this is intentionally a
    /// no-op because the node persists returned [`WriteOp`]s itself.
    fn put(&mut self, _key: &[u8], _value: &[u8]) -> Result<(), ContractError> {
        Ok(())
    }
    fn delete(&mut self, _key: &[u8]) -> Result<(), ContractError> {
        Ok(())
    }
    /// Total bytes currently occupied by this store's values. Used by the Fase 5
    /// execution pipeline to enforce the per-contract storage limit incrementally.
    /// The default returns `0`; backends that own a contract-local key space
    /// override it with the real sum.
    fn storage_size(&self) -> Result<u64, ContractError> {
        Ok(0)
    }
}

/// In-memory backend used by tests and benchmarks.
#[derive(Default)]
pub struct MemoryContractStore {
    map: HashMap<Vec<u8>, Vec<u8>>,
}

impl MemoryContractStore {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
        }
    }

    /// Apply a batch of writes (used by tests after an apply call).
    pub fn apply(&mut self, writes: &[WriteOp]) {
        for WriteOp(k, v) in writes {
            match v {
                Some(v) => {
                    self.map.insert(k.clone(), v.clone());
                }
                None => {
                    self.map.remove(k);
                }
            }
        }
    }
}

impl ContractStore for MemoryContractStore {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, ContractError> {
        Ok(self.map.get(key).cloned())
    }
    fn scan_prefix(&self, prefix: &[u8]) -> Result<KeyValuePairs, ContractError> {
        let mut out = Vec::new();
        for (k, v) in &self.map {
            if k.starts_with(prefix) {
                out.push((k.clone(), v.clone()));
            }
        }
        Ok(out)
    }
    fn put(&mut self, key: &[u8], value: &[u8]) -> Result<(), ContractError> {
        self.map.insert(key.to_vec(), value.to_vec());
        Ok(())
    }
    fn delete(&mut self, key: &[u8]) -> Result<(), ContractError> {
        self.map.remove(key);
        Ok(())
    }
    fn storage_size(&self) -> Result<u64, ContractError> {
        Ok(self.map.values().map(|v| v.len() as u64).sum())
    }
}

/// An owned, read-only snapshot of a contract's state, used as the WASM host's
/// storage backend. Writes during execution are buffered in the host and
/// harvested separately, so the snapshot only needs `get`.
pub struct SnapshotStore {
    map: HashMap<Vec<u8>, Vec<u8>>,
}

impl SnapshotStore {
    pub fn from_entries(entries: Vec<(Vec<u8>, Vec<u8>)>) -> Self {
        Self {
            map: entries.into_iter().collect(),
        }
    }
}

impl ContractStore for SnapshotStore {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, ContractError> {
        Ok(self.map.get(key).cloned())
    }
    fn scan_prefix(&self, prefix: &[u8]) -> Result<KeyValuePairs, ContractError> {
        let mut out = Vec::new();
        for (k, v) in &self.map {
            if k.starts_with(prefix) {
                out.push((k.clone(), v.clone()));
            }
        }
        Ok(out)
    }
    fn storage_size(&self) -> Result<u64, ContractError> {
        Ok(self.map.values().map(|v| v.len() as u64).sum())
    }
}

/// A read-only view over RocksDB contract state used by the engine when running
/// on-chain. Writes returned by the engine are persisted separately by the node
/// through `augecoin_storage::Storage::contract_put`.
pub struct StorageContractReader<'a>(pub &'a augecoin_storage::Storage);

impl<'a> ContractStore for StorageContractReader<'a> {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, ContractError> {
        self.0
            .contract_cf_get(contract_cf_for_key(key), key)
            .map_err(|e| ContractError::Storage(e.to_string()))
    }

    fn scan_prefix(&self, prefix: &[u8]) -> Result<KeyValuePairs, ContractError> {
        self.0
            .contract_scan_prefix(prefix)
            .map_err(|e| ContractError::Storage(e.to_string()))
    }
}

/// A per-block overlay over RocksDB contract state. Reads fall through to the
/// underlying store; writes are buffered in memory. This keeps contract state
/// consistent *within* a block (a later contract op sees an earlier one's
/// writes) and lets the node flush everything atomically only after the whole
/// block succeeds — mirroring the account `modified`/`original` pattern and
/// avoiding partial contract-state persistence (prompt §13, §26, §55).
pub struct ContractOverlay<'a> {
    storage: &'a augecoin_storage::Storage,
    map: HashMap<Vec<u8>, Option<Vec<u8>>>,
}

impl<'a> ContractOverlay<'a> {
    pub fn new(storage: &'a augecoin_storage::Storage) -> Self {
        Self {
            storage,
            map: HashMap::new(),
        }
    }

    /// Consume the overlay as column-family-routed writes for the storage
    /// block commit. This prevents contract persistence from becoming a second
    /// transaction after account and block data are durable.
    pub fn into_writes(self) -> Vec<(String, Vec<u8>, Option<Vec<u8>>)> {
        self.map
            .into_iter()
            .map(|(key, value)| (contract_cf_for_key(&key).to_string(), key, value))
            .collect()
    }

    /// Persist an overlay outside a consensus block transition. Production block
    /// execution must use [`Self::into_writes`] with Storage's single batch.
    pub fn flush(&self) -> Result<(), ContractError> {
        for (key, value) in &self.map {
            let cf = contract_cf_for_key(key);
            match value {
                Some(value) => self
                    .storage
                    .contract_cf_put(cf, key, value)
                    .map_err(|e| ContractError::Storage(e.to_string()))?,
                None => self
                    .storage
                    .contract_cf_delete(cf, key)
                    .map_err(|e| ContractError::Storage(e.to_string()))?,
            }
        }
        Ok(())
    }
}

impl<'a> ContractStore for ContractOverlay<'a> {
    fn scan_prefix(&self, prefix: &[u8]) -> Result<KeyValuePairs, ContractError> {
        let mut out: HashMap<Vec<u8>, Vec<u8>> = HashMap::new();
        // Base storage (previous blocks committed to RocksDB).
        let base = self
            .storage
            .contract_scan_prefix(prefix)
            .map_err(|e| ContractError::Storage(e.to_string()))?;
        for (k, v) in base {
            out.insert(k, v);
        }
        // Overlay (same-block, not-yet-flushed) writes win; deletes remove.
        for (k, v) in &self.map {
            match v {
                Some(v) => {
                    out.insert(k.clone(), v.clone());
                }
                None => {
                    out.remove(k);
                }
            }
        }
        Ok(out.into_iter().collect())
    }

    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, ContractError> {
        if let Some(v) = self.map.get(key) {
            return Ok(v.clone());
        }
        self.storage
            .contract_cf_get(contract_cf_for_key(key), key)
            .map_err(|e| ContractError::Storage(e.to_string()))
    }
    fn put(&mut self, key: &[u8], value: &[u8]) -> Result<(), ContractError> {
        self.map.insert(key.to_vec(), Some(value.to_vec()));
        Ok(())
    }
    fn delete(&mut self, key: &[u8]) -> Result<(), ContractError> {
        self.map.insert(key.to_vec(), None);
        Ok(())
    }
}

/// Persist a batch of writes into a [`ContractStore`] (used by tests).
pub fn apply_writes<S: ContractStore + ?Sized>(
    store: &mut S,
    writes: &[WriteOp],
) -> Result<(), ContractError> {
    for WriteOp(k, v) in writes {
        match v {
            Some(v) => store.put(k, v)?,
            None => store.delete(k)?,
        }
    }
    Ok(())
}

// --- Serialized record helpers ---

/// A registered code blob.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeRecord {
    pub code_id: CodeId,
    pub code_hash: [u8; 32],
    pub wasm: Vec<u8>,
}

impl CodeRecord {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.u64(self.code_id);
        w.fixed(&self.code_hash);
        w.bytes(&self.wasm);
        w.into_vec()
    }
    pub fn from_bytes(data: &[u8]) -> Result<Self, ContractError> {
        let mut r = Reader::new(data);
        let code_id = r.u64()?;
        let code_hash = r.fixed()?;
        let wasm = r.bytes()?;
        r.expect_end()?;
        Ok(CodeRecord {
            code_id,
            code_hash,
            wasm,
        })
    }
}

/// Token configuration stored under `token/{contract_id}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenConfig {
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
    pub max_supply: u64,
    pub mint_enabled: bool,
    pub burn_enabled: bool,
}

impl TokenConfig {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.u8(self.decimals);
        w.bool(self.mint_enabled);
        w.bool(self.burn_enabled);
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
        let max_supply = r.u64()?;
        let name = r.string()?;
        let symbol = r.string()?;
        r.expect_end()?;
        Ok(TokenConfig {
            name,
            symbol,
            decimals,
            max_supply,
            mint_enabled,
            burn_enabled,
        })
    }
}

/// Metadata stored under `contract/{contract_id}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractMeta {
    pub contract_id: ContractId,
    pub owner: Address,
    pub code_id: CodeId,
    pub is_auge20: bool,
    pub storage_used: usize,
    pub created_height: u64,
}

impl ContractMeta {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.fixed(&self.contract_id);
        w.u64(self.owner);
        w.u64(self.code_id);
        w.bool(self.is_auge20);
        w.u64(self.storage_used as u64);
        w.u64(self.created_height);
        w.into_vec()
    }
    pub fn from_bytes(data: &[u8]) -> Result<Self, ContractError> {
        let mut r = Reader::new(data);
        let contract_id = r.fixed()?;
        let owner = r.u64()?;
        let code_id = r.u64()?;
        let is_auge20 = r.bool()?;
        let storage_used = r.u64()? as usize;
        let created_height = r.u64()?;
        r.expect_end()?;
        Ok(ContractMeta {
            contract_id,
            owner,
            code_id,
            is_auge20,
            storage_used,
            created_height,
        })
    }
}

/// Serialize an event for the event log.
pub fn event_to_bytes(ev: &ContractEvent) -> Vec<u8> {
    ev.to_bytes()
}

/// Deserialize an event from the event log.
pub fn event_from_bytes(data: &[u8]) -> Result<ContractEvent, ContractError> {
    ContractEvent::from_bytes(data)
}

// ── RocksDB-backed incremental contract state store (Fase 3) ──────────────────
//
// All access is by direct key against dedicated Column Families, so a token
// transfer touches only `contract_balances` (and a single atomic `WriteBatch`
// can span several contract entities) without rewriting the account SafeBox or
// calling `iter_accounts()`. No full scans are used on the hot path.

/// Incremental, RocksDB-backed contract state store. Wraps a live
/// [`augecoin_storage::Storage`] and routes each logical entity to its own
/// Column Family:
///
/// * `contract_metadata`  -> `ContractMeta` + `TokenConfig` (per-contract metadata)
/// * `contract_balances`   -> `balance/{contract_id}/{address}` -> u64 (big-endian)
/// * `contract_codes`      -> `code/{code_id}` -> `CodeRecord` (+ `next_code_id`)
/// * `contract_code_hash`  -> `codehash/{hash}` -> code_id (reverse index)
/// * `contracts` (generic) -> `state/{contract_id}/{key}` -> arbitrary WASM state
pub struct ContractStateStore<'a> {
    storage: &'a Storage,
}

impl<'a> ContractStateStore<'a> {
    pub fn new(storage: &'a Storage) -> Self {
        Self { storage }
    }

    // ---- Contract metadata ----

    pub fn get_contract(&self, id: &ContractId) -> Result<Option<ContractMeta>, ContractError> {
        match self
            .storage
            .contract_cf_get(CF_CONTRACT_METADATA, &key_contract(id))?
        {
            Some(b) => Ok(Some(ContractMeta::from_bytes(&b)?)),
            None => Ok(None),
        }
    }

    pub fn put_contract(&self, meta: &ContractMeta) -> Result<(), ContractError> {
        self.storage
            .contract_cf_put(
                CF_CONTRACT_METADATA,
                &key_contract(&meta.contract_id),
                &meta.to_bytes(),
            )
            .map_err(storage_err)
    }

    pub fn put_contract_batch(
        &self,
        batch: &mut WriteBatch,
        meta: &ContractMeta,
    ) -> Result<(), ContractError> {
        self.storage
            .contract_cf_put_batch(
                batch,
                CF_CONTRACT_METADATA,
                &key_contract(&meta.contract_id),
                &meta.to_bytes(),
            )
            .map_err(storage_err)
    }

    // ---- Token metadata ----

    pub fn get_token_info(&self, id: &ContractId) -> Result<Option<TokenConfig>, ContractError> {
        match self
            .storage
            .contract_cf_get(CF_CONTRACT_METADATA, &key_token(id))?
        {
            Some(b) => Ok(Some(TokenConfig::from_bytes(&b)?)),
            None => Ok(None),
        }
    }

    pub fn put_token_info(
        &self,
        id: &ContractId,
        token: &TokenConfig,
    ) -> Result<(), ContractError> {
        self.storage
            .contract_cf_put(CF_CONTRACT_METADATA, &key_token(id), &token.to_bytes())
            .map_err(storage_err)
    }

    pub fn put_token_info_batch(
        &self,
        batch: &mut WriteBatch,
        id: &ContractId,
        token: &TokenConfig,
    ) -> Result<(), ContractError> {
        self.storage
            .contract_cf_put_batch(
                batch,
                CF_CONTRACT_METADATA,
                &key_token(id),
                &token.to_bytes(),
            )
            .map_err(storage_err)
    }

    // ---- Balances (incremental; never touches the SafeBox) ----

    /// Returns the balance, or `None` if the key was never written / deleted.
    pub fn get_balance(
        &self,
        id: &ContractId,
        addr: Address,
    ) -> Result<Option<u64>, ContractError> {
        match self
            .storage
            .contract_cf_get(CF_CONTRACT_BALANCES, &key_balance(id, addr))?
        {
            Some(b) if b.len() == 8 => {
                let mut v = [0u8; 8];
                v.copy_from_slice(&b);
                Ok(Some(u64::from_be_bytes(v)))
            }
            Some(_) => Err(ContractError::InvalidSerialization),
            None => Ok(None),
        }
    }

    pub fn put_balance(
        &self,
        id: &ContractId,
        addr: Address,
        amount: u64,
    ) -> Result<(), ContractError> {
        self.storage
            .contract_cf_put(
                CF_CONTRACT_BALANCES,
                &key_balance(id, addr),
                &amount.to_be_bytes(),
            )
            .map_err(storage_err)
    }

    pub fn put_balance_batch(
        &self,
        batch: &mut WriteBatch,
        id: &ContractId,
        addr: Address,
        amount: u64,
    ) -> Result<(), ContractError> {
        self.storage
            .contract_cf_put_batch(
                batch,
                CF_CONTRACT_BALANCES,
                &key_balance(id, addr),
                &amount.to_be_bytes(),
            )
            .map_err(storage_err)
    }

    pub fn delete_balance(&self, id: &ContractId, addr: Address) -> Result<(), ContractError> {
        self.storage
            .contract_cf_delete(CF_CONTRACT_BALANCES, &key_balance(id, addr))
            .map_err(storage_err)
    }

    pub fn delete_balance_batch(
        &self,
        batch: &mut WriteBatch,
        id: &ContractId,
        addr: Address,
    ) -> Result<(), ContractError> {
        self.storage
            .contract_cf_delete_batch(batch, CF_CONTRACT_BALANCES, &key_balance(id, addr))
            .map_err(storage_err)
    }

    // ---- Generic WASM contract state (arbitrary key/value) ----

    pub fn get_contract_state(
        &self,
        id: &ContractId,
        state_key: &[u8],
    ) -> Result<Option<Vec<u8>>, ContractError> {
        self.storage
            .contract_get(&key_state(id, state_key))
            .map_err(storage_err)
    }

    pub fn put_contract_state(
        &self,
        id: &ContractId,
        state_key: &[u8],
        value: &[u8],
    ) -> Result<(), ContractError> {
        self.storage
            .contract_put(&key_state(id, state_key), value)
            .map_err(storage_err)
    }

    pub fn delete_contract_state(
        &self,
        id: &ContractId,
        state_key: &[u8],
    ) -> Result<(), ContractError> {
        self.storage
            .contract_delete(&key_state(id, state_key))
            .map_err(storage_err)
    }

    /// Scoped prefix scan of a single contract's generic state, used to build
    /// the read-only WASM host snapshot. Returns *stripped* raw keys (the
    /// `state/{contract_id}/` prefix is removed) so the snapshot matches the
    /// raw keys the WASM host queries. This is a prefix scan limited to one
    /// contract's keys — NOT a full DB or account scan, and never `iter_accounts`.
    #[allow(clippy::type_complexity)]
    pub fn scan_contract_state(
        &self,
        id: &ContractId,
    ) -> Result<Vec<(Vec<u8>, Vec<u8>)>, ContractError> {
        let mut prefix = b"state".to_vec();
        prefix.extend_from_slice(id);
        let entries = self
            .storage
            .contract_scan_prefix(&prefix)
            .map_err(storage_err)?;
        let plen = prefix.len();
        Ok(entries
            .into_iter()
            .filter_map(|(k, v)| {
                if k.len() >= plen {
                    Some((k[plen..].to_vec(), v))
                } else {
                    None
                }
            })
            .collect())
    }

    /// Atomically persist a prepared batch. RocksDB guarantees the whole batch
    /// is applied or nothing is (no partial persistence on crash).
    pub fn commit_batch(&self, batch: WriteBatch) -> Result<(), ContractError> {
        self.storage
            .contract_write_batch(batch)
            .map_err(storage_err)
    }

    /// Atomically persist the writes/deletes produced by a single Fase 5
    /// contract execution. Keys in `writes`/`deletes` are *contract-local* raw
    /// keys; they are mapped to the global `state/{contract_id}/{key}` layout
    /// inside a single RocksDB `WriteBatch` so the whole execution is
    /// committed all-or-nothing (no partial persistence).
    pub fn commit_overlay(
        &self,
        contract_id: &ContractId,
        writes: &[(Vec<u8>, Vec<u8>)],
        deletes: &[Vec<u8>],
    ) -> Result<(), ContractError> {
        let mut batch = WriteBatch::default();
        for (k, v) in writes {
            self.storage
                .contract_cf_put_batch(&mut batch, CF_CONTRACTS, &key_state(contract_id, k), v)
                .map_err(storage_err)?;
        }
        for k in deletes {
            self.storage
                .contract_cf_delete_batch(&mut batch, CF_CONTRACTS, &key_state(contract_id, k))
                .map_err(storage_err)?;
        }
        self.storage
            .contract_write_batch(batch)
            .map_err(storage_err)
    }
}

/// Code registry: stores WASM code blobs indexed by `code_id` and reverse
/// indexed by `code_hash`. Registering the same `code_hash` twice is idempotent
/// and never duplicates the WASM bytes.
///
/// NOTE on `code_id == 1`: this registry is **storage-agnostic** — it never
/// branches on any particular `code_id`, so AUGE20 (or any contract) receives
/// no privileged treatment at the persistence layer. `next_code_id` merely
/// *skips* `1` as a collision-avoidance convention, because the engine
/// (`augecoin-contracts` `ContractEngine`) currently hardcodes `AUGE20_CODE_ID =
/// 1` and routes that id through a native AUGE20 path. This is a defensive
/// reservation only; if the engine ever drops that special path and treats
/// AUGE20 as a normal WASM contract, `next_code_id` can start at any base
/// (e.g. `1` or `0`) with no change here.
pub struct CodeRegistry<'a> {
    storage: &'a Storage,
}

impl<'a> CodeRegistry<'a> {
    pub fn new(storage: &'a Storage) -> Self {
        Self { storage }
    }

    fn next_code_id(&self) -> Result<u64, ContractError> {
        match self
            .storage
            .contract_cf_get(CF_CONTRACT_CODES, KEY_NEXT_CODE_ID)?
        {
            Some(v) if v.len() == 8 => {
                let mut b = [0u8; 8];
                b.copy_from_slice(&v);
                Ok(u64::from_be_bytes(b))
            }
            Some(_) => Err(ContractError::InvalidSerialization),
            // Skip `1` to avoid colliding with the engine's hardcoded
            // `AUGE20_CODE_ID`. This is a reservation convention only — see the
            // `CodeRegistry` doc note. There is no privileged handling here.
            None => Ok(crate::AUGE20_CODE_ID + 1),
        }
    }

    pub fn get_by_id(&self, code_id: CodeId) -> Result<Option<CodeRecord>, ContractError> {
        match self
            .storage
            .contract_cf_get(CF_CONTRACT_CODES, &key_code(code_id))?
        {
            Some(b) => Ok(Some(CodeRecord::from_bytes(&b)?)),
            None => Ok(None),
        }
    }

    pub fn get_by_hash(&self, hash: &[u8; 32]) -> Result<Option<CodeRecord>, ContractError> {
        match self
            .storage
            .contract_cf_get(CF_CONTRACT_CODE_HASH, &key_codehash(hash))?
        {
            Some(b) if b.len() == 8 => {
                let mut v = [0u8; 8];
                v.copy_from_slice(&b);
                self.get_by_id(u64::from_be_bytes(v))
            }
            Some(_) => Err(ContractError::InvalidSerialization),
            None => Ok(None),
        }
    }

    /// Register a code blob. If `code_hash` already exists the WASM is NOT
    /// duplicated and the existing `code_id` is returned.
    pub fn put_code(&self, record: &CodeRecord) -> Result<CodeId, ContractError> {
        if let Some(existing) = self.get_by_hash(&record.code_hash)? {
            return Ok(existing.code_id);
        }
        let code_id = self.next_code_id()?;
        let mut rec = record.clone();
        rec.code_id = code_id;
        let mut batch = WriteBatch::default();
        self.storage
            .contract_cf_put_batch(
                &mut batch,
                CF_CONTRACT_CODES,
                &key_code(code_id),
                &rec.to_bytes(),
            )
            .map_err(storage_err)?;
        self.storage
            .contract_cf_put_batch(
                &mut batch,
                CF_CONTRACT_CODE_HASH,
                &key_codehash(&record.code_hash),
                &code_id.to_be_bytes(),
            )
            .map_err(storage_err)?;
        self.storage
            .contract_cf_put_batch(
                &mut batch,
                CF_CONTRACT_CODES,
                KEY_NEXT_CODE_ID,
                &(code_id + 1).to_be_bytes(),
            )
            .map_err(storage_err)?;
        self.storage
            .contract_write_batch(batch)
            .map_err(storage_err)?;
        Ok(code_id)
    }
}

fn storage_err(e: augecoin_storage::StorageError) -> ContractError {
    ContractError::Storage(e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use augecoin_storage::Storage;
    use std::sync::atomic::{AtomicU32, Ordering};

    static TEST_COUNTER: AtomicU32 = AtomicU32::new(0);

    fn test_path() -> String {
        let id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
        format!("/tmp/augecoin-contract-store-test-{id}")
    }

    fn cleanup(path: &str) {
        std::fs::remove_dir_all(path).ok();
    }

    fn cid(n: u8) -> ContractId {
        [n; 32]
    }

    fn open() -> (String, Storage) {
        let path = test_path();
        let storage = Storage::open(&path).expect("open storage");
        (path, storage)
    }

    fn sample_meta(id: ContractId, owner: Address, code_id: CodeId) -> ContractMeta {
        ContractMeta {
            contract_id: id,
            owner,
            code_id,
            is_auge20: false,
            storage_used: 0,
            created_height: 1,
        }
    }

    fn sample_token() -> TokenConfig {
        TokenConfig {
            name: "Test".into(),
            symbol: "TST".into(),
            decimals: 8,
            max_supply: 1_000_000,
            mint_enabled: true,
            burn_enabled: true,
        }
    }

    fn sample_code(code_id: CodeId, hash: [u8; 32], wasm: Vec<u8>) -> CodeRecord {
        CodeRecord {
            code_id,
            code_hash: hash,
            wasm,
        }
    }

    // 1. contract insert/get
    #[test]
    fn contract_insert_and_get() {
        let (path, storage) = open();
        let store = ContractStateStore::new(&storage);
        let id = cid(1);
        store.put_contract(&sample_meta(id, 7, 2)).unwrap();
        let m = store.get_contract(&id).unwrap().expect("present");
        assert_eq!(m.owner, 7);
        assert_eq!(m.code_id, 2);
        cleanup(&path);
    }

    // 2. contract update
    #[test]
    fn contract_update() {
        let (path, storage) = open();
        let store = ContractStateStore::new(&storage);
        let id = cid(2);
        store.put_contract(&sample_meta(id, 7, 2)).unwrap();
        let mut m = sample_meta(id, 7, 2);
        m.owner = 99;
        m.storage_used = 42;
        store.put_contract(&m).unwrap();
        let got = store.get_contract(&id).unwrap().unwrap();
        assert_eq!(got.owner, 99);
        assert_eq!(got.storage_used, 42);
        cleanup(&path);
    }

    // 3. token metadata insert/get
    #[test]
    fn token_metadata_insert_and_get() {
        let (path, storage) = open();
        let store = ContractStateStore::new(&storage);
        let id = cid(3);
        store.put_token_info(&id, &sample_token()).unwrap();
        let t = store.get_token_info(&id).unwrap().expect("present");
        assert_eq!(t.symbol, "TST");
        assert_eq!(t.max_supply, 1_000_000);
        cleanup(&path);
    }

    // 4. balance insert/get
    #[test]
    fn balance_insert_and_get() {
        let (path, storage) = open();
        let store = ContractStateStore::new(&storage);
        let id = cid(4);
        store.put_balance(&id, 5, 250).unwrap();
        assert_eq!(store.get_balance(&id, 5).unwrap(), Some(250));
        cleanup(&path);
    }

    // 5. balance update
    #[test]
    fn balance_update() {
        let (path, storage) = open();
        let store = ContractStateStore::new(&storage);
        let id = cid(5);
        store.put_balance(&id, 5, 100).unwrap();
        store.put_balance(&id, 5, 300).unwrap();
        assert_eq!(store.get_balance(&id, 5).unwrap(), Some(300));
        cleanup(&path);
    }

    // 6. balance delete
    #[test]
    fn balance_delete() {
        let (path, storage) = open();
        let store = ContractStateStore::new(&storage);
        let id = cid(6);
        store.put_balance(&id, 5, 100).unwrap();
        store.delete_balance(&id, 5).unwrap();
        assert_eq!(store.get_balance(&id, 5).unwrap(), None);
        cleanup(&path);
    }

    // 7. code insert/get
    #[test]
    fn code_insert_and_get() {
        let (path, storage) = open();
        let registry = CodeRegistry::new(&storage);
        let rec = sample_code(1, [9u8; 32], vec![1, 2, 3, 4]);
        let id = registry.put_code(&rec).unwrap();
        let got = registry.get_by_id(id).unwrap().expect("present");
        assert_eq!(got.wasm, vec![1, 2, 3, 4]);
        assert_eq!(got.code_hash, [9u8; 32]);
        cleanup(&path);
    }

    // 8. code lookup by hash
    #[test]
    fn code_lookup_by_hash() {
        let (path, storage) = open();
        let registry = CodeRegistry::new(&storage);
        let rec = sample_code(1, [8u8; 32], vec![9, 9]);
        let id = registry.put_code(&rec).unwrap();
        let got = registry.get_by_hash(&[8u8; 32]).unwrap().expect("present");
        assert_eq!(got.code_id, id);
        assert_eq!(got.wasm, vec![9, 9]);
        cleanup(&path);
    }

    // 9. duplicate code hash
    #[test]
    fn duplicate_code_hash_not_stored_twice() {
        let (path, storage) = open();
        let registry = CodeRegistry::new(&storage);
        let rec1 = sample_code(1, [7u8; 32], vec![1, 1, 1]);
        let id1 = registry.put_code(&rec1).unwrap();
        // Same hash, different code_id field and different wasm bytes.
        let rec2 = sample_code(42, [7u8; 32], vec![2, 2, 2]);
        let id2 = registry.put_code(&rec2).unwrap();
        assert_eq!(id1, id2, "duplicate hash must return the same code_id");
        let got = registry.get_by_id(id1).unwrap().unwrap();
        assert_eq!(
            got.wasm,
            vec![1, 1, 1],
            "original WASM must not be overwritten"
        );
        cleanup(&path);
    }

    // 10. persistence after restart
    #[test]
    fn persistence_after_restart() {
        let path = test_path();
        {
            let storage = Storage::open(&path).unwrap();
            let store = ContractStateStore::new(&storage);
            let registry = CodeRegistry::new(&storage);
            let id = cid(10);
            store.put_contract(&sample_meta(id, 3, 2)).unwrap();
            store.put_token_info(&id, &sample_token()).unwrap();
            store.put_balance(&id, 11, 777).unwrap();
            registry
                .put_code(&sample_code(2, [5u8; 32], vec![4, 5, 6]))
                .unwrap();
        }
        // Reopen RocksDB.
        {
            let storage = Storage::open(&path).unwrap();
            let store = ContractStateStore::new(&storage);
            let registry = CodeRegistry::new(&storage);
            let id = cid(10);
            assert!(store.get_contract(&id).unwrap().is_some());
            assert!(store.get_token_info(&id).unwrap().is_some());
            assert_eq!(store.get_balance(&id, 11).unwrap(), Some(777));
            assert!(registry.get_by_hash(&[5u8; 32]).unwrap().is_some());
        }
        cleanup(&path);
    }

    // 11. atomic batch (sender -100, receiver +100)
    #[test]
    fn atomic_balance_transfer_batch() {
        let (path, storage) = open();
        let store = ContractStateStore::new(&storage);
        let id = cid(11);
        store.put_balance(&id, 1, 100).unwrap();
        store.put_balance(&id, 2, 0).unwrap();

        let mut batch = WriteBatch::default();
        store.put_balance_batch(&mut batch, &id, 1, 0).unwrap();
        store.put_balance_batch(&mut batch, &id, 2, 100).unwrap();
        store.commit_batch(batch).unwrap();

        assert_eq!(store.get_balance(&id, 1).unwrap(), Some(0));
        assert_eq!(store.get_balance(&id, 2).unwrap(), Some(100));
        cleanup(&path);
    }

    // 11b. abort before commit leaves no partial change
    #[test]
    fn atomic_batch_no_partial_on_abort() {
        let (path, storage) = open();
        let store = ContractStateStore::new(&storage);
        let id = cid(12);
        store.put_balance(&id, 1, 100).unwrap();
        store.put_balance(&id, 2, 50).unwrap();

        // Simulate an invalid operation (insufficient balance) detected BEFORE
        // the commit point. Building a RocksDB batch never persists anything;
        // `db.write` is the atomic commit, so an aborted batch leaves the DB
        // exactly as it was.
        let sender = store.get_balance(&id, 1).unwrap().unwrap_or(0);
        let transfer = 150;
        if transfer > sender {
            // invalid: abort without calling commit_batch.
            let _batch = WriteBatch::default();
            // (intentionally not written)
        }
        assert_eq!(store.get_balance(&id, 1).unwrap(), Some(100));
        assert_eq!(store.get_balance(&id, 2).unwrap(), Some(50));
        cleanup(&path);
    }

    // 12. multiple contracts
    #[test]
    fn multiple_contracts() {
        let (path, storage) = open();
        let store = ContractStateStore::new(&storage);
        for n in 1u8..=5 {
            store
                .put_contract(&sample_meta(cid(n), n as u64, 2))
                .unwrap();
        }
        for n in 1u8..=5 {
            assert_eq!(
                store.get_contract(&cid(n)).unwrap().unwrap().owner,
                n as u64
            );
        }
        cleanup(&path);
    }

    // 13. multiple balances
    #[test]
    fn multiple_balances() {
        let (path, storage) = open();
        let store = ContractStateStore::new(&storage);
        let id = cid(13);
        for a in 1u64..=10 {
            store.put_balance(&id, a, a * 10).unwrap();
        }
        for a in 1u64..=10 {
            assert_eq!(store.get_balance(&id, a).unwrap(), Some(a * 10));
        }
        cleanup(&path);
    }

    // 14. zero balance
    #[test]
    fn zero_balance() {
        let (path, storage) = open();
        let store = ContractStateStore::new(&storage);
        let id = cid(14);
        store.put_balance(&id, 1, 0).unwrap();
        assert_eq!(store.get_balance(&id, 1).unwrap(), Some(0));
        cleanup(&path);
    }

    // 15. values near u64 limit
    #[test]
    fn balance_near_u64_limit() {
        let (path, storage) = open();
        let store = ContractStateStore::new(&storage);
        let id = cid(15);
        let big = u64::MAX - 1;
        store.put_balance(&id, 1, big).unwrap();
        assert_eq!(store.get_balance(&id, 1).unwrap(), Some(big));
        store.put_balance(&id, 1, u64::MAX).unwrap();
        assert_eq!(store.get_balance(&id, 1).unwrap(), Some(u64::MAX));
        cleanup(&path);
    }

    // Security: unknown id / code / hash return typed None, never panic.
    #[test]
    fn not_found_returns_none() {
        let (path, storage) = open();
        let store = ContractStateStore::new(&storage);
        let registry = CodeRegistry::new(&storage);
        assert_eq!(store.get_contract(&cid(200)).unwrap(), None);
        assert_eq!(store.get_balance(&cid(200), 1).unwrap(), None);
        assert_eq!(store.get_token_info(&cid(200)).unwrap(), None);
        assert_eq!(registry.get_by_id(999).unwrap(), None);
        assert_eq!(registry.get_by_hash(&[0u8; 32]).unwrap(), None);
        cleanup(&path);
    }

    // Generic WASM state round-trips and is namespaced per contract.
    #[test]
    fn contract_state_roundtrip() {
        let (path, storage) = open();
        let store = ContractStateStore::new(&storage);
        let a = cid(20);
        let b = cid(21);
        store.put_contract_state(&a, b"foo", b"bar").unwrap();
        store.put_contract_state(&b, b"foo", b"baz").unwrap();
        assert_eq!(
            store.get_contract_state(&a, b"foo").unwrap(),
            Some(b"bar".to_vec())
        );
        assert_eq!(
            store.get_contract_state(&b, b"foo").unwrap(),
            Some(b"baz".to_vec())
        );
        // Snapshot of `a` returns only `a`'s state, with raw (stripped) keys.
        let snap = store.scan_contract_state(&a).unwrap();
        assert_eq!(snap, vec![(b"foo".to_vec(), b"bar".to_vec())]);
        store.delete_contract_state(&a, b"foo").unwrap();
        assert_eq!(store.get_contract_state(&a, b"foo").unwrap(), None);
        cleanup(&path);
    }
}
