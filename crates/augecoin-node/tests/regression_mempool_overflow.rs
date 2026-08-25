//! Regression test for the mempool/gossip liveness bug.
//!
//! The original failure: with `max_operations` set high the block builder
//! dumped the *entire* mempool into a single proposal. At some number of
//! operations the serialized block exceeded the gossipsub `max_transmit_size`,
//! every proposal was rejected with `MessageTooLarge` and the chain stalled at
//! the current height with no self-recovery.
//!
//! This test asserts that:
//!   * a proposed block is always byte-bounded (never rejected as oversized),
//!   * operations that do not fit stay in the mempool for a later block,
//!   * the chain keeps advancing until the mempool is drained (no stall).
//!
//! To keep the test fast in debug builds (ed25519 sign+verify is ~17ms/op),
//! each operation is a large `MultiOperation` with `RECEIVERS` receivers, so a
//! modest op count comfortably exceeds the 2 MiB transmit budget.

use augecoin_core::account::Account;
use augecoin_core::block::OperationBlock;
use augecoin_core::mempool::{Mempool, MempoolConfig};
use augecoin_core::operation::{
    Operation, OperationPayload, OperationType, ReceiverInfo, SenderInfo,
};
use augecoin_crypto::hdkeys::HdWallet;
use augecoin_crypto::signature::{Ed25519KeyPair, Ed25519Signature};
use augecoin_node::consensus::{create_dev_validator_set_and_keys, ConsensusEngine};
use augecoin_storage::Storage;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

static TEST_COUNTER: AtomicU32 = AtomicU32::new(5000);

/// Number of receivers per operation — large enough that each op serializes to
/// ~20 KiB, so a small number of operations exceeds the byte budget.
const RECEIVERS: usize = 1000;
const TARGET_OPS: usize = 300;

fn temp_path() -> String {
    format!(
        "/tmp/augecoin-regression-{}",
        TEST_COUNTER.fetch_add(1, Ordering::SeqCst)
    )
}

fn create_keypair(seed: u64) -> Ed25519KeyPair {
    let seed_bytes = seed.to_be_bytes();
    let mut full_seed = [0u8; 64];
    full_seed[..8].copy_from_slice(&seed_bytes);
    HdWallet::from_seed(&full_seed).derive_keypair(0)
}

fn make_account(num: u64, kp: &Ed25519KeyPair, balance: u64, n_operation: u64) -> Account {
    let pk = kp.verifying_key();
    let mut acc = Account::new(num, pk.to_bytes(), 10);
    if balance > 0 {
        acc.add_balance(balance).unwrap();
    }
    acc.n_operation = n_operation;
    acc
}

fn make_multi_op(sender: u64, n_op: u64, to: u64, receivers: usize) -> Operation {
    let receivers: Vec<ReceiverInfo> = (0..receivers)
        .map(|_| ReceiverInfo {
            account: to,
            amount: 1,
            payload: vec![],
        })
        .collect();
    Operation {
        op_type: OperationType::MultiOperation,
        payload: OperationPayload::MultiOperation {
            senders: vec![SenderInfo {
                account: sender,
                n_operation: n_op,
                amount: 1,
                payload: vec![],
            }],
            receivers,
            changers: vec![],
            fee: 0,
        },
        signatures: vec![],
        chain_id: 1,
    }
}

fn sign_op(kp: &Ed25519KeyPair, op: &Operation) -> Operation {
    let message = op.to_bytes_stripped();
    let sig = kp.sign(&message);
    Operation {
        signatures: vec![sig],
        ..op.clone()
    }
}

/// The full serialized wire size of a *light* `LightProposal` consensus message
/// carrying `block`'s header + Merkle root + tx hashes — this is what gossipsub
/// carries for the proposal under the light-proposal transport.
fn proposal_wire_size(block: &OperationBlock) -> usize {
    use augecoin_core::proposal::op_hash;
    let proposal = augecoin_core::proposal::ProposalMessage {
        header: block.header.clone(),
        merkle_root: block.header.operations_hash,
        tx_hashes: block.operations.iter().map(op_hash).collect(),
        leader_signature: block.leader_signature.clone(),
    };
    let event = augecoin_node::consensus::ConsensusEvent::LightProposal {
        height: block.header.block_number,
        proposal: Box::new(proposal),
        from_id: block.header.leader_id,
    };
    augecoin_node::consensus::serialize_consensus_msg(&event).len()
}

/// The full serialized wire size of a `CommitNotification` carrying `block`
/// with `quorum_sigs` — the largest consensus message for this block.
fn commit_wire_size(block: &OperationBlock, quorum_sigs: Vec<Ed25519Signature>) -> usize {
    let event = augecoin_node::consensus::ConsensusEvent::CommitNotification {
        height: block.header.block_number,
        block_bytes: block.to_bytes(),
        quorum_sigs,
        from_id: block.header.leader_id,
    };
    augecoin_node::consensus::serialize_consensus_msg(&event).len()
}

#[test]
fn block_is_byte_bounded_and_chain_advances() {
    let max_transmit = augecoin_core::limits::max_transmit_size();
    let max_block = augecoin_core::limits::max_block_serialized_size();

    let storage = Arc::new(Storage::open(temp_path()).unwrap());
    let sender_kp = create_keypair(777);
    let receiver_kp = create_keypair(778);

    let sender = make_account(100, &sender_kp, TARGET_OPS as u64 + 10_000, 0);
    storage.put_account(&sender).unwrap();
    let receiver = make_account(200, &receiver_kp, 0, 0);
    storage.put_account(&receiver).unwrap();

    let mempool = Arc::new(Mutex::new(Mempool::new(MempoolConfig {
        min_fee: 0,
        max_operations: 20_000,
        chain_id: 1,
        ttl_seconds: 0,
    })));

    // Admit more operations (by byte size) than fit in a single transmissible
    // block. Each op is ~20 KiB, so TARGET_OPS comfortably exceeds the budget.
    for n in 0..TARGET_OPS as u64 {
        let op = sign_op(&sender_kp, &make_multi_op(100, n, 200, RECEIVERS));
        mempool
            .lock()
            .unwrap()
            .validate_and_admit(op, storage.as_ref(), 1_000)
            .expect("op should be admitted");
    }
    assert_eq!(mempool.lock().unwrap().len(), TARGET_OPS);

    // Build the consensus engine as validator 0.
    let (validator_set, keys, _admin) = create_dev_validator_set_and_keys(4);
    let my_keypair = keys[0].1.clone();
    let engine = ConsensusEngine::new(
        0,
        4,
        my_keypair,
        validator_set,
        storage.clone(),
        mempool.clone(),
        15,
    );

    // First proposed block must be byte-bounded and leave ops behind.
    let block = engine
        .build_block(1, [0u8; 64], [0u8; 64])
        .expect("block should build");
    assert!(
        block.operations.len() < TARGET_OPS,
        "block must be byte-bounded: included {} of {} ops",
        block.operations.len(),
        TARGET_OPS
    );
    assert!(
        block.serialized_size() <= max_block,
        "block serialized size {} exceeds safety-margin target {}",
        block.serialized_size(),
        max_block
    );
    assert!(
        proposal_wire_size(&block) <= max_transmit,
        "proposal wire size {} exceeds max_transmit {} (would be MessageTooLarge)",
        proposal_wire_size(&block),
        max_transmit
    );
    assert_eq!(
        mempool.lock().unwrap().len(),
        TARGET_OPS,
        "build_block is non-destructive; the full mempool remains pending for later blocks"
    );

    // Simulate quorum signatures (3 of 4) and the commit message.
    let quorum_sigs: Vec<Ed25519Signature> = (0..3)
        .map(|i| Ed25519Signature {
            bytes: [i as u8; 64],
        })
        .collect();
    let mut committed = block.clone();
    committed.quorum_signatures = quorum_sigs.clone();
    let commit_wire = commit_wire_size(&committed, quorum_sigs.clone());
    assert!(
        commit_wire <= max_transmit,
        "commit wire size {} exceeds max_transmit {} (would be MessageTooLarge)",
        commit_wire,
        max_transmit
    );

    // The chain must keep advancing until the mempool drains (no stall).
    let mut height = 1u64;
    let mut prev_hash = [0u8; 64];
    let mut total_committed = 0usize;
    for _ in 0..100 {
        let b = engine
            .build_block(height, prev_hash, [0u8; 64])
            .expect("block should keep building");
        if b.operations.is_empty() {
            break;
        }
        assert!(
            b.serialized_size() <= max_block,
            "every block must stay within the byte budget"
        );
        total_committed += b.operations.len();
        mempool.lock().unwrap().remove_committed(&b.operations);
        prev_hash = b.hash();
        height += 1;
    }

    assert_eq!(
        total_committed, TARGET_OPS,
        "all operations must eventually be committed"
    );
    assert!(mempool.lock().unwrap().is_empty());
    eprintln!(
        "regression: {} ops drained across {} blocks (max_block={}, max_transmit={})",
        total_committed,
        height - 1,
        max_block,
        max_transmit
    );
}
