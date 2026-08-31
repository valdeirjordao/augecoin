//! End-to-end transaction test: Wallet/SDK -> RPC sendOperation -> Mempool ->
//! Consensus (block build + quorum) -> execute_block -> state, then RPC query.
//!
//! No mocks: real signatures, real mempool admission, real block construction,
//! real quorum verification and real execution.

use augecoin_core::account::Account;
use augecoin_core::mempool::{Mempool, MempoolConfig};
use augecoin_core::operation::{
    Operation, OperationPayload, OperationType, ReceiverInfo, SenderInfo,
};
use augecoin_crypto::signature::HybridKeyPair;
use augecoin_node::consensus::{create_dev_validator_set_and_keys, ConsensusEngine};
use augecoin_node::execution::{execute_block, verify_block_quorum};
use augecoin_rpc::endpoints::{handle_send_operation, AppState, NodeStatus, SendOperationParams};
use augecoin_storage::Storage;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

const AUGE: u64 = 100_000_000;

static TEST_COUNTER: AtomicU32 = AtomicU32::new(10_000);

fn temp_path() -> String {
    format!(
        "/tmp/augecoin-tx-e2e-{}",
        TEST_COUNTER.fetch_add(1, Ordering::SeqCst)
    )
}

fn make_account(num: u64, kp: &HybridKeyPair, balance: u64, n_operation: u64) -> Account {
    let vk = kp.verifying_key();
    let mut acc = Account::new(num, vk.to_bytes(), 100);
    if balance > 0 {
        acc.add_balance(balance).unwrap();
    }
    acc.n_operation = n_operation;
    acc
}

fn signed_transfer(
    kp: &HybridKeyPair,
    chain_id: u64,
    sender: u64,
    n_operation: u64,
    to: u64,
    amount: u64,
    fee: u64,
) -> Operation {
    let op = Operation {
        chain_id,
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
                amount,
                payload: vec![],
            }],
            changers: vec![],
            fee,
        },
        signatures: vec![],
    };
    let message = op.to_bytes_stripped();
    let signature = kp.sign(&message);
    Operation {
        signatures: vec![signature],
        ..op
    }
}

#[test]
fn full_transaction_lifecycle_via_rpc_mempool_consensus() {
    let path = temp_path();
    let storage = Arc::new(Storage::open(&path).unwrap());
    let chain_id = 2u64;

    // Accounts: sender A=100, receiver B=200.
    let kp_a = HybridKeyPair::generate();
    let kp_b = HybridKeyPair::generate();

    let account_a = make_account(100, &kp_a, 1_000 * AUGE, 0);
    let account_b = make_account(200, &kp_b, 5 * AUGE, 0);
    storage.put_account(&account_a).unwrap();
    storage.put_account(&account_b).unwrap();

    // Official mempool (shared by RPC and consensus).
    let mempool = Arc::new(Mutex::new(Mempool::new(MempoolConfig {
        min_fee: 0,
        max_operations: 10_000,
        chain_id,
        ttl_seconds: 3600,
        contract_gas_reserve: 100_000,
    })));

    let node_status = Arc::new(NodeStatus {
        block_height: AtomicU64::new(0),
        latest_block_hash: Mutex::new([0u8; 64]),
        peers_connected: AtomicU32::new(0),
        peers_gossipsub_consensus: AtomicU32::new(0),
        peers_kademlia_total: AtomicU32::new(0),
        syncing: AtomicU64::new(0),
        sync_target_height: AtomicU64::new(0),
        mempool_size: AtomicU64::new(0),
        current_round: AtomicU64::new(0),
        current_view: AtomicU64::new(0),
        validator_id: AtomicU64::new(0),
        chain_id: AtomicU64::new(chain_id),
        uptime_seconds: AtomicU64::new(0),
        last_consensus_error: Mutex::new(None),
    });

    let state = AppState {
        storage: storage.clone(),
        mempool: mempool.clone(),
        node_status: node_status.clone(),
        api_keys: augecoin_rpc::auth::ApiKeyStore::empty(),
        rate_limiter: augecoin_rpc::auth::RateLimiter::new(
            1000,
            std::time::Duration::from_secs(60),
        ),
        sensitive_rate_limiter: augecoin_rpc::auth::RateLimiter::new(
            50,
            std::time::Duration::from_secs(60),
        ),
        require_admin_auth: false,
        faucet_keypair: None,
        faucet_account: 1,
        faucet_amount: 100,
        faucet_claims: std::sync::Mutex::new(std::collections::HashMap::new()),
        admin_keypair: None,
        op_broadcaster: None,
    };

    // ── 1. Wallet signs a transfer of 10 AUGE (A -> B) ───────────────────
    let amount = 10 * AUGE;

    // A signed operation cannot create value by claiming more for its receiver
    // than its sender provides.
    let mut inflation = signed_transfer(&kp_a, chain_id, 100, 0, 200, amount, 0);
    if let OperationPayload::Transaction { receivers, .. } = &mut inflation.payload {
        receivers[0].amount = amount + 1;
    }
    inflation.signatures = vec![kp_a.sign(&inflation.to_bytes_stripped())];
    let inflation_result = handle_send_operation(
        SendOperationParams {
            hex: hex::encode(inflation.to_bytes()),
        },
        &state,
    )
    .unwrap();
    assert!(!inflation_result.accepted);
    assert!(inflation_result.error.unwrap().contains("not conserved"));

    let op = signed_transfer(&kp_a, chain_id, 100, 0, 200, amount, 0);
    let op_hex = hex::encode(op.to_bytes());

    // ── 2. RPC sendOperation -> mempool ──────────────────────────────────
    let result = handle_send_operation(SendOperationParams { hex: op_hex }, &state).unwrap();
    assert!(result.accepted, "operation should be accepted: {result:?}");
    assert!(!result.op_hash_hex.is_empty());
    assert_eq!(mempool.lock().unwrap().len(), 1, "mempool must hold 1 op");

    // ── 3. getpendings reflects the mempool ──────────────────────────────
    let pendings = augecoin_rpc::endpoints::handle_get_pendings(&state).unwrap();
    assert_eq!(pendings.size, 1);

    // ── 4. Wrong chain_id must be rejected ───────────────────────────────
    let bad_op = signed_transfer(&kp_a, 999, 100, 0, 200, amount, 0);
    let bad_hex = hex::encode(bad_op.to_bytes());
    let bad = handle_send_operation(SendOperationParams { hex: bad_hex }, &state).unwrap();
    assert!(!bad.accepted);
    assert!(bad.error.unwrap().contains("chain_id"));

    // ── 5. Consensus: build block from mempool, gather quorum ────────────
    let (validator_set, all_keys, _admin) = create_dev_validator_set_and_keys(4);
    let (_, my_kp) = &all_keys[0];
    let engine = ConsensusEngine::new(
        0,
        4,
        my_kp.clone(),
        validator_set.clone(),
        storage.clone(),
        mempool.clone(),
        60,
    );

    let block = engine
        .build_block(1, [0u8; 64], [0u8; 64])
        .expect("leader should build block");
    assert_eq!(
        block.operations.len(),
        1,
        "block must include the transaction"
    );

    // All 4 validators sign.
    let mut quorum_sigs = Vec::new();
    for (_vi, kp) in &all_keys {
        quorum_sigs.push(kp.sign(&block.hash()));
    }

    let mut full_block = block.clone();
    full_block.quorum_signatures = quorum_sigs.clone();

    let valid = verify_block_quorum(&full_block, &validator_set).expect("quorum must verify");
    assert_eq!(valid, 4);

    let mut repeated_signature_block = block.clone();
    repeated_signature_block.quorum_signatures = vec![quorum_sigs[0].clone(); 3];
    assert!(verify_block_quorum(&repeated_signature_block, &validator_set).is_err());

    // ── 6. Execute block and commit state ────────────────────────────────
    execute_block(&full_block, &storage).expect("block must execute");
    storage.put_block(&full_block).unwrap();
    storage.put_height(1).unwrap();

    // ── 7. Query final state (RPC read path) ─────────────────────────────
    let a = storage.get_account(100).unwrap().unwrap();
    let b = storage.get_account(200).unwrap().unwrap();
    assert_eq!(
        a.balance,
        1_000 * AUGE - amount,
        "sender balance must decrease"
    );
    assert_eq!(a.n_operation, 1, "sender n_operation must increment");
    assert_eq!(
        b.balance,
        5 * AUGE + amount,
        "receiver balance must increase"
    );

    // Block query works.
    let stored = storage.get_block(1).unwrap().unwrap();
    assert_eq!(stored.header.block_number, 1);
    assert_eq!(stored.operations.len(), 1);

    // Account query via RPC handler.
    let acc_info = augecoin_rpc::endpoints::handle_get_account(
        augecoin_rpc::endpoints::GetAccountParams {
            account_number: Some(100),
            address: None,
        },
        &state,
    )
    .unwrap();
    assert_eq!(acc_info.balance, a.balance);

    // Mempool emptied after commit.
    mempool
        .lock()
        .unwrap()
        .remove_committed(&full_block.operations);
    assert_eq!(mempool.lock().unwrap().len(), 0);

    std::fs::remove_dir_all(&path).ok();
}

#[test]
fn mempool_rejects_duplicate_and_bad_nonce_via_rpc() {
    let path = temp_path();
    let storage = Arc::new(Storage::open(&path).unwrap());
    let chain_id = 2u64;
    let kp_a = HybridKeyPair::generate();
    storage
        .put_account(&make_account(100, &kp_a, 1_000 * AUGE, 0))
        .unwrap();

    let mempool = Arc::new(Mutex::new(Mempool::new(MempoolConfig {
        min_fee: 0,
        max_operations: 10_000,
        chain_id,
        ttl_seconds: 3600,
        contract_gas_reserve: 100_000,
    })));
    let node_status = Arc::new(NodeStatus {
        chain_id: AtomicU64::new(chain_id),
        ..Default::default()
    });
    let state = AppState {
        storage: storage.clone(),
        mempool: mempool.clone(),
        node_status,
        api_keys: augecoin_rpc::auth::ApiKeyStore::empty(),
        rate_limiter: augecoin_rpc::auth::RateLimiter::new(
            1000,
            std::time::Duration::from_secs(60),
        ),
        sensitive_rate_limiter: augecoin_rpc::auth::RateLimiter::new(
            50,
            std::time::Duration::from_secs(60),
        ),
        require_admin_auth: false,
        faucet_keypair: None,
        faucet_account: 1,
        faucet_amount: 100,
        faucet_claims: std::sync::Mutex::new(std::collections::HashMap::new()),
        admin_keypair: None,
        op_broadcaster: None,
    };

    let op = signed_transfer(&kp_a, chain_id, 100, 0, 200, AUGE, 0);
    let ok = handle_send_operation(
        SendOperationParams {
            hex: hex::encode(op.to_bytes()),
        },
        &state,
    )
    .unwrap();
    assert!(ok.accepted);

    // Duplicate (same sender + n_operation) rejected.
    let dup = handle_send_operation(
        SendOperationParams {
            hex: hex::encode(op.to_bytes()),
        },
        &state,
    )
    .unwrap();
    assert!(!dup.accepted);
    assert!(dup.error.unwrap().contains("duplicate"));

    // Wrong nonce rejected.
    let bad_nonce = signed_transfer(&kp_a, chain_id, 100, 5, 200, AUGE, 0);
    let res = handle_send_operation(
        SendOperationParams {
            hex: hex::encode(bad_nonce.to_bytes()),
        },
        &state,
    )
    .unwrap();
    assert!(!res.accepted);
    assert!(res.error.unwrap().contains("n_operation"));

    std::fs::remove_dir_all(&path).ok();
}
