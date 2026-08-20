pub mod checkpoint;
pub mod state_root;

use augecoin_core::account::Account;
use augecoin_core::block::OperationBlock;
use augecoin_core::safe_box::SafeBox;
use rocksdb::{ColumnFamilyDescriptor, DBCompressionType, Options, WriteBatch, DB};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use thiserror::Error;

pub(crate) const CF_ACCOUNTS: &str = "accounts";
const CF_BLOCKS: &str = "blocks";
const CF_SAFEBOX: &str = "safebox";
const CF_OP_INDEX: &str = "op_index";
const CF_VALIDATOR_SET: &str = "validator_set";
const CF_EQUIVOCATION_PROOFS: &str = "equivocation_proofs";
const CF_FAUCET_CLAIMS: &str = "faucet_claims";

const ALL_COLUMN_FAMILIES: &[&str] = &[
    CF_ACCOUNTS,
    CF_BLOCKS,
    CF_SAFEBOX,
    CF_OP_INDEX,
    CF_VALIDATOR_SET,
    CF_EQUIVOCATION_PROOFS,
    CF_FAUCET_CLAIMS,
];

// RocksDB tuning — see DECISIONS: disk growth was dominated by full SafeBox
// rewrites every block plus WAL-driven flushes. These options reduce write
// amplification and bound the info log.
const WAL_SIZE: u64 = 1024 * 1024 * 1024; // 1 GiB max total WAL
const MAX_LOG_FILE_SIZE: usize = 64 * 1024 * 1024; // rotate info LOG at 64 MiB
const KEEP_LOG_FILE_NUM: usize = 10; // keep at most 10 info LOG files
const LOG_FILE_TIME_TO_ROLL: usize = 86400; // seconds (1 day)
const WRITE_BUFFER_SIZE: usize = 64 * 1024 * 1024; // 64 MiB memtable
const MAX_WRITE_BUFFER_NUMBER: i32 = 3;
const MIN_WRITE_BUFFER_NUMBER_TO_MERGE: i32 = 1;
const TARGET_FILE_SIZE_BASE: u64 = 64 * 1024 * 1024; // 64 MiB SST target
const MAX_BYTES_FOR_LEVEL_BASE: u64 = 512 * 1024 * 1024; // 512 MiB L1

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("database error: {0}")]
    Database(String),
    #[error("serialization error: {0}")]
    Serialization(String),
}

/// Resident in-memory SafeBox cache plus the dirty account set.
///
/// The SafeBox is consensus-critical (its Merkle hash is committed into each
/// block header), but its *serialization* to RocksDB is only a cache: the same
/// state is reconstructible from the `accounts` column family. This cache keeps
/// the full SafeBox alive in memory and persists it only periodically, so a
/// block commit touches only the modified accounts instead of rewriting ~76 MB.
struct SafeboxCache {
    /// The resident SafeBox; `None` until first load.
    safebox: Option<SafeBox>,
    /// Resident index of in-use Ed25519 public keys → number of accounts
    /// currently using each key. Derived from the SafeBox accounts; used by
    /// `CreateAccount` to detect duplicate keys without an O(n) account scan.
    /// Ref-counted because multiple accounts can share a key (auto-created
    /// accounts) and `ChangeKey` moves a key between accounts.
    pubkey_index: HashMap<[u8; 32], u64>,
    /// AUGEID → sale listing, for accounts currently in `ForSale` state.
    sale_index: HashMap<u64, SaleListing>,
    /// AUGEID → pending gift, for accounts currently in `GiftPending` state.
    gift_index: HashMap<u64, GiftListing>,
    /// Account numbers modified since the last snapshot.
    dirty: std::collections::HashSet<u64>,
    /// Blocks committed since the last snapshot.
    blocks_since_snapshot: u64,
    /// Snapshot interval (blocks). 0 means snapshot on every commit.
    snapshot_interval: u64,
    /// Total full snapshots persisted.
    snapshot_total: u64,
    /// Bytes of the last persisted snapshot.
    last_snapshot_bytes: u64,
}

/// A marketplace listing for a single AUGEID.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaleListing {
    pub account_number: u64,
    pub seller_public_key: [u8; 32],
    pub price: u64,
    pub listed_at_block: u64,
}

/// A pending AUGEID gift awaiting the recipient's accept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GiftListing {
    pub account_number: u64,
    pub from_account_number: u64,
    pub recipient_public_key: [u8; 32],
    pub gifted_at_block: u64,
}

impl SafeboxCache {
    fn new() -> Self {
        SafeboxCache {
            safebox: None,
            pubkey_index: HashMap::new(),
            sale_index: HashMap::new(),
            gift_index: HashMap::new(),
            dirty: std::collections::HashSet::new(),
            blocks_since_snapshot: 0,
            snapshot_interval: 0,
            snapshot_total: 0,
            last_snapshot_bytes: 0,
        }
    }

    fn bump_pubkey(&mut self, pubkey: &[u8; 32], delta: i64) {
        let entry = self.pubkey_index.entry(*pubkey).or_insert(0);
        let next = (*entry as i64).saturating_add(delta).max(0) as u64;
        if next == 0 {
            self.pubkey_index.remove(pubkey);
        } else {
            *entry = next;
        }
    }

    /// Refresh the `sale_index`/`gift_index` entries for a single account based
    /// on its current state. Called on load and after every account mutation so
    /// the derived indices stay O(1) without ever scanning the whole SafeBox.
    fn sync_derived_indices(&mut self, account: &Account, block: u64) {
        let num = account.account_number;
        match account.account_info.state {
            augecoin_core::account::AccountState::ForSale => {
                self.sale_index.insert(
                    num,
                    SaleListing {
                        account_number: num,
                        seller_public_key: account.account_info.account_key.ed25519_public_key,
                        price: account.account_info.price,
                        listed_at_block: block,
                    },
                );
            }
            _ => {
                self.sale_index.remove(&num);
            }
        }

        match account.account_info.state {
            augecoin_core::account::AccountState::GiftPending => {
                if let Some(recipient) = &account.account_info.new_public_key {
                    self.gift_index.insert(
                        num,
                        GiftListing {
                            account_number: num,
                            from_account_number: 0,
                            recipient_public_key: recipient.ed25519_public_key,
                            gifted_at_block: block,
                        },
                    );
                }
            }
            _ => {
                self.gift_index.remove(&num);
            }
        }
    }
}

pub struct Storage {
    pub(crate) db: DB,
    path: PathBuf,
    safebox_cache: Mutex<SafeboxCache>,
}

impl Storage {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, StorageError> {
        let path_buf = path.as_ref().to_path_buf();
        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.create_missing_column_families(true);
        opts.set_max_total_wal_size(WAL_SIZE);
        opts.set_max_log_file_size(MAX_LOG_FILE_SIZE);
        opts.set_keep_log_file_num(KEEP_LOG_FILE_NUM);
        opts.set_log_file_time_to_roll(LOG_FILE_TIME_TO_ROLL);
        opts.set_max_background_jobs(4);

        let cf_descriptors: Vec<ColumnFamilyDescriptor> = ALL_COLUMN_FAMILIES
            .iter()
            .map(|name| ColumnFamilyDescriptor::new(*name, tuned_cf_options()))
            .collect();

        let db = DB::open_cf_descriptors(&opts, path, cf_descriptors)
            .map_err(|e| StorageError::Database(e.to_string()))?;

        Ok(Storage {
            db,
            path: path_buf,
            safebox_cache: Mutex::new(SafeboxCache::new()),
        })
    }

    /// Configure the SafeBox snapshot interval (blocks between full snapshots).
    pub fn set_safebox_snapshot_interval(&self, interval: u64) {
        if let Ok(mut cache) = self.safebox_cache.lock() {
            cache.snapshot_interval = interval;
        }
    }

    /// Ensure the resident SafeBox is loaded, deriving it from the `accounts`
    /// column family when no snapshot exists yet.
    fn ensure_safebox_loaded(cache: &mut SafeboxCache, db: &DB) -> Result<(), StorageError> {
        if cache.safebox.is_some() {
            return Ok(());
        }
        let mut safebox = SafeBox::new(augecoin_core::constants::CT_BUILD_PROTOCOL, 0);
        let cf = db
            .cf_handle(CF_ACCOUNTS)
            .expect("accounts column family must exist");
        let iter = db.iterator_cf(&cf, rocksdb::IteratorMode::Start);
        for item in iter {
            let (_key, value) = item.map_err(|e| StorageError::Database(e.to_string()))?;
            if let Ok(account) = Account::from_bytes(&value) {
                cache.bump_pubkey(&account.account_info.account_key.ed25519_public_key, 1);
                cache.sync_derived_indices(&account, 0);
                safebox.add_account(account);
            }
        }
        safebox.update_hash();
        cache.safebox = Some(safebox);
        Ok(())
    }

    /// Return a clone of the current resident SafeBox (loading it if needed).
    pub fn safebox(&self) -> Result<SafeBox, StorageError> {
        let mut cache = self
            .safebox_cache
            .lock()
            .map_err(|_| StorageError::Database("safebox cache lock poisoned".into()))?;
        Self::ensure_safebox_loaded(&mut cache, &self.db)?;
        Ok(cache.safebox.as_ref().expect("safebox loaded").clone())
    }

    /// Return the current SafeBox hash (Merkle root) without touching RocksDB.
    pub fn safebox_hash(&self) -> Result<[u8; 64], StorageError> {
        let mut cache = self
            .safebox_cache
            .lock()
            .map_err(|_| StorageError::Database("safebox cache lock poisoned".into()))?;
        Self::ensure_safebox_loaded(&mut cache, &self.db)?;
        Ok(cache
            .safebox
            .as_ref()
            .expect("safebox loaded")
            .header
            .safe_box_hash)
    }

    /// Return the current name index (clone) for name-uniqueness validation.
    pub fn safebox_name_index(&self) -> Result<BTreeMap<String, u64>, StorageError> {
        let mut cache = self
            .safebox_cache
            .lock()
            .map_err(|_| StorageError::Database("safebox cache lock poisoned".into()))?;
        Self::ensure_safebox_loaded(&mut cache, &self.db)?;
        Ok(cache
            .safebox
            .as_ref()
            .expect("safebox loaded")
            .name_index
            .clone())
    }

    /// True if any committed account already owns this Ed25519 public key.
    /// Backed by the resident `pubkey_index`, so it is O(1) — no account scan.
    pub fn pubkey_in_use(&self, pubkey: &[u8; 32]) -> Result<bool, StorageError> {
        let mut cache = self
            .safebox_cache
            .lock()
            .map_err(|_| StorageError::Database("safebox cache lock poisoned".into()))?;
        Self::ensure_safebox_loaded(&mut cache, &self.db)?;
        Ok(cache.pubkey_index.contains_key(pubkey))
    }

    /// Highest committed account number, from the resident SafeBox (O(1)).
    pub fn max_account_number_resident(&self) -> Result<u64, StorageError> {
        let mut cache = self
            .safebox_cache
            .lock()
            .map_err(|_| StorageError::Database("safebox cache lock poisoned".into()))?;
        Self::ensure_safebox_loaded(&mut cache, &self.db)?;
        Ok(cache
            .safebox
            .as_ref()
            .expect("safebox loaded")
            .max_account_number())
    }

    /// Resolve a name (e.g. "CarlosPay") to an account number via the resident
    /// SafeBox name index. O(1).
    pub fn resolve_name(&self, name: &str) -> Result<Option<u64>, StorageError> {
        let mut cache = self
            .safebox_cache
            .lock()
            .map_err(|_| StorageError::Database("safebox cache lock poisoned".into()))?;
        Self::ensure_safebox_loaded(&mut cache, &self.db)?;
        Ok(cache
            .safebox
            .as_ref()
            .expect("safebox loaded")
            .is_name_taken(name))
    }

    /// All current sale listings, sorted by account number. O(1) index access.
    pub fn list_for_sale(&self) -> Result<Vec<SaleListing>, StorageError> {
        let cache = self
            .safebox_cache
            .lock()
            .map_err(|_| StorageError::Database("safebox cache lock poisoned".into()))?;
        let mut listings: Vec<SaleListing> = cache.sale_index.values().cloned().collect();
        listings.sort_by_key(|l| l.account_number);
        Ok(listings)
    }

    /// All pending gifts, sorted by account number. O(1) index access.
    pub fn list_pending_gifts(&self) -> Result<Vec<GiftListing>, StorageError> {
        let cache = self
            .safebox_cache
            .lock()
            .map_err(|_| StorageError::Database("safebox cache lock poisoned".into()))?;
        let mut gifts: Vec<GiftListing> = cache.gift_index.values().cloned().collect();
        gifts.sort_by_key(|g| g.account_number);
        Ok(gifts)
    }

    /// Apply a set of modified accounts to the resident SafeBox and return the
    /// new hash. This is the incremental commit path: it updates only the
    /// in-memory SafeBox and persists a full snapshot only when the snapshot
    /// interval has elapsed (or when `force_snapshot` is set).
    ///
    /// Callers MUST have already persisted the modified accounts via
    /// `put_account` before calling this, so the `accounts` CF stays in sync
    /// with the resident cache.
    pub fn commit_safebox_incremental(
        &self,
        modified: &HashMap<u64, Account>,
        force_snapshot: bool,
    ) -> Result<[u8; 64], StorageError> {
        let mut cache = self
            .safebox_cache
            .lock()
            .map_err(|_| StorageError::Database("safebox cache lock poisoned".into()))?;
        Self::ensure_safebox_loaded(&mut cache, &self.db)?;

        let mut dirty_nums: Vec<u64> = Vec::with_capacity(modified.len());
        {
            let old_pubkeys: Vec<Option<[u8; 32]>> = {
                let safebox = cache.safebox.as_ref().expect("safebox loaded");
                modified
                    .values()
                    .map(|account| {
                        safebox
                            .get_account(account.account_number)
                            .map(|old| old.account_info.account_key.ed25519_public_key)
                    })
                    .collect()
            };
            for (account, old_pk) in modified.values().zip(old_pubkeys.iter()) {
                if let Some(pk) = old_pk {
                    cache.bump_pubkey(pk, -1);
                }
                cache.bump_pubkey(&account.account_info.account_key.ed25519_public_key, 1);
                cache.sync_derived_indices(account, 0);
                dirty_nums.push(account.account_number);
            }
            let safebox = cache.safebox.as_mut().expect("safebox loaded");
            for account in modified.values() {
                safebox.add_account(account.clone());
            }
            safebox.header.end_block = safebox.header.start_block + 1;
            safebox.update_hash();
        }
        for num in dirty_nums {
            cache.dirty.insert(num);
        }

        let hash = cache
            .safebox
            .as_ref()
            .expect("safebox loaded")
            .header
            .safe_box_hash;

        cache.blocks_since_snapshot = cache.blocks_since_snapshot.saturating_add(1);
        let interval = cache.snapshot_interval;
        let should_snapshot =
            force_snapshot || interval == 0 || cache.blocks_since_snapshot >= interval;

        if should_snapshot {
            let snapshot_bytes = {
                let safebox = cache.safebox.as_ref().expect("safebox loaded");
                self.put_safe_box(safebox)?;
                safebox.to_bytes().len() as u64
            };
            cache.blocks_since_snapshot = 0;
            cache.snapshot_total = cache.snapshot_total.saturating_add(1);
            cache.last_snapshot_bytes = snapshot_bytes;
            cache.dirty.clear();
        }

        Ok(hash)
    }

    /// Number of accounts currently marked dirty (not yet snapshotted).
    pub fn dirty_accounts(&self) -> u64 {
        self.safebox_cache
            .lock()
            .map(|c| c.dirty.len() as u64)
            .unwrap_or(0)
    }

    /// Atomically commit a block: writes the modified accounts, the block
    /// header, the chain height, and an optional validator-set change into a
    /// single `WriteBatch`, so a crash cannot leave a partially-persisted block.
    /// Also updates the resident SafeBox + pubkey index and returns the new
    /// SafeBox hash. A full snapshot is written into the same batch only when
    /// the snapshot interval has elapsed.
    pub fn commit_block_atomic(
        &self,
        modified: &HashMap<u64, Account>,
        block: &OperationBlock,
        validator_set_bytes: Option<&[u8]>,
        force_snapshot: bool,
    ) -> Result<[u8; 64], StorageError> {
        let mut cache = self
            .safebox_cache
            .lock()
            .map_err(|_| StorageError::Database("safebox cache lock poisoned".into()))?;
        Self::ensure_safebox_loaded(&mut cache, &self.db)?;

        let mut batch = WriteBatch::default();
        let cf_accounts = self.cf(CF_ACCOUNTS);
        let cf_blocks = self.cf(CF_BLOCKS);
        let cf_validator = self.cf(CF_VALIDATOR_SET);

        // (1) Persist modified accounts + block + height atomically.
        let mut dirty_nums: Vec<u64> = Vec::with_capacity(modified.len());
        for account in modified.values() {
            batch.put_cf(
                &cf_accounts,
                account.account_number.to_be_bytes(),
                account.to_bytes(),
            );
            dirty_nums.push(account.account_number);
        }
        batch.put_cf(
            &cf_blocks,
            block.header.block_number.to_be_bytes(),
            block.to_bytes(),
        );
        batch.put_cf(
            &cf_validator,
            b"height",
            block.header.block_number.to_be_bytes(),
        );
        if let Some(vs) = validator_set_bytes {
            batch.put_cf(&cf_validator, b"vset", vs);
        }

        // (2) Update the resident SafeBox + pubkey index in memory.
        {
            let old_pubkeys: Vec<Option<[u8; 32]>> = {
                let safebox = cache.safebox.as_ref().expect("safebox loaded");
                modified
                    .values()
                    .map(|account| {
                        safebox
                            .get_account(account.account_number)
                            .map(|old| old.account_info.account_key.ed25519_public_key)
                    })
                    .collect()
            };
            for (account, old_pk) in modified.values().zip(old_pubkeys.iter()) {
                if let Some(pk) = old_pk {
                    cache.bump_pubkey(pk, -1);
                }
                cache.bump_pubkey(&account.account_info.account_key.ed25519_public_key, 1);
                cache.sync_derived_indices(account, block.header.block_number);
            }
            let safebox = cache.safebox.as_mut().expect("safebox loaded");
            for account in modified.values() {
                safebox.add_account(account.clone());
            }
            safebox.header.end_block = safebox.header.start_block + 1;
            safebox.update_hash();
        }
        for num in dirty_nums {
            cache.dirty.insert(num);
        }

        let hash = cache
            .safebox
            .as_ref()
            .expect("safebox loaded")
            .header
            .safe_box_hash;

        cache.blocks_since_snapshot = cache.blocks_since_snapshot.saturating_add(1);
        let interval = cache.snapshot_interval;
        let should_snapshot =
            force_snapshot || interval == 0 || cache.blocks_since_snapshot >= interval;

        // (3) Append the periodic snapshot to the same atomic batch.
        if should_snapshot {
            let cf_safebox = self.cf(CF_SAFEBOX);
            let safebox = cache.safebox.as_ref().expect("safebox loaded");
            batch.put_cf(&cf_safebox, b"current", safebox.to_bytes());
        }

        // (4) Single atomic write.
        self.db
            .write(batch)
            .map_err(|e| StorageError::Database(e.to_string()))?;

        if should_snapshot {
            cache.blocks_since_snapshot = 0;
            cache.snapshot_total = cache.snapshot_total.saturating_add(1);
            cache.last_snapshot_bytes = cache
                .safebox
                .as_ref()
                .expect("safebox loaded")
                .to_bytes()
                .len() as u64;
            cache.dirty.clear();
        }

        Ok(hash)
    }

    /// Total full snapshots persisted.
    pub fn snapshot_total(&self) -> u64 {
        self.safebox_cache
            .lock()
            .map(|c| c.snapshot_total)
            .unwrap_or(0)
    }

    /// Bytes of the last persisted snapshot.
    pub fn last_snapshot_bytes(&self) -> u64 {
        self.safebox_cache
            .lock()
            .map(|c| c.last_snapshot_bytes)
            .unwrap_or(0)
    }

    /// Size of the RocksDB info LOG file (bytes), for the `rocksdb_log_size` gauge.
    pub fn rocksdb_log_size(&self) -> u64 {
        std::fs::metadata(self.path.join("LOG"))
            .map(|m| m.len())
            .unwrap_or(0)
    }

    /// Force a full SafeBox snapshot to RocksDB (shutdown / export / recovery).
    pub fn flush_safebox_snapshot(&self) -> Result<(), StorageError> {
        let safebox = self.safebox()?;
        self.put_safe_box(&safebox)?;
        if let Ok(mut cache) = self.safebox_cache.lock() {
            cache.snapshot_total = cache.snapshot_total.saturating_add(1);
            cache.last_snapshot_bytes = safebox.to_bytes().len() as u64;
            cache.dirty.clear();
            cache.blocks_since_snapshot = 0;
        }
        Ok(())
    }

    fn cf(&self, name: &str) -> rocksdb::ColumnFamilyRef<'_> {
        self.db.cf_handle(name).expect("column family must exist")
    }

    pub fn put_account(&self, account: &Account) -> Result<(), StorageError> {
        let cf = self.cf(CF_ACCOUNTS);
        let key = account.account_number.to_be_bytes();
        let value = account.to_bytes();
        self.db
            .put_cf(&cf, key, value)
            .map_err(|e| StorageError::Database(e.to_string()))
    }

    pub fn get_account(&self, account_number: u64) -> Result<Option<Account>, StorageError> {
        let cf = self.cf(CF_ACCOUNTS);
        let key = account_number.to_be_bytes();
        match self.db.get_cf(&cf, key) {
            Ok(Some(value)) => {
                let account = Account::from_bytes(&value)
                    .map_err(|e| StorageError::Serialization(e.to_string()))?;
                Ok(Some(account))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(StorageError::Database(e.to_string())),
        }
    }

    pub fn delete_account(&self, account_number: u64) -> Result<(), StorageError> {
        let cf = self.cf(CF_ACCOUNTS);
        let key = account_number.to_be_bytes();
        self.db
            .delete_cf(&cf, key)
            .map_err(|e| StorageError::Database(e.to_string()))
    }

    pub fn put_block(&self, block: &OperationBlock) -> Result<(), StorageError> {
        let cf = self.cf(CF_BLOCKS);
        let key = block.header.block_number.to_be_bytes();
        let value = block.to_bytes();
        self.db
            .put_cf(&cf, key, value)
            .map_err(|e| StorageError::Database(e.to_string()))
    }

    pub fn get_block(&self, block_number: u64) -> Result<Option<OperationBlock>, StorageError> {
        let cf = self.cf(CF_BLOCKS);
        let key = block_number.to_be_bytes();
        match self.db.get_cf(&cf, key) {
            Ok(Some(value)) => {
                let block = OperationBlock::from_bytes(&value).map_err(|e| {
                    StorageError::Serialization(format!("block deserialization: {e}"))
                })?;
                Ok(Some(block))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(StorageError::Database(e.to_string())),
        }
    }

    pub fn put_safe_box(&self, safe_box: &SafeBox) -> Result<(), StorageError> {
        let cf = self.cf(CF_SAFEBOX);
        let key = b"current";
        let value = safe_box.to_bytes();
        self.db
            .put_cf(&cf, key, value)
            .map_err(|e| StorageError::Database(e.to_string()))
    }

    pub fn get_safe_box(&self) -> Result<Option<SafeBox>, StorageError> {
        let cf = self.cf(CF_SAFEBOX);
        match self.db.get_cf(&cf, b"current") {
            Ok(Some(value)) => {
                let sb = SafeBox::from_bytes(&value)
                    .ok_or_else(|| StorageError::Serialization("invalid safebox data".into()))?;
                Ok(Some(sb))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(StorageError::Database(e.to_string())),
        }
    }

    pub fn put_validator_set_bytes(&self, data: &[u8]) -> Result<(), StorageError> {
        let cf = self.cf(CF_VALIDATOR_SET);
        self.db
            .put_cf(&cf, b"vset", data)
            .map_err(|e| StorageError::Database(e.to_string()))
    }

    pub fn get_validator_set_bytes(&self) -> Result<Option<Vec<u8>>, StorageError> {
        let cf = self.cf(CF_VALIDATOR_SET);
        self.db
            .get_cf(&cf, b"vset")
            .map(|opt| opt.map(|v| v.to_vec()))
            .map_err(|e| StorageError::Database(e.to_string()))
    }

    pub fn put_height(&self, height: u64) -> Result<(), StorageError> {
        let cf = self.cf(CF_VALIDATOR_SET);
        self.db
            .put_cf(&cf, b"height", height.to_be_bytes())
            .map_err(|e| StorageError::Database(e.to_string()))
    }

    pub fn get_height(&self) -> Result<u64, StorageError> {
        let cf = self.cf(CF_VALIDATOR_SET);
        match self.db.get_cf(&cf, b"height") {
            Ok(Some(value)) if value.len() >= 8 => {
                Ok(u64::from_be_bytes(value[0..8].try_into().unwrap()))
            }
            Ok(_) => Ok(0),
            Err(e) => Err(StorageError::Database(e.to_string())),
        }
    }

    pub fn put_equivocation_proof(
        &self,
        height: u64,
        validator_id: u64,
        data: &[u8],
    ) -> Result<(), StorageError> {
        let cf = self.cf(CF_EQUIVOCATION_PROOFS);
        let mut key = Vec::with_capacity(16);
        key.extend_from_slice(&height.to_be_bytes());
        key.extend_from_slice(&validator_id.to_be_bytes());
        self.db
            .put_cf(&cf, key, data)
            .map_err(|e| StorageError::Database(e.to_string()))
    }

    pub fn get_equivocation_proof(
        &self,
        height: u64,
        validator_id: u64,
    ) -> Result<Option<Vec<u8>>, StorageError> {
        let cf = self.cf(CF_EQUIVOCATION_PROOFS);
        let mut key = Vec::with_capacity(16);
        key.extend_from_slice(&height.to_be_bytes());
        key.extend_from_slice(&validator_id.to_be_bytes());
        self.db
            .get_cf(&cf, key)
            .map(|opt| opt.map(|v| v.to_vec()))
            .map_err(|e| StorageError::Database(e.to_string()))
    }

    pub fn put_faucet_claim(
        &self,
        address: &str,
        timestamp: u64,
        tx_hash: &str,
        status: &str,
    ) -> Result<(), StorageError> {
        let mut value = timestamp.to_be_bytes().to_vec();
        value.extend_from_slice(&(tx_hash.len() as u16).to_be_bytes());
        value.extend_from_slice(tx_hash.as_bytes());
        value.extend_from_slice(&(status.len() as u16).to_be_bytes());
        value.extend_from_slice(status.as_bytes());
        self.db
            .put_cf(&self.cf(CF_FAUCET_CLAIMS), address.as_bytes(), value)
            .map_err(|e| StorageError::Database(e.to_string()))
    }

    pub fn get_faucet_claim_timestamp(&self, address: &str) -> Result<Option<u64>, StorageError> {
        match self
            .db
            .get_cf(&self.cf(CF_FAUCET_CLAIMS), address.as_bytes())
        {
            Ok(Some(value)) if value.len() >= 8 => {
                Ok(Some(u64::from_be_bytes(value[..8].try_into().unwrap())))
            }
            Ok(Some(_)) => Err(StorageError::Serialization("invalid faucet claim".into())),
            Ok(None) => Ok(None),
            Err(e) => Err(StorageError::Database(e.to_string())),
        }
    }

    pub fn list_equivocation_proofs(&self) -> Result<Vec<(u64, u64, Vec<u8>)>, StorageError> {
        let cf = self.cf(CF_EQUIVOCATION_PROOFS);
        let mut proofs = Vec::new();
        let iter = self.db.iterator_cf(&cf, rocksdb::IteratorMode::Start);
        for item in iter.filter_map(|r| r.ok()) {
            if item.0.len() >= 16 {
                let height = u64::from_be_bytes(item.0[0..8].try_into().unwrap());
                let validator_id = u64::from_be_bytes(item.0[8..16].try_into().unwrap());
                proofs.push((height, validator_id, item.1.to_vec()));
            }
        }
        Ok(proofs)
    }

    /// Delete all blocks with block_number < cutoff_height.
    /// SafeBox retains all account state independently of the blockchain.
    /// Returns the number of blocks pruned.
    pub fn prune_blocks(&self, cutoff_height: u64) -> Result<u64, StorageError> {
        let cf = self.cf(CF_BLOCKS);
        let mut pruned = 0u64;
        let height = self.get_height()?;

        for block_num in 0..cutoff_height.min(height + 1) {
            let key = block_num.to_be_bytes();
            match self.db.get_cf(&cf, key) {
                Ok(Some(_)) => {
                    self.db
                        .delete_cf(&cf, key)
                        .map_err(|e| StorageError::Database(e.to_string()))?;
                    pruned += 1;
                }
                Ok(None) => continue,
                Err(e) => return Err(StorageError::Database(e.to_string())),
            }
        }
        Ok(pruned)
    }

    /// Return (min_block, max_block) present in the blocks CF, or None if empty.
    pub fn block_height_range(&self) -> Result<Option<(u64, u64)>, StorageError> {
        let cf = self.cf(CF_BLOCKS);
        let iter = self.db.iterator_cf(&cf, rocksdb::IteratorMode::Start);
        let blocks: Vec<u64> = iter
            .filter_map(|r| r.ok())
            .filter_map(|(k, _)| {
                if k.len() >= 8 {
                    Some(u64::from_be_bytes(k[0..8].try_into().unwrap()))
                } else {
                    None
                }
            })
            .collect();
        if blocks.is_empty() {
            Ok(None)
        } else {
            let min = blocks.iter().min().copied().unwrap();
            let max = blocks.iter().max().copied().unwrap();
            Ok(Some((min, max)))
        }
    }

    pub fn max_account_number(&self) -> Result<u64, StorageError> {
        let mut max_num = 0u64;
        let mut probe = 0u64;
        let mut missing_streak = 0u32;
        while missing_streak < 10 {
            match self.get_account(probe) {
                Ok(Some(_)) => {
                    max_num = probe;
                    missing_streak = 0;
                }
                _ => {
                    missing_streak += 1;
                }
            }
            probe += 1;
        }
        Ok(max_num)
    }

    pub fn iter_accounts(&self) -> Result<Vec<Account>, StorageError> {
        let cf = self.cf(CF_ACCOUNTS);
        let mut accounts = Vec::new();
        let iter = self.db.iterator_cf(&cf, rocksdb::IteratorMode::Start);
        for item in iter.filter_map(|r| r.ok()) {
            if let Ok(account) = Account::from_bytes(&item.1) {
                accounts.push(account);
            }
        }
        Ok(accounts)
    }
}

impl augecoin_core::mempool::AccountLookup for Storage {
    fn get_account(&self, account_number: u64) -> Result<Option<Account>, String> {
        Storage::get_account(self, account_number).map_err(|e| e.to_string())
    }
}

/// Column-family options tuned to reduce write amplification: larger memtables
/// and SSTs mean fewer, bigger compactions instead of the thousands of tiny
/// `WAL Full` flushes observed before this change.
fn tuned_cf_options() -> Options {
    let mut opts = Options::default();
    opts.set_write_buffer_size(WRITE_BUFFER_SIZE);
    opts.set_max_write_buffer_number(MAX_WRITE_BUFFER_NUMBER);
    opts.set_min_write_buffer_number_to_merge(MIN_WRITE_BUFFER_NUMBER_TO_MERGE);
    opts.set_target_file_size_base(TARGET_FILE_SIZE_BASE);
    opts.set_max_bytes_for_level_base(MAX_BYTES_FOR_LEVEL_BASE);
    opts.set_level_zero_file_num_compaction_trigger(4);
    opts.set_level_zero_slowdown_writes_trigger(20);
    opts.set_level_zero_stop_writes_trigger(36);
    opts.set_compression_type(DBCompressionType::Lz4);
    opts
}

#[cfg(test)]
mod tests {
    use super::*;
    use augecoin_core::account::{AccountInfo, AccountKey};
    use augecoin_core::block::OperationBlockHeader;
    use augecoin_core::HybridSignature;
    use std::sync::atomic::{AtomicU32, Ordering};

    static TEST_COUNTER: AtomicU32 = AtomicU32::new(0);

    fn test_path() -> String {
        let id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
        let path = format!("/tmp/augecoin-test-db-{id}");
        path
    }

    fn cleanup(path: &str) {
        std::fs::remove_dir_all(path).ok();
    }

    fn test_account(num: u64, balance: u64) -> Account {
        let mut ed = [0u8; 32];
        ed[0..8].copy_from_slice(&num.to_be_bytes());
        Account {
            account_number: num,
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
    fn write_and_read_account() {
        let path = test_path();
        let storage = Storage::open(&path).unwrap();
        storage.put_account(&test_account(1, 5000)).unwrap();
        let acc = storage.get_account(1).unwrap().expect("should exist");
        assert_eq!(acc.balance, 5000);
        cleanup(&path);
    }

    #[test]
    fn read_nonexistent_account() {
        let path = test_path();
        let storage = Storage::open(&path).unwrap();
        assert!(storage.get_account(999).unwrap().is_none());
        cleanup(&path);
    }

    #[test]
    fn overwrite_account() {
        let path = test_path();
        let storage = Storage::open(&path).unwrap();
        let mut acc = test_account(2, 100);
        storage.put_account(&acc).unwrap();
        acc.balance = 300;
        storage.put_account(&acc).unwrap();
        assert_eq!(storage.get_account(2).unwrap().unwrap().balance, 300);
        cleanup(&path);
    }

    #[test]
    fn delete_account() {
        let path = test_path();
        let storage = Storage::open(&path).unwrap();
        storage.put_account(&test_account(3, 50)).unwrap();
        storage.delete_account(3).unwrap();
        assert!(storage.get_account(3).unwrap().is_none());
        cleanup(&path);
    }

    #[test]
    fn height_persistence() {
        let path = test_path();
        let storage = Storage::open(&path).unwrap();
        assert_eq!(storage.get_height().unwrap(), 0);
        storage.put_height(42).unwrap();
        assert_eq!(storage.get_height().unwrap(), 42);
        cleanup(&path);
    }

    #[test]
    fn safebox_persistence() {
        let path = test_path();
        let storage = Storage::open(&path).unwrap();
        let mut sb = SafeBox::new(5, 0);
        sb.add_account(test_account(0, 100));
        sb.add_account(test_account(1, 200));
        sb.update_hash();
        storage.put_safe_box(&sb).unwrap();

        let restored = storage.get_safe_box().unwrap().unwrap();
        assert_eq!(restored.account_count(), 2);
        cleanup(&path);
    }

    #[test]
    fn iter_accounts() {
        let path = test_path();
        let storage = Storage::open(&path).unwrap();
        for i in 0..5 {
            storage.put_account(&test_account(i, i * 100)).unwrap();
        }
        let accounts = storage.iter_accounts().unwrap();
        assert_eq!(accounts.len(), 5);
        cleanup(&path);
    }

    #[test]
    fn corruption_detected_on_block_read() {
        let path = test_path();
        let storage = Storage::open(&path).unwrap();

        let cf_blocks = storage.cf(CF_BLOCKS);
        let key = 7u64.to_be_bytes();
        let valid_block_bytes = vec![42u8; 256];
        storage
            .db
            .put_cf(&cf_blocks, key, &valid_block_bytes)
            .unwrap();

        let checksum_before = storage.verify_checksum();

        let mut corrupted = valid_block_bytes.clone();
        corrupted[10] ^= 0xFF;
        storage
            .db
            .delete_cf(&cf_blocks, key)
            .expect("delete should succeed");
        storage
            .db
            .put_cf(&cf_blocks, key, &corrupted)
            .expect("reinsert should succeed");

        let result = storage.get_block(7);
        assert!(
            result.is_err(),
            "reading corrupted block should return error, got: {result:?}"
        );

        let checksum_after = storage.verify_checksum();
        assert_eq!(
            checksum_before, checksum_after,
            "verify_checksum should be unchanged after block corruption (accounts unchanged)"
        );

        cleanup(&path);
    }

    #[test]
    fn prune_blocks_deletes_old_blocks() {
        let path = test_path();
        let storage = Storage::open(&path).unwrap();
        storage.put_height(200).unwrap();

        let block = empty_block(0);
        for i in 0..10u64 {
            let mut b = block.clone();
            b.header.block_number = i;
            storage.put_block(&b).unwrap();
        }

        assert!(storage.get_block(0).unwrap().is_some());
        assert!(storage.get_block(5).unwrap().is_some());

        let pruned = storage.prune_blocks(5).unwrap();
        assert_eq!(pruned, 5);

        assert!(storage.get_block(0).unwrap().is_none());
        assert!(storage.get_block(4).unwrap().is_none());
        assert!(storage.get_block(5).unwrap().is_some());
        assert!(storage.get_block(9).unwrap().is_some());

        let range = storage.block_height_range().unwrap().unwrap();
        assert_eq!(range.0, 5);
        assert_eq!(range.1, 9);

        cleanup(&path);
    }

    #[test]
    fn prune_blocks_preserves_account_state() {
        let path = test_path();
        let storage = Storage::open(&path).unwrap();
        storage.put_height(200).unwrap();

        storage.put_account(&test_account(10, 5000)).unwrap();

        let block = empty_block(0);
        for i in 0..3u64 {
            let mut b = block.clone();
            b.header.block_number = i;
            storage.put_block(&b).unwrap();
        }

        let checksum_before = storage.verify_checksum();
        assert_eq!(storage.get_account(10).unwrap().unwrap().balance, 5000);

        storage.prune_blocks(2).unwrap();

        assert!(storage.get_block(0).unwrap().is_none());
        assert!(storage.get_block(2).unwrap().is_some());
        assert_eq!(storage.get_account(10).unwrap().unwrap().balance, 5000);

        let checksum_after = storage.verify_checksum();
        assert_eq!(checksum_before, checksum_after);

        cleanup(&path);
    }

    #[test]
    fn prune_blocks_empty_db_returns_zero() {
        let path = test_path();
        let storage = Storage::open(&path).unwrap();
        let pruned = storage.prune_blocks(100).unwrap();
        assert_eq!(pruned, 0);
        assert!(storage.block_height_range().unwrap().is_none());
        cleanup(&path);
    }

    fn empty_block(num: u64) -> OperationBlock {
        OperationBlock {
            header: OperationBlockHeader {
                block_number: num,
                account_key: [0u8; 32],
                reward: 0,
                fee: 0,
                protocol_version: 5,
                protocol_available: 5,
                timestamp: 0,
                initial_safe_box_hash: [0u8; 64],
                operations_hash: [0u8; 64],
                block_payload: vec![],
                proof_of_work: [0u8; 32],
                previous_proof_of_work: [0u8; 32],
                leader_id: 0,
                chain_id: 1,
            },
            operations: vec![],
            leader_signature: HybridSignature { bytes: [0u8; 64] },
            quorum_signatures: vec![],
            block_hash: [0u8; 64],
        }
    }

    // ── Incremental SafeBox tests ─────────────────────────────────────────

    #[test]
    fn safebox_incremental_matches_full_rebuild() {
        let path = test_path();
        let storage = Storage::open(&path).unwrap();
        for i in 0..20u64 {
            storage.put_account(&test_account(i, i * 100)).unwrap();
        }

        // Force a full snapshot to establish the canonical hash.
        storage.flush_safebox_snapshot().unwrap();
        let snapshot_hash = storage
            .get_safe_box()
            .unwrap()
            .unwrap()
            .header
            .safe_box_hash;

        // Incrementally modify a few accounts and compare against a fresh
        // storage that rebuilt the full SafeBox from scratch.
        let mut modified = HashMap::new();
        for i in [0u64, 5, 19] {
            let mut acc = test_account(i, i * 100 + 7);
            acc.balance += 7;
            storage.put_account(&acc).unwrap();
            modified.insert(i, acc);
        }
        let inc_hash = storage.commit_safebox_incremental(&modified, true).unwrap();

        assert_ne!(
            inc_hash, snapshot_hash,
            "hash must change after modification"
        );

        // Reconstruct the expected SafeBox by hand (accounts CF is source of truth).
        let expected = {
            let mut sb = augecoin_core::safe_box::SafeBox::new(
                augecoin_core::constants::CT_BUILD_PROTOCOL,
                0,
            );
            for acc in storage.iter_accounts().unwrap() {
                sb.add_account(acc);
            }
            sb.update_hash();
            sb.header.safe_box_hash
        };
        assert_eq!(
            inc_hash, expected,
            "incremental hash must equal full rebuild"
        );
        cleanup(&path);
    }

    #[test]
    fn safebox_snapshot_interval_throttles_persistence() {
        let path = test_path();
        let storage = Storage::open(&path).unwrap();
        storage.set_safebox_snapshot_interval(10);
        for i in 0..5u64 {
            storage.put_account(&test_account(i, i)).unwrap();
        }
        storage.flush_safebox_snapshot().unwrap();

        // Commit 9 blocks of one modified account each: no snapshot should be
        // persisted until the 10th commit.
        let mut h: Option<[u8; 64]> = None;
        for b in 0..9u64 {
            let mut acc = test_account(0, b + 1);
            acc.balance = b + 1;
            let mut m = HashMap::new();
            m.insert(0, acc.clone());
            storage.put_account(&acc).unwrap();
            h = Some(storage.commit_safebox_incremental(&m, false).unwrap());
        }
        assert_eq!(storage.snapshot_total(), 1, "no snapshot within interval");

        let mut acc = test_account(0, 99);
        acc.balance = 99;
        let mut m = HashMap::new();
        m.insert(0, acc.clone());
        storage.put_account(&acc).unwrap();
        let h2 = storage.commit_safebox_incremental(&m, false).unwrap();

        assert_ne!(h2, h.unwrap());
        assert_eq!(storage.snapshot_total(), 2, "snapshot fired at interval");
        assert_eq!(
            storage.dirty_accounts(),
            0,
            "dirty set cleared after snapshot"
        );

        // Recovery: a fresh Storage opening the same path reconstructs the same hash.
        drop(storage);
        let reopened = Storage::open(&path).unwrap();
        assert_eq!(reopened.safebox_hash().unwrap(), h2);
        cleanup(&path);
    }

    #[test]
    fn safebox_recovers_without_snapshot() {
        let path = test_path();
        let storage = Storage::open(&path).unwrap();
        for i in 0..10u64 {
            storage.put_account(&test_account(i, i * 5)).unwrap();
        }
        // Never flush a snapshot; rely solely on the accounts CF for recovery.
        let mut acc = test_account(3, 1234);
        acc.balance = 1234;
        let mut m = HashMap::new();
        m.insert(3, acc.clone());
        storage.put_account(&acc).unwrap();
        let hash = storage.commit_safebox_incremental(&m, false).unwrap();

        drop(storage);
        let reopened = Storage::open(&path).unwrap();
        assert_eq!(
            reopened.safebox_hash().unwrap(),
            hash,
            "hash reconstructed from accounts CF matches"
        );
        cleanup(&path);
    }

    #[test]
    fn safebox_name_index_is_consistent() {
        let path = test_path();
        let storage = Storage::open(&path).unwrap();
        let mut a1 = test_account(1, 100);
        a1.name = Some("alice".to_string());
        storage.put_account(&a1).unwrap();
        let mut a2 = test_account(2, 200);
        a2.name = Some("bob".to_string());
        storage.put_account(&a2).unwrap();
        storage.flush_safebox_snapshot().unwrap();

        let idx = storage.safebox_name_index().unwrap();
        assert_eq!(idx.get("alice"), Some(&1));
        assert_eq!(idx.get("bob"), Some(&2));
        cleanup(&path);
    }

    #[test]
    fn pubkey_index_detects_duplicate_keys() {
        let path = test_path();
        let storage = Storage::open(&path).unwrap();
        for i in 0..5u64 {
            storage.put_account(&test_account(i, i * 10)).unwrap();
        }
        storage.flush_safebox_snapshot().unwrap();

        let pk0 = test_account(0, 10)
            .account_info
            .account_key
            .ed25519_public_key;
        let pk4 = test_account(4, 40)
            .account_info
            .account_key
            .ed25519_public_key;
        assert!(storage.pubkey_in_use(&pk0).unwrap());
        assert!(storage.pubkey_in_use(&pk4).unwrap());
        assert!(!storage.pubkey_in_use(&[0xFF; 32]).unwrap());
        cleanup(&path);
    }

    #[test]
    fn pubkey_index_tracks_key_changes() {
        let path = test_path();
        let storage = Storage::open(&path).unwrap();
        storage.put_account(&test_account(1, 100)).unwrap();
        storage.flush_safebox_snapshot().unwrap();

        let old_pk = test_account(1, 100)
            .account_info
            .account_key
            .ed25519_public_key;
        assert!(storage.pubkey_in_use(&old_pk).unwrap());

        // Change key: the old key must be released and the new one registered.
        let mut acc = test_account(1, 100);
        acc.account_info.account_key.ed25519_public_key = [0xAB; 32];
        let mut modified = HashMap::new();
        modified.insert(1, acc.clone());
        storage.put_account(&acc).unwrap();
        storage.commit_safebox_incremental(&modified, true).unwrap();

        assert!(!storage.pubkey_in_use(&old_pk).unwrap(), "old key released");
        assert!(
            storage.pubkey_in_use(&[0xAB; 32]).unwrap(),
            "new key registered"
        );
        cleanup(&path);
    }

    #[test]
    fn commit_block_atomic_writes_block_height_accounts() {
        let path = test_path();
        let storage = Storage::open(&path).unwrap();

        let mut modified = HashMap::new();
        let mut acc = test_account(10, 500);
        acc.balance = 500;
        modified.insert(10, acc.clone());

        let block = empty_block(7);
        let hash = storage
            .commit_block_atomic(&modified, &block, None, true)
            .unwrap();

        assert_eq!(storage.get_height().unwrap(), 7);
        assert!(storage.get_block(7).unwrap().is_some());
        assert_eq!(storage.get_account(10).unwrap().unwrap().balance, 500);
        assert_ne!(hash, [0u8; 64]);
        cleanup(&path);
    }

    #[test]
    fn commit_block_atomic_is_atomic_on_validator_set() {
        let path = test_path();
        let storage = Storage::open(&path).unwrap();

        let modified = HashMap::new();
        let block = empty_block(1);
        let vs = b"validator-set-bytes";
        storage
            .commit_block_atomic(&modified, &block, Some(vs), true)
            .unwrap();

        assert_eq!(storage.get_validator_set_bytes().unwrap().unwrap(), vs);
        assert_eq!(storage.get_height().unwrap(), 1);
        cleanup(&path);
    }

    #[test]
    fn resolve_name_returns_account_number() {
        let path = test_path();
        let storage = Storage::open(&path).unwrap();
        let mut acc = test_account(42, 100);
        acc.name = Some("CarlosPay".to_string());
        storage.put_account(&acc).unwrap();
        storage.flush_safebox_snapshot().unwrap();

        assert_eq!(storage.resolve_name("CarlosPay").unwrap(), Some(42));
        assert_eq!(storage.resolve_name("Nobody").unwrap(), None);
        cleanup(&path);
    }

    #[test]
    fn sale_index_lists_for_sale() {
        let path = test_path();
        let storage = Storage::open(&path).unwrap();

        let mut acc = test_account(10, 0);
        acc.account_info.state = augecoin_core::account::AccountState::ForSale;
        acc.account_info.price = 1234;
        storage.put_account(&acc).unwrap();

        let mut owned = test_account(11, 0);
        owned.account_info.state = augecoin_core::account::AccountState::Owned;
        storage.put_account(&owned).unwrap();

        storage.flush_safebox_snapshot().unwrap();

        let listings = storage.list_for_sale().unwrap();
        assert_eq!(listings.len(), 1);
        assert_eq!(listings[0].account_number, 10);
        assert_eq!(listings[0].price, 1234);
        cleanup(&path);
    }

    #[test]
    fn gift_index_clears_after_accept() {
        let path = test_path();
        let storage = Storage::open(&path).unwrap();

        let mut acc = test_account(20, 0);
        acc.account_info.state = augecoin_core::account::AccountState::GiftPending;
        acc.account_info.new_public_key = Some(AccountKey {
            ed25519_public_key: [0xAB; 32],
        });
        storage.put_account(&acc).unwrap();
        storage.flush_safebox_snapshot().unwrap();

        assert_eq!(storage.list_pending_gifts().unwrap().len(), 1);

        // Accept: transition to Owned, clear the pending gift.
        let mut accepted = acc.clone();
        accepted.account_info.state = augecoin_core::account::AccountState::Owned;
        accepted.account_info.new_public_key = None;
        accepted.account_info.account_key = AccountKey {
            ed25519_public_key: [0xAB; 32],
        };
        let mut modified = HashMap::new();
        modified.insert(20, accepted.clone());
        storage.put_account(&accepted).unwrap();
        storage.commit_safebox_incremental(&modified, true).unwrap();

        assert_eq!(storage.list_pending_gifts().unwrap().len(), 0);
        cleanup(&path);
    }
}
