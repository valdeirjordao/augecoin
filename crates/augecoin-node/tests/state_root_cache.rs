//! Integration tests for the consensus state-root cache (performance
//! mitigation A) at the execution layer.
//!
//! These prove the property the cache must preserve: `verify_block_pre_state`
//! accepts exactly the state that the previous block committed, and rejects it
//! as soon as the consensus state changes — with no full rescan needed for the
//! unchanged case.

use augecoin_core::account::Account;
use augecoin_core::block::{OperationBlock, OperationBlockHeader};
use augecoin_core::emission::block_reward;
use augecoin_storage::Storage;

fn temp_path(tag: &str) -> String {
    let path = format!("/tmp/augecoin-src-cache-{}-{}", tag, std::process::id());
    std::fs::remove_dir_all(&path).ok();
    path
}

fn empty_block(height: u64, initial_root: [u8; 64]) -> OperationBlock {
    OperationBlock {
        header: OperationBlockHeader {
            chain_id: 1,
            block_number: height,
            account_key: [0x11; 32],
            reward: block_reward(height),
            fee: 0,
            protocol_version: 5,
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
fn pre_state_uses_the_committed_root_and_tracks_changes() {
    let path = temp_path("prestate");
    let storage = Storage::open(&path).unwrap();
    seed(&storage, 8);

    // Block N is built on the current root and committed.
    let root_before = storage.consensus_state_root().unwrap();
    let block_n = empty_block(1, root_before);
    let root_after_n = augecoin_node::execution::execute_block(&block_n, &storage).unwrap();
    assert_ne!(root_before, root_after_n);

    // Block N+1 built on the post-commit root must validate. The root is the
    // value the node cached at the end of block N, so this is the reuse path.
    let block_n1 = empty_block(2, root_after_n);
    augecoin_node::execution::verify_block_pre_state(&block_n1, &storage)
        .expect("post-commit root must validate the next block");

    // A stale root (the pre-block-N value) must be rejected: the cache must not
    // accept a root that no longer matches the persisted state.
    let stale = empty_block(2, root_before);
    assert!(
        augecoin_node::execution::verify_block_pre_state(&stale, &storage).is_err(),
        "a stale pre-state root must be rejected"
    );

    // Any consensus-relevant mutation must invalidate the cached root so the
    // next validation sees the new state.
    storage
        .put_account(&Account::new(1000, [0x33; 32], 1))
        .unwrap();
    let root_after_mutation = storage.consensus_state_root().unwrap();
    assert_ne!(root_after_mutation, root_after_n);
    assert!(
        augecoin_node::execution::verify_block_pre_state(&block_n1, &storage).is_err(),
        "after a state change the previous root must no longer validate"
    );

    std::fs::remove_dir_all(&path).ok();
}

#[test]
fn root_is_stable_under_repeated_reads() {
    let path = temp_path("stable");
    let storage = Storage::open(&path).unwrap();
    seed(&storage, 16);
    let r1 = storage.consensus_state_root().unwrap();
    for _ in 0..5 {
        assert_eq!(storage.consensus_state_root().unwrap(), r1);
    }
    std::fs::remove_dir_all(&path).ok();
}
