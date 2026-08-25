use crate::constants::CT_MAX_ACCOUNT_DATA;
use augecoin_crypto::hash::blake3_512;
use thiserror::Error;

pub const ED25519_PK_LEN: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountState {
    Unknown = 0,
    Normal = 1,
    ForSale = 2,
    ForAtomicAccountSwap = 3,
    ForAtomicCoinSwap = 4,
    /// A freshly-emitted AUGEID owned by the block leader. It has no definitive
    /// owner/key yet and cannot send or receive AUGE.
    Reserved = 5,
    /// A former `Reserved` AUGEID that has been sold/donated: it now has a
    /// definitive `account_key`, initialized `n_operation` and `account_seal`.
    Owned = 6,
    /// An AUGEID that has been donated and is awaiting the recipient's accept.
    GiftPending = 7,
}

impl AccountState {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(AccountState::Unknown),
            1 => Some(AccountState::Normal),
            2 => Some(AccountState::ForSale),
            3 => Some(AccountState::ForAtomicAccountSwap),
            4 => Some(AccountState::ForAtomicCoinSwap),
            5 => Some(AccountState::Reserved),
            6 => Some(AccountState::Owned),
            7 => Some(AccountState::GiftPending),
            _ => None,
        }
    }

    pub fn to_u8(self) -> u8 {
        self as u8
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountInfo {
    pub state: AccountState,
    pub account_key: AccountKey,
    pub locked_until_block: u64,
    pub price: u64,
    pub account_to_pay: u64,
    pub new_public_key: Option<AccountKey>,
    pub hashed_secret: [u8; 32],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AccountKey {
    pub ed25519_public_key: [u8; ED25519_PK_LEN],
}

impl AccountKey {
    pub fn empty() -> Self {
        AccountKey {
            ed25519_public_key: [0u8; ED25519_PK_LEN],
        }
    }

    pub fn is_empty(&self) -> bool {
        self.ed25519_public_key == [0u8; ED25519_PK_LEN]
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    pub account_number: u64,
    pub account_info: AccountInfo,
    pub balance: u64,
    pub updated_on_block_passive_mode: u64,
    pub updated_on_block_active_mode: u64,
    pub n_operation: u64,
    pub name: Option<String>,
    pub account_type: u16,
    pub account_data: Vec<u8>,
    pub account_seal: Vec<u8>,
}

#[derive(Debug, Error)]
pub enum AccountError {
    #[error("insufficient balance: have {balance}, need {required}")]
    InsufficientBalance { balance: u64, required: u64 },
    #[error("arithmetic overflow in balance operation")]
    BalanceOverflow,
    #[error("invalid serialized data: {0}")]
    InvalidSerialization(String),
    #[error("account not found: {0}")]
    AccountNotFound(u64),
    #[error("name too long: {0} bytes (max 64)")]
    NameTooLong(usize),
    #[error("account data too long: {len} bytes (max {max})")]
    AccountDataTooLong { len: usize, max: usize },
    #[error("seal too long: {0} bytes (max 32)")]
    SealTooLong(usize),
}

fn write_u64(buf: &mut Vec<u8>, v: u64) {
    buf.extend_from_slice(&v.to_be_bytes());
}

fn read_u64(data: &[u8], pos: &mut usize) -> Result<u64, AccountError> {
    if *pos + 8 > data.len() {
        return Err(AccountError::InvalidSerialization("unexpected EOF".into()));
    }
    let bytes: [u8; 8] = data[*pos..*pos + 8].try_into().unwrap();
    *pos += 8;
    Ok(u64::from_be_bytes(bytes))
}

fn write_u16(buf: &mut Vec<u8>, v: u16) {
    buf.extend_from_slice(&v.to_be_bytes());
}

fn read_u16(data: &[u8], pos: &mut usize) -> Result<u16, AccountError> {
    if *pos + 2 > data.len() {
        return Err(AccountError::InvalidSerialization("unexpected EOF".into()));
    }
    let bytes: [u8; 2] = data[*pos..*pos + 2].try_into().unwrap();
    *pos += 2;
    Ok(u16::from_be_bytes(bytes))
}

fn write_fixed(buf: &mut Vec<u8>, bytes: &[u8]) {
    buf.extend_from_slice(bytes);
}

fn read_fixed<const N: usize>(data: &[u8], pos: &mut usize) -> Result<[u8; N], AccountError> {
    if *pos + N > data.len() {
        return Err(AccountError::InvalidSerialization("unexpected EOF".into()));
    }
    let mut arr = [0u8; N];
    arr.copy_from_slice(&data[*pos..*pos + N]);
    *pos += N;
    Ok(arr)
}

fn write_prefixed_bytes(buf: &mut Vec<u8>, bytes: &[u8]) {
    buf.extend_from_slice(&(bytes.len() as u16).to_be_bytes());
    buf.extend_from_slice(bytes);
}

fn read_prefixed_bytes(data: &[u8], pos: &mut usize) -> Result<Vec<u8>, AccountError> {
    let len = read_u16(data, pos)? as usize;
    if *pos + len > data.len() {
        return Err(AccountError::InvalidSerialization("unexpected EOF".into()));
    }
    let bytes = data[*pos..*pos + len].to_vec();
    *pos += len;
    Ok(bytes)
}

fn write_optional_str(buf: &mut Vec<u8>, s: &Option<String>) {
    match s {
        Some(name) => {
            let bytes = name.as_bytes();
            write_u16(buf, bytes.len() as u16);
            buf.extend_from_slice(bytes);
        }
        None => {
            write_u16(buf, 0xFFFF);
        }
    }
}

fn read_optional_str(data: &[u8], pos: &mut usize) -> Result<Option<String>, AccountError> {
    let len = read_u16(data, pos)? as usize;
    if len == 0xFFFF {
        return Ok(None);
    }
    if *pos + len > data.len() {
        return Err(AccountError::InvalidSerialization("unexpected EOF".into()));
    }
    let name = String::from_utf8(data[*pos..*pos + len].to_vec())
        .map_err(|_| AccountError::InvalidSerialization("invalid UTF-8 in name".into()))?;
    *pos += len;
    Ok(Some(name))
}

impl AccountInfo {
    #[allow(clippy::should_implement_trait)]
    pub fn default() -> Self {
        AccountInfo {
            state: AccountState::Normal,
            account_key: AccountKey::empty(),
            locked_until_block: 0,
            price: 0,
            account_to_pay: 0,
            new_public_key: None,
            hashed_secret: [0u8; 32],
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.push(self.state.to_u8());
        write_fixed(&mut buf, &self.account_key.ed25519_public_key);
        write_u64(&mut buf, self.locked_until_block);
        write_u64(&mut buf, self.price);
        write_u64(&mut buf, self.account_to_pay);
        match &self.new_public_key {
            Some(pk) => {
                buf.push(1);
                write_fixed(&mut buf, &pk.ed25519_public_key);
            }
            None => {
                buf.push(0);
            }
        }
        write_fixed(&mut buf, &self.hashed_secret);
        buf
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self, AccountError> {
        let mut pos = 0;
        if pos >= data.len() {
            return Err(AccountError::InvalidSerialization("unexpected EOF".into()));
        }
        let state = AccountState::from_u8(data[pos])
            .ok_or_else(|| AccountError::InvalidSerialization("invalid account state".into()))?;
        pos += 1;
        let ed25519_public_key = read_fixed(data, &mut pos)?;
        let locked_until_block = read_u64(data, &mut pos)?;
        let price = read_u64(data, &mut pos)?;
        let account_to_pay = read_u64(data, &mut pos)?;
        if pos >= data.len() {
            return Err(AccountError::InvalidSerialization("unexpected EOF".into()));
        }
        let has_new_key = data[pos];
        pos += 1;
        let new_public_key = if has_new_key == 1 {
            let ed = read_fixed(data, &mut pos)?;
            Some(AccountKey {
                ed25519_public_key: ed,
            })
        } else {
            None
        };
        let hashed_secret = read_fixed(data, &mut pos)?;

        Ok(AccountInfo {
            state,
            account_key: AccountKey { ed25519_public_key },
            locked_until_block,
            price,
            account_to_pay,
            new_public_key,
            hashed_secret,
        })
    }
}

impl Account {
    pub fn new(
        account_number: u64,
        ed25519_public_key: [u8; ED25519_PK_LEN],
        updated_on_block: u64,
    ) -> Self {
        Account {
            account_number,
            account_info: AccountInfo {
                state: AccountState::Normal,
                account_key: AccountKey { ed25519_public_key },
                locked_until_block: 0,
                price: 0,
                account_to_pay: 0,
                new_public_key: None,
                hashed_secret: [0u8; 32],
            },
            balance: 0,
            updated_on_block_passive_mode: updated_on_block,
            updated_on_block_active_mode: updated_on_block,
            n_operation: 0,
            name: None,
            account_type: 0,
            account_data: Vec::new(),
            account_seal: Vec::new(),
        }
    }

    pub fn add_balance(&mut self, amount: u64) -> Result<(), AccountError> {
        self.balance = self
            .balance
            .checked_add(amount)
            .ok_or(AccountError::BalanceOverflow)?;
        Ok(())
    }

    pub fn subtract_balance(&mut self, amount: u64) -> Result<(), AccountError> {
        self.balance =
            self.balance
                .checked_sub(amount)
                .ok_or(AccountError::InsufficientBalance {
                    balance: self.balance,
                    required: amount,
                })?;
        Ok(())
    }

    pub fn increment_n_operation(&mut self) {
        self.n_operation = self.n_operation.saturating_add(1);
    }

    pub fn is_locked(&self, current_block: u64) -> bool {
        self.account_info.locked_until_block >= current_block
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        write_u64(&mut buf, self.account_number);
        buf.extend_from_slice(&self.account_info.to_bytes());
        write_u64(&mut buf, self.balance);
        write_u64(&mut buf, self.updated_on_block_passive_mode);
        write_u64(&mut buf, self.updated_on_block_active_mode);
        write_u64(&mut buf, self.n_operation);
        write_optional_str(&mut buf, &self.name);
        write_u16(&mut buf, self.account_type);
        write_prefixed_bytes(&mut buf, &self.account_data);
        write_prefixed_bytes(&mut buf, &self.account_seal);
        buf
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self, AccountError> {
        let mut pos = 0;
        let account_number = read_u64(data, &mut pos)?;

        if pos >= data.len() {
            return Err(AccountError::InvalidSerialization("unexpected EOF".into()));
        }
        let state = AccountState::from_u8(data[pos])
            .ok_or_else(|| AccountError::InvalidSerialization("invalid account state".into()))?;
        pos += 1;
        let ed25519_public_key = read_fixed(data, &mut pos)?;
        let locked_until_block = read_u64(data, &mut pos)?;
        let price = read_u64(data, &mut pos)?;
        let account_to_pay = read_u64(data, &mut pos)?;

        if pos >= data.len() {
            return Err(AccountError::InvalidSerialization("unexpected EOF".into()));
        }
        let has_new_key = data[pos];
        pos += 1;
        let new_public_key = if has_new_key == 1 {
            let ed = read_fixed(data, &mut pos)?;
            Some(AccountKey {
                ed25519_public_key: ed,
            })
        } else {
            None
        };

        let hashed_secret = read_fixed(data, &mut pos)?;

        let account_info = AccountInfo {
            state,
            account_key: AccountKey { ed25519_public_key },
            locked_until_block,
            price,
            account_to_pay,
            new_public_key,
            hashed_secret,
        };

        let balance = read_u64(data, &mut pos)?;
        let updated_on_block_passive_mode = read_u64(data, &mut pos)?;
        let updated_on_block_active_mode = read_u64(data, &mut pos)?;
        let n_operation = read_u64(data, &mut pos)?;
        let name = read_optional_str(data, &mut pos)?;
        let account_type = read_u16(data, &mut pos)?;
        let account_data = read_prefixed_bytes(data, &mut pos)?;
        let account_seal = read_prefixed_bytes(data, &mut pos)?;

        if account_data.len() > CT_MAX_ACCOUNT_DATA {
            return Err(AccountError::AccountDataTooLong {
                len: account_data.len(),
                max: CT_MAX_ACCOUNT_DATA,
            });
        }
        if account_seal.len() > 32 {
            return Err(AccountError::SealTooLong(account_seal.len()));
        }

        Ok(Account {
            account_number,
            account_info,
            balance,
            updated_on_block_passive_mode,
            updated_on_block_active_mode,
            n_operation,
            name,
            account_type,
            account_data,
            account_seal,
        })
    }

    pub fn hash(&self) -> [u8; 64] {
        blake3_512(&self.to_bytes())
    }

    /// Compute a new account seal = first 32 bytes of blake3_512(previous_seal || operation_hash).
    /// This creates a cryptographically verifiable chain of all account state changes,
    /// enabling Layer-2 protocols to audit the full history without the blockchain.
    pub fn compute_account_seal(previous_seal: &[u8], operation_hash: &[u8]) -> Vec<u8> {
        let mut input = Vec::with_capacity(previous_seal.len() + operation_hash.len());
        input.extend_from_slice(previous_seal);
        input.extend_from_slice(operation_hash);
        blake3_512(&input)[..32].to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_key(b: u8) -> AccountKey {
        let mut ed = [0u8; ED25519_PK_LEN];
        ed[0] = b;
        AccountKey {
            ed25519_public_key: ed,
        }
    }

    fn test_account() -> Account {
        Account::new(1, make_key(1).ed25519_public_key, 100)
    }

    #[test]
    fn create_account() {
        let account = test_account();
        assert_eq!(account.account_number, 1);
        assert_eq!(account.balance, 0);
        assert_eq!(account.n_operation, 0);
        assert_eq!(account.account_info.state, AccountState::Normal);
    }

    #[test]
    fn add_subtract_balance() {
        let mut account = test_account();
        account.add_balance(1000).unwrap();
        assert_eq!(account.balance, 1000);
        account.subtract_balance(400).unwrap();
        assert_eq!(account.balance, 600);
        assert!(account.subtract_balance(700).is_err());
    }

    #[test]
    fn increment_n_operation() {
        let mut account = test_account();
        account.increment_n_operation();
        assert_eq!(account.n_operation, 1);
    }

    #[test]
    fn locked_account() {
        let mut account = test_account();
        account.account_info.locked_until_block = 500;
        assert!(account.is_locked(100));
        assert!(account.is_locked(500));
        assert!(!account.is_locked(501));
    }

    #[test]
    fn serialize_deserialize_roundtrip() {
        let mut account = test_account();
        account.add_balance(5000).unwrap();
        account.increment_n_operation();
        account.name = Some("teste.auge".to_string());
        account.account_type = 42;
        account.account_data = vec![1, 2, 3, 4];
        account.account_seal = vec![5, 6, 7, 8];
        account.account_info.state = AccountState::ForSale;
        account.account_info.price = 999;
        account.account_info.account_to_pay = 10;
        account.account_info.new_public_key = Some(make_key(2));

        let bytes = account.to_bytes();
        let restored = Account::from_bytes(&bytes).unwrap();
        assert_eq!(account, restored);
    }

    #[test]
    fn serialize_deserialize_minimal() {
        let account = test_account();
        let bytes = account.to_bytes();
        let restored = Account::from_bytes(&bytes).unwrap();
        assert_eq!(account, restored);
    }

    #[test]
    fn hash_is_deterministic() {
        let account = test_account();
        assert_eq!(account.hash(), account.hash());
    }

    #[test]
    fn hash_changes_with_balance() {
        let mut account = test_account();
        let h1 = account.hash();
        account.add_balance(100).unwrap();
        assert_ne!(h1, account.hash());
    }

    #[test]
    fn invalid_account_state_rejected() {
        let mut bytes = test_account().to_bytes();
        bytes[8] = 99;
        let result = Account::from_bytes(&bytes);
        assert!(result.is_err());
    }

    #[test]
    fn account_seal_chains_operations() {
        let seal0: Vec<u8> = vec![];
        let op1 = [1u8; 64];
        let op2 = [2u8; 64];

        let seal1 = Account::compute_account_seal(&seal0, &op1);
        let seal2 = Account::compute_account_seal(&seal1, &op2);

        let seal2_alt = Account::compute_account_seal(&seal0, &op2);
        assert_ne!(seal2, seal2_alt);

        let seal2_again = Account::compute_account_seal(&seal1, &op2);
        assert_eq!(seal2, seal2_again);

        assert_eq!(seal1.len(), 32);
        assert_eq!(seal2.len(), 32);
    }

    #[test]
    fn account_seal_different_operation_produces_different_seal() {
        let seal_prev: Vec<u8> = vec![1, 2, 3, 4];
        let op_a = [0xAA; 64];
        let op_b = [0xBB; 64];

        let seal_a = Account::compute_account_seal(&seal_prev, &op_a);
        let seal_b = Account::compute_account_seal(&seal_prev, &op_b);

        assert_ne!(seal_a, seal_b);
    }
}
