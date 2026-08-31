use crate::{Storage, CF_ACCOUNTS};
use augecoin_core::account::Account;
use rocksdb::IteratorMode;
use std::collections::HashMap;

const CHECKPOINT_VERSION: u8 = 1;

#[derive(Debug)]
pub struct AccountSnapshot {
    height: u64,
    accounts: HashMap<u64, Account>,
}

impl AccountSnapshot {
    pub fn height(&self) -> u64 {
        self.height
    }
    pub fn len(&self) -> usize {
        self.accounts.len()
    }
    pub fn is_empty(&self) -> bool {
        self.accounts.is_empty()
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.push(CHECKPOINT_VERSION);
        buf.extend_from_slice(&self.height.to_be_bytes());
        buf.extend_from_slice(&(self.accounts.len() as u32).to_be_bytes());

        let mut sorted_keys: Vec<u64> = self.accounts.keys().copied().collect();
        sorted_keys.sort();

        for key in sorted_keys {
            let account = &self.accounts[&key];
            buf.extend_from_slice(&key.to_be_bytes());
            let acc_bytes = account.to_bytes();
            buf.extend_from_slice(&(acc_bytes.len() as u32).to_be_bytes());
            buf.extend_from_slice(&acc_bytes);
        }
        buf
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self, String> {
        let mut pos = 0;
        if pos >= data.len() || data[pos] != CHECKPOINT_VERSION {
            return Err("invalid checkpoint version".into());
        }
        pos += 1;

        if pos + 8 > data.len() {
            return Err("unexpected EOF".into());
        }
        let height = u64::from_be_bytes(data[pos..pos + 8].try_into().unwrap());
        pos += 8;

        if pos + 4 > data.len() {
            return Err("unexpected EOF".into());
        }
        let account_count = u32::from_be_bytes(data[pos..pos + 4].try_into().unwrap()) as usize;
        pos += 4;

        let mut accounts = HashMap::with_capacity(account_count);
        for _ in 0..account_count {
            if pos + 8 > data.len() {
                return Err("unexpected EOF".into());
            }
            let account_number = u64::from_be_bytes(data[pos..pos + 8].try_into().unwrap());
            pos += 8;

            if pos + 4 > data.len() {
                return Err("unexpected EOF".into());
            }
            let acc_len = u32::from_be_bytes(data[pos..pos + 4].try_into().unwrap()) as usize;
            pos += 4;

            if pos + acc_len > data.len() {
                return Err("unexpected EOF".into());
            }
            let account = Account::from_bytes(&data[pos..pos + acc_len])
                .map_err(|e| format!("invalid account in checkpoint: {e}"))?;
            pos += acc_len;

            accounts.insert(account_number, account);
        }
        Ok(AccountSnapshot { height, accounts })
    }
}

impl Storage {
    pub fn capture_snapshot(&self, height: u64) -> AccountSnapshot {
        let cf = self
            .db
            .cf_handle(CF_ACCOUNTS)
            .expect("accounts column family must exist");
        let mut accounts = HashMap::new();
        let iter = self.db.iterator_cf(&cf, IteratorMode::Start);
        for item in iter {
            let (key, value) = item.expect("rocksdb iteration error");
            let account_number = u64::from_be_bytes(key.as_ref().try_into().unwrap_or([0u8; 8]));
            if let Ok(account) = Account::from_bytes(&value) {
                accounts.insert(account_number, account);
            }
        }
        AccountSnapshot { height, accounts }
    }

    pub fn restore_snapshot(&self, snapshot: &AccountSnapshot) -> Result<(), crate::StorageError> {
        let cf = self
            .db
            .cf_handle(CF_ACCOUNTS)
            .expect("accounts column family must exist");
        let cf_address = self
            .db
            .cf_handle(crate::CF_ADDRESS_INDEX)
            .expect("address index column family must exist");

        let existing_keys: Vec<u64> = {
            let iter = self.db.iterator_cf(&cf, IteratorMode::Start);
            iter.map(|item| {
                let (key, _) = item.expect("rocksdb iteration error");
                u64::from_be_bytes(key.as_ref().try_into().unwrap_or([0u8; 8]))
            })
            .collect()
        };

        let existing_addresses: Vec<Vec<u8>> = self
            .db
            .iterator_cf(&cf_address, IteratorMode::Start)
            .map(|item| item.map(|(key, _)| key.to_vec()))
            .collect::<Result<_, _>>()
            .map_err(|e| crate::StorageError::Database(e.to_string()))?;

        let mut batch = rocksdb::WriteBatch::default();
        for key in existing_keys {
            batch.delete_cf(&cf, key.to_be_bytes());
        }
        for key in existing_addresses {
            batch.delete_cf(&cf_address, key);
        }
        for account in snapshot.accounts.values() {
            batch.put_cf(
                &cf,
                account.account_number.to_be_bytes(),
                account.to_bytes(),
            );
            if let Ok(public_key) = ed25519_dalek::VerifyingKey::from_bytes(
                &account.account_info.account_key.ed25519_public_key,
            ) {
                let address =
                    augecoin_crypto::address::AddressHash::from_public_key(public_key.to_bytes());
                batch.put_cf(
                    &cf_address,
                    address.hash,
                    account.account_number.to_be_bytes(),
                );
            }
        }
        let cf_validator = self
            .db
            .cf_handle(crate::CF_VALIDATOR_SET)
            .expect("validator set column family must exist");
        batch.put_cf(&cf_validator, b"height", snapshot.height.to_be_bytes());
        self.db
            .write(batch)
            .map_err(|e| crate::StorageError::Database(e.to_string()))?;

        // All resident indices derive from accounts; discard them only after
        // the replacement transaction is durable.
        let mut cache = self
            .safebox_cache
            .lock()
            .map_err(|_| crate::StorageError::Database("safebox cache lock poisoned".into()))?;
        cache.safebox = None;
        cache.pubkey_index.clear();
        cache.address_index.clear();
        cache.sale_index.clear();
        cache.gift_index.clear();
        cache.dirty.clear();
        cache.blocks_since_snapshot = 0;
        Ok(())
    }

    pub fn rollback_to_account_state(
        &self,
        previous: &HashMap<u64, Account>,
    ) -> Result<(), crate::StorageError> {
        for account in previous.values() {
            self.put_account(account)?;
        }
        Ok(())
    }

    pub fn verify_checksum(&self) -> [u8; 64] {
        let cf = self
            .db
            .cf_handle(CF_ACCOUNTS)
            .expect("accounts column family must exist");
        let mut combined = Vec::new();
        let iter = self.db.iterator_cf(&cf, IteratorMode::Start);
        for item in iter {
            let (key, value) = item.expect("rocksdb iteration error");
            combined.extend_from_slice(&key);
            combined.extend_from_slice(&value);
        }
        augecoin_core::hash::blake3_512(&combined)
    }

    pub fn create_checkpoint(&self, height: u64) -> AccountSnapshot {
        self.capture_snapshot(height)
    }

    pub fn restore_from_checkpoint(
        &self,
        checkpoint: &AccountSnapshot,
    ) -> Result<(), crate::StorageError> {
        self.restore_snapshot(checkpoint)
    }
}

#[cfg(test)]
mod checkpoint_tests {
    use super::*;
    use crate::Storage;
    use augecoin_core::account::{Account, AccountInfo, AccountKey};
    use std::sync::atomic::{AtomicU32, Ordering};

    static COUNTER: AtomicU32 = AtomicU32::new(3000);

    fn temp_path() -> String {
        format!(
            "/tmp/augecoin-ck-{}",
            COUNTER.fetch_add(1, Ordering::SeqCst)
        )
    }

    fn make_account(number: u64, balance: u64) -> Account {
        let mut ed = [0u8; 32];
        ed[0..8].copy_from_slice(&number.to_be_bytes());
        Account {
            account_number: number,
            account_info: AccountInfo {
                account_key: AccountKey {
                    ed25519_public_key: ed,
                },
                ..AccountInfo::default()
            },
            balance,
            updated_on_block_passive_mode: 100,
            updated_on_block_active_mode: 100,
            n_operation: 0,
            name: None,
            account_type: 0,
            account_data: vec![],
            account_seal: vec![],
        }
    }

    #[test]
    fn checkpoint_roundtrip_bytes() {
        let mut accounts = HashMap::new();
        accounts.insert(1, make_account(1, 100));
        accounts.insert(2, make_account(2, 200));
        let snapshot = AccountSnapshot {
            height: 42,
            accounts,
        };
        let bytes = snapshot.to_bytes();
        let restored = AccountSnapshot::from_bytes(&bytes).unwrap();
        assert_eq!(restored.height, 42);
        assert_eq!(restored.accounts.len(), 2);
        assert_eq!(restored.accounts[&1].balance, 100);
    }

    #[test]
    fn create_and_restore_checkpoint_reproduces_state() {
        let path = temp_path();
        let storage = Storage::open(&path).unwrap();
        for i in 0..5 {
            storage.put_account(&make_account(i, i * 50)).unwrap();
        }
        let checkpoint = storage.create_checkpoint(10);
        assert_eq!(checkpoint.len(), 5);

        for i in 0..5 {
            let mut acc = make_account(i, 999);
            acc.balance = 999;
            storage.put_account(&acc).unwrap();
        }

        storage.restore_from_checkpoint(&checkpoint).unwrap();
        for i in 0..5 {
            let acc = storage.get_account(i).unwrap().unwrap();
            assert_eq!(acc.balance, i * 50);
        }
        std::fs::remove_dir_all(&path).ok();
    }

    #[test]
    fn restore_nonexistent_checkpoint_fails() {
        assert!(AccountSnapshot::from_bytes(&[99u8; 10]).is_err());
    }

    #[test]
    fn rollback_restores_state() {
        let path = temp_path();
        let storage = Storage::open(&path).unwrap();
        let acc = make_account(1, 100);
        storage.put_account(&acc).unwrap();
        let mut previous = HashMap::new();
        previous.insert(1, acc.clone());

        let mut modified = acc.clone();
        modified.balance = 600;
        storage.put_account(&modified).unwrap();
        storage.rollback_to_account_state(&previous).unwrap();
        assert_eq!(storage.get_account(1).unwrap().unwrap().balance, 100);
        std::fs::remove_dir_all(&path).ok();
    }

    #[test]
    fn corruption_is_detectable() {
        let path = temp_path();
        let storage = Storage::open(&path).unwrap();
        storage.put_account(&make_account(1, 100)).unwrap();
        let c1 = storage.verify_checksum();
        storage.put_account(&make_account(2, 200)).unwrap();
        let c2 = storage.verify_checksum();
        assert_ne!(c1, c2);
        std::fs::remove_dir_all(&path).ok();
    }

    #[test]
    fn recovery_from_checkpoint_after_corruption() {
        let path = temp_path();
        let storage = Storage::open(&path).unwrap();
        storage.put_account(&make_account(1, 100)).unwrap();
        storage.put_account(&make_account(2, 200)).unwrap();
        let checkpoint = storage.create_checkpoint(5);

        storage.put_account(&make_account(3, 300)).unwrap();
        let mut acc = make_account(1, 999);
        acc.balance = 999;
        storage.put_account(&acc).unwrap();

        storage.restore_from_checkpoint(&checkpoint).unwrap();
        assert_eq!(storage.get_account(1).unwrap().unwrap().balance, 100);
        assert_eq!(storage.get_account(2).unwrap().unwrap().balance, 200);
        assert!(storage.get_account(3).unwrap().is_none());
        std::fs::remove_dir_all(&path).ok();
    }

    #[test]
    fn recover_state_after_corruption_mid_chain() {
        let path = temp_path();
        let storage = Storage::open(&path).unwrap();

        let initial_accounts: Vec<u64> = (0..6).collect();
        let mut initial_balances: std::collections::HashMap<u64, u64> =
            std::collections::HashMap::new();
        for &num in &initial_accounts {
            let balance = (num + 1) * 100;
            initial_balances.insert(num, balance);
            storage.put_account(&make_account(num, balance)).unwrap();
        }

        let checkpoint = storage.create_checkpoint(5);
        assert_eq!(checkpoint.height(), 5);
        assert_eq!(checkpoint.len(), 6);

        let later_accounts: Vec<u64> = (10..16).collect();
        for &num in &later_accounts {
            storage.put_account(&make_account(num, num * 50)).unwrap();
        }

        for &num in &initial_accounts {
            let mut acc = make_account(num, 0xFFF);
            acc.balance = 0xFFF;
            acc.n_operation = 99;
            storage.put_account(&acc).unwrap();
        }

        let cf = storage
            .db
            .cf_handle(CF_ACCOUNTS)
            .expect("accounts cf must exist");
        for &num in &later_accounts {
            let key_bytes = num.to_be_bytes();
            storage
                .db
                .delete_cf(&cf, key_bytes)
                .expect("delete should succeed");
            let mut key = Vec::from(key_bytes);
            key[0] ^= 0xFF;
            storage
                .db
                .put_cf(&cf, key, [0xDE, 0xAD, 0xBE, 0xEF])
                .expect("corrupt insert should succeed");
        }

        storage
            .restore_from_checkpoint(&checkpoint)
            .expect("restore should succeed");

        for &num in &initial_accounts {
            let acc = storage
                .get_account(num)
                .unwrap()
                .unwrap_or_else(|| panic!("account {num} must exist after restore"));
            let expected_balance = initial_balances[&num];
            assert_eq!(
                acc.balance, expected_balance,
                "account {num} balance should match checkpoint: expected {expected_balance}, got {}",
                acc.balance
            );
            assert_eq!(
                acc.n_operation, 0,
                "account {num} n_operation should be reset to checkpoint value"
            );
        }

        for &num in &later_accounts {
            assert!(
                storage.get_account(num).unwrap().is_none(),
                "account {num} from later height should not exist after restore"
            );
        }

        std::fs::remove_dir_all(&path).ok();
    }
}
