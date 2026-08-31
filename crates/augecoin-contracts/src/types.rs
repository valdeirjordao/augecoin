//! Fundamental, reusable smart-contract types for AUGECOIN WASM Lite V1.
//!
//! This is the clean, dependency-light *type layer* of `augecoin-contracts`.
//! It intentionally contains no execution logic and does not touch the
//! consensus, the SafeBox, or the block format. The engine and node layers are
//! expected to build on top of these types.
//!
//! Reused primitives (never redefined here):
//! * `Address`  — chain account number (`u64`)
//! * `CodeId`   — registered WASM code id (`u64`)
//! * `ContractId` — deployed contract id (`[u8; 32]`)
//!
//! All monetary values use integer arithmetic (`u64`/`u8`). Floating point is
//! never used for value representation.

use crate::codec::{Reader, Writer};
use crate::{Address, CodeId, ContractError, ContractId};

/// A WASM code blob hash (blake3, 32 bytes).
pub type CodeHash = [u8; 32];

/// Maximum number of decimal places a token may declare in V1.
pub const MAX_TOKEN_DECIMALS: u8 = 18;

/// Maximum allowed length (bytes) of a token name.
pub const MAX_TOKEN_NAME_LEN: usize = 64;

/// Maximum allowed length (bytes) of a token symbol.
pub const MAX_TOKEN_SYMBOL_LEN: usize = 16;

// ---------------------------------------------------------------------------
// Contract identity & state
// ---------------------------------------------------------------------------

/// Immutable identity metadata of a deployed contract (V1).
///
/// Contains only what is needed to identify and validate a contract: the
/// deterministic id, the code it was instantiated from, its owner, and the
/// code hash used for verification. Mutable runtime data lives in
/// [`ContractState`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractInfo {
    pub contract_id: ContractId,
    pub code_id: CodeId,
    pub owner: Address,
    pub code_hash: CodeHash,
    /// Block height at which the contract was created (metadata, not mutable
    /// runtime state).
    pub created_height: u64,
}

impl ContractInfo {
    pub fn new(
        contract_id: ContractId,
        code_id: CodeId,
        owner: Address,
        code_hash: CodeHash,
        created_height: u64,
    ) -> Self {
        Self {
            contract_id,
            code_id,
            owner,
            code_hash,
            created_height,
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.fixed(&self.contract_id);
        w.u64(self.code_id);
        w.u64(self.owner);
        w.fixed(&self.code_hash);
        w.u64(self.created_height);
        w.into_vec()
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self, ContractError> {
        let mut r = Reader::new(data);
        let contract_id = r.fixed()?;
        let code_id = r.u64()?;
        let owner = r.u64()?;
        let code_hash = r.fixed()?;
        let created_height = r.u64()?;
        r.expect_end()?;
        Ok(Self {
            contract_id,
            code_id,
            owner,
            code_hash,
            created_height,
        })
    }
}

/// Mutable per-contract runtime state (V1).
///
/// Kept separate from [`ContractInfo`] so a state update does not rewrite the
/// immutable identity. In V1 this is just the accounted storage size, updated
/// incrementally as the contract writes/removes keys.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ContractState {
    pub contract_id: ContractId,
    pub storage_used: u64,
}

impl ContractState {
    pub fn new(contract_id: ContractId, storage_used: u64) -> Self {
        Self {
            contract_id,
            storage_used,
        }
    }

    /// Add `delta` bytes to the accounted storage, guarding against overflow.
    pub fn checked_add_storage(&mut self, delta: u64) -> Result<(), ContractError> {
        self.storage_used = self
            .storage_used
            .checked_add(delta)
            .ok_or(ContractError::ArithmeticOverflow)?;
        Ok(())
    }

    /// Remove `delta` bytes from the accounted storage, guarding underflow.
    pub fn checked_sub_storage(&mut self, delta: u64) -> Result<(), ContractError> {
        self.storage_used = self
            .storage_used
            .checked_sub(delta)
            .ok_or(ContractError::ArithmeticOverflow)?;
        Ok(())
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.fixed(&self.contract_id);
        w.u64(self.storage_used);
        w.into_vec()
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self, ContractError> {
        let mut r = Reader::new(data);
        let contract_id = r.fixed()?;
        let storage_used = r.u64()?;
        r.expect_end()?;
        Ok(Self {
            contract_id,
            storage_used,
        })
    }
}

// ---------------------------------------------------------------------------
// Token (AUGE20) types
// ---------------------------------------------------------------------------

/// Token static + dynamic info (AUGE20, V1).
///
/// Uses integer arithmetic only. `total_supply` is the live circulating supply
/// (starts at `initial_supply`), `max_supply` is the hard cap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenInfo {
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
    pub total_supply: u64,
    pub max_supply: u64,
    pub owner: Address,
    pub mint_enabled: bool,
    pub burn_enabled: bool,
}

impl TokenInfo {
    /// Build and validate a token from its construction parameters.
    ///
    /// `mint_enabled` / `burn_enabled` default to `false`; enable them with
    /// [`TokenInfo::with_mint`] / [`TokenInfo::with_burn`].
    ///
    /// Validations:
    /// * `decimals <= MAX_TOKEN_DECIMALS`
    /// * `name` / `symbol` are non-empty and within length bounds
    /// * `initial_supply <= max_supply`
    pub fn new(
        name: String,
        symbol: String,
        decimals: u8,
        initial_supply: u64,
        max_supply: u64,
        owner: Address,
    ) -> Result<Self, ContractError> {
        let info = Self {
            name,
            symbol,
            decimals,
            total_supply: initial_supply,
            max_supply,
            owner,
            mint_enabled: false,
            burn_enabled: false,
        };
        info.validate()?;
        Ok(info)
    }

    /// Enable minting for this token.
    pub fn with_mint(mut self, enabled: bool) -> Self {
        self.mint_enabled = enabled;
        self
    }

    /// Enable burning for this token.
    pub fn with_burn(mut self, enabled: bool) -> Self {
        self.burn_enabled = enabled;
        self
    }

    /// Validate the static properties of the token (decimals and metadata).
    pub fn validate(&self) -> Result<(), ContractError> {
        if self.decimals > MAX_TOKEN_DECIMALS {
            return Err(ContractError::InvalidDecimals);
        }
        if self.name.is_empty() || self.name.len() > MAX_TOKEN_NAME_LEN {
            return Err(ContractError::InvalidTokenMetadata);
        }
        if self.symbol.is_empty() || self.symbol.len() > MAX_TOKEN_SYMBOL_LEN {
            return Err(ContractError::InvalidTokenMetadata);
        }
        if self.total_supply > self.max_supply {
            return Err(ContractError::MaxSupplyExceeded);
        }
        Ok(())
    }

    /// Mint `amount` tokens, updating `total_supply`.
    ///
    /// * rejects `amount == 0`
    /// * guards against arithmetic overflow
    /// * enforces `total_supply + amount <= max_supply`
    pub fn checked_mint(&mut self, amount: u64) -> Result<(), ContractError> {
        if amount == 0 {
            return Err(ContractError::InvalidAmount);
        }
        let new_supply = self
            .total_supply
            .checked_add(amount)
            .ok_or(ContractError::ArithmeticOverflow)?;
        if new_supply > self.max_supply {
            return Err(ContractError::MaxSupplyExceeded);
        }
        self.total_supply = new_supply;
        Ok(())
    }

    /// Burn `amount` tokens, updating `total_supply`.
    ///
    /// * rejects `amount == 0`
    /// * guards against arithmetic underflow
    pub fn checked_burn(&mut self, amount: u64) -> Result<(), ContractError> {
        if amount == 0 {
            return Err(ContractError::InvalidAmount);
        }
        let new_supply = self
            .total_supply
            .checked_sub(amount)
            .ok_or(ContractError::ArithmeticOverflow)?;
        self.total_supply = new_supply;
        Ok(())
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.u8(self.decimals);
        w.bool(self.mint_enabled);
        w.bool(self.burn_enabled);
        w.u64(self.total_supply);
        w.u64(self.max_supply);
        w.u64(self.owner);
        w.string(&self.name);
        w.string(&self.symbol);
        w.into_vec()
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self, ContractError> {
        let mut r = Reader::new(data);
        let decimals = r.u8()?;
        let mint_enabled = r.bool()?;
        let burn_enabled = r.bool()?;
        let total_supply = r.u64()?;
        let max_supply = r.u64()?;
        let owner = r.u64()?;
        let name = r.string()?;
        let symbol = r.string()?;
        r.expect_end()?;
        Ok(Self {
            name,
            symbol,
            decimals,
            total_supply,
            max_supply,
            owner,
            mint_enabled,
            burn_enabled,
        })
    }
}

/// A single token balance entry (V1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenBalance {
    pub contract_id: ContractId,
    pub address: Address,
    pub amount: u64,
}

impl TokenBalance {
    /// Build a balance entry. A balance of `0` is represented by the *absence*
    /// of the entry, so constructing one with `amount == 0` is rejected.
    pub fn new(
        contract_id: ContractId,
        address: Address,
        amount: u64,
    ) -> Result<Self, ContractError> {
        if amount == 0 {
            return Err(ContractError::InvalidAmount);
        }
        Ok(Self {
            contract_id,
            address,
            amount,
        })
    }

    /// Return a new balance equal to `self + amount`, guarding overflow.
    pub fn checked_add(&self, amount: u64) -> Result<Self, ContractError> {
        if amount == 0 {
            return Err(ContractError::InvalidAmount);
        }
        let new_amount = self
            .amount
            .checked_add(amount)
            .ok_or(ContractError::ArithmeticOverflow)?;
        Ok(Self {
            contract_id: self.contract_id,
            address: self.address,
            amount: new_amount,
        })
    }

    /// Return a new balance equal to `self - amount`, guarding underflow.
    pub fn checked_sub(&self, amount: u64) -> Result<Self, ContractError> {
        if amount == 0 {
            return Err(ContractError::InvalidAmount);
        }
        let new_amount = self
            .amount
            .checked_sub(amount)
            .ok_or(ContractError::ArithmeticOverflow)?;
        Ok(Self {
            contract_id: self.contract_id,
            address: self.address,
            amount: new_amount,
        })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.fixed(&self.contract_id);
        w.u64(self.address);
        w.u64(self.amount);
        w.into_vec()
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self, ContractError> {
        let mut r = Reader::new(data);
        let contract_id = r.fixed()?;
        let address = r.u64()?;
        let amount = r.u64()?;
        r.expect_end()?;
        Ok(Self {
            contract_id,
            address,
            amount,
        })
    }
}

// ---------------------------------------------------------------------------
// Operation payloads
// ---------------------------------------------------------------------------

/// Payload of the `StoreCode` operation: register a WASM blob.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreCode {
    pub wasm_code: Vec<u8>,
}

impl StoreCode {
    pub fn new(wasm_code: Vec<u8>) -> Self {
        Self { wasm_code }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.bytes(&self.wasm_code);
        w.into_vec()
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self, ContractError> {
        let mut r = Reader::new(data);
        let wasm_code = r.bytes()?;
        r.expect_end()?;
        Ok(Self { wasm_code })
    }
}

/// Payload of the `CreateContract` operation: instantiate a stored code blob.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateContract {
    pub code_id: CodeId,
    pub instantiate_data: Vec<u8>,
}

impl CreateContract {
    pub fn new(code_id: CodeId, instantiate_data: Vec<u8>) -> Self {
        Self {
            code_id,
            instantiate_data,
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.u64(self.code_id);
        w.bytes(&self.instantiate_data);
        w.into_vec()
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self, ContractError> {
        let mut r = Reader::new(data);
        let code_id = r.u64()?;
        let instantiate_data = r.bytes()?;
        r.expect_end()?;
        Ok(Self {
            code_id,
            instantiate_data,
        })
    }
}

/// Payload of the `ExecuteContract` operation: call a deployed contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecuteContract {
    pub contract_id: ContractId,
    pub call_data: Vec<u8>,
}

impl ExecuteContract {
    pub fn new(contract_id: ContractId, call_data: Vec<u8>) -> Self {
        Self {
            contract_id,
            call_data,
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.fixed(&self.contract_id);
        w.bytes(&self.call_data);
        w.into_vec()
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self, ContractError> {
        let mut r = Reader::new(data);
        let contract_id = r.fixed()?;
        let call_data = r.bytes()?;
        r.expect_end()?;
        Ok(Self {
            contract_id,
            call_data,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{code_hash_of, contract_id_of};

    // 1. criação de ContractInfo
    #[test]
    fn contract_info_creation() {
        let h = [9u8; 32];
        let info = ContractInfo::new([1u8; 32], 1, 42, h, 7);
        assert_eq!(info.code_id, 1);
        assert_eq!(info.owner, 42);
        assert_eq!(info.code_hash, h);
        assert_eq!(info.created_height, 7);
    }

    // 2. criação de TokenInfo
    #[test]
    fn token_info_creation() {
        let t = TokenInfo::new("Auge".into(), "AUG".into(), 8, 1_000, 10_000, 1)
            .unwrap()
            .with_mint(true)
            .with_burn(true);
        assert_eq!(t.total_supply, 1_000);
        assert_eq!(t.max_supply, 10_000);
        assert!(t.validate().is_ok());
    }

    // 3. initial_supply <= max_supply
    #[test]
    fn token_info_initial_le_max_ok() {
        let t = TokenInfo::new("A".into(), "A".into(), 0, 10, 10, 1);
        assert!(t.is_ok());
    }

    // 4. initial_supply > max_supply
    #[test]
    fn token_info_initial_gt_max_fails() {
        let r = TokenInfo::new("A".into(), "A".into(), 0, 11, 10, 1);
        assert!(matches!(r, Err(ContractError::MaxSupplyExceeded)));
    }

    // 5. amount zero
    #[test]
    fn amount_zero_rejected() {
        let bal = TokenBalance::new([0u8; 32], 1, 0);
        assert!(matches!(bal, Err(ContractError::InvalidAmount)));

        let mut t = TokenInfo::new("A".into(), "A".into(), 0, 5, 10, 1)
            .unwrap()
            .with_mint(true)
            .with_burn(true);
        assert!(matches!(
            t.checked_mint(0),
            Err(ContractError::InvalidAmount)
        ));
        assert!(matches!(
            t.checked_burn(0),
            Err(ContractError::InvalidAmount)
        ));
    }

    // 6. checked_add
    #[test]
    fn token_balance_checked_add() {
        let b = TokenBalance::new([0u8; 32], 1, 100).unwrap();
        let r = b.checked_add(50).unwrap();
        assert_eq!(r.amount, 150);
        // overflow
        let big = TokenBalance::new([0u8; 32], 1, u64::MAX - 1).unwrap();
        assert!(big.checked_add(2).is_err());
    }

    // 7. checked_sub
    #[test]
    fn token_balance_checked_sub() {
        let b = TokenBalance::new([0u8; 32], 1, 100).unwrap();
        let r = b.checked_sub(40).unwrap();
        assert_eq!(r.amount, 60);
        // underflow
        assert!(b.checked_sub(101).is_err());
    }

    // 8. serialização/deserialização
    #[test]
    fn roundtrip_serialization() {
        let h = [3u8; 32];
        let info = ContractInfo::new([1u8; 32], 5, 9, h, 12);
        let back = ContractInfo::from_bytes(&info.to_bytes()).unwrap();
        assert_eq!(info, back);

        let mut st = ContractState::new([2u8; 32], 0);
        st.checked_add_storage(123).unwrap();
        let back_st = ContractState::from_bytes(&st.to_bytes()).unwrap();
        assert_eq!(st, back_st);

        let mut t = TokenInfo::new("Nome".into(), "SYM".into(), 6, 100, 1000, 3)
            .unwrap()
            .with_mint(true)
            .with_burn(false);
        let back_t = TokenInfo::from_bytes(&t.to_bytes()).unwrap();
        assert_eq!(t, back_t);
        // mutate then re-check
        t.checked_mint(50).unwrap();
        assert_eq!(t.total_supply, 150);

        let b = TokenBalance::new([4u8; 32], 7, 250).unwrap();
        let back_b = TokenBalance::from_bytes(&b.to_bytes()).unwrap();
        assert_eq!(b, back_b);
    }

    // 9. igualdade dos tipos
    #[test]
    fn type_equality() {
        let a = StoreCode::new(vec![1, 2, 3]);
        let b = StoreCode::new(vec![1, 2, 3]);
        let c = StoreCode::new(vec![1, 2, 4]);
        assert_eq!(a, b);
        assert_ne!(a, c);

        let x = CreateContract::new(1, vec![9]);
        let y = CreateContract::new(1, vec![9]);
        let z = CreateContract::new(2, vec![9]);
        assert_eq!(x, y);
        assert_ne!(x, z);

        let e1 = ExecuteContract::new([5u8; 32], vec![1]);
        let e2 = ExecuteContract::new([5u8; 32], vec![1]);
        assert_eq!(e1, e2);
    }

    // 10. IDs/hash
    #[test]
    fn ids_and_hash() {
        let wasm = vec![0x00, 0x61, 0x73, 0x6d];
        let h = code_hash_of(&wasm);
        assert_eq!(h.len(), 32);

        let c1 = contract_id_of(1, 1, &[1]);
        let c2 = contract_id_of(1, 1, &[1]);
        let c3 = contract_id_of(2, 1, &[1]);
        assert_eq!(c1, c2);
        assert_ne!(c1, c3);

        let sc = StoreCode::new(wasm);
        let sc_back = StoreCode::from_bytes(&sc.to_bytes()).unwrap();
        assert_eq!(sc, sc_back);

        let cc = CreateContract::new(1, vec![1, 2]);
        let cc_back = CreateContract::from_bytes(&cc.to_bytes()).unwrap();
        assert_eq!(cc, cc_back);

        let ec = ExecuteContract::new(c1, vec![7]);
        let ec_back = ExecuteContract::from_bytes(&ec.to_bytes()).unwrap();
        assert_eq!(ec, ec_back);
    }

    #[test]
    fn invalid_decimals_rejected() {
        let r = TokenInfo::new("A".into(), "A".into(), 19, 1, 10, 1);
        assert!(matches!(r, Err(ContractError::InvalidDecimals)));
    }

    #[test]
    fn invalid_metadata_rejected() {
        let empty_name = TokenInfo::new("".into(), "A".into(), 0, 1, 10, 1);
        assert!(matches!(
            empty_name,
            Err(ContractError::InvalidTokenMetadata)
        ));

        let empty_sym = TokenInfo::new("A".into(), "".into(), 0, 1, 10, 1);
        assert!(matches!(
            empty_sym,
            Err(ContractError::InvalidTokenMetadata)
        ));
    }

    #[test]
    fn mint_enforces_max_supply() {
        let mut t = TokenInfo::new("A".into(), "A".into(), 0, 5, 10, 1)
            .unwrap()
            .with_mint(true)
            .with_burn(true);
        assert!(t.checked_mint(5).is_ok());
        assert!(matches!(
            t.checked_mint(1),
            Err(ContractError::MaxSupplyExceeded)
        ));
    }

    #[test]
    fn contract_state_checked_arithmetic() {
        let mut s = ContractState::new([0u8; 32], 10);
        assert!(s.checked_add_storage(u64::MAX - 9).is_err());
        s.checked_sub_storage(10).unwrap();
        assert_eq!(s.storage_used, 0);
        assert!(s.checked_sub_storage(1).is_err());
    }
}
