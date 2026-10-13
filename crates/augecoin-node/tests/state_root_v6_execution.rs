//! Execution-layer integration for the protocol-v6 SMT state root.
//!
//! Proves the end-to-end consensus path: a block built on the v6 root executes
//! and commits, the returned post-state root validates the next block, a stale
//! root is rejected, and a legacy (v5) node refuses a v6 block.

use augecoin_core::account::Account;
use augecoin_core::block::{OperationBlock, OperationBlockHeader};
use augecoin_core::emission::block_reward;
use augecoin_storage::Storage;

fn temp_path(tag: &str) -> String {
    let path = format!("/tmp/augecoin-smt-exec-{}-{}", tag, std::process::id());
    std::fs::remove_dir_all(&path).ok();
    path
}

fn empty_block(height: u64, protocol_version: u16, initial_root: [u8; 64]) -> OperationBlock {
    OperationBlock {
        header: OperationBlockHeader {
            chain_id: 1,
            block_number: height,
            account_key: [0x11; 32],
            reward: block_reward(height),
            fee: 0,
            protocol_version,
            protocol_available: 6,
            timestamp: 1_700_000_000 + height,
            initial_safe_box_hash: initial_root,
            operations_hash: [0u8; 64],
            block_payload: vec![],
            proof_of_work: [0u8; 32],
            previous_proof_of_work: [0u8; 32],
            leader_id: 0,
        },
        operations: vec![],
        leader_signature: augecoin_crypto::signature::Ed25519Signature { bytes: [0u8; 64] },
        quorum_signatures: vec![],
        block_hash: [0u8; 64],
    }
}

fn seed(storage: &Storage, n: u64) {
    for i in 1..=n {
        storage
            .put_account(&Account::new(i, [0x22; 32], 1))
            .unwrap();
    }
}

#[test]
fn v6_commit_and_pre_state_validation() {
    let path = temp_path("v6");
    let storage = Storage::open(&path).unwrap();
    storage.set_state_tree_activation(1);
    storage.migrate_state_tree().unwrap();
    seed(&storage, 8);

    let root_before = storage.active_consensus_state_root(1).unwrap();
    let block1 = empty_block(1, 6, root_before);
    let root_after = augecoin_node::execution::execute_block(&block1, &storage).unwrap();
    assert_ne!(root_before, root_after);

    let block2 = empty_block(2, 6, root_after);
    augecoin_node::execution::verify_block_pre_state(&block2, &storage)
        .expect("post-commit v6 root must validate the next block");

    // A stale pre-state root must be rejected.
    let stale = empty_block(2, 6, root_before);
    assert!(augecoin_node::execution::verify_block_pre_state(&stale, &storage).is_err());

    std::fs::remove_dir_all(&path).ok();
}

#[test]
fn legacy_node_rejects_v6_block() {
    let path = temp_path("legacy");
    let storage = Storage::open(&path).unwrap();
    // No activation: protocol 5 only.
    seed(&storage, 4);
    let root = storage.consensus_state_root().unwrap();
    let v6_block = empty_block(1, 6, root);
    assert!(
        augecoin_node::execution::verify_block_pre_state(&v6_block, &storage).is_err(),
        "a legacy node must reject a v6 block rather than silently accept it"
    );
    std::fs::remove_dir_all(&path).ok();
}
