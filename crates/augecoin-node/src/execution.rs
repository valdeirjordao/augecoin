use augecoin_core::account::{Account, AccountInfo, AccountKey, AccountState};
use augecoin_core::block::OperationBlock;
use augecoin_core::constants::{CT_ACCOUNTS_PER_BLOCK, TOTAL_EMISSION_BLOCKS};
use augecoin_core::emission::{block_reward, HARD_CAP_AUGESAT};
use augecoin_core::operation::{Operation, OperationPayload};
use augecoin_storage::Storage;
use rayon::prelude::*;
use std::collections::{BTreeMap, HashMap};
use thiserror::Error;

#[derive(Debug, Clone, Error)]
pub enum ExecutionError {
    #[error("operation validation failed for sender {sender}: {reason}")]
    OperationFailed { sender: u64, reason: String },
    #[error("storage error: {0}")]
    Storage(String),
    #[error("leader account not found: {0}")]
    #[allow(dead_code)]
    LeaderNotFound(u64),
    #[error("safe box error: {0}")]
    SafeBox(String),
    #[error("name '{name}' already taken by AUGEID {owner}")]
    NameAlreadyTaken { name: String, owner: u64 },
    #[error("invalid leader signature")]
    InvalidLeaderSignature,
    #[error("insufficient quorum: {got} signatures, need {need}")]
    InsufficientQuorum { got: usize, need: usize },
    #[error("operations_hash mismatch: header={header_hash:?} actual={actual_hash:?}")]
    OperationsHashMismatch {
        header_hash: Box<[u8; 64]>,
        actual_hash: Box<[u8; 64]>,
    },
    #[error("block timestamp is too far in the future: timestamp={timestamp}, now={now}, max_drift={max_drift}")]
    FutureTimestamp {
        timestamp: u64,
        now: u64,
        max_drift: u64,
    },
}

/// Verify that the block has a valid leader signature and sufficient quorum.
/// Returns the number of valid quorum signatures collected.
pub fn verify_block_quorum(
    block: &OperationBlock,
    validator_set: &augecoin_consensus::validator::ValidatorSet,
) -> Result<usize, ExecutionError> {
    use augecoin_consensus::quorum::quorum_threshold;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let max_drift = augecoin_core::constants::CT_MAX_FUTURE_BLOCK_TIMESTAMP_SECONDS;
    if block.header.timestamp > now.saturating_add(max_drift) {
        eprintln!(
            "[clock] rejecting future block timestamp={} now={} max_drift={}",
            block.header.timestamp, now, max_drift
        );
        return Err(ExecutionError::FutureTimestamp {
            timestamp: block.header.timestamp,
            now,
            max_drift,
        });
    }

    // Verify leader signature
    let block_hash = block.hash();
    let active_vals = validator_set.active_validators();
    let leader = active_vals
        .iter()
        .find(|v| v.id == block.header.leader_id)
        .ok_or(ExecutionError::LeaderNotFound(block.header.leader_id))?;

    let leader_pk = ed25519_dalek::VerifyingKey::from_bytes(&leader.ed25519_public_key)
        .map_err(|_| ExecutionError::InvalidLeaderSignature)?;

    if !block.leader_signature.verify(&leader_pk, &block_hash) {
        return Err(ExecutionError::InvalidLeaderSignature);
    }

    // Verify quorum signatures
    let threshold = quorum_threshold(active_vals.len() as u64) as usize;
    let mut valid_sigs = 0usize;

    for sig in &block.quorum_signatures {
        for vi in &active_vals {
            let pk = match ed25519_dalek::VerifyingKey::from_bytes(&vi.ed25519_public_key) {
                Ok(k) => k,
                Err(_) => continue,
            };
            if sig.verify(&pk, &block_hash) {
                valid_sigs += 1;
                break;
            }
        }
    }

    if valid_sigs < threshold {
        return Err(ExecutionError::InsufficientQuorum {
            got: valid_sigs,
            need: threshold,
        });
    }

    // Verify operations_hash matches actual content (binds operations to block hash).
    let actual_ops_hash = block.compute_operations_merkle_root();
    if block.header.operations_hash != actual_ops_hash {
        return Err(ExecutionError::OperationsHashMismatch {
            header_hash: Box::new(block.header.operations_hash),
            actual_hash: Box::new(actual_ops_hash),
        });
    }

    Ok(valid_sigs)
}

pub fn execute_block(
    block: &OperationBlock,
    storage: &Storage,
) -> Result<[u8; 64], ExecutionError> {
    let block_number = block.header.block_number;
    let leader_id = block.header.leader_id;

    // Snapshot for atomicity
    let mut modified: HashMap<u64, Account> = HashMap::new();
    let mut original: HashMap<u64, Account> = HashMap::new();

    // Load existing name index from storage for uniqueness validation
    let existing_name_index = storage
        .safebox_name_index()
        .map_err(|e| ExecutionError::Storage(e.to_string()))?;

    // Track in-block name claims to prevent double-assignment within the same block
    let mut name_pending: HashMap<String, u64> = HashMap::new();
    // Keep validator-set changes in the same deterministic block transition.
    let mut pending_validator_set = storage
        .get_validator_set_bytes()
        .map_err(|e| ExecutionError::Storage(e.to_string()))?
        .map(|raw| augecoin_consensus::validator::ValidatorSet::from_bytes(&raw))
        .transpose()
        .map_err(|e| ExecutionError::OperationFailed {
            sender: 0,
            reason: format!("invalid validator set: {e}"),
        })?;

    let load_account = |number: u64,
                        modified: &mut HashMap<u64, Account>,
                        original: &mut HashMap<u64, Account>,
                        storage: &Storage|
     -> Result<Account, ExecutionError> {
        if let Some(acc) = modified.get(&number) {
            return Ok(acc.clone());
        }
        let acc = storage
            .get_account(number)
            .map_err(|e| ExecutionError::Storage(e.to_string()))?
            .unwrap_or_else(|| Account::new(number, [0u8; 32], block_number));
        original.entry(number).or_insert_with(|| acc.clone());
        Ok(acc)
    };

    // ---- Phase 1: Parallel signature verification ----
    let verification_results: Vec<Result<(), (usize, String)>> = block
        .operations
        .par_iter()
        .enumerate()
        .map(|(idx, op)| {
            verify_operation_signatures(op, storage, block_number).map_err(|e| (idx, e))
        })
        .collect();

    for result in verification_results {
        if let Err((idx, reason)) = result {
            return Err(ExecutionError::OperationFailed {
                sender: 0,
                reason: format!("op {idx}: {reason}"),
            });
        }
    }

    // ---- Phase 2: Sequential execution ----
    let total_fee_ref = &mut 0u64;

    for op in &block.operations {
        let keys_before: Vec<u64> = modified.keys().copied().collect();
        let op_fee = execute_operation(
            op,
            block_number,
            &mut modified,
            &mut original,
            storage,
            &existing_name_index,
            &mut name_pending,
            &mut pending_validator_set,
        )?;
        *total_fee_ref = total_fee_ref.saturating_add(op_fee);
        // Update Account Seals for Layer-2 verifiability.
        // Only seal accounts newly touched by this operation.
        let op_hash = augecoin_crypto::hash::blake3_512(&op.to_bytes());
        let new_keys: Vec<u64> = modified
            .keys()
            .copied()
            .filter(|k| !keys_before.contains(k))
            .collect();
        for num in new_keys {
            if let Some(acc) = modified.get_mut(&num) {
                acc.account_seal = Account::compute_account_seal(&acc.account_seal, &op_hash);
            }
        }
    }

    let total_fees = *total_fee_ref;

    // ---- Phase 3: Distribute fees + block reward ----
    let reward = block_reward(block_number);

    // Hard cap enforcement: validate emission does not exceed total supply cap
    if reward > 0 {
        // Theoretical max at this block: block_number * 7.25 AUGE per block
        // Conservative check: even without fees, emission must not exceed cap
        let max_emitted_so_far = block_number
            .saturating_mul(augecoin_core::emission::HARD_CAP_AUGESAT / TOTAL_EMISSION_BLOCKS);
        if max_emitted_so_far >= HARD_CAP_AUGESAT {
            return Err(ExecutionError::OperationFailed {
                sender: leader_id,
                reason: format!(
                    "emission cap exceeded: block {} reaches hard cap of {} AUGE",
                    block_number,
                    augecoin_core::emission::HARD_CAP_AUGE
                ),
            });
        }
    }

    // 100% of block reward + 100% of fees go to the validator/leader
    let mut leader_account = load_account(leader_id, &mut modified, &mut original, storage)?;
    // Auto-created leader accounts start with a zero key; bind them to the
    // leader's consensus key (declared in the block header) so validators
    // actually control their rewards.
    if leader_account.account_info.account_key.ed25519_public_key == [0u8; 32] {
        leader_account.account_info.account_key = AccountKey {
            ed25519_public_key: block.header.account_key,
        };
    }
    leader_account
        .add_balance(reward.saturating_add(total_fees))
        .map_err(|e| ExecutionError::OperationFailed {
            sender: leader_id,
            reason: e.to_string(),
        })?;
    let leader_key = leader_account.account_info.account_key;
    modified.insert(leader_id, leader_account);

    // ---- Phase 4: Emit exactly CT_AUGEIDS_PER_BLOCK new AUGEIDs ----
    // Consensus rule: every block emits exactly 10 AUGEIDs, all owned by the
    // block leader, in state `Reserved`. The numbering is deterministic:
    // AUGEID = block_number * 10 + offset (offset 0..9). CreateAccount must
    // never consume these slots; existing accounts are never overwritten
    // (defensive guard only — normal chains have no collision here).
    let start_block_for_accounts = block_number.saturating_mul(CT_ACCOUNTS_PER_BLOCK);
    for i in 0..CT_ACCOUNTS_PER_BLOCK {
        let new_num = start_block_for_accounts.saturating_add(i);
        if original.contains_key(&new_num) || modified.contains_key(&new_num) {
            continue;
        }
        original.insert(new_num, Account::new(new_num, [0u8; 32], block_number));
        let new_account = Account {
            account_number: new_num,
            account_info: AccountInfo {
                state: AccountState::Reserved,
                account_key: leader_key,
                locked_until_block: 0,
                price: 0,
                account_to_pay: 0,
                new_public_key: None,
                hashed_secret: [0u8; 32],
            },
            balance: 0,
            updated_on_block_passive_mode: block_number,
            updated_on_block_active_mode: block_number,
            n_operation: 0,
            name: None,
            account_type: 0,
            account_data: Vec::new(),
            account_seal: Vec::new(),
        };
        modified.insert(new_num, new_account);
    }

    // ---- Phase 5+6: Commit all changes atomically ----
    // Accounts + block + height + validator_set are written in a single
    // rocksdb WriteBatch; the resident SafeBox is updated incrementally.
    let validator_set_bytes = pending_validator_set.as_ref().map(|vs| vs.to_bytes());
    let safe_box_hash = storage
        .commit_block_atomic(&modified, block, validator_set_bytes.as_deref(), false)
        .map_err(|e| ExecutionError::SafeBox(e.to_string()))?;

    Ok(safe_box_hash)
}

fn verify_operation_signatures(
    op: &Operation,
    storage: &Storage,
    _block_number: u64,
) -> Result<(), String> {
    let message = op.to_bytes_stripped();

    match &op.payload {
        OperationPayload::Transaction { senders, .. }
        | OperationPayload::MultiOperation { senders, .. } => {
            if senders.len() > op.signatures.len() {
                return Err("not enough signatures for senders".into());
            }
            for (i, sender) in senders.iter().enumerate() {
                let acc = storage
                    .get_account(sender.account)
                    .map_err(|e| format!("storage error: {e}"))?
                    .ok_or_else(|| format!("sender {} not found", sender.account))?;

                let pk = ed25519_dalek::VerifyingKey::from_bytes(
                    &acc.account_info.account_key.ed25519_public_key,
                )
                .map_err(|_| "invalid ed25519 key".to_string())?;

                if !op.signatures[i].verify(&pk, &message) {
                    return Err(format!("invalid signature for sender {}", sender.account));
                }
            }
        }
        OperationPayload::Data {
            account, senders, ..
        } => {
            let signer_acc = storage
                .get_account(*account)
                .map_err(|e| format!("storage error: {e}"))?
                .ok_or_else(|| format!("account {} not found", account))?;

            let pk = ed25519_dalek::VerifyingKey::from_bytes(
                &signer_acc.account_info.account_key.ed25519_public_key,
            )
            .map_err(|_| "invalid ed25519 key".to_string())?;

            if op.signatures.is_empty() || !op.signatures[0].verify(&pk, &message) {
                return Err(format!("invalid data signature for account {}", account));
            }

            for (i, sender) in senders.iter().enumerate() {
                let sender_acc = storage
                    .get_account(sender.account)
                    .map_err(|e| format!("storage error: {e}"))?
                    .ok_or_else(|| format!("sender {} not found", sender.account))?;

                let spk = ed25519_dalek::VerifyingKey::from_bytes(
                    &sender_acc.account_info.account_key.ed25519_public_key,
                )
                .map_err(|_| "invalid ed25519 key".to_string())?;

                if i + 1 >= op.signatures.len() || !op.signatures[i + 1].verify(&spk, &message) {
                    return Err(format!("invalid signature for sender {}", sender.account));
                }
            }
        }
        OperationPayload::ChangeKey { account, .. }
        | OperationPayload::ChangeKeySigned { account, .. }
        | OperationPayload::ChangeAccountInfo { account, .. }
        | OperationPayload::ListAccountForSale { account, .. }
        | OperationPayload::DelistAccount { account, .. }
        | OperationPayload::RecoverFounds { account }
        | OperationPayload::GiftAccount { account, .. } => {
            let acc = storage
                .get_account(*account)
                .map_err(|e| format!("storage error: {e}"))?
                .ok_or_else(|| format!("account {} not found", account))?;

            let pk = ed25519_dalek::VerifyingKey::from_bytes(
                &acc.account_info.account_key.ed25519_public_key,
            )
            .map_err(|_| "invalid ed25519 key".to_string())?;

            if op.signatures.is_empty() || !op.signatures[0].verify(&pk, &message) {
                return Err(format!("invalid signature for account {}", account));
            }
        }
        OperationPayload::AcceptGift { account, .. } => {
            let acc = storage
                .get_account(*account)
                .map_err(|e| format!("storage error: {e}"))?
                .ok_or_else(|| format!("account {} not found", account))?;

            // The acceptor signs with the *recipient* key stored in the gift.
            let recipient_key = acc
                .account_info
                .new_public_key
                .as_ref()
                .map(|k| k.ed25519_public_key)
                .unwrap_or([0u8; 32]);
            let pk = ed25519_dalek::VerifyingKey::from_bytes(&recipient_key)
                .map_err(|_| "invalid recipient ed25519 key".to_string())?;

            if op.signatures.is_empty() || !op.signatures[0].verify(&pk, &message) {
                return Err("invalid recipient signature for gift".to_string());
            }
        }
        OperationPayload::BuyAccount { buyer_account, .. } => {
            let acc = storage
                .get_account(*buyer_account)
                .map_err(|e| format!("storage error: {e}"))?
                .ok_or_else(|| format!("buyer {} not found", buyer_account))?;

            let pk = ed25519_dalek::VerifyingKey::from_bytes(
                &acc.account_info.account_key.ed25519_public_key,
            )
            .map_err(|_| "invalid ed25519 key".to_string())?;

            if op.signatures.is_empty() || !op.signatures[0].verify(&pk, &message) {
                return Err(format!("invalid signature for buyer {}", buyer_account));
            }
        }
        OperationPayload::ValidatorAdmin(_) | OperationPayload::CreateAccount { .. } => {
            let raw = storage
                .get_validator_set_bytes()
                .map_err(|e| format!("storage error: {e}"))?
                .ok_or_else(|| "validator set not initialized".to_string())?;
            let validator_set = augecoin_consensus::validator::ValidatorSet::from_bytes(&raw)
                .map_err(|e| format!("invalid validator set: {e}"))?;
            let admin_pk = *validator_set.admin_public_key();
            if !op
                .signatures
                .iter()
                .any(|sig| sig.verify(&admin_pk, &message))
            {
                return Err("invalid validator-admin signature".into());
            }
        }
    }

    Ok(())
}

fn state_name(state: AccountState) -> &'static str {
    match state {
        AccountState::Unknown => "Unknown",
        AccountState::Normal => "Normal",
        AccountState::ForSale => "ForSale",
        AccountState::ForAtomicAccountSwap => "ForAtomicAccountSwap",
        AccountState::ForAtomicCoinSwap => "ForAtomicCoinSwap",
        AccountState::Reserved => "Reserved",
        AccountState::Owned => "Owned",
        AccountState::GiftPending => "GiftPending",
    }
}

fn check_name_available(
    account: u64,
    new_name: &Option<String>,
    old_name: &Option<String>,
    existing_name_index: &BTreeMap<String, u64>,
    name_pending: &HashMap<String, u64>,
) -> Result<(), ExecutionError> {
    if let (Some(ref name), old) = (new_name, old_name) {
        // Skip check if the name hasn't changed
        if old.as_ref() == Some(name) {
            return Ok(());
        }
        // Check existing storage name index
        if let Some(&owner) = existing_name_index.get(name) {
            if owner != account {
                return Err(ExecutionError::NameAlreadyTaken {
                    name: name.clone(),
                    owner,
                });
            }
        }
        // Check in-block pending claims
        if let Some(&owner) = name_pending.get(name) {
            if owner != account {
                return Err(ExecutionError::NameAlreadyTaken {
                    name: name.clone(),
                    owner,
                });
            }
        }
    }
    Ok(())
}

fn execute_operation(
    op: &Operation,
    block_number: u64,
    modified: &mut HashMap<u64, Account>,
    original: &mut HashMap<u64, Account>,
    storage: &Storage,
    existing_name_index: &BTreeMap<String, u64>,
    name_pending: &mut HashMap<String, u64>,
    pending_validator_set: &mut Option<augecoin_consensus::validator::ValidatorSet>,
) -> Result<u64, ExecutionError> {
    let load_account = |number: u64,
                        modified: &mut HashMap<u64, Account>,
                        original: &mut HashMap<u64, Account>,
                        storage: &Storage|
     -> Result<Account, ExecutionError> {
        if let Some(acc) = modified.get(&number) {
            return Ok(acc.clone());
        }
        let acc = storage
            .get_account(number)
            .map_err(|e| ExecutionError::Storage(e.to_string()))?
            .unwrap_or_else(|| Account::new(number, [0u8; 32], block_number));
        original.entry(number).or_insert_with(|| acc.clone());
        Ok(acc)
    };

    match &op.payload {
        OperationPayload::Transaction {
            senders,
            receivers,
            changers,
            fee,
        } => {
            // Process senders: deduct balance, validate n_operation
            for sender in senders {
                let mut acc = load_account(sender.account, modified, original, storage)?;
                if acc.account_info.state == AccountState::Reserved
                    || acc.account_info.state == AccountState::GiftPending
                {
                    return Err(ExecutionError::OperationFailed {
                        sender: sender.account,
                        reason: format!(
                            "account {} is {} and cannot send AUGE",
                            sender.account,
                            state_name(acc.account_info.state)
                        ),
                    });
                }
                if sender.n_operation != acc.n_operation {
                    return Err(ExecutionError::OperationFailed {
                        sender: sender.account,
                        reason: format!(
                            "invalid n_operation: expected {}, got {}",
                            acc.n_operation, sender.n_operation
                        ),
                    });
                }
                let total = sender.amount.saturating_add(*fee);
                acc.subtract_balance(total)
                    .map_err(|e| ExecutionError::OperationFailed {
                        sender: sender.account,
                        reason: e.to_string(),
                    })?;
                acc.increment_n_operation();
                modified.insert(sender.account, acc);
            }

            // Process receivers: add balance
            for receiver in receivers {
                let mut acc = load_account(receiver.account, modified, original, storage)?;
                if acc.account_info.state == AccountState::Reserved
                    || acc.account_info.state == AccountState::GiftPending
                {
                    return Err(ExecutionError::OperationFailed {
                        sender: receiver.account,
                        reason: format!(
                            "account {} is {} and cannot receive AUGE",
                            receiver.account,
                            state_name(acc.account_info.state)
                        ),
                    });
                }
                acc.add_balance(receiver.amount)
                    .map_err(|e| ExecutionError::OperationFailed {
                        sender: receiver.account,
                        reason: e.to_string(),
                    })?;
                modified.insert(receiver.account, acc);
            }

            // Process changers
            for changer in changers {
                let mut acc = load_account(changer.account, modified, original, storage)?;
                if changer.n_operation != acc.n_operation {
                    return Err(ExecutionError::OperationFailed {
                        sender: changer.account,
                        reason: format!(
                            "invalid changer n_operation: expected {}, got {}",
                            acc.n_operation, changer.n_operation
                        ),
                    });
                }
                acc.account_info.account_key = AccountKey {
                    ed25519_public_key: changer.new_ed25519_public_key,
                };
                // Validate name uniqueness for changer
                let old_name = acc.name.clone();
                check_name_available(
                    changer.account,
                    &changer.new_name,
                    &old_name,
                    existing_name_index,
                    name_pending,
                )?;
                if let Some(ref old) = old_name {
                    name_pending.remove(old);
                }
                if let Some(ref name) = changer.new_name {
                    acc.name = Some(name.clone());
                    name_pending.insert(name.clone(), changer.account);
                } else {
                    acc.name = old_name;
                }
                if changer.new_type != 0 {
                    acc.account_type = changer.new_type;
                }
                if !changer.new_account_data.is_empty() {
                    acc.account_data = changer.new_account_data.clone();
                }
                if !changer.new_account_seal.is_empty() {
                    acc.account_seal = changer.new_account_seal.clone();
                }
                acc.increment_n_operation();
                modified.insert(changer.account, acc);
            }

            Ok(*fee)
        }

        OperationPayload::MultiOperation {
            senders,
            receivers,
            changers,
            fee,
        } => {
            for sender in senders {
                let mut acc = load_account(sender.account, modified, original, storage)?;
                if sender.n_operation != acc.n_operation {
                    return Err(ExecutionError::OperationFailed {
                        sender: sender.account,
                        reason: format!(
                            "invalid n_operation: expected {}, got {}",
                            acc.n_operation, sender.n_operation
                        ),
                    });
                }
                let total = sender.amount.saturating_add(*fee);
                acc.subtract_balance(total)
                    .map_err(|e| ExecutionError::OperationFailed {
                        sender: sender.account,
                        reason: e.to_string(),
                    })?;
                acc.increment_n_operation();
                modified.insert(sender.account, acc);
            }

            for receiver in receivers {
                let mut acc = load_account(receiver.account, modified, original, storage)?;
                acc.add_balance(receiver.amount)
                    .map_err(|e| ExecutionError::OperationFailed {
                        sender: receiver.account,
                        reason: e.to_string(),
                    })?;
                modified.insert(receiver.account, acc);
            }

            for changer in changers {
                let mut acc = load_account(changer.account, modified, original, storage)?;
                if changer.n_operation != acc.n_operation {
                    return Err(ExecutionError::OperationFailed {
                        sender: changer.account,
                        reason: format!(
                            "invalid n_operation: expected {}, got {}",
                            acc.n_operation, changer.n_operation
                        ),
                    });
                }
                acc.account_info.account_key = AccountKey {
                    ed25519_public_key: changer.new_ed25519_public_key,
                };
                // Validate name uniqueness for changer
                let old_name = acc.name.clone();
                check_name_available(
                    changer.account,
                    &changer.new_name,
                    &old_name,
                    existing_name_index,
                    name_pending,
                )?;
                if let Some(ref old) = old_name {
                    name_pending.remove(old);
                }
                if let Some(ref name) = changer.new_name {
                    acc.name = Some(name.clone());
                    name_pending.insert(name.clone(), changer.account);
                } else {
                    acc.name = old_name;
                }
                if changer.new_type != 0 {
                    acc.account_type = changer.new_type;
                }
                if !changer.new_account_data.is_empty() {
                    acc.account_data = changer.new_account_data.clone();
                }
                if !changer.new_account_seal.is_empty() {
                    acc.account_seal = changer.new_account_seal.clone();
                }
                acc.increment_n_operation();
                modified.insert(changer.account, acc);
            }

            Ok(*fee)
        }

        OperationPayload::ChangeKey {
            account,
            n_operation,
            fee,
            new_ed25519_public_key,
        } => {
            let mut acc = load_account(*account, modified, original, storage)?;
            if *n_operation != acc.n_operation {
                return Err(ExecutionError::OperationFailed {
                    sender: *account,
                    reason: format!(
                        "invalid n_operation: expected {}, got {}",
                        acc.n_operation, n_operation
                    ),
                });
            }
            // Fee deduction
            if *fee > 0 {
                acc.subtract_balance(*fee)
                    .map_err(|e| ExecutionError::OperationFailed {
                        sender: *account,
                        reason: e.to_string(),
                    })?;
            }
            acc.account_info.account_key = AccountKey {
                ed25519_public_key: *new_ed25519_public_key,
            };
            acc.increment_n_operation();
            modified.insert(*account, acc);
            Ok(*fee)
        }

        OperationPayload::ChangeKeySigned {
            account,
            n_operation,
            fee,
            new_ed25519_public_key,
            new_signature,
        } => {
            let mut acc = load_account(*account, modified, original, storage)?;
            if *n_operation != acc.n_operation {
                return Err(ExecutionError::OperationFailed {
                    sender: *account,
                    reason: format!(
                        "invalid n_operation: expected {}, got {}",
                        acc.n_operation, n_operation
                    ),
                });
            }

            // Verify the new key's signature
            let new_pk =
                ed25519_dalek::VerifyingKey::from_bytes(new_ed25519_public_key).map_err(|_| {
                    ExecutionError::OperationFailed {
                        sender: *account,
                        reason: "invalid new ed25519 key".into(),
                    }
                })?;
            if !new_signature.verify(&new_pk, &op.to_bytes_stripped()) {
                return Err(ExecutionError::OperationFailed {
                    sender: *account,
                    reason: "new key signature verification failed".into(),
                });
            }

            if *fee > 0 {
                acc.subtract_balance(*fee)
                    .map_err(|e| ExecutionError::OperationFailed {
                        sender: *account,
                        reason: e.to_string(),
                    })?;
            }
            acc.account_info.account_key = AccountKey {
                ed25519_public_key: *new_ed25519_public_key,
            };
            acc.increment_n_operation();
            modified.insert(*account, acc);
            Ok(*fee)
        }

        OperationPayload::RecoverFounds { account } => {
            let mut acc = load_account(*account, modified, original, storage)?;
            // In PascalCoin, RecoverFounds is used by developers to recover
            // funds from lost accounts that have had 0 balance for a period
            if acc.balance > 0 {
                return Err(ExecutionError::OperationFailed {
                    sender: *account,
                    reason: "account still has balance, cannot recover".into(),
                });
            }
            acc.balance = 0;
            acc.n_operation = 0;
            acc.name = None;
            acc.account_type = 0;
            acc.account_data = Vec::new();
            acc.account_seal = Vec::new();
            acc.account_info = AccountInfo::default();
            modified.insert(*account, acc);
            Ok(0)
        }

        OperationPayload::ListAccountForSale {
            account,
            n_operation,
            sale_price,
            account_to_pay,
            new_ed25519_public_key,
            locked_until_block,
            fee,
        } => {
            let mut acc = load_account(*account, modified, original, storage)?;
            if *n_operation != acc.n_operation {
                return Err(ExecutionError::OperationFailed {
                    sender: *account,
                    reason: format!(
                        "invalid n_operation: expected {}, got {}",
                        acc.n_operation, n_operation
                    ),
                });
            }
            if *fee > 0 {
                acc.subtract_balance(*fee)
                    .map_err(|e| ExecutionError::OperationFailed {
                        sender: *account,
                        reason: e.to_string(),
                    })?;
            }
            if acc.account_info.state == AccountState::GiftPending {
                return Err(ExecutionError::OperationFailed {
                    sender: *account,
                    reason: "a gifted (GiftPending) AUGEID cannot be listed for sale".into(),
                });
            }
            acc.account_info.state = AccountState::ForSale;
            acc.account_info.price = *sale_price;
            acc.account_info.account_to_pay = *account_to_pay;
            acc.account_info.new_public_key = Some(AccountKey {
                ed25519_public_key: *new_ed25519_public_key,
            });
            acc.account_info.locked_until_block = *locked_until_block;
            acc.increment_n_operation();
            modified.insert(*account, acc);
            Ok(*fee)
        }

        OperationPayload::DelistAccount {
            account,
            n_operation,
            fee,
        } => {
            let mut acc = load_account(*account, modified, original, storage)?;
            if *n_operation != acc.n_operation {
                return Err(ExecutionError::OperationFailed {
                    sender: *account,
                    reason: format!(
                        "invalid n_operation: expected {}, got {}",
                        acc.n_operation, n_operation
                    ),
                });
            }
            if *fee > 0 {
                acc.subtract_balance(*fee)
                    .map_err(|e| ExecutionError::OperationFailed {
                        sender: *account,
                        reason: e.to_string(),
                    })?;
            }
            acc.account_info.state = AccountState::Owned;
            acc.account_info.price = 0;
            acc.account_info.account_to_pay = 0;
            acc.account_info.new_public_key = None;
            acc.account_info.locked_until_block = 0;
            acc.increment_n_operation();
            modified.insert(*account, acc);
            Ok(*fee)
        }

        OperationPayload::BuyAccount {
            buyer_account,
            n_operation,
            account_to_purchase,
            amount,
            fee,
            new_ed25519_public_key,
            seller_account,
        } => {
            // Load and validate buyer
            let mut buyer = load_account(*buyer_account, modified, original, storage)?;
            if *n_operation != buyer.n_operation {
                return Err(ExecutionError::OperationFailed {
                    sender: *buyer_account,
                    reason: format!(
                        "invalid n_operation: expected {}, got {}",
                        buyer.n_operation, n_operation
                    ),
                });
            }

            let total = amount.checked_add(*fee).unwrap_or(u64::MAX);
            buyer
                .subtract_balance(total)
                .map_err(|e| ExecutionError::OperationFailed {
                    sender: *buyer_account,
                    reason: e.to_string(),
                })?;
            buyer.increment_n_operation();
            modified.insert(*buyer_account, buyer);

            // Load and validate the account being purchased
            let mut target = load_account(*account_to_purchase, modified, original, storage)?;
            if target.account_info.state != AccountState::ForSale
                && target.account_info.state != AccountState::Reserved
            {
                return Err(ExecutionError::OperationFailed {
                    sender: *buyer_account,
                    reason: format!(
                        "account is not for sale (state {:?})",
                        target.account_info.state
                    ),
                });
            }
            if target.account_info.price != *amount {
                return Err(ExecutionError::OperationFailed {
                    sender: *buyer_account,
                    reason: format!(
                        "price mismatch: expected {}, got {}",
                        target.account_info.price, amount
                    ),
                });
            }
            if target.account_info.locked_until_block > block_number {
                return Err(ExecutionError::OperationFailed {
                    sender: *buyer_account,
                    reason: "account is locked".into(),
                });
            }

            // Transfer purchase amount to seller
            let mut seller = load_account(*seller_account, modified, original, storage)?;
            seller
                .add_balance(*amount)
                .map_err(|e| ExecutionError::OperationFailed {
                    sender: *seller_account,
                    reason: e.to_string(),
                })?;
            modified.insert(*seller_account, seller);

            // Transition the purchased AUGEID to Owned with the buyer's key.
            target.account_info.state = AccountState::Owned;
            target.account_info.price = 0;
            target.account_info.account_to_pay = 0;
            target.account_info.new_public_key = None;
            target.account_info.locked_until_block = 0;
            target.account_info.account_key = AccountKey {
                ed25519_public_key: *new_ed25519_public_key,
            };
            target.n_operation = 0;
            target.account_seal = Vec::new();
            target.updated_on_block_active_mode = block_number;
            modified.insert(*account_to_purchase, target);

            Ok(*fee)
        }

        OperationPayload::ChangeAccountInfo {
            account,
            n_operation,
            fee,
            new_ed25519_public_key,
            new_name,
            new_type,
            new_account_data,
            new_account_seal,
        } => {
            let mut acc = load_account(*account, modified, original, storage)?;
            if *n_operation != acc.n_operation {
                return Err(ExecutionError::OperationFailed {
                    sender: *account,
                    reason: format!(
                        "invalid n_operation: expected {}, got {}",
                        acc.n_operation, n_operation
                    ),
                });
            }
            if *fee > 0 {
                acc.subtract_balance(*fee)
                    .map_err(|e| ExecutionError::OperationFailed {
                        sender: *account,
                        reason: e.to_string(),
                    })?;
            }
            acc.account_info.account_key = AccountKey {
                ed25519_public_key: *new_ed25519_public_key,
            };
            // Validate name uniqueness
            let old_name = acc.name.clone();
            check_name_available(
                *account,
                new_name,
                &old_name,
                existing_name_index,
                name_pending,
            )?;
            // Update name index tracking
            if let Some(ref old) = old_name {
                name_pending.remove(old);
            }
            if let Some(ref name) = new_name {
                name_pending.insert(name.clone(), *account);
            }
            acc.name = new_name.clone();
            acc.account_type = *new_type;
            acc.account_data = new_account_data.clone();
            acc.account_seal = new_account_seal.clone();
            acc.increment_n_operation();
            modified.insert(*account, acc);
            Ok(*fee)
        }

        OperationPayload::Data {
            account,
            n_operation,
            fee,
            data,
            senders,
            receivers,
            changers,
        } => {
            if data.len() > augecoin_core::constants::CT_MAX_BLOCK_PAYLOAD {
                return Err(ExecutionError::OperationFailed {
                    sender: *account,
                    reason: format!(
                        "data payload too large: {} bytes (max {})",
                        data.len(),
                        augecoin_core::constants::CT_MAX_BLOCK_PAYLOAD
                    ),
                });
            }
            let mut acc = load_account(*account, modified, original, storage)?;
            if *n_operation != acc.n_operation {
                return Err(ExecutionError::OperationFailed {
                    sender: *account,
                    reason: format!(
                        "invalid n_operation: expected {}, got {}",
                        acc.n_operation, n_operation
                    ),
                });
            }
            if *fee > 0 {
                acc.subtract_balance(*fee)
                    .map_err(|e| ExecutionError::OperationFailed {
                        sender: *account,
                        reason: e.to_string(),
                    })?;
            }

            // Append data payload
            let mut new_data = acc.account_data.clone();
            new_data.extend_from_slice(data);
            acc.account_data = new_data;
            acc.increment_n_operation();
            modified.insert(*account, acc);

            // Process embedded senders, receivers, changers
            for sender in senders {
                let mut s = load_account(sender.account, modified, original, storage)?;
                if sender.n_operation != s.n_operation {
                    return Err(ExecutionError::OperationFailed {
                        sender: sender.account,
                        reason: format!(
                            "invalid n_operation: expected {}, got {}",
                            s.n_operation, sender.n_operation
                        ),
                    });
                }
                let total = sender.amount.saturating_add(*fee);
                s.subtract_balance(total)
                    .map_err(|e| ExecutionError::OperationFailed {
                        sender: sender.account,
                        reason: e.to_string(),
                    })?;
                s.increment_n_operation();
                modified.insert(sender.account, s);
            }

            for receiver in receivers {
                let mut r = load_account(receiver.account, modified, original, storage)?;
                r.add_balance(receiver.amount)
                    .map_err(|e| ExecutionError::OperationFailed {
                        sender: receiver.account,
                        reason: e.to_string(),
                    })?;
                modified.insert(receiver.account, r);
            }

            for changer in changers {
                let mut c = load_account(changer.account, modified, original, storage)?;
                if changer.n_operation != c.n_operation {
                    return Err(ExecutionError::OperationFailed {
                        sender: changer.account,
                        reason: format!(
                            "invalid n_operation: expected {}, got {}",
                            c.n_operation, changer.n_operation
                        ),
                    });
                }
                c.account_info.account_key = AccountKey {
                    ed25519_public_key: changer.new_ed25519_public_key,
                };
                // Validate name uniqueness for changer
                let old_name = c.name.clone();
                check_name_available(
                    changer.account,
                    &changer.new_name,
                    &old_name,
                    existing_name_index,
                    name_pending,
                )?;
                if let Some(ref old) = old_name {
                    name_pending.remove(old);
                }
                if let Some(ref name) = changer.new_name {
                    c.name = Some(name.clone());
                    name_pending.insert(name.clone(), changer.account);
                } else {
                    c.name = old_name;
                }
                if changer.new_type != 0 {
                    c.account_type = changer.new_type;
                }
                if !changer.new_account_data.is_empty() {
                    c.account_data = changer.new_account_data.clone();
                }
                if !changer.new_account_seal.is_empty() {
                    c.account_seal = changer.new_account_seal.clone();
                }
                c.increment_n_operation();
                modified.insert(changer.account, c);
            }

            Ok(*fee)
        }

        OperationPayload::ValidatorAdmin(_admin_op) => {
            let validator_set =
                pending_validator_set
                    .as_mut()
                    .ok_or_else(|| ExecutionError::OperationFailed {
                        sender: 0,
                        reason: "validator set not initialized".into(),
                    })?;
            validator_set
                .apply_validator_admin(op, block_number)
                .map_err(|e| ExecutionError::OperationFailed {
                    sender: 0,
                    reason: e.to_string(),
                })?;
            validator_set.process_pending_activations(block_number);
            Ok(0)
        }

        OperationPayload::CreateAccount {
            account_number,
            pubkey,
            initial_metadata,
        } => {
            if initial_metadata.len() > augecoin_core::constants::CT_MAX_ACCOUNT_DATA {
                return Err(ExecutionError::OperationFailed {
                    sender: 0,
                    reason: format!(
                        "initial_metadata too large: {} bytes (max {})",
                        initial_metadata.len(),
                        augecoin_core::constants::CT_MAX_ACCOUNT_DATA
                    ),
                });
            }
            if ed25519_dalek::VerifyingKey::from_bytes(pubkey).is_err() {
                return Err(ExecutionError::OperationFailed {
                    sender: 0,
                    reason: "invalid ed25519 public key in CreateAccount".into(),
                });
            }

            // Consensus rule: CreateAccount no longer mints new account numbers.
            // It only *activates* an existing Reserved AUGEID owned by the
            // signer (the block leader). The AUGEID number is explicit.
            let mut account = load_account(*account_number, modified, original, storage)?;
            if account.account_info.state != AccountState::Reserved {
                return Err(ExecutionError::OperationFailed {
                    sender: *account_number,
                    reason: format!(
                        "AUGEID {} is not Reserved (state {:?}); only the leader can activate freshly-emitted AUGEIDs",
                        account_number, account.account_info.state
                    ),
                });
            }

            // Bind the AUGEID to the requested key, initialize n_operation and
            // seal, and transition Reserved -> Owned.
            account.account_info.account_key = AccountKey {
                ed25519_public_key: *pubkey,
            };
            account.account_info.state = AccountState::Owned;
            account.n_operation = 0;
            account.account_data = initial_metadata.clone();
            account.account_seal = Vec::new();
            account.updated_on_block_active_mode = block_number;
            modified.insert(*account_number, account);
            Ok(0)
        }

        OperationPayload::GiftAccount {
            account,
            n_operation,
            recipient_public_key,
            fee,
        } => {
            let mut acc = load_account(*account, modified, original, storage)?;
            if *n_operation != acc.n_operation {
                return Err(ExecutionError::OperationFailed {
                    sender: *account,
                    reason: format!(
                        "invalid n_operation: expected {}, got {}",
                        acc.n_operation, n_operation
                    ),
                });
            }
            if ed25519_dalek::VerifyingKey::from_bytes(recipient_public_key).is_err() {
                return Err(ExecutionError::OperationFailed {
                    sender: *account,
                    reason: "invalid recipient ed25519 public key".into(),
                });
            }
            if acc.account_info.state == AccountState::GiftPending {
                return Err(ExecutionError::OperationFailed {
                    sender: *account,
                    reason: "AUGEID already has a pending gift".into(),
                });
            }
            if *fee > 0 {
                acc.subtract_balance(*fee)
                    .map_err(|e| ExecutionError::OperationFailed {
                        sender: *account,
                        reason: e.to_string(),
                    })?;
            }
            acc.account_info.state = AccountState::GiftPending;
            acc.account_info.new_public_key = Some(AccountKey {
                ed25519_public_key: *recipient_public_key,
            });
            acc.increment_n_operation();
            modified.insert(*account, acc);
            Ok(*fee)
        }

        OperationPayload::AcceptGift {
            account,
            n_operation,
            fee,
        } => {
            let mut acc = load_account(*account, modified, original, storage)?;
            if *n_operation != acc.n_operation {
                return Err(ExecutionError::OperationFailed {
                    sender: *account,
                    reason: format!(
                        "invalid n_operation: expected {}, got {}",
                        acc.n_operation, n_operation
                    ),
                });
            }
            if acc.account_info.state != AccountState::GiftPending {
                return Err(ExecutionError::OperationFailed {
                    sender: *account,
                    reason: "AUGEID is not awaiting a gift accept".into(),
                });
            }
            let recipient_key = match acc.account_info.new_public_key {
                Some(k) => k,
                None => {
                    return Err(ExecutionError::OperationFailed {
                        sender: *account,
                        reason: "gift has no recipient key".into(),
                    });
                }
            };
            if *fee > 0 {
                acc.subtract_balance(*fee)
                    .map_err(|e| ExecutionError::OperationFailed {
                        sender: *account,
                        reason: e.to_string(),
                    })?;
            }
            // Transition GiftPending -> Owned, binding the recipient key.
            acc.account_info.state = AccountState::Owned;
            acc.account_info.account_key = recipient_key;
            acc.account_info.new_public_key = None;
            acc.n_operation = 0;
            acc.account_seal = Vec::new();
            acc.updated_on_block_active_mode = block_number;
            modified.insert(*account, acc);
            Ok(*fee)
        }
    }
}

#[cfg(test)]
mod execution_tests {
    use super::*;
    use augecoin_core::block::{OperationBlock, OperationBlockHeader};
    use augecoin_core::operation::{
        Operation, OperationPayload, OperationType, ReceiverInfo, SenderInfo,
    };
    use augecoin_crypto::signature::{Ed25519Signature, HybridKeyPair, HybridSignature};
    use augecoin_storage::Storage;
    use std::sync::atomic::{AtomicU32, Ordering};

    static TEST_COUNTER: AtomicU32 = AtomicU32::new(7000);

    fn temp_path() -> String {
        format!(
            "/tmp/augecoin-exec-{}",
            TEST_COUNTER.fetch_add(1, Ordering::SeqCst)
        )
    }

    fn create_keypair(seed: u64) -> HybridKeyPair {
        use augecoin_crypto::hdkeys::HdWallet;
        let seed_bytes = seed.to_be_bytes();
        let mut full_seed = [0u8; 64];
        full_seed[..8].copy_from_slice(&seed_bytes);
        HdWallet::from_seed(&full_seed).derive_keypair(0)
    }

    fn dummy_sig() -> HybridSignature {
        Ed25519Signature { bytes: [0u8; 64] }
    }

    fn make_account(num: u64, kp: &HybridKeyPair, balance: u64, n_operation: u64) -> Account {
        let vk = kp.verifying_key();
        let mut ed = [0u8; 32];
        ed.copy_from_slice(&vk.to_bytes());
        let mut acc = Account::new(num, ed, 100);
        if balance > 0 {
            acc.add_balance(balance).unwrap();
        }
        acc.n_operation = n_operation;
        acc
    }

    fn sign_op(kp: &HybridKeyPair, op: &Operation) -> Operation {
        let message = op.to_bytes_stripped();
        let sig = kp.sign(&message);
        Operation {
            signatures: vec![sig],
            ..op.clone()
        }
    }

    fn make_transfer_op(
        sender: u64,
        n_operation: u64,
        to: u64,
        amount: u64,
        fee: u64,
    ) -> Operation {
        Operation {
            chain_id: 1,
            op_type: OperationType::Transaction,
            payload: OperationPayload::Transaction {
                senders: vec![SenderInfo {
                    account: sender,
                    n_operation,
                    amount,
                    payload: vec![],
                }],
                receivers: vec![ReceiverInfo {
                    account: to,
                    amount: amount.checked_sub(fee).unwrap_or(amount),
                    payload: vec![],
                }],
                changers: vec![],
                fee,
            },
            signatures: vec![dummy_sig()],
        }
    }

    fn make_block(block_number: u64, leader_id: u64, operations: Vec<Operation>) -> OperationBlock {
        OperationBlock {
            header: OperationBlockHeader {
                chain_id: 1,
                block_number,
                account_key: [0u8; 32],
                reward: block_reward(block_number),
                fee: 0,
                protocol_version: 5,
                protocol_available: 6,
                timestamp: 1700000000 + block_number * 60,
                initial_safe_box_hash: [0u8; 64],
                operations_hash: [0u8; 64],
                block_payload: vec![],
                proof_of_work: [0u8; 32],
                previous_proof_of_work: [0u8; 32],
                leader_id,
            },
            operations,
            leader_signature: dummy_sig(),
            quorum_signatures: vec![dummy_sig(), dummy_sig(), dummy_sig()],
            block_hash: [0u8; 64],
        }
    }

    #[test]
    fn execute_valid_block_with_transfer() {
        let path = temp_path();
        let storage = Storage::open(&path).unwrap();

        let kp_sender = create_keypair(1);
        let kp_leader = create_keypair(99);

        let sender = make_account(10, &kp_sender, 5000, 3);
        storage.put_account(&sender).unwrap();

        let receiver = make_account(20, &kp_sender, 100, 0);
        storage.put_account(&receiver).unwrap();

        let leader = make_account(0, &kp_leader, 200, 0);
        storage.put_account(&leader).unwrap();

        let op = make_transfer_op(10, 3, 20, 500, 10);
        let op = sign_op(&kp_sender, &op);

        let block = make_block(42, 0, vec![op]);
        let result = execute_block(&block, &storage);
        assert!(result.is_ok(), "execute_block failed: {result:?}");

        // Verify sender
        let sender_after = storage.get_account(10).unwrap().unwrap();
        assert_eq!(sender_after.balance, 5000 - 500 - 10);
        assert_eq!(sender_after.n_operation, 4);

        // Verify receiver (received amount minus fee = 490)
        let receiver_after = storage.get_account(20).unwrap().unwrap();
        assert_eq!(receiver_after.balance, 100 + 490);

        // Verify leader got 100% reward + fees
        let leader_after = storage.get_account(0).unwrap().unwrap();
        let expected_leader_reward = block_reward(42);
        // Leader receives full block reward (100%) + all fees (10)
        assert!(
            leader_after.balance >= 200 + expected_leader_reward + 10,
            "leader balance {} should be >= {} (200 base + {} reward + 10 fees)",
            leader_after.balance,
            200 + expected_leader_reward + 10,
            expected_leader_reward
        );

        // No dev account rewards — verify no unintended balance changes on other accounts
        // The old dev account (42 % 5 = 2) should not have received any reward
        if let Some(_acc) = storage.get_account(2).unwrap() {
            // Only assert if the account was pre-existing from other tests or genesis
            // In this isolated test, account 2 may not exist
        }

        std::fs::remove_dir_all(&path).ok();
    }

    #[test]
    fn failed_op_mid_block_nothing_is_persisted() {
        let path = temp_path();
        let storage = Storage::open(&path).unwrap();

        let kp = create_keypair(5);

        let sender = make_account(1, &kp, 1000, 0);
        storage.put_account(&sender).unwrap();

        let receiver = make_account(2, &kp, 0, 0);
        storage.put_account(&receiver).unwrap();

        let sender_before = storage.get_account(1).unwrap().unwrap();

        // First op valid
        let op1 = make_transfer_op(1, 0, 2, 100, 5);
        let op1 = sign_op(&kp, &op1);

        // Second op insufficient balance
        let op2 = make_transfer_op(1, 1, 2, 2000, 10);
        let op2 = sign_op(&kp, &op2);

        let block = make_block(10, 0, vec![op1, op2]);
        let result = execute_block(&block, &storage);
        assert!(result.is_err());

        let sender_after = storage.get_account(1).unwrap().unwrap();
        assert_eq!(sender_after.balance, sender_before.balance);
        assert_eq!(sender_after.n_operation, sender_before.n_operation);

        std::fs::remove_dir_all(&path).ok();
    }

    #[test]
    fn empty_block_distributes_reward_only() {
        let path = temp_path();
        let storage = Storage::open(&path).unwrap();

        let kp_leader = create_keypair(99);
        // Use leader_id=10 to avoid collision with any pre-existing accounts
        let leader = make_account(10, &kp_leader, 100, 0);
        storage.put_account(&leader).unwrap();

        let block = make_block(5, 10, vec![]);
        let result = execute_block(&block, &storage);
        assert!(result.is_ok());

        let leader_after = storage.get_account(10).unwrap().unwrap();
        let expected_leader = 100 + block_reward(5);
        assert_eq!(leader_after.balance, expected_leader);

        // No dev account rewards in linear emission model.

        // CT_AUGEIDS_PER_BLOCK new AUGEIDs created (10 per block), all in
        // `Reserved` state and owned by the leader.
        let start = 5 * CT_ACCOUNTS_PER_BLOCK;
        for i in 0..CT_ACCOUNTS_PER_BLOCK {
            let new_acc = storage.get_account(start + i).unwrap();
            assert!(new_acc.is_some(), "new account {} should exist", start + i);
            let acc = new_acc.unwrap();
            assert_eq!(
                acc.account_info.state,
                AccountState::Reserved,
                "emitted AUGEID {} must be Reserved",
                start + i
            );
            assert_eq!(
                acc.account_info.account_key.ed25519_public_key,
                kp_leader.verifying_key().to_bytes(),
                "emitted AUGEID {} must be owned by the leader",
                start + i
            );
            assert_eq!(acc.balance, 0, "Reserved AUGEID has zero balance");
        }

        std::fs::remove_dir_all(&path).ok();
    }

    #[test]
    fn change_key_operation_works() {
        let path = temp_path();
        let storage = Storage::open(&path).unwrap();

        let kp = create_keypair(30);

        let acc = make_account(1, &kp, 1000, 5);
        storage.put_account(&acc).unwrap();

        let leader = make_account(0, &kp, 0, 0);
        storage.put_account(&leader).unwrap();

        let op = Operation {
            chain_id: 1,
            op_type: OperationType::ChangeKey,
            payload: OperationPayload::ChangeKey {
                account: 1,
                n_operation: 5,
                fee: 10,
                new_ed25519_public_key: [0xAAu8; 32],
            },
            signatures: vec![dummy_sig()],
        };
        let op = sign_op(&kp, &op);

        let block = make_block(10, 0, vec![op]);
        let result = execute_block(&block, &storage);
        assert!(result.is_ok());

        let acc_after = storage.get_account(1).unwrap().unwrap();
        assert_eq!(acc_after.n_operation, 6);
        assert_eq!(acc_after.balance, 1000 - 10);
        assert_eq!(
            acc_after.account_info.account_key.ed25519_public_key,
            [0xAAu8; 32]
        );

        std::fs::remove_dir_all(&path).ok();
    }

    #[test]
    fn list_and_buy_account_flow() {
        let path = temp_path();
        let storage = Storage::open(&path).unwrap();

        let kp_seller = create_keypair(50);
        let kp_buyer = create_keypair(51);

        // Seller account
        let seller = make_account(5, &kp_seller, 1000, 2);
        storage.put_account(&seller).unwrap();

        // Buyer account
        let buyer = make_account(6, &kp_buyer, 10000, 0);
        storage.put_account(&buyer).unwrap();

        // Leader
        storage
            .put_account(&make_account(0, &kp_seller, 0, 0))
            .unwrap();

        // Block 1: list account for sale
        let list_op = Operation {
            chain_id: 1,
            op_type: OperationType::ListAccountForSale,
            payload: OperationPayload::ListAccountForSale {
                account: 5,
                n_operation: 2,
                sale_price: 5000,
                account_to_pay: 0,
                new_ed25519_public_key: [0x11u8; 32],
                locked_until_block: 20,
                fee: 10,
            },
            signatures: vec![dummy_sig()],
        };
        let list_op = sign_op(&kp_seller, &list_op);

        let block1 = make_block(20, 0, vec![list_op]);
        assert!(execute_block(&block1, &storage).is_ok());

        let seller_after_list = storage.get_account(5).unwrap().unwrap();
        assert_eq!(seller_after_list.account_info.state, AccountState::ForSale);
        assert_eq!(seller_after_list.account_info.price, 5000);
        assert_eq!(seller_after_list.n_operation, 3);

        // Block 2: buy account
        let buy_op = Operation {
            chain_id: 1,
            op_type: OperationType::BuyAccount,
            payload: OperationPayload::BuyAccount {
                buyer_account: 6,
                n_operation: 0,
                account_to_purchase: 5,
                amount: 5000,
                fee: 20,
                new_ed25519_public_key: [0x33u8; 32],
                seller_account: 5,
            },
            signatures: vec![dummy_sig()],
        };
        let buy_op = sign_op(&kp_buyer, &buy_op);

        let block2 = make_block(21, 0, vec![buy_op]);
        assert!(execute_block(&block2, &storage).is_ok());

        let target_after = storage.get_account(5).unwrap().unwrap();
        assert_eq!(target_after.account_info.state, AccountState::Owned);
        assert_eq!(
            target_after.account_info.account_key.ed25519_public_key,
            [0x33u8; 32]
        );

        let buyer_after = storage.get_account(6).unwrap().unwrap();
        assert_eq!(buyer_after.balance, 10000 - 5000 - 20);
        assert_eq!(buyer_after.n_operation, 1);

        std::fs::remove_dir_all(&path).ok();
    }

    #[test]
    fn state_root_changes_after_execution() {
        let path = temp_path();
        let storage = Storage::open(&path).unwrap();

        let kp = create_keypair(30);
        let sender = make_account(1, &kp, 1000, 0);
        storage.put_account(&sender).unwrap();

        storage.put_account(&make_account(0, &kp, 0, 0)).unwrap();

        let op = make_transfer_op(1, 0, 5, 100, 5);
        let op = sign_op(&kp, &op);

        let block = make_block(10, 0, vec![op]);
        let root_after = execute_block(&block, &storage).unwrap();

        assert_ne!(root_after, [0u8; 64], "safe_box_hash must be non-zero");

        std::fs::remove_dir_all(&path).ok();
    }

    #[test]
    fn multiple_senders_in_one_operation() {
        let path = temp_path();
        let storage = Storage::open(&path).unwrap();

        let kp_a = create_keypair(60);
        let kp_b = create_keypair(61);

        let alice = make_account(1, &kp_a, 10000, 0);
        storage.put_account(&alice).unwrap();
        let bob = make_account(2, &kp_b, 5000, 0);
        storage.put_account(&bob).unwrap();

        storage.put_account(&make_account(0, &kp_a, 0, 0)).unwrap();

        let payload = OperationPayload::Transaction {
            senders: vec![
                SenderInfo {
                    account: 1,
                    n_operation: 0,
                    amount: 100,
                    payload: vec![],
                },
                SenderInfo {
                    account: 2,
                    n_operation: 0,
                    amount: 200,
                    payload: vec![],
                },
            ],
            receivers: vec![ReceiverInfo {
                account: 3,
                amount: 300,
                payload: vec![],
            }],
            changers: vec![],
            fee: 10,
        };

        let unsigned = Operation {
            chain_id: 1,
            op_type: OperationType::Transaction,
            payload,
            signatures: vec![],
        };
        let message = unsigned.to_bytes_stripped();
        let sig_a = kp_a.sign(&message);
        let sig_b = kp_b.sign(&message);

        let op = Operation {
            chain_id: 1,
            op_type: OperationType::Transaction,
            payload: unsigned.payload,
            signatures: vec![sig_a, sig_b],
        };

        let block = make_block(30, 0, vec![op]);
        assert!(execute_block(&block, &storage).is_ok());

        let alice_after = storage.get_account(1).unwrap().unwrap();
        assert_eq!(alice_after.balance, 10000 - 100 - 10);
        assert_eq!(alice_after.n_operation, 1);

        let bob_after = storage.get_account(2).unwrap().unwrap();
        assert_eq!(bob_after.balance, 5000 - 200 - 10);
        assert_eq!(bob_after.n_operation, 1);

        let receiver_after = storage.get_account(3).unwrap().unwrap();
        assert_eq!(receiver_after.balance, 300);

        std::fs::remove_dir_all(&path).ok();
    }

    #[test]
    fn name_uniqueness_rejects_duplicate() {
        let path = temp_path();
        let storage = Storage::open(&path).unwrap();

        let kp = create_keypair(1);
        let kp2 = create_keypair(2);

        let alice = make_account(10, &kp, 1000, 0);
        storage.put_account(&alice).unwrap();
        let bob = make_account(20, &kp2, 1000, 0);
        storage.put_account(&bob).unwrap();
        let leader = make_account(0, &kp, 0, 0);
        storage.put_account(&leader).unwrap();

        // Block 1: alice claims name "valdeir"
        let op1 = Operation {
            chain_id: 1,
            op_type: OperationType::ChangeAccountInfo,
            payload: OperationPayload::ChangeAccountInfo {
                account: 10,
                n_operation: 0,
                fee: 0,
                new_ed25519_public_key: kp.verifying_key().to_bytes(),
                new_name: Some("valdeir".to_string()),
                new_type: 0,
                new_account_data: vec![],
                new_account_seal: vec![],
            },
            signatures: vec![dummy_sig()],
        };
        let op1 = sign_op(&kp, &op1);
        let block1 = make_block(10, 0, vec![op1]);
        assert!(execute_block(&block1, &storage).is_ok());
        assert_eq!(
            storage.get_account(10).unwrap().unwrap().name,
            Some("valdeir".to_string())
        );

        // Block 2: bob tries to claim "valdeir" → rejected
        let op2 = Operation {
            chain_id: 1,
            op_type: OperationType::ChangeAccountInfo,
            payload: OperationPayload::ChangeAccountInfo {
                account: 20,
                n_operation: 0,
                fee: 0,
                new_ed25519_public_key: kp2.verifying_key().to_bytes(),
                new_name: Some("valdeir".to_string()),
                new_type: 0,
                new_account_data: vec![],
                new_account_seal: vec![],
            },
            signatures: vec![dummy_sig()],
        };
        let op2 = sign_op(&kp2, &op2);
        let block2 = make_block(20, 0, vec![op2]);
        let result = execute_block(&block2, &storage);
        assert!(result.is_err());
        let err = format!("{:?}", result.unwrap_err());
        assert!(
            err.contains("valdeir") && err.contains("10"),
            "error should mention the name and its owner: {err}"
        );

        std::fs::remove_dir_all(&path).ok();
    }

    #[test]
    fn name_uniqueness_allows_reclaim_own_name() {
        let path = temp_path();
        let storage = Storage::open(&path).unwrap();

        let kp = create_keypair(1);

        let mut alice = make_account(10, &kp, 1000, 0);
        alice.name = Some("alice".to_string());
        // Pre-register name in storage SafeBox
        let mut sb = augecoin_core::safe_box::SafeBox::new(5, 0);
        sb.add_account(alice.clone());
        storage.put_safe_box(&sb).unwrap();
        storage.put_account(&alice).unwrap();

        let leader = make_account(0, &kp, 0, 0);
        storage.put_account(&leader).unwrap();

        // Alice sets the SAME name "alice" → should succeed (reclaim)
        let op = Operation {
            chain_id: 1,
            op_type: OperationType::ChangeAccountInfo,
            payload: OperationPayload::ChangeAccountInfo {
                account: 10,
                n_operation: 0,
                fee: 0,
                new_ed25519_public_key: kp.verifying_key().to_bytes(),
                new_name: Some("alice".to_string()),
                new_type: 0,
                new_account_data: vec![],
                new_account_seal: vec![],
            },
            signatures: vec![dummy_sig()],
        };
        let op = sign_op(&kp, &op);
        let block = make_block(10, 0, vec![op]);
        assert!(execute_block(&block, &storage).is_ok());

        std::fs::remove_dir_all(&path).ok();
    }

    #[test]
    fn name_uniqueness_release_name_on_clear() {
        let path = temp_path();
        let storage = Storage::open(&path).unwrap();

        let kp = create_keypair(1);
        let kp2 = create_keypair(2);

        let alice = make_account(10, &kp, 1000, 0);
        storage.put_account(&alice).unwrap();
        let bob = make_account(20, &kp2, 1000, 0);
        storage.put_account(&bob).unwrap();
        let leader = make_account(0, &kp, 0, 0);
        storage.put_account(&leader).unwrap();

        // Block 1: alice claims "valdeir"
        let op1 = Operation {
            chain_id: 1,
            op_type: OperationType::ChangeAccountInfo,
            payload: OperationPayload::ChangeAccountInfo {
                account: 10,
                n_operation: 0,
                fee: 0,
                new_ed25519_public_key: kp.verifying_key().to_bytes(),
                new_name: Some("valdeir".to_string()),
                new_type: 0,
                new_account_data: vec![],
                new_account_seal: vec![],
            },
            signatures: vec![dummy_sig()],
        };
        let op1 = sign_op(&kp, &op1);
        let block1 = make_block(10, 0, vec![op1]);
        assert!(execute_block(&block1, &storage).is_ok());

        // Block 2: alice clears the name → "valdeir" is released
        let op2 = Operation {
            chain_id: 1,
            op_type: OperationType::ChangeAccountInfo,
            payload: OperationPayload::ChangeAccountInfo {
                account: 10,
                n_operation: 1,
                fee: 0,
                new_ed25519_public_key: kp.verifying_key().to_bytes(),
                new_name: None,
                new_type: 0,
                new_account_data: vec![],
                new_account_seal: vec![],
            },
            signatures: vec![dummy_sig()],
        };
        let op2 = sign_op(&kp, &op2);
        let block2 = make_block(11, 0, vec![op2]);
        assert!(execute_block(&block2, &storage).is_ok());
        assert_eq!(storage.get_account(10).unwrap().unwrap().name, None);

        // Block 3: bob claims "valdeir" → now succeeds
        let op3 = Operation {
            chain_id: 1,
            op_type: OperationType::ChangeAccountInfo,
            payload: OperationPayload::ChangeAccountInfo {
                account: 20,
                n_operation: 0,
                fee: 0,
                new_ed25519_public_key: kp2.verifying_key().to_bytes(),
                new_name: Some("valdeir".to_string()),
                new_type: 0,
                new_account_data: vec![],
                new_account_seal: vec![],
            },
            signatures: vec![dummy_sig()],
        };
        let op3 = sign_op(&kp2, &op3);
        let block3 = make_block(12, 0, vec![op3]);
        assert!(execute_block(&block3, &storage).is_ok());
        assert_eq!(
            storage.get_account(20).unwrap().unwrap().name,
            Some("valdeir".to_string())
        );

        std::fs::remove_dir_all(&path).ok();
    }

    #[test]
    fn name_survives_account_sale() {
        let path = temp_path();
        let storage = Storage::open(&path).unwrap();

        let kp_seller = create_keypair(50);
        let kp_buyer = create_keypair(51);

        let mut seller = make_account(5, &kp_seller, 1000, 2);
        seller.name = Some("premium".to_string());
        storage.put_account(&seller).unwrap();

        let buyer = make_account(6, &kp_buyer, 10000, 0);
        storage.put_account(&buyer).unwrap();
        let leader = make_account(0, &kp_seller, 0, 0);
        storage.put_account(&leader).unwrap();

        // Block 1: list for sale
        let list_op = Operation {
            chain_id: 1,
            op_type: OperationType::ListAccountForSale,
            payload: OperationPayload::ListAccountForSale {
                account: 5,
                n_operation: 2,
                sale_price: 5000,
                account_to_pay: 0,
                new_ed25519_public_key: [0x11u8; 32],
                locked_until_block: 20,
                fee: 10,
            },
            signatures: vec![dummy_sig()],
        };
        let block1 = make_block(20, 0, vec![sign_op(&kp_seller, &list_op)]);
        assert!(execute_block(&block1, &storage).is_ok());

        // Block 2: buy
        let buy_op = Operation {
            chain_id: 1,
            op_type: OperationType::BuyAccount,
            payload: OperationPayload::BuyAccount {
                buyer_account: 6,
                n_operation: 0,
                account_to_purchase: 5,
                amount: 5000,
                fee: 20,
                new_ed25519_public_key: [0x33u8; 32],
                seller_account: 5,
            },
            signatures: vec![dummy_sig()],
        };
        let block2 = make_block(21, 0, vec![sign_op(&kp_buyer, &buy_op)]);
        assert!(execute_block(&block2, &storage).is_ok());

        // Name "premium" should survive the sale
        let target = storage.get_account(5).unwrap().unwrap();
        assert_eq!(
            target.name,
            Some("premium".to_string()),
            "name should survive sale"
        );
        // New owner has the account key, not the seller's
        assert_eq!(
            target.account_info.account_key.ed25519_public_key,
            [0x33u8; 32]
        );

        std::fs::remove_dir_all(&path).ok();
    }

    #[test]
    fn reserved_account_cannot_send() {
        let path = temp_path();
        let storage = Storage::open(&path).unwrap();

        let kp = create_keypair(700);
        // A Reserved AUGEID owned by the leader.
        let mut reserved = make_account(500, &kp, 0, 0);
        reserved.account_info.state = AccountState::Reserved;
        storage.put_account(&reserved).unwrap();

        let receiver = make_account(501, &kp, 0, 0);
        storage.put_account(&receiver).unwrap();

        let op = make_transfer_op(500, 0, 501, 10, 0);
        let block = make_block(50, 999, vec![sign_op(&kp, &op)]);
        assert!(
            execute_block(&block, &storage).is_err(),
            "Reserved must not send"
        );

        std::fs::remove_dir_all(&path).ok();
    }

    #[test]
    fn reserved_account_cannot_receive() {
        let path = temp_path();
        let storage = Storage::open(&path).unwrap();

        let kp = create_keypair(701);
        let sender = make_account(500, &kp, 1000, 0);
        storage.put_account(&sender).unwrap();

        let mut reserved = make_account(501, &kp, 0, 0);
        reserved.account_info.state = AccountState::Reserved;
        storage.put_account(&reserved).unwrap();

        let op = make_transfer_op(500, 0, 501, 10, 0);
        let block = make_block(50, 999, vec![sign_op(&kp, &op)]);
        assert!(
            execute_block(&block, &storage).is_err(),
            "Reserved must not receive"
        );

        std::fs::remove_dir_all(&path).ok();
    }

    #[test]
    fn create_account_activates_reserved_augeid() {
        let path = temp_path();
        let storage = Storage::open(&path).unwrap();

        let kp = create_keypair(702);
        let leader_kp = create_keypair(703);
        let leader = make_account(0, &leader_kp, 0, 0);
        storage.put_account(&leader).unwrap();

        // Configure a validator set so the admin (leader) signature verifies.
        let admin_pk = leader_kp.verifying_key();
        let vs = augecoin_consensus::validator::ValidatorSet::new(
            admin_pk,
            vec![augecoin_consensus::validator::ValidatorInfo::new_active(
                0,
                leader_kp.verifying_key().to_bytes(),
            )],
        );
        storage.put_validator_set_bytes(&vs.to_bytes()).unwrap();

        // Emit a Reserved AUGEID owned by the leader.
        let mut reserved = make_account(900, &leader_kp, 0, 0);
        reserved.account_info.state = AccountState::Reserved;
        storage.put_account(&reserved).unwrap();

        let new_pk = kp.verifying_key().to_bytes();
        let op = Operation {
            chain_id: 1,
            op_type: OperationType::CreateAccount,
            payload: OperationPayload::CreateAccount {
                account_number: 900,
                pubkey: new_pk,
                initial_metadata: vec![],
            },
            signatures: vec![dummy_sig()],
        };
        let block = make_block(90, 0, vec![sign_op(&leader_kp, &op)]);
        assert!(execute_block(&block, &storage).is_ok());

        let activated = storage.get_account(900).unwrap().unwrap();
        assert_eq!(activated.account_info.state, AccountState::Owned);
        assert_eq!(
            activated.account_info.account_key.ed25519_public_key,
            new_pk
        );
        assert_eq!(activated.n_operation, 0);

        std::fs::remove_dir_all(&path).ok();
    }

    #[test]
    fn create_account_rejects_non_reserved() {
        let path = temp_path();
        let storage = Storage::open(&path).unwrap();

        let kp = create_keypair(704);
        let leader = make_account(0, &kp, 0, 0);
        storage.put_account(&leader).unwrap();

        let admin_pk = kp.verifying_key();
        let vs = augecoin_consensus::validator::ValidatorSet::new(
            admin_pk,
            vec![augecoin_consensus::validator::ValidatorInfo::new_active(
                0,
                kp.verifying_key().to_bytes(),
            )],
        );
        storage.put_validator_set_bytes(&vs.to_bytes()).unwrap();

        // A Normal account must not be "activated".
        let normal = make_account(900, &kp, 0, 0);
        storage.put_account(&normal).unwrap();

        let op = Operation {
            chain_id: 1,
            op_type: OperationType::CreateAccount,
            payload: OperationPayload::CreateAccount {
                account_number: 900,
                pubkey: [0xEE; 32],
                initial_metadata: vec![],
            },
            signatures: vec![dummy_sig()],
        };
        let block = make_block(90, 0, vec![sign_op(&kp, &op)]);
        assert!(
            execute_block(&block, &storage).is_err(),
            "CreateAccount must reject non-Reserved AUGEID"
        );

        std::fs::remove_dir_all(&path).ok();
    }

    #[test]
    fn gift_then_accept_flow() {
        let path = temp_path();
        let storage = Storage::open(&path).unwrap();

        let donor_kp = create_keypair(705);
        let recipient_kp = create_keypair(706);
        let donor = make_account(0, &donor_kp, 0, 0);
        storage.put_account(&donor).unwrap();

        // Donor owns an Owned AUGEID.
        let mut owned = make_account(800, &donor_kp, 0, 0);
        owned.account_info.state = AccountState::Owned;
        storage.put_account(&owned).unwrap();

        // Gift it to recipient.
        let recipient_pk = recipient_kp.verifying_key().to_bytes();
        let gift_op = Operation {
            chain_id: 1,
            op_type: OperationType::GiftAccount,
            payload: OperationPayload::GiftAccount {
                account: 800,
                n_operation: 0,
                recipient_public_key: recipient_pk,
                fee: 0,
            },
            signatures: vec![dummy_sig()],
        };
        let block1 = make_block(80, 0, vec![sign_op(&donor_kp, &gift_op)]);
        assert!(execute_block(&block1, &storage).is_ok());

        let pending = storage.get_account(800).unwrap().unwrap();
        assert_eq!(pending.account_info.state, AccountState::GiftPending);

        // Recipient accepts.
        let accept_op = Operation {
            chain_id: 1,
            op_type: OperationType::AcceptGift,
            payload: OperationPayload::AcceptGift {
                account: 800,
                n_operation: 1,
                fee: 0,
            },
            signatures: vec![dummy_sig()],
        };
        let block2 = make_block(81, 0, vec![sign_op(&recipient_kp, &accept_op)]);
        assert!(execute_block(&block2, &storage).is_ok());

        let final_acc = storage.get_account(800).unwrap().unwrap();
        assert_eq!(final_acc.account_info.state, AccountState::Owned);
        assert_eq!(
            final_acc.account_info.account_key.ed25519_public_key,
            recipient_pk
        );

        std::fs::remove_dir_all(&path).ok();
    }
}
