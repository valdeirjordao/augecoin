//! Typed, incremental contract state persistence (Fase 3).
//!
//! This module is the clean, reusable storage layer that future phases
//! (`node` → `ContractEngine` → `ContractStateStore` → RocksDB) will use. It is
//! deliberately decoupled from the SafeBox and from consensus:
//!
//! * State lives in dedicated RocksDB column families (`contract_metadata`,
//!   `contract_balances`, `contract_codes`, `contract_code_hash`), never in the
//!   account SafeBox and never via `iter_accounts()`.
//! * Every read/write is by direct key — no full scans on the hot path.
//! * Multi-entity updates (e.g. a token transfer touching two balances) are
//!   committed through a single `WriteBatch`, so they are all-or-nothing.
//! * All values are serialized with the crate's deterministic big-endian codec
//!   (`types::{ContractInfo,ContractState,TokenInfo,TokenBalance}` and
//!   `store::CodeRecord`); decode errors surface as typed `ContractError`s.

use crate::store::CodeRecord;
use crate::types::{CodeHash, ContractInfo, ContractState, TokenBalance, TokenInfo};
use crate::{code_hash_of, Address, CodeId, ContractError, ContractId};
use augecoin_storage::{
    Storage, WriteBatch, CF_CONTRACT_BALANCES, CF_CONTRACT_CODES, CF_CONTRACT_CODE_HASH,
    CF_CONTRACT_METADATA,
};

/// Reserved code id for the built-in AUGE20 token (must never be assigned by the
/// registry). Mirrors the engine's convention.
pub const RESERVED_AUGE20_CODE_ID: CodeId = 1;
/// First id the registry will hand out (1 is reserved).
const FIRST_USER_CODE_ID: CodeId = 2;
const NEXT_CODE_ID_KEY: &[u8] = b"next_code_id";

// ---- key construction (direct, fixed-layout) --------------------------------

fn cid(id: &ContractId) -> &[u8] {
    id.as_slice()
}

fn meta_key(id: &ContractId) -> Vec<u8> {
    cid(id).to_vec()
}

fn state_key(id: &ContractId) -> Vec<u8> {
    let mut k = cid(id).to_vec();
    k.extend_from_slice(b".state");
    k
}

fn token_key(id: &ContractId) -> Vec<u8> {
    let mut k = cid(id).to_vec();
    k.extend_from_slice(b".token");
    k
}

fn balance_key(id: &ContractId, addr: Address) -> Vec<u8> {
    let mut k = cid(id).to_vec();
    k.extend_from_slice(&addr.to_be_bytes());
    k
}

fn code_id_key(id: CodeId) -> Vec<u8> {
    id.to_be_bytes().to_vec()
}

// ---- ContractStateStore -----------------------------------------------------

/// Typed, incremental persistence for deployed contracts and their token state.
///
/// Wraps a `augecoin_storage::Storage` and routes each entity to its own column
/// family. No method ever touches `iter_accounts()` or the SafeBox.
pub struct ContractStateStore<'a> {
    storage: &'a Storage,
}

impl<'a> ContractStateStore<'a> {
    pub fn new(storage: &'a Storage) -> Self {
        Self { storage }
    }

    // -- contract identity (ContractInfo) --

    pub fn get_contract(&self, id: &ContractId) -> Result<Option<ContractInfo>, ContractError> {
        match self
            .storage
            .contract_cf_get(CF_CONTRACT_METADATA, &meta_key(id))?
        {
            Some(v) => Ok(Some(ContractInfo::from_bytes(&v)?)),
            None => Ok(None),
        }
    }

    pub fn put_contract(&self, c: &ContractInfo) -> Result<(), ContractError> {
        self.storage.contract_cf_put(
            CF_CONTRACT_METADATA,
            &meta_key(&c.contract_id),
            &c.to_bytes(),
        )?;
        Ok(())
    }

    // -- mutable contract runtime state (ContractState) --

    pub fn get_contract_state(
        &self,
        id: &ContractId,
    ) -> Result<Option<ContractState>, ContractError> {
        match self
            .storage
            .contract_cf_get(CF_CONTRACT_METADATA, &state_key(id))?
        {
            Some(v) => Ok(Some(ContractState::from_bytes(&v)?)),
            None => Ok(None),
        }
    }

    pub fn put_contract_state(&self, s: &ContractState) -> Result<(), ContractError> {
        self.storage.contract_cf_put(
            CF_CONTRACT_METADATA,
            &state_key(&s.contract_id),
            &s.to_bytes(),
        )?;
        Ok(())
    }

    // -- token info (TokenInfo) --

    pub fn get_token_info(&self, id: &ContractId) -> Result<Option<TokenInfo>, ContractError> {
        match self
            .storage
            .contract_cf_get(CF_CONTRACT_METADATA, &token_key(id))?
        {
            Some(v) => Ok(Some(TokenInfo::from_bytes(&v)?)),
            None => Ok(None),
        }
    }

    pub fn put_token_info(&self, id: &ContractId, t: &TokenInfo) -> Result<(), ContractError> {
        self.storage
            .contract_cf_put(CF_CONTRACT_METADATA, &token_key(id), &t.to_bytes())?;
        Ok(())
    }

    // -- token balances (TokenBalance) --

    pub fn get_balance(
        &self,
        id: &ContractId,
        addr: Address,
    ) -> Result<Option<TokenBalance>, ContractError> {
        match self
            .storage
            .contract_cf_get(CF_CONTRACT_BALANCES, &balance_key(id, addr))?
        {
            Some(v) => Ok(Some(TokenBalance::from_bytes(&v)?)),
            None => Ok(None),
        }
    }

    pub fn put_balance(&self, b: &TokenBalance) -> Result<(), ContractError> {
        if b.amount == 0 {
            return Err(ContractError::InvalidAmount);
        }
        self.storage.contract_cf_put(
            CF_CONTRACT_BALANCES,
            &balance_key(&b.contract_id, b.address),
            &b.to_bytes(),
        )?;
        Ok(())
    }

    pub fn delete_balance(&self, id: &ContractId, addr: Address) -> Result<(), ContractError> {
        self.storage
            .contract_cf_delete(CF_CONTRACT_BALANCES, &balance_key(id, addr))?;
        Ok(())
    }

    /// Apply several state writes atomically. If any op fails to serialize or is
    /// invalid, the whole batch is discarded (nothing is persisted). This is the
    /// primitive a future token transfer uses to move value between two balances
    /// in a single all-or-nothing RocksDB write.
    pub fn write_atomic(&self, ops: &[ContractStateWrite]) -> Result<(), ContractError> {
        let mut batch = WriteBatch::default();
        for op in ops {
            match op {
                ContractStateWrite::PutContract(c) => {
                    let v = c.to_bytes();
                    self.storage.contract_cf_put_batch(
                        &mut batch,
                        CF_CONTRACT_METADATA,
                        &meta_key(&c.contract_id),
                        &v,
                    )?;
                }
                ContractStateWrite::PutContractState(s) => {
                    let v = s.to_bytes();
                    self.storage.contract_cf_put_batch(
                        &mut batch,
                        CF_CONTRACT_METADATA,
                        &state_key(&s.contract_id),
                        &v,
                    )?;
                }
                ContractStateWrite::PutTokenInfo { contract_id, info } => {
                    let v = info.to_bytes();
                    self.storage.contract_cf_put_batch(
                        &mut batch,
                        CF_CONTRACT_METADATA,
                        &token_key(contract_id),
                        &v,
                    )?;
                }
                ContractStateWrite::PutBalance(b) => {
                    if b.amount == 0 {
                        return Err(ContractError::InvalidAmount);
                    }
                    let v = b.to_bytes();
                    self.storage.contract_cf_put_batch(
                        &mut batch,
                        CF_CONTRACT_BALANCES,
                        &balance_key(&b.contract_id, b.address),
                        &v,
                    )?;
                }
                ContractStateWrite::DeleteBalance {
                    contract_id,
                    address,
                } => {
                    self.storage.contract_cf_delete_batch(
                        &mut batch,
                        CF_CONTRACT_BALANCES,
                        &balance_key(contract_id, *address),
                    )?;
                }
            }
        }
        self.storage.contract_write_batch(batch)?;
        Ok(())
    }
}

/// A single typed write applied by [`ContractStateStore::write_atomic`].
#[derive(Debug, Clone)]
pub enum ContractStateWrite {
    PutContract(ContractInfo),
    PutContractState(ContractState),
    PutTokenInfo {
        contract_id: ContractId,
        info: TokenInfo,
    },
    PutBalance(TokenBalance),
    DeleteBalance {
        contract_id: ContractId,
        address: Address,
    },
}

// ---- CodeRegistry -----------------------------------------------------------

/// Registry of stored WASM code blobs, keyed by `code_id` and by `code_hash`.
///
/// If the same `code_hash` is registered twice, the WASM is NOT duplicated:
/// the second call returns the already-assigned `code_id`.
pub struct CodeRegistry<'a> {
    storage: &'a Storage,
}

impl<'a> CodeRegistry<'a> {
    pub fn new(storage: &'a Storage) -> Self {
        Self { storage }
    }

    pub fn get_by_id(&self, id: CodeId) -> Result<Option<CodeRecord>, ContractError> {
        match self
            .storage
            .contract_cf_get(CF_CONTRACT_CODES, &code_id_key(id))?
        {
            Some(v) => Ok(Some(CodeRecord::from_bytes(&v)?)),
            None => Ok(None),
        }
    }

    pub fn get_by_hash(&self, hash: CodeHash) -> Result<Option<CodeRecord>, ContractError> {
        match self
            .storage
            .contract_cf_get(CF_CONTRACT_CODE_HASH, hash.as_slice())?
        {
            Some(id_bytes) if id_bytes.len() == 8 => {
                let id = u64::from_be_bytes(id_bytes[..8].try_into().unwrap());
                self.get_by_id(id)
            }
            _ => Ok(None),
        }
    }

    /// Store `wasm`, returning its `code_id`. Idempotent on hash: if the hash
    /// already exists, the existing id is returned and no second copy is stored.
    pub fn put_code(&self, wasm: &[u8]) -> Result<CodeId, ContractError> {
        let hash = code_hash_of(wasm);
        if let Some(existing) = self.get_by_hash(hash)? {
            return Ok(existing.code_id);
        }

        let next = match self
            .storage
            .contract_cf_get(CF_CONTRACT_CODES, NEXT_CODE_ID_KEY)?
        {
            Some(b) if b.len() == 8 => u64::from_be_bytes(b[..8].try_into().unwrap()),
            _ => FIRST_USER_CODE_ID,
        };
        let id = next;

        let rec = CodeRecord {
            code_id: id,
            code_hash: hash,
            wasm: wasm.to_vec(),
        };
        let rec_bytes = rec.to_bytes();
        let id_bytes = id.to_be_bytes();
        let next_bytes = (id + 1).to_be_bytes();

        let mut batch = WriteBatch::default();
        self.storage.contract_cf_put_batch(
            &mut batch,
            CF_CONTRACT_CODES,
            &code_id_key(id),
            &rec_bytes,
        )?;
        self.storage.contract_cf_put_batch(
            &mut batch,
            CF_CONTRACT_CODE_HASH,
            hash.as_slice(),
            &id_bytes,
        )?;
        self.storage.contract_cf_put_batch(
            &mut batch,
            CF_CONTRACT_CODES,
            NEXT_CODE_ID_KEY,
            &next_bytes,
        )?;
        self.storage.contract_write_batch(batch)?;
        Ok(id)
    }
}

// ---- tests ------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    static CTR: AtomicU32 = AtomicU32::new(0);

    fn tmp_storage() -> (Storage, String) {
        let id = CTR.fetch_add(1, Ordering::SeqCst);
        let path = format!("/tmp/augecoin-state-store-{id}");
        std::fs::remove_dir_all(&path).ok();
        (Storage::open(&path).unwrap(), path)
    }

    fn cleanup(path: &str) {
        std::fs::remove_dir_all(path).ok();
    }

    fn sample_contract(id: ContractId) -> ContractInfo {
        ContractInfo::new(id, 1, 7, [2u8; 32], 5)
    }

    fn sample_token() -> TokenInfo {
        TokenInfo::new("Auge".into(), "AUG".into(), 8, 1_000, 10_000, 3)
            .unwrap()
            .with_mint(true)
            .with_burn(true)
    }

    // 1. contract insert/get
    #[test]
    fn contract_insert_get() {
        let (s, p) = tmp_storage();
        let store = ContractStateStore::new(&s);
        let info = sample_contract([1u8; 32]);
        store.put_contract(&info).unwrap();
        let got = store.get_contract(&[1u8; 32]).unwrap().unwrap();
        assert_eq!(got, info);
        cleanup(&p);
    }

    // 2. contract update
    #[test]
    fn contract_update() {
        let (s, p) = tmp_storage();
        let store = ContractStateStore::new(&s);
        let mut info = sample_contract([1u8; 32]);
        store.put_contract(&info).unwrap();
        info.owner = 99;
        store.put_contract(&info).unwrap();
        let got = store.get_contract(&[1u8; 32]).unwrap().unwrap();
        assert_eq!(got.owner, 99);
        cleanup(&p);
    }

    // 3. token metadata insert/get
    #[test]
    fn token_metadata_insert_get() {
        let (s, p) = tmp_storage();
        let store = ContractStateStore::new(&s);
        let t = sample_token();
        store.put_token_info(&[1u8; 32], &t).unwrap();
        let got = store.get_token_info(&[1u8; 32]).unwrap().unwrap();
        assert_eq!(got, t);
        cleanup(&p);
    }

    // 4. balance insert/get
    #[test]
    fn balance_insert_get() {
        let (s, p) = tmp_storage();
        let store = ContractStateStore::new(&s);
        let b = TokenBalance::new([1u8; 32], 5, 250).unwrap();
        store.put_balance(&b).unwrap();
        let got = store.get_balance(&[1u8; 32], 5).unwrap().unwrap();
        assert_eq!(got, b);
        cleanup(&p);
    }

    // 5. balance update
    #[test]
    fn balance_update() {
        let (s, p) = tmp_storage();
        let store = ContractStateStore::new(&s);
        let mut b = TokenBalance::new([1u8; 32], 5, 250).unwrap();
        store.put_balance(&b).unwrap();
        b = TokenBalance::new([1u8; 32], 5, 999).unwrap();
        store.put_balance(&b).unwrap();
        let got = store.get_balance(&[1u8; 32], 5).unwrap().unwrap();
        assert_eq!(got.amount, 999);
        cleanup(&p);
    }

    // 6. balance delete
    #[test]
    fn balance_delete() {
        let (s, p) = tmp_storage();
        let store = ContractStateStore::new(&s);
        let b = TokenBalance::new([1u8; 32], 5, 250).unwrap();
        store.put_balance(&b).unwrap();
        assert!(store.get_balance(&[1u8; 32], 5).unwrap().is_some());
        store.delete_balance(&[1u8; 32], 5).unwrap();
        assert!(store.get_balance(&[1u8; 32], 5).unwrap().is_none());
        cleanup(&p);
    }

    // 7. code insert/get
    #[test]
    fn code_insert_get() {
        let (s, p) = tmp_storage();
        let reg = CodeRegistry::new(&s);
        let wasm = vec![0x00, 0x61, 0x73, 0x6d, 1, 2, 3, 4];
        let id = reg.put_code(&wasm).unwrap();
        let rec = reg.get_by_id(id).unwrap().unwrap();
        assert_eq!(rec.wasm, wasm);
        assert_eq!(rec.code_hash, code_hash_of(&wasm));
        cleanup(&p);
    }

    // 8. code lookup by hash
    #[test]
    fn code_lookup_by_hash() {
        let (s, p) = tmp_storage();
        let reg = CodeRegistry::new(&s);
        let wasm = vec![0x00, 0x61, 0x73, 0x6d, 9, 9, 9, 9];
        let id = reg.put_code(&wasm).unwrap();
        let rec = reg.get_by_hash(code_hash_of(&wasm)).unwrap().unwrap();
        assert_eq!(rec.code_id, id);
        cleanup(&p);
    }

    // 9. duplicate code hash -> no duplicate WASM
    #[test]
    fn duplicate_code_hash_returns_same_id() {
        let (s, p) = tmp_storage();
        let reg = CodeRegistry::new(&s);
        let wasm = vec![0x00, 0x61, 0x73, 0x6d, 7, 7, 7, 7];
        let id1 = reg.put_code(&wasm).unwrap();
        let id2 = reg.put_code(&wasm).unwrap();
        assert_eq!(id1, id2);
        // A second record must not have been created.
        let count = (FIRST_USER_CODE_ID..=id1 + 1)
            .filter(|id| reg.get_by_id(*id).unwrap().is_some())
            .count();
        assert_eq!(count, 1);
        cleanup(&p);
    }

    // 10. persistence after restart
    #[test]
    fn persistence_after_restart() {
        let (s, p) = tmp_storage();
        {
            let store = ContractStateStore::new(&s);
            let reg = CodeRegistry::new(&s);
            store.put_contract(&sample_contract([1u8; 32])).unwrap();
            store.put_token_info(&[1u8; 32], &sample_token()).unwrap();
            store
                .put_balance(&TokenBalance::new([1u8; 32], 5, 123).unwrap())
                .unwrap();
            let wasm = vec![0x00, 0x61, 0x73, 0x6d, 4, 4, 4, 4];
            let code_id = reg.put_code(&wasm).unwrap();
            // force code path exercised
            assert!(reg.get_by_id(code_id).unwrap().is_some());
        }
        drop(s);

        let s2 = Storage::open(&p).unwrap();
        let store = ContractStateStore::new(&s2);
        let reg = CodeRegistry::new(&s2);
        assert!(store.get_contract(&[1u8; 32]).unwrap().is_some());
        assert!(store.get_token_info(&[1u8; 32]).unwrap().is_some());
        assert!(store.get_balance(&[1u8; 32], 5).unwrap().is_some());
        let wasm = vec![0x00, 0x61, 0x73, 0x6d, 4, 4, 4, 4];
        assert!(reg.get_by_hash(code_hash_of(&wasm)).unwrap().is_some());
        cleanup(&p);
    }

    // 11. atomic batch (two balances)
    #[test]
    fn atomic_batch_two_balances() {
        let (s, p) = tmp_storage();
        let store = ContractStateStore::new(&s);
        let id = [1u8; 32];
        store
            .write_atomic(&[
                ContractStateWrite::PutBalance(TokenBalance::new(id, 5, 100).unwrap()),
                ContractStateWrite::PutBalance(TokenBalance::new(id, 6, 200).unwrap()),
            ])
            .unwrap();
        assert_eq!(store.get_balance(&id, 5).unwrap().unwrap().amount, 100);
        assert_eq!(store.get_balance(&id, 6).unwrap().unwrap().amount, 200);
        cleanup(&p);
    }

    // 11b. atomic batch with an invalid op -> nothing persisted
    #[test]
    fn atomic_batch_aborts_on_invalid() {
        let (s, p) = tmp_storage();
        let store = ContractStateStore::new(&s);
        let id = [1u8; 32];
        store
            .put_balance(&TokenBalance::new(id, 5, 100).unwrap())
            .unwrap();
        // The second op has amount == 0 and must abort the whole batch.
        let bad = ContractStateWrite::PutBalance(TokenBalance {
            contract_id: id,
            address: 6,
            amount: 0,
        });
        let res = store.write_atomic(&[
            ContractStateWrite::PutBalance(TokenBalance::new(id, 7, 50).unwrap()),
            bad,
        ]);
        assert!(matches!(res, Err(ContractError::InvalidAmount)));
        // Neither the valid nor the invalid write may have persisted.
        assert!(store.get_balance(&id, 7).unwrap().is_none());
        assert_eq!(store.get_balance(&id, 5).unwrap().unwrap().amount, 100);
        cleanup(&p);
    }

    // 12. multiple contracts
    #[test]
    fn multiple_contracts() {
        let (s, p) = tmp_storage();
        let store = ContractStateStore::new(&s);
        for i in 0..5u8 {
            let mut id = [0u8; 32];
            id[0] = i;
            store.put_contract(&sample_contract(id)).unwrap();
        }
        for i in 0..5u8 {
            let mut id = [0u8; 32];
            id[0] = i;
            assert!(store.get_contract(&id).unwrap().is_some());
        }
        cleanup(&p);
    }

    // 13. multiple balances
    #[test]
    fn multiple_balances() {
        let (s, p) = tmp_storage();
        let store = ContractStateStore::new(&s);
        let id = [1u8; 32];
        for a in 0..5u64 {
            store
                .put_balance(&TokenBalance::new(id, a, (a + 1) * 10).unwrap())
                .unwrap();
        }
        for a in 0..5u64 {
            assert_eq!(
                store.get_balance(&id, a).unwrap().unwrap().amount,
                (a + 1) * 10
            );
        }
        cleanup(&p);
    }

    // 14. zero balance rejected
    #[test]
    fn zero_balance_rejected() {
        let (s, p) = tmp_storage();
        let store = ContractStateStore::new(&s);
        let r = TokenBalance::new([1u8; 32], 5, 0);
        assert!(matches!(r, Err(ContractError::InvalidAmount)));
        // Even constructing directly and calling put_balance must be rejected.
        let direct = TokenBalance {
            contract_id: [1u8; 32],
            address: 5,
            amount: 0,
        };
        assert!(matches!(
            store.put_balance(&direct),
            Err(ContractError::InvalidAmount)
        ));
        cleanup(&p);
    }

    // 15. values near u64::MAX
    #[test]
    fn near_max_u64_balance() {
        let (s, p) = tmp_storage();
        let store = ContractStateStore::new(&s);
        let b = TokenBalance::new([1u8; 32], 5, u64::MAX).unwrap();
        store.put_balance(&b).unwrap();
        let got = store.get_balance(&[1u8; 32], 5).unwrap().unwrap();
        assert_eq!(got.amount, u64::MAX);
        cleanup(&p);
    }

    // --- security / error handling ---

    #[test]
    fn missing_ids_return_none_not_error() {
        let (s, p) = tmp_storage();
        let store = ContractStateStore::new(&s);
        let reg = CodeRegistry::new(&s);
        assert!(store.get_contract(&[9u8; 32]).unwrap().is_none());
        assert!(store.get_balance(&[9u8; 32], 1).unwrap().is_none());
        assert!(reg.get_by_id(9999).unwrap().is_none());
        assert!(reg.get_by_hash([9u8; 32]).unwrap().is_none());
        cleanup(&p);
    }

    #[test]
    fn corrupted_value_is_typed_error() {
        let (s, p) = tmp_storage();
        let store = ContractStateStore::new(&s);
        // Write garbage directly where a balance should live.
        store
            .storage
            .contract_cf_put(
                CF_CONTRACT_BALANCES,
                &balance_key(&[1u8; 32], 5),
                b"not-a-balance",
            )
            .unwrap();
        let r = store.get_balance(&[1u8; 32], 5);
        assert!(matches!(r, Err(ContractError::InvalidSerialization)));
        cleanup(&p);
    }
}
