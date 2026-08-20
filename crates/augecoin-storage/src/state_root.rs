use augecoin_core::account::Account;
use augecoin_core::hash::blake3_512 as hash;
use rocksdb::{IteratorMode, DB};
use std::collections::BTreeMap;

const EMPTY_STATE_ROOT: [u8; 64] = [0u8; 64];

pub fn compute_state_root(db: &DB, cf_accounts: &rocksdb::ColumnFamilyRef) -> [u8; 64] {
    let mut accounts: BTreeMap<u64, Account> = BTreeMap::new();

    let iter = db.iterator_cf(cf_accounts, IteratorMode::Start);
    for item in iter {
        let (key, value) = item.expect("rocksdb iteration error");
        let account_number = u64::from_be_bytes(key.as_ref().try_into().unwrap_or([0u8; 8]));
        if let Ok(account) = Account::from_bytes(&value) {
            accounts.insert(account_number, account);
        }
    }

    if accounts.is_empty() {
        return EMPTY_STATE_ROOT;
    }

    let mut hashes: Vec<[u8; 64]> = accounts.values().map(|acc| acc.hash()).collect();

    while hashes.len() > 1 {
        if !hashes.len().is_multiple_of(2) {
            hashes.push(*hashes.last().unwrap());
        }
        let mut next_level = Vec::with_capacity(hashes.len() / 2);
        for chunk in hashes.chunks(2) {
            let mut combined = Vec::with_capacity(128);
            combined.extend_from_slice(&chunk[0]);
            combined.extend_from_slice(&chunk[1]);
            next_level.push(hash(&combined));
        }
        hashes = next_level;
    }

    hashes[0]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Storage, CF_ACCOUNTS};
    use augecoin_core::account::{Account, AccountInfo, AccountKey};

    fn test_account(number: u64, balance: u64) -> Account {
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

    fn temp_path() -> String {
        use std::sync::atomic::{AtomicU32, Ordering};
        static C: AtomicU32 = AtomicU32::new(1000);
        format!(
            "/tmp/augecoin-state-root-{}",
            C.fetch_add(1, Ordering::SeqCst)
        )
    }

    #[test]
    fn state_root_is_deterministic() {
        let path = temp_path();
        let storage = Storage::open(&path).unwrap();
        for i in 0..5 {
            storage.put_account(&test_account(i, i * 10)).unwrap();
        }
        let cf = storage.db.cf_handle(CF_ACCOUNTS).unwrap();
        let root1 = compute_state_root(&storage.db, &cf);
        let root2 = compute_state_root(&storage.db, &cf);
        assert_eq!(root1, root2);
        std::fs::remove_dir_all(&path).ok();
    }

    #[test]
    fn state_root_changes_when_account_changes() {
        let path = temp_path();
        let storage = Storage::open(&path).unwrap();
        for i in 0..3 {
            storage.put_account(&test_account(i, i * 10)).unwrap();
        }
        let cf = storage.db.cf_handle(CF_ACCOUNTS).unwrap();
        let root1 = compute_state_root(&storage.db, &cf);
        storage.put_account(&test_account(1, 100)).unwrap();
        let root2 = compute_state_root(&storage.db, &cf);
        assert_ne!(root1, root2);
        std::fs::remove_dir_all(&path).ok();
    }

    #[test]
    fn empty_state_root_is_well_defined() {
        let path = temp_path();
        let storage = Storage::open(&path).unwrap();
        let cf = storage.db.cf_handle(CF_ACCOUNTS).unwrap();
        assert_eq!(compute_state_root(&storage.db, &cf), EMPTY_STATE_ROOT);
        std::fs::remove_dir_all(&path).ok();
    }

    #[test]
    fn single_account_state_root() {
        let path = temp_path();
        let storage = Storage::open(&path).unwrap();
        storage.put_account(&test_account(0, 50)).unwrap();
        let cf = storage.db.cf_handle(CF_ACCOUNTS).unwrap();
        let root = compute_state_root(&storage.db, &cf);
        assert_ne!(root, EMPTY_STATE_ROOT);
        std::fs::remove_dir_all(&path).ok();
    }
}
