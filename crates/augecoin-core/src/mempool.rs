use crate::account::Account;
use crate::constants::MIN_FEE_AUGESAT;
use crate::operation::{Operation, OperationPayload, SenderInfo};
use crate::proposal::{op_hash, TxHash};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use thiserror::Error;

/// Minimal read-only account access required by the mempool.
///
/// This trait decouples the mempool from the concrete `Storage` type so that
/// both the node (consensus) and the RPC layer can share the *same* mempool
/// instance without introducing a dependency cycle between crates.
pub trait AccountLookup {
    fn get_account(&self, account_number: u64) -> Result<Option<Account>, String>;
}

#[derive(Debug, Clone, Error)]
pub enum MempoolError {
    #[error("invalid signature")]
    InvalidSignature,
    #[error("signature verification failed: sender account {0} not found")]
    SenderNotFound(u64),
    #[error("invalid n_operation for sender {sender}: expected {expected}, got {actual}")]
    InvalidNOperation {
        sender: u64,
        expected: u64,
        actual: u64,
    },
    #[error("insufficient balance for account {account}: have {balance}, need {required}")]
    InsufficientBalance {
        account: u64,
        balance: u64,
        required: u64,
    },
    #[error("fee too low: {fee} < minimum {minimum}")]
    FeeTooLow { fee: u64, minimum: u64 },
    #[error("duplicate operation (same sender {sender} and n_operation {n_operation})")]
    DuplicateOperation { sender: u64, n_operation: u64 },
    #[error("duplicate operation (same op hash)")]
    DuplicateOpHash,
    #[error("account {account} in invalid state for this operation: {reason}")]
    InvalidAccountState { account: u64, reason: String },
    #[error("storage error: {0}")]
    Storage(String),
    #[error("mempool full")]
    MempoolFull,
    #[error("wrong chain_id: operation {actual}, expected {expected}")]
    WrongChainId { expected: u64, actual: u64 },
    #[error("operation expired")]
    Expired,
    #[error("admin public key not configured; cannot validate admin operations")]
    AdminKeyNotConfigured,
    #[error("transfer value is not conserved: senders provide {sent}, receivers claim {received}")]
    UnbalancedTransfer { sent: u64, received: u64 },
    #[error("transfer value total overflow")]
    TransferValueOverflow,
}

#[derive(Debug, Clone)]
pub struct MempoolConfig {
    pub min_fee: u64,
    /// Upper bound on the number of pending operations. This is a memory/DoS
    /// safety ceiling only — it does *not* guarantee a transmissible block.
    /// The block builder decides how many operations go into a block by
    /// serialized byte size (see `augecoin_core::limits`). Keep this generous.
    pub max_operations: usize,
    /// Chain ID this mempool belongs to. Operations with a different
    /// `chain_id` are rejected (replay protection across networks).
    pub chain_id: u64,
    /// Time-to-live in seconds for a pending operation before it is evicted.
    /// Use `0` to disable expiration.
    pub ttl_seconds: u64,
    /// Maximum native reserve (augesat) a Contract operation may demand from
    /// its sender: `fee + gas_reserve` must be covered by the sender's balance
    /// (mirrors the execution-time check `fee + DEFAULT_GAS_LIMIT *
    /// GAS_PRICE_AUGESAT`). The node wires this from
    /// `augecoin_contracts::gas`; the default matches the built-in values.
    pub contract_gas_reserve: u64,
}

impl Default for MempoolConfig {
    fn default() -> Self {
        MempoolConfig {
            min_fee: MIN_FEE_AUGESAT,
            max_operations: 20_000,
            chain_id: crate::constants::CT_CHAIN_ID_MAINNET,
            ttl_seconds: 3_600,
            contract_gas_reserve: 100_000,
        }
    }
}

#[derive(Debug)]
pub struct Mempool {
    config: MempoolConfig,
    operations: Vec<Operation>,
    admitted_at: Vec<u64>,
    sender_n_ops: HashMap<u64, u64>,
    op_index: HashMap<(u64, u64), usize>,
    /// Resident O(1) index by transaction hash, used to answer
    /// `GetTransactions` immediately without a linear scan. Updated
    /// incrementally on admission, removal and confirmation — never rebuilt
    /// from scratch on the fetch path.
    by_hash: HashMap<TxHash, Arc<Operation>>,
    admin_pubkey: Option<[u8; 32]>,
}

impl Mempool {
    pub fn new(config: MempoolConfig) -> Self {
        Mempool {
            config,
            operations: Vec::new(),
            admitted_at: Vec::new(),
            sender_n_ops: HashMap::new(),
            op_index: HashMap::new(),
            by_hash: HashMap::new(),
            admin_pubkey: None,
        }
    }

    /// Set the admin public key used to verify `ValidatorAdmin` and
    /// `CreateAccount` operations at admission time. Without it, such
    /// operations are rejected (they can never be applied by consensus).
    pub fn set_admin_pubkey(&mut self, pubkey: [u8; 32]) {
        self.admin_pubkey = Some(pubkey);
    }

    pub fn len(&self) -> usize {
        self.operations.len()
    }

    pub fn is_empty(&self) -> bool {
        self.operations.is_empty()
    }

    pub fn is_full(&self) -> bool {
        self.operations.len() >= self.config.max_operations
    }

    pub fn chain_id(&self) -> u64 {
        self.config.chain_id
    }

    pub fn validate_and_admit(
        &mut self,
        op: Operation,
        lookup: &dyn AccountLookup,
        now: u64,
    ) -> Result<(), MempoolError> {
        if self.is_full() {
            return Err(MempoolError::MempoolFull);
        }

        if op.chain_id != self.config.chain_id {
            return Err(MempoolError::WrongChainId {
                expected: self.config.chain_id,
                actual: op.chain_id,
            });
        }

        // Dedupe by operation hash FIRST: byte-identical resubmissions (client
        // retries, peer re-gossip) must never occupy more than one slot — the
        // (sender, n_operation) index alone misses payloads without senders.
        let op_hash = op_hash(&op);
        if self.by_hash.contains_key(&op_hash) {
            return Err(MempoolError::DuplicateOpHash);
        }

        self.validate_signatures(&op, lookup)?;

        // Duplicate check before n_operation validation
        for sender in payload_senders(&op.payload) {
            let key = (sender.account, sender.n_operation);
            if self.op_index.contains_key(&key) {
                return Err(MempoolError::DuplicateOperation {
                    sender: sender.account,
                    n_operation: sender.n_operation,
                });
            }
        }

        self.validate_n_operation_chaining(&op, lookup)?;
        self.validate_balance_and_fee(&op, lookup)?;

        let idx = self.operations.len();
        self.operations.push(op.clone());
        self.admitted_at.push(now);
        self.by_hash.insert(op_hash, Arc::new(op.clone()));

        for sender in payload_senders(&op.payload) {
            let key = (sender.account, sender.n_operation);
            self.op_index.insert(key, idx);
            self.sender_n_ops.insert(
                sender.account,
                self.sender_n_ops
                    .get(&sender.account)
                    .map_or(sender.n_operation, |&prev| prev.max(sender.n_operation)),
            );
        }

        Ok(())
    }

    /// Remove operations whose admission timestamp is older than `ttl_seconds`.
    ///
    /// Design decision (see ADR): a sender's pending operations form a strictly
    /// sequential nonce chain, so when the *front* of a chain expires every
    /// later nonce of that sender becomes orphaned (it can never be applied
    /// because its predecessor is gone). Those orphaned operations are evicted
    /// together with the expired one; the sender must re-submit from the
    /// expired nonce.
    ///
    /// Every eviction is logged with the sender account, nonce and the reason
    /// `expired_ttl`. Returns the number of operations evicted.
    pub fn evict_expired(&mut self, now: u64) -> usize {
        if self.config.ttl_seconds == 0 {
            return 0;
        }
        let cutoff = now.saturating_sub(self.config.ttl_seconds);

        let mut to_remove: HashSet<usize> = self
            .admitted_at
            .iter()
            .enumerate()
            .filter(|(_, &t)| t < cutoff)
            .map(|(i, _)| i)
            .collect();

        if to_remove.is_empty() {
            return 0;
        }

        // Cascade: for each sender whose front nonce expired, remove every
        // pending op of that sender with a higher nonce (orphaned chain).
        let mut orphan_front: HashMap<u64, u64> = HashMap::new();
        for &idx in &to_remove {
            for (account, n_operation) in self.op_nonce_pairs(&self.operations[idx]) {
                let entry = orphan_front.entry(account).or_insert(u64::MAX);
                if n_operation < *entry {
                    *entry = n_operation;
                }
            }
        }

        for (i, op) in self.operations.iter().enumerate() {
            if to_remove.contains(&i) {
                continue;
            }
            for (account, n_operation) in self.op_nonce_pairs(op) {
                if let Some(&front) = orphan_front.get(&account) {
                    if n_operation > front {
                        to_remove.insert(i);
                        break;
                    }
                }
            }
        }

        let mut evicted = 0;
        let mut sorted: Vec<usize> = to_remove.into_iter().collect();
        sorted.sort_unstable();
        for &idx in sorted.iter().rev() {
            let op = &self.operations[idx];
            for (account, n_operation) in self.op_nonce_pairs(op) {
                eprintln!(
                    "[mempool] evicting operation expired_ttl account={account} n_operation={n_operation}"
                );
            }
            self.by_hash.remove(&op_hash(op));
            self.operations.remove(idx);
            self.admitted_at.remove(idx);
            evicted += 1;
        }
        if evicted > 0 {
            self.rebuild_index();
        }
        evicted
    }

    /// Evict lowest-priority operations (oldest first) down to `max_operations`.
    /// Returns the number evicted.
    pub fn evict_lowest_priority(&mut self) -> usize {
        let mut evicted = 0;
        while self.operations.len() > self.config.max_operations {
            // Drop oldest admitted operation (FIFO eviction policy).
            self.by_hash.remove(&op_hash(&self.operations[0]));
            self.operations.remove(0);
            self.admitted_at.remove(0);
            evicted += 1;
        }
        if evicted > 0 {
            self.rebuild_index();
        }
        evicted
    }

    pub fn remove(&mut self, sender: u64, n_operation: u64) -> Option<Operation> {
        let key = (sender, n_operation);
        if let Some(&idx) = self.op_index.get(&key) {
            if idx < self.operations.len() {
                let removed_hash = op_hash(&self.operations[idx]);
                let removed = self.operations.remove(idx);
                self.admitted_at.remove(idx);
                self.op_index.remove(&key);
                self.by_hash.remove(&removed_hash);
                self.rebuild_index();
                return Some(removed);
            }
        }
        None
    }

    pub fn pending(&self) -> Vec<&Operation> {
        self.operations.iter().collect()
    }

    pub fn pending_cloned(&self) -> Vec<Operation> {
        self.operations.clone()
    }

    pub fn clear(&mut self) {
        self.operations.clear();
        self.admitted_at.clear();
        self.sender_n_ops.clear();
        self.op_index.clear();
        self.by_hash.clear();
    }

    /// O(1) lookup of an operation by its transaction hash. Returns `None` when
    /// the operation is not resident (already committed or never seen).
    pub fn get_by_hash(&self, hash: &TxHash) -> Option<Arc<Operation>> {
        self.by_hash.get(hash).cloned()
    }

    /// True if an operation with `hash` is resident in the mempool.
    pub fn contains_hash(&self, hash: &TxHash) -> bool {
        self.by_hash.contains_key(hash)
    }

    /// Remove operations whose nonce is already behind the committed account.
    /// This is needed after a block is committed by another validator while a
    /// local proposal containing the same pending operations is still alive.
    pub fn remove_stale(&mut self, lookup: &dyn AccountLookup) -> usize {
        let mut stale = Vec::new();
        for (idx, op) in self.operations.iter().enumerate() {
            let is_stale = payload_senders(&op.payload).iter().any(|sender| {
                lookup
                    .get_account(sender.account)
                    .ok()
                    .flatten()
                    .is_some_and(|account| sender.n_operation < account.n_operation)
            });
            if is_stale {
                stale.push(idx);
            }
        }

        for idx in stale.iter().rev() {
            self.by_hash.remove(&op_hash(&self.operations[*idx]));
            self.operations.remove(*idx);
            self.admitted_at.remove(*idx);
        }
        if !stale.is_empty() {
            self.rebuild_index();
        }
        stale.len()
    }

    /// Remove operations already included in `committed`.
    ///
    /// Operations with senders are removed by `(account, n_operation)`; those
    /// without senders (ValidatorAdmin, CreateAccount, …) are removed by exact
    /// operation equality so they are never re-included in a later block.
    ///
    /// This is a single bulk pass: indices are resolved against the *original*
    /// layout, removed in one go, and the by-hash / by-nonce indexes are
    /// updated once — never rebuilt linearly per operation.
    pub fn remove_committed(&mut self, committed: &[Operation]) {
        let mut indices: HashSet<usize> = HashSet::new();
        for op in committed {
            let senders = payload_senders(&op.payload);
            if senders.is_empty() {
                if let Some(idx) = self.operations.iter().position(|o| o == op) {
                    indices.insert(idx);
                }
            } else {
                for sender in senders {
                    if let Some(&idx) = self.op_index.get(&(sender.account, sender.n_operation)) {
                        indices.insert(idx);
                    }
                }
            }
        }
        if indices.is_empty() {
            return;
        }
        let mut sorted: Vec<usize> = indices.into_iter().collect();
        sorted.sort_unstable();
        for &idx in sorted.iter().rev() {
            self.by_hash.remove(&op_hash(&self.operations[idx]));
            self.operations.remove(idx);
            self.admitted_at.remove(idx);
        }
        self.rebuild_index();
    }

    /// Re-run every admission-time check for an operation that is already
    /// resident, against *current* chain state. The block builder calls this
    /// before including a pending op in a proposal: an op admitted earlier may
    /// have become invalid (state changed, balance moved, nonce consumed).
    /// Mirrors the commit-time execution checks so a stale pending op can no
    /// longer poison block production.
    pub fn precheck_for_block(
        &self,
        op: &Operation,
        lookup: &dyn AccountLookup,
    ) -> Result<(), MempoolError> {
        self.validate_signatures(op, lookup)?;
        self.validate_block_n_operations(op, lookup)?;
        self.validate_balance_and_fee(op, lookup)
    }

    /// Nonce check for the block builder, mirroring execution exactly: the
    /// op's nonce must equal the account's *current on-chain* n_operation.
    /// (Deliberately NOT the mempool chaining used at admission — the op is
    /// already resident here, so counting its own nonce would shift the
    /// expectation and reject every pending operation.)
    fn validate_block_n_operations(
        &self,
        op: &Operation,
        lookup: &dyn AccountLookup,
    ) -> Result<(), MempoolError> {
        for (account, n_operation) in self.op_nonce_pairs(op) {
            let on_chain = lookup
                .get_account(account)
                .map_err(MempoolError::Storage)?
                .map(|a| a.n_operation)
                .unwrap_or(0);
            if n_operation != on_chain {
                return Err(MempoolError::InvalidNOperation {
                    sender: account,
                    expected: on_chain,
                    actual: n_operation,
                });
            }
        }
        Ok(())
    }

    /// Quarantine: evict every pending operation of `sender`. When execution
    /// rejects one op of a sender's nonce chain, every later nonce of that
    /// sender is orphaned anyway (same cascade rule as TTL expiry).
    /// Returns the number of operations evicted.
    pub fn evict_all_for_sender(&mut self, sender: u64) -> usize {
        let mut to_remove: HashSet<usize> = HashSet::new();
        for (idx, op) in self.operations.iter().enumerate() {
            for (account, _) in self.op_nonce_pairs(op) {
                if account == sender {
                    to_remove.insert(idx);
                    break;
                }
            }
        }
        let mut evicted = 0;
        let mut sorted: Vec<usize> = to_remove.into_iter().collect();
        sorted.sort_unstable();
        for &idx in sorted.iter().rev() {
            let op = &self.operations[idx];
            let h = op_hash(op);
            eprintln!(
                "[mempool] quarantining operation of sender={sender} (failed block validation) hash={:x?}",
                &h[..8]
            );
            self.by_hash.remove(&op_hash(op));
            self.operations.remove(idx);
            self.admitted_at.remove(idx);
            evicted += 1;
        }
        if evicted > 0 {
            self.rebuild_index();
        }
        evicted
    }

    fn rebuild_index(&mut self) {
        self.op_index.clear();
        self.sender_n_ops.clear();
        for (idx, op) in self.operations.iter().enumerate() {
            for sender in payload_senders(&op.payload) {
                let key = (sender.account, sender.n_operation);
                self.op_index.insert(key, idx);
                self.sender_n_ops.insert(
                    sender.account,
                    self.sender_n_ops
                        .get(&sender.account)
                        .map_or(sender.n_operation, |&prev| prev.max(sender.n_operation)),
                );
            }
        }
    }

    fn account_for_payload(&self, payload: &OperationPayload) -> Option<u64> {
        match payload {
            OperationPayload::ChangeKey { account, .. } => Some(*account),
            OperationPayload::ChangeKeySigned { account, .. } => Some(*account),
            OperationPayload::ListAccountForSale { account, .. } => Some(*account),
            OperationPayload::DelistAccount { account, .. } => Some(*account),
            OperationPayload::BuyAccount { buyer_account, .. } => Some(*buyer_account),
            OperationPayload::ChangeAccountInfo { account, .. } => Some(*account),
            OperationPayload::Data { account, .. } => Some(*account),
            OperationPayload::Contract { account, .. } => Some(*account),
            _ => None,
        }
    }

    fn payload_n_operation(&self, payload: &OperationPayload) -> Option<u64> {
        match payload {
            OperationPayload::ChangeKey { n_operation, .. } => Some(*n_operation),
            OperationPayload::ChangeKeySigned { n_operation, .. } => Some(*n_operation),
            OperationPayload::ListAccountForSale { n_operation, .. } => Some(*n_operation),
            OperationPayload::DelistAccount { n_operation, .. } => Some(*n_operation),
            OperationPayload::BuyAccount { n_operation, .. } => Some(*n_operation),
            OperationPayload::ChangeAccountInfo { n_operation, .. } => Some(*n_operation),
            OperationPayload::Data { n_operation, .. } => Some(*n_operation),
            OperationPayload::GiftAccount { n_operation, .. } => Some(*n_operation),
            OperationPayload::AcceptGift { n_operation, .. } => Some(*n_operation),
            OperationPayload::Contract { n_operation, .. } => Some(*n_operation),
            _ => None,
        }
    }

    /// All `(account, n_operation)` pairs an operation advances. Used for the
    /// orphan cascade on TTL expiry: any pending op with a higher nonce than an
    /// expired nonce for the same sender is orphaned.
    fn op_nonce_pairs(&self, op: &Operation) -> Vec<(u64, u64)> {
        let mut pairs = Vec::new();
        match &op.payload {
            OperationPayload::Transaction { senders, .. }
            | OperationPayload::AddressTransaction { senders, .. }
            | OperationPayload::MultiOperation { senders, .. } => {
                for s in senders {
                    pairs.push((s.account, s.n_operation));
                }
            }
            OperationPayload::Data {
                account,
                n_operation,
                senders,
                ..
            } => {
                pairs.push((*account, *n_operation));
                for s in senders {
                    pairs.push((s.account, s.n_operation));
                }
            }
            _ => {
                if let (Some(account), Some(n_op)) = (
                    self.account_for_payload(&op.payload),
                    self.payload_n_operation(&op.payload),
                ) {
                    pairs.push((account, n_op));
                }
            }
        }
        pairs
    }

    fn payload_fee(&self, payload: &OperationPayload) -> u64 {
        match payload {
            OperationPayload::Transaction { fee, .. } => *fee,
            OperationPayload::AddressTransaction { fee, .. } => *fee,
            OperationPayload::MultiOperation { fee, .. } => *fee,
            OperationPayload::ChangeKey { fee, .. } => *fee,
            OperationPayload::ChangeKeySigned { fee, .. } => *fee,
            OperationPayload::ListAccountForSale { fee, .. } => *fee,
            OperationPayload::DelistAccount { fee, .. } => *fee,
            OperationPayload::BuyAccount { fee, .. } => *fee,
            OperationPayload::ChangeAccountInfo { fee, .. } => *fee,
            OperationPayload::Data { fee, .. } => *fee,
            OperationPayload::RecoverFounds { .. } => 0,
            OperationPayload::ValidatorAdmin(_) => 0,
            OperationPayload::CreateAccount { .. } => 0,
            OperationPayload::GiftAccount { fee, .. } => *fee,
            OperationPayload::AcceptGift { fee, .. } => *fee,
            OperationPayload::Contract { fee, .. } => *fee,
        }
    }

    fn validate_signatures(
        &self,
        op: &Operation,
        lookup: &dyn AccountLookup,
    ) -> Result<(), MempoolError> {
        if op.signatures.is_empty() {
            return Err(MempoolError::InvalidSignature);
        }

        let message = op.to_bytes_stripped();

        match &op.payload {
            OperationPayload::Transaction { senders, .. }
            | OperationPayload::AddressTransaction { senders, .. }
            | OperationPayload::MultiOperation { senders, .. } => {
                for (i, sender) in senders.iter().enumerate() {
                    let account = lookup
                        .get_account(sender.account)
                        .map_err(MempoolError::Storage)?
                        .ok_or(MempoolError::SenderNotFound(sender.account))?;

                    let pk = ed25519_dalek::VerifyingKey::from_bytes(
                        &account.account_info.account_key.ed25519_public_key,
                    )
                    .map_err(|_| MempoolError::InvalidSignature)?;

                    if i >= op.signatures.len() || !op.signatures[i].verify(&pk, &message) {
                        return Err(MempoolError::InvalidSignature);
                    }
                }
            }
            OperationPayload::Data {
                account, senders, ..
            } => {
                let acct = lookup
                    .get_account(*account)
                    .map_err(MempoolError::Storage)?
                    .ok_or(MempoolError::SenderNotFound(*account))?;

                let pk = ed25519_dalek::VerifyingKey::from_bytes(
                    &acct.account_info.account_key.ed25519_public_key,
                )
                .map_err(|_| MempoolError::InvalidSignature)?;

                if op.signatures.is_empty() || !op.signatures[0].verify(&pk, &message) {
                    return Err(MempoolError::InvalidSignature);
                }

                for (i, sender) in senders.iter().enumerate() {
                    let sender_acct = lookup
                        .get_account(sender.account)
                        .map_err(MempoolError::Storage)?
                        .ok_or(MempoolError::SenderNotFound(sender.account))?;

                    let spk = ed25519_dalek::VerifyingKey::from_bytes(
                        &sender_acct.account_info.account_key.ed25519_public_key,
                    )
                    .map_err(|_| MempoolError::InvalidSignature)?;

                    if i + 1 >= op.signatures.len() || !op.signatures[i + 1].verify(&spk, &message)
                    {
                        return Err(MempoolError::InvalidSignature);
                    }
                }
            }
            OperationPayload::ValidatorAdmin(_) | OperationPayload::CreateAccount { .. } => {
                let admin_pk_bytes = self
                    .admin_pubkey
                    .ok_or(MempoolError::AdminKeyNotConfigured)?;
                let pk = ed25519_dalek::VerifyingKey::from_bytes(&admin_pk_bytes)
                    .map_err(|_| MempoolError::InvalidSignature)?;
                if !op.signatures.iter().any(|sig| sig.verify(&pk, &message)) {
                    return Err(MempoolError::InvalidSignature);
                }
            }
            _ => {
                if let Some(account_num) = self.account_for_payload(&op.payload) {
                    let account = lookup
                        .get_account(account_num)
                        .map_err(MempoolError::Storage)?
                        .ok_or(MempoolError::SenderNotFound(account_num))?;

                    let pk = ed25519_dalek::VerifyingKey::from_bytes(
                        &account.account_info.account_key.ed25519_public_key,
                    )
                    .map_err(|_| MempoolError::InvalidSignature)?;

                    if !op.signatures[0].verify(&pk, &message) {
                        return Err(MempoolError::InvalidSignature);
                    }
                }
            }
        }

        Ok(())
    }

    fn validate_n_operation_chaining(
        &self,
        op: &Operation,
        lookup: &dyn AccountLookup,
    ) -> Result<(), MempoolError> {
        match &op.payload {
            OperationPayload::Transaction { senders, .. }
            | OperationPayload::AddressTransaction { senders, .. }
            | OperationPayload::MultiOperation { senders, .. } => {
                for sender in senders {
                    self.check_single_n_operation(sender.account, sender.n_operation, lookup)?;
                }
            }
            OperationPayload::Data {
                account,
                n_operation,
                senders,
                ..
            } => {
                self.check_single_n_operation(*account, *n_operation, lookup)?;
                for sender in senders {
                    self.check_single_n_operation(sender.account, sender.n_operation, lookup)?;
                }
            }
            _ => {
                if let (Some(account), Some(n_op)) = (
                    self.account_for_payload(&op.payload),
                    self.payload_n_operation(&op.payload),
                ) {
                    self.check_single_n_operation(account, n_op, lookup)?;
                }
            }
        }
        Ok(())
    }

    fn check_single_n_operation(
        &self,
        account: u64,
        n_operation: u64,
        lookup: &dyn AccountLookup,
    ) -> Result<(), MempoolError> {
        let on_chain = lookup
            .get_account(account)
            .map_err(MempoolError::Storage)?
            .map(|a| a.n_operation)
            .unwrap_or(0);

        let mempool_next = self
            .sender_n_ops
            .get(&account)
            .map_or(on_chain, |&pending| pending + 1);

        let expected = std::cmp::max(on_chain, mempool_next);

        if n_operation != expected {
            return Err(MempoolError::InvalidNOperation {
                sender: account,
                expected,
                actual: n_operation,
            });
        }

        Ok(())
    }

    fn validate_balance_and_fee(
        &self,
        op: &Operation,
        lookup: &dyn AccountLookup,
    ) -> Result<(), MempoolError> {
        use crate::account::AccountState;

        // ── CreateAccount: mirrors execution — the target AUGEID must exist
        //    and be in `Reserved` state (a Normal/Owned id can never be
        //    "created"). Without this check an admitted op fails at every
        //    commit attempt and stalls block production until TTL.
        if let OperationPayload::CreateAccount { account_number, .. } = &op.payload {
            let account = lookup
                .get_account(*account_number)
                .map_err(MempoolError::Storage)?
                .ok_or(MempoolError::SenderNotFound(*account_number))?;
            if account.account_info.state != AccountState::Reserved {
                return Err(MempoolError::InvalidAccountState {
                    account: *account_number,
                    reason: format!(
                        "createaccount requires a Reserved AUGEID, got {:?}",
                        account.account_info.state
                    ),
                });
            }
        }

        // These operations are authorized by the node admin key rather than
        // debited from an account, so they intentionally carry no fee.
        if matches!(
            &op.payload,
            OperationPayload::ValidatorAdmin(_) | OperationPayload::CreateAccount { .. }
        ) {
            return Ok(());
        }

        // ── GiftAccount: mirrors execution — a GiftPending AUGEID already has
        //    a pending gift, and any fee > 0 must be covered by the gift owner
        //    (Reserved accounts hold no balance, so only fee 0 passes there).
        if let OperationPayload::GiftAccount { account, fee, .. } = &op.payload {
            let account_data = lookup
                .get_account(*account)
                .map_err(MempoolError::Storage)?
                .ok_or(MempoolError::SenderNotFound(*account))?;
            if account_data.account_info.state == AccountState::GiftPending {
                return Err(MempoolError::InvalidAccountState {
                    account: *account,
                    reason: "AUGEID already has a pending gift".into(),
                });
            }
            if *fee > 0 && account_data.balance < *fee {
                return Err(MempoolError::InsufficientBalance {
                    account: *account,
                    balance: account_data.balance,
                    required: *fee,
                });
            }
            if *fee == 0 && account_data.account_info.state == AccountState::Reserved {
                return Ok(());
            }
        }

        // ── AcceptGift: mirrors execution — only GiftPending AUGEIDs can be
        //    accepted, and any fee > 0 must be covered (a GiftPending account
        //    holds no balance, so zero-fee is the only valid form there).
        if let OperationPayload::AcceptGift { account, fee, .. } = &op.payload {
            let account_data = lookup
                .get_account(*account)
                .map_err(MempoolError::Storage)?
                .ok_or(MempoolError::SenderNotFound(*account))?;
            if account_data.account_info.state != AccountState::GiftPending {
                return Err(MempoolError::InvalidAccountState {
                    account: *account,
                    reason: "AUGEID is not awaiting a gift accept".into(),
                });
            }
            if *fee > 0 && account_data.balance < *fee {
                return Err(MempoolError::InsufficientBalance {
                    account: *account,
                    balance: account_data.balance,
                    required: *fee,
                });
            }
            if *fee == 0 {
                // GiftPending accounts hold no balance: zero-fee accept is the
                // documented special case (mirrors execution).
                return Ok(());
            }
            // fee > 0: falls through to the min-fee floor below.
        }

        // ── Contract: mirrors execution — the sender's native balance must
        //    reserve `fee + gas_reserve` up front.
        if let OperationPayload::Contract { account, fee, .. } = &op.payload {
            let account_data = lookup
                .get_account(*account)
                .map_err(MempoolError::Storage)?
                .ok_or(MempoolError::SenderNotFound(*account))?;
            if account_data.account_info.state == AccountState::Reserved
                || account_data.account_info.state == AccountState::GiftPending
            {
                return Err(MempoolError::InvalidAccountState {
                    account: *account,
                    reason: "Reserved/GiftPending accounts cannot execute contracts".into(),
                });
            }
            let reserved = fee.saturating_add(self.config.contract_gas_reserve);
            if reserved > 0 && account_data.balance < reserved {
                return Err(MempoolError::InsufficientBalance {
                    account: *account,
                    balance: account_data.balance,
                    required: reserved,
                });
            }
            // falls through to the min-fee floor below (no bypass)
        }

        // ── Transaction / MultiOperation: mirrors execution — Reserved and
        //    GiftPending accounts can neither send nor (by account number)
        //    receive AUGE. (AddressTransaction receivers are exempt: their
        //    address-based path handles first-receive activation itself.)
        let restrict_senders = |payload: &OperationPayload| -> bool {
            matches!(
                payload,
                OperationPayload::Transaction { .. } | OperationPayload::MultiOperation { .. }
            )
        };
        if restrict_senders(&op.payload) {
            if let OperationPayload::Transaction {
                senders, receivers, ..
            }
            | OperationPayload::MultiOperation {
                senders, receivers, ..
            } = &op.payload
            {
                for sender in senders {
                    let account = lookup
                        .get_account(sender.account)
                        .map_err(MempoolError::Storage)?
                        .ok_or(MempoolError::SenderNotFound(sender.account))?;
                    if account.account_info.state == AccountState::Reserved
                        || account.account_info.state == AccountState::GiftPending
                    {
                        return Err(MempoolError::InvalidAccountState {
                            account: sender.account,
                            reason: format!(
                                "account is {:?} and cannot send AUGE",
                                account.account_info.state
                            ),
                        });
                    }
                }
                for receiver in receivers {
                    if let Some(acc) = lookup
                        .get_account(receiver.account)
                        .map_err(MempoolError::Storage)?
                    {
                        if acc.account_info.state == AccountState::Reserved
                            || acc.account_info.state == AccountState::GiftPending
                        {
                            return Err(MempoolError::InvalidAccountState {
                                account: receiver.account,
                                reason: format!(
                                    "account is {:?} and cannot receive AUGE",
                                    acc.account_info.state
                                ),
                            });
                        }
                    }
                }
            }
        }

        let fee = self.payload_fee(&op.payload);

        match &op.payload {
            OperationPayload::Transaction {
                senders, receivers, ..
            }
            | OperationPayload::MultiOperation {
                senders, receivers, ..
            } => validate_transfer_conservation(
                senders.iter().map(|sender| sender.amount),
                receivers.iter().map(|receiver| receiver.amount),
            )?,
            OperationPayload::AddressTransaction {
                senders, receivers, ..
            } => validate_transfer_conservation(
                senders.iter().map(|sender| sender.amount),
                receivers.iter().map(|receiver| receiver.amount),
            )?,
            OperationPayload::Data {
                senders, receivers, ..
            } => validate_transfer_conservation(
                senders.iter().map(|sender| sender.amount),
                receivers.iter().map(|receiver| receiver.amount),
            )?,
            _ => {}
        }

        if fee < self.config.min_fee {
            return Err(MempoolError::FeeTooLow {
                fee,
                minimum: self.config.min_fee,
            });
        }

        match &op.payload {
            OperationPayload::Transaction { senders, .. }
            | OperationPayload::MultiOperation { senders, .. } => {
                for sender in senders {
                    let total = sender.amount.saturating_add(fee);
                    let account = lookup
                        .get_account(sender.account)
                        .map_err(MempoolError::Storage)?
                        .ok_or(MempoolError::SenderNotFound(sender.account))?;

                    if account.balance < total {
                        return Err(MempoolError::InsufficientBalance {
                            account: sender.account,
                            balance: account.balance,
                            required: total,
                        });
                    }
                }
            }
            OperationPayload::BuyAccount {
                buyer_account,
                amount,
                fee,
                ..
            } => {
                let total = amount.checked_add(*fee).unwrap_or(u64::MAX);
                let account = lookup
                    .get_account(*buyer_account)
                    .map_err(MempoolError::Storage)?
                    .ok_or(MempoolError::SenderNotFound(*buyer_account))?;

                if account.balance < total {
                    return Err(MempoolError::InsufficientBalance {
                        account: *buyer_account,
                        balance: account.balance,
                        required: total,
                    });
                }
            }
            _ => {}
        }

        Ok(())
    }
}

fn validate_transfer_conservation(
    mut sender_amounts: impl Iterator<Item = u64>,
    mut receiver_amounts: impl Iterator<Item = u64>,
) -> Result<(), MempoolError> {
    let sent = sender_amounts.try_fold(0u64, |total, amount| total.checked_add(amount));
    let received = receiver_amounts.try_fold(0u64, |total, amount| total.checked_add(amount));

    match (sent, received) {
        (Some(sent), Some(received)) if sent == received => Ok(()),
        (Some(sent), Some(received)) => Err(MempoolError::UnbalancedTransfer { sent, received }),
        _ => Err(MempoolError::TransferValueOverflow),
    }
}

impl Default for Mempool {
    fn default() -> Self {
        Self::new(MempoolConfig::default())
    }
}

/// Return the sender references for a payload. A free function so callers can
/// mutate `self` while iterating the returned references.
fn payload_senders(payload: &OperationPayload) -> Vec<&SenderInfo> {
    let mut senders = Vec::new();
    match payload {
        OperationPayload::Transaction { senders: s, .. } => senders.extend(s.iter()),
        OperationPayload::AddressTransaction { senders: s, .. } => senders.extend(s.iter()),
        OperationPayload::MultiOperation { senders: s, .. } => senders.extend(s.iter()),
        OperationPayload::Data { senders: s, .. } => senders.extend(s.iter()),
        _ => {}
    }
    senders
}

#[cfg(test)]
mod mempool_tests {
    use super::*;
    use crate::operation::{OperationType, ReceiverInfo};
    use augecoin_crypto::signature::HybridKeyPair;
    use std::collections::HashMap;

    struct MockLookup {
        accounts: HashMap<u64, Account>,
    }

    impl AccountLookup for MockLookup {
        fn get_account(&self, account_number: u64) -> Result<Option<Account>, String> {
            Ok(self.accounts.get(&account_number).cloned())
        }
    }

    fn create_keypair(seed: u64) -> HybridKeyPair {
        use augecoin_crypto::hdkeys::HdWallet;
        let seed_bytes = seed.to_be_bytes();
        let mut full_seed = [0u8; 64];
        full_seed[..8].copy_from_slice(&seed_bytes);
        HdWallet::from_seed(&full_seed).derive_keypair(0)
    }

    fn create_account(
        num: u64,
        keypair: &HybridKeyPair,
        balance: u64,
        n_operation: u64,
    ) -> Account {
        let vk = keypair.verifying_key();
        let mut ed_pk = [0u8; 32];
        ed_pk.copy_from_slice(&vk.to_bytes());
        let mut acc = Account::new(num, ed_pk, 100);
        if balance > 0 {
            acc.add_balance(balance).unwrap();
        }
        acc.n_operation = n_operation;
        acc
    }

    fn sign_transaction(
        keypair: &HybridKeyPair,
        chain_id: u64,
        senders: Vec<SenderInfo>,
        receivers: Vec<ReceiverInfo>,
        fee: u64,
    ) -> Operation {
        let payload = OperationPayload::Transaction {
            senders,
            receivers,
            changers: vec![],
            fee,
        };
        let op = Operation {
            chain_id,
            op_type: OperationType::Transaction,
            payload,
            signatures: vec![],
        };
        let message = op.to_bytes_stripped();
        let signature = keypair.sign(&message);
        Operation {
            chain_id,
            op_type: OperationType::Transaction,
            payload: op.payload,
            signatures: vec![signature],
        }
    }

    fn transfer(account: u64, n_operation: u64, amount: u64) -> SenderInfo {
        SenderInfo {
            account,
            n_operation,
            amount,
            payload: vec![],
        }
    }

    fn receiver(account: u64, amount: u64) -> ReceiverInfo {
        ReceiverInfo {
            account,
            amount,
            payload: vec![],
        }
    }

    fn default_config() -> MempoolConfig {
        MempoolConfig {
            min_fee: 0,
            max_operations: 10_000,
            chain_id: 1,
            ttl_seconds: 3600,
            contract_gas_reserve: 100_000,
        }
    }

    #[test]
    fn valid_operation_is_admitted() {
        let keypair = create_keypair(42);
        let mut lookup = MockLookup {
            accounts: HashMap::new(),
        };
        lookup
            .accounts
            .insert(1, create_account(1, &keypair, 10000, 5));

        let mut mempool = Mempool::new(default_config());
        let op = sign_transaction(
            &keypair,
            1,
            vec![transfer(1, 5, 1000)],
            vec![receiver(2, 1000)],
            100,
        );
        assert!(mempool.validate_and_admit(op, &lookup, 1000).is_ok());
        assert_eq!(mempool.len(), 1);
    }

    #[test]
    fn unbalanced_transfer_is_rejected() {
        let keypair = create_keypair(42);
        let mut lookup = MockLookup {
            accounts: HashMap::new(),
        };
        lookup
            .accounts
            .insert(1, create_account(1, &keypair, 10_000, 0));

        let op = sign_transaction(
            &keypair,
            1,
            vec![transfer(1, 0, 100)],
            vec![receiver(2, 101)],
            1,
        );
        let mut mempool = Mempool::new(default_config());
        assert!(matches!(
            mempool.validate_and_admit(op, &lookup, 1000),
            Err(MempoolError::UnbalancedTransfer {
                sent: 100,
                received: 101,
            })
        ));
    }

    #[test]
    fn wrong_chain_id_is_rejected() {
        let keypair = create_keypair(42);
        let mut lookup = MockLookup {
            accounts: HashMap::new(),
        };
        lookup
            .accounts
            .insert(1, create_account(1, &keypair, 10000, 0));

        let mut mempool = Mempool::new(default_config());
        let op = sign_transaction(
            &keypair,
            2,
            vec![transfer(1, 0, 100)],
            vec![receiver(2, 100)],
            10,
        );
        assert!(matches!(
            mempool.validate_and_admit(op, &lookup, 1000),
            Err(MempoolError::WrongChainId {
                expected: 1,
                actual: 2
            })
        ));
    }

    #[test]
    fn invalid_signature_is_rejected() {
        let keypair = create_keypair(42);
        let wrong_kp = create_keypair(99);
        let mut lookup = MockLookup {
            accounts: HashMap::new(),
        };
        lookup
            .accounts
            .insert(1, create_account(1, &keypair, 10000, 0));

        let mut mempool = Mempool::new(default_config());
        let op = sign_transaction(
            &wrong_kp,
            1,
            vec![transfer(1, 0, 1000)],
            vec![receiver(2, 1000)],
            10,
        );
        assert!(matches!(
            mempool.validate_and_admit(op, &lookup, 1000),
            Err(MempoolError::InvalidSignature)
        ));
    }

    #[test]
    fn replay_n_operation_is_rejected() {
        let keypair = create_keypair(42);
        let mut lookup = MockLookup {
            accounts: HashMap::new(),
        };
        lookup
            .accounts
            .insert(1, create_account(1, &keypair, 10000, 5));

        let mut mempool = Mempool::new(default_config());
        let op = sign_transaction(
            &keypair,
            1,
            vec![transfer(1, 3, 1000)],
            vec![receiver(2, 1000)],
            10,
        );
        assert!(matches!(
            mempool.validate_and_admit(op, &lookup, 1000),
            Err(MempoolError::InvalidNOperation { .. })
        ));
    }

    #[test]
    fn insufficient_balance_is_rejected() {
        let keypair = create_keypair(42);
        let mut lookup = MockLookup {
            accounts: HashMap::new(),
        };
        lookup
            .accounts
            .insert(1, create_account(1, &keypair, 100, 0));

        let mut mempool = Mempool::new(default_config());
        let op = sign_transaction(
            &keypair,
            1,
            vec![transfer(1, 0, 500)],
            vec![receiver(2, 500)],
            50,
        );
        assert!(matches!(
            mempool.validate_and_admit(op, &lookup, 1000),
            Err(MempoolError::InsufficientBalance { .. })
        ));
    }

    #[test]
    fn duplicate_operation_is_rejected() {
        let keypair = create_keypair(42);
        let mut lookup = MockLookup {
            accounts: HashMap::new(),
        };
        lookup
            .accounts
            .insert(1, create_account(1, &keypair, 10000, 5));

        let mut mempool = Mempool::new(default_config());
        let op1 = sign_transaction(
            &keypair,
            1,
            vec![transfer(1, 5, 1000)],
            vec![receiver(2, 1000)],
            10,
        );
        let op2 = sign_transaction(
            &keypair,
            1,
            vec![transfer(1, 5, 500)],
            vec![receiver(3, 500)],
            5,
        );
        mempool.validate_and_admit(op1, &lookup, 1000).unwrap();
        assert!(matches!(
            mempool.validate_and_admit(op2, &lookup, 1000),
            Err(MempoolError::DuplicateOperation { .. })
        ));
    }

    #[test]
    fn mempool_respects_max_size() {
        let keypair = create_keypair(42);
        let mut lookup = MockLookup {
            accounts: HashMap::new(),
        };
        lookup
            .accounts
            .insert(1, create_account(1, &keypair, 100000, 0));

        let config = MempoolConfig {
            min_fee: 0,
            max_operations: 2,
            chain_id: 1,
            ttl_seconds: 3600,
            contract_gas_reserve: 100_000,
        };
        let mut mempool = Mempool::new(config);

        mempool
            .validate_and_admit(
                sign_transaction(
                    &keypair,
                    1,
                    vec![transfer(1, 0, 100)],
                    vec![receiver(2, 100)],
                    0,
                ),
                &lookup,
                1000,
            )
            .unwrap();
        mempool
            .validate_and_admit(
                sign_transaction(
                    &keypair,
                    1,
                    vec![transfer(1, 1, 100)],
                    vec![receiver(2, 100)],
                    0,
                ),
                &lookup,
                1000,
            )
            .unwrap();
        assert!(mempool.is_full());

        let result = mempool.validate_and_admit(
            sign_transaction(
                &keypair,
                1,
                vec![transfer(1, 2, 100)],
                vec![receiver(3, 100)],
                0,
            ),
            &lookup,
            1000,
        );
        assert!(matches!(result, Err(MempoolError::MempoolFull)));
    }

    #[test]
    fn evict_expired_removes_old_operations() {
        let keypair = create_keypair(42);
        let mut lookup = MockLookup {
            accounts: HashMap::new(),
        };
        lookup
            .accounts
            .insert(1, create_account(1, &keypair, 100000, 0));

        let config = MempoolConfig {
            min_fee: 0,
            max_operations: 100,
            chain_id: 1,
            ttl_seconds: 60,
            contract_gas_reserve: 100_000,
        };
        let mut mempool = Mempool::new(config);

        // Admitted at t=1000 (now), then expire at t=2000 (beyond 60s ttl).
        mempool
            .validate_and_admit(
                sign_transaction(
                    &keypair,
                    1,
                    vec![transfer(1, 0, 100)],
                    vec![receiver(2, 100)],
                    0,
                ),
                &lookup,
                1000,
            )
            .unwrap();
        assert_eq!(mempool.len(), 1);

        let evicted = mempool.evict_expired(2000);
        assert_eq!(evicted, 1);
        assert!(mempool.is_empty());
    }

    #[test]
    fn evict_expired_cascades_orphaned_nonces() {
        let keypair = create_keypair(42);
        let mut lookup = MockLookup {
            accounts: HashMap::new(),
        };
        lookup
            .accounts
            .insert(1, create_account(1, &keypair, 100000, 0));

        let config = MempoolConfig {
            min_fee: 0,
            max_operations: 100,
            chain_id: 1,
            ttl_seconds: 60,
            contract_gas_reserve: 100_000,
        };
        let mut mempool = Mempool::new(config);

        // Front of the nonce chain admitted at t=100, the next nonce later.
        mempool
            .validate_and_admit(
                sign_transaction(
                    &keypair,
                    1,
                    vec![transfer(1, 0, 100)],
                    vec![receiver(2, 100)],
                    0,
                ),
                &lookup,
                100,
            )
            .unwrap();
        mempool
            .validate_and_admit(
                sign_transaction(
                    &keypair,
                    1,
                    vec![transfer(1, 1, 100)],
                    vec![receiver(2, 100)],
                    0,
                ),
                &lookup,
                200,
            )
            .unwrap();
        assert_eq!(mempool.len(), 2);

        // At t=170 (cutoff=110) only the front (admitted at 100) is past TTL;
        // the orphaned nonce 1 must cascade-evict with it.
        let evicted = mempool.evict_expired(170);
        assert_eq!(evicted, 2, "orphaned nonce must be evicted with its front");
        assert!(mempool.is_empty());
    }

    #[test]
    fn remove_committed_removes_included_operations() {
        let keypair = create_keypair(42);
        let mut lookup = MockLookup {
            accounts: HashMap::new(),
        };
        lookup
            .accounts
            .insert(1, create_account(1, &keypair, 100000, 0));

        let mut mempool = Mempool::new(default_config());
        let op = sign_transaction(
            &keypair,
            1,
            vec![transfer(1, 0, 100)],
            vec![receiver(2, 100)],
            0,
        );
        mempool
            .validate_and_admit(op.clone(), &lookup, 1000)
            .unwrap();
        assert_eq!(mempool.len(), 1);

        mempool.remove_committed(&[op]);
        assert!(mempool.is_empty());
    }

    #[test]
    fn remove_stale_removes_operations_behind_committed_nonce() {
        let keypair = create_keypair(42);
        let mut lookup = MockLookup {
            accounts: HashMap::new(),
        };
        lookup
            .accounts
            .insert(1, create_account(1, &keypair, 100000, 0));

        let mut mempool = Mempool::new(default_config());
        let op = sign_transaction(
            &keypair,
            1,
            vec![transfer(1, 0, 100)],
            vec![receiver(2, 100)],
            0,
        );
        mempool.validate_and_admit(op, &lookup, 1000).unwrap();

        lookup.accounts.get_mut(&1).unwrap().n_operation = 1;
        assert_eq!(mempool.remove_stale(&lookup), 1);
        assert!(mempool.is_empty());
    }

    #[test]
    fn hash_index_lookup_and_removal() {
        let keypair = create_keypair(42);
        let mut lookup = MockLookup {
            accounts: HashMap::new(),
        };
        lookup
            .accounts
            .insert(1, create_account(1, &keypair, 100000, 0));

        let mut mempool = Mempool::new(default_config());
        let op = sign_transaction(
            &keypair,
            1,
            vec![transfer(1, 0, 100)],
            vec![receiver(2, 100)],
            0,
        );
        mempool
            .validate_and_admit(op.clone(), &lookup, 1000)
            .unwrap();
        let h = op_hash(&op);
        assert!(mempool.contains_hash(&h));
        assert_eq!(mempool.get_by_hash(&h).unwrap().as_ref(), &op);

        mempool.remove_committed(&[op]);
        assert!(!mempool.contains_hash(&h));
        assert!(mempool.get_by_hash(&h).is_none());
    }

    #[test]
    fn hash_index_is_absent_for_unknown_hash() {
        let mempool = Mempool::new(default_config());
        assert!(mempool.get_by_hash(&[0xAAu8; 64]).is_none());
    }

    // ── Hardening: admissão espelha execução + dedupe + quarentena ────────

    use crate::account::AccountState;

    fn sign_payload_op(
        keypair: &HybridKeyPair,
        op_type: OperationType,
        payload: OperationPayload,
        chain_id: u64,
    ) -> Operation {
        let op = Operation {
            chain_id,
            op_type,
            payload,
            signatures: vec![],
        };
        let message = op.to_bytes_stripped();
        let signature = keypair.sign(&message);
        Operation {
            chain_id,
            op_type,
            payload: op.payload,
            signatures: vec![signature],
        }
    }

    fn account_in_state(
        num: u64,
        keypair: &HybridKeyPair,
        balance: u64,
        n_operation: u64,
        state: AccountState,
    ) -> Account {
        let mut acc = create_account(num, keypair, balance, n_operation);
        acc.account_info.state = state;
        acc
    }

    fn accept_gift_op(
        keypair: &HybridKeyPair,
        account: u64,
        n_operation: u64,
        fee: u64,
    ) -> Operation {
        sign_payload_op(
            keypair,
            OperationType::AcceptGift,
            OperationPayload::AcceptGift {
                account,
                n_operation,
                fee,
            },
            1,
        )
    }

    fn contract_op(keypair: &HybridKeyPair, account: u64, n_operation: u64, fee: u64) -> Operation {
        sign_payload_op(
            keypair,
            OperationType::Contract,
            OperationPayload::Contract {
                account,
                n_operation,
                fee,
                op_type: 2,
                data: vec![],
            },
            1,
        )
    }

    fn createaccount_op(
        admin: &HybridKeyPair,
        account_number: u64,
        owner: &HybridKeyPair,
    ) -> Operation {
        let mut pubkey = [0u8; 32];
        pubkey.copy_from_slice(&owner.verifying_key().to_bytes());
        sign_payload_op(
            admin,
            OperationType::CreateAccount,
            OperationPayload::CreateAccount {
                account_number,
                pubkey,
                initial_metadata: vec![],
            },
            1,
        )
    }

    #[test]
    fn duplicate_op_hash_is_rejected() {
        // Ops sem senders (AcceptGift/GiftAccount/Contract/…) escapavam do
        // dedupe por (sender, nonce) — agora o dedupe por hash pega tudo.
        let leader = create_keypair(80);
        let member = create_keypair(81);
        let mut lookup = MockLookup {
            accounts: HashMap::new(),
        };
        lookup.accounts.insert(
            15839,
            account_in_state(15839, &leader, 0, 1, AccountState::GiftPending),
        );
        let mut mempool = Mempool::new(default_config());
        let op = accept_gift_op(&member, 15839, 1, 0);
        mempool
            .validate_and_admit(op.clone(), &lookup, 1000)
            .unwrap();
        assert!(matches!(
            mempool.validate_and_admit(op, &lookup, 1000),
            Err(MempoolError::DuplicateOpHash)
        ));
    }

    #[test]
    fn accept_gift_fee_zero_on_gift_pending_is_admitted() {
        let leader = create_keypair(80);
        let member = create_keypair(81);
        let mut lookup = MockLookup {
            accounts: HashMap::new(),
        };
        lookup.accounts.insert(
            15839,
            account_in_state(15839, &leader, 0, 1, AccountState::GiftPending),
        );
        let mut mempool = Mempool::new(default_config());
        assert!(mempool
            .validate_and_admit(accept_gift_op(&member, 15839, 1, 0), &lookup, 1000)
            .is_ok());
    }

    #[test]
    fn accept_gift_with_fee_on_zero_balance_is_rejected() {
        let leader = create_keypair(80);
        let member = create_keypair(81);
        let mut lookup = MockLookup {
            accounts: HashMap::new(),
        };
        lookup.accounts.insert(
            15839,
            account_in_state(15839, &leader, 0, 1, AccountState::GiftPending),
        );
        let mut mempool = Mempool::new(default_config());
        assert!(matches!(
            mempool.validate_and_admit(accept_gift_op(&member, 15839, 1, 1000), &lookup, 1000),
            Err(MempoolError::InsufficientBalance { .. })
        ));
    }

    #[test]
    fn accept_gift_on_non_gift_pending_is_rejected() {
        let member = create_keypair(81);
        let mut lookup = MockLookup {
            accounts: HashMap::new(),
        };
        lookup.accounts.insert(
            15839,
            account_in_state(15839, &member, 1000, 1, AccountState::Owned),
        );
        let mut mempool = Mempool::new(default_config());
        assert!(matches!(
            mempool.validate_and_admit(accept_gift_op(&member, 15839, 1, 0), &lookup, 1000),
            Err(MempoolError::InvalidAccountState { .. })
        ));
    }

    #[test]
    fn contract_without_fee_and_gas_reserve_is_rejected() {
        let kp = create_keypair(90);
        let mut lookup = MockLookup {
            accounts: HashMap::new(),
        };
        // Saldo 50k < reserve padrão 100k → a execução rejeitaria; a admissão também.
        lookup
            .accounts
            .insert(1, account_in_state(1, &kp, 50_000, 0, AccountState::Owned));
        let mut mempool = Mempool::new(default_config());
        assert!(matches!(
            mempool.validate_and_admit(contract_op(&kp, 1, 0, 0), &lookup, 1000),
            Err(MempoolError::InsufficientBalance {
                required: 100_000,
                ..
            })
        ));
    }

    #[test]
    fn contract_with_full_reserve_is_admitted() {
        let kp = create_keypair(90);
        let mut lookup = MockLookup {
            accounts: HashMap::new(),
        };
        lookup
            .accounts
            .insert(1, account_in_state(1, &kp, 200_000, 0, AccountState::Owned));
        let mut mempool = Mempool::new(default_config());
        assert!(mempool
            .validate_and_admit(contract_op(&kp, 1, 0, 0), &lookup, 1000)
            .is_ok());
    }

    #[test]
    fn contract_on_gift_pending_sender_is_rejected() {
        let leader = create_keypair(80);
        let mut lookup = MockLookup {
            accounts: HashMap::new(),
        };
        lookup.accounts.insert(
            15839,
            account_in_state(15839, &leader, 1_000_000, 1, AccountState::GiftPending),
        );
        let mut mempool = Mempool::new(default_config());
        assert!(matches!(
            mempool.validate_and_admit(contract_op(&leader, 15839, 1, 0), &lookup, 1000),
            Err(MempoolError::InvalidAccountState { .. })
        ));
    }

    #[test]
    fn createaccount_on_non_reserved_is_rejected() {
        let admin = create_keypair(50);
        let owner = create_keypair(51);
        let mut lookup = MockLookup {
            accounts: HashMap::new(),
        };
        // Account::new cria em estado Normal — exatamente o caso do incidente.
        lookup.accounts.insert(25, create_account(25, &admin, 0, 0));
        let mut mempool = Mempool::new(default_config());
        mempool.set_admin_pubkey(admin.verifying_key().to_bytes());
        assert!(matches!(
            mempool.validate_and_admit(createaccount_op(&admin, 25, &owner), &lookup, 1000),
            Err(MempoolError::InvalidAccountState { .. })
        ));
        // Em Reserved a admissão passa.
        lookup.accounts.get_mut(&25).unwrap().account_info.state = AccountState::Reserved;
        assert!(mempool
            .validate_and_admit(createaccount_op(&admin, 25, &owner), &lookup, 1000)
            .is_ok());
    }

    #[test]
    fn transaction_to_gift_pending_receiver_is_rejected() {
        let sender_kp = create_keypair(60);
        let receiver_kp = create_keypair(61);
        let mut lookup = MockLookup {
            accounts: HashMap::new(),
        };
        lookup.accounts.insert(
            1,
            account_in_state(1, &sender_kp, 100_000, 0, AccountState::Owned),
        );
        lookup.accounts.insert(
            9,
            account_in_state(9, &receiver_kp, 0, 0, AccountState::GiftPending),
        );
        let mut mempool = Mempool::new(default_config());
        assert!(matches!(
            mempool.validate_and_admit(
                sign_transaction(
                    &sender_kp,
                    1,
                    vec![transfer(1, 0, 1000)],
                    vec![receiver(9, 1000)],
                    100
                ),
                &lookup,
                1000
            ),
            Err(MempoolError::InvalidAccountState { .. })
        ));
    }

    #[test]
    fn precheck_for_block_rejects_op_whose_state_changed_after_admission() {
        let leader = create_keypair(80);
        let member = create_keypair(81);
        let mut lookup = MockLookup {
            accounts: HashMap::new(),
        };
        lookup.accounts.insert(
            15839,
            account_in_state(15839, &leader, 0, 1, AccountState::GiftPending),
        );
        let mut mempool = Mempool::new(default_config());
        let op = accept_gift_op(&member, 15839, 1, 0);
        mempool
            .validate_and_admit(op.clone(), &lookup, 1000)
            .unwrap();

        // Simula outro validador commitando o aceite: a conta vira Owned.
        lookup.accounts.get_mut(&15839).unwrap().account_info.state = AccountState::Owned;
        assert!(mempool.precheck_for_block(&op, &lookup).is_err());
    }

    #[test]
    fn evict_all_for_sender_removes_whole_chain() {
        let kp1 = create_keypair(70);
        let kp2 = create_keypair(71);
        let mut lookup = MockLookup {
            accounts: HashMap::new(),
        };
        lookup.accounts.insert(
            1,
            account_in_state(1, &kp1, 100_000, 0, AccountState::Owned),
        );
        lookup.accounts.insert(
            2,
            account_in_state(2, &kp2, 100_000, 0, AccountState::Owned),
        );
        let mut mempool = Mempool::new(default_config());
        mempool
            .validate_and_admit(
                sign_transaction(
                    &kp1,
                    1,
                    vec![transfer(1, 0, 100)],
                    vec![receiver(2, 100)],
                    0,
                ),
                &lookup,
                1000,
            )
            .unwrap();
        mempool
            .validate_and_admit(
                sign_transaction(
                    &kp1,
                    1,
                    vec![transfer(1, 1, 100)],
                    vec![receiver(2, 100)],
                    0,
                ),
                &lookup,
                1000,
            )
            .unwrap();
        mempool
            .validate_and_admit(
                sign_transaction(
                    &kp2,
                    1,
                    vec![transfer(2, 0, 100)],
                    vec![receiver(1, 100)],
                    0,
                ),
                &lookup,
                1000,
            )
            .unwrap();
        assert_eq!(mempool.len(), 3);
        assert_eq!(mempool.evict_all_for_sender(1), 2);
        assert_eq!(mempool.len(), 1);
    }
}
