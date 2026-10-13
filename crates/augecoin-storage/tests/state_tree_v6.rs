//! Integration tests for the protocol-v6 incremental SMT state root.
//!
//! These exercise the real RocksDB-backed `Storage`:
//!   * incremental block commits converge with a full rebuild of the same state;
//!   * two independent databases derive the same root (determinism);
//!   * a reopened database reproduces the root;
//!   * the legacy mode writes no `state_nodes` and keeps the flat root;
//!   * a block carrying the wrong protocol version is rejected by the commit guard.

use augecoin_core::account::Account;
use augecoin_core::block::{OperationBlock, OperationBlockHeader};
use augecoin_storage::Storage;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};

static COUNTER: AtomicU32 = AtomicU32::new(7000);

fn temp_path(tag: &str) -> String {
    format!(
        "/tmp/augecoin-smt-{}-{}-{}",
        tag,
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::SeqCst)
    )
}

fn block(height: u64, protocol_version: u16) -> OperationBlock {
    OperationBlock {
        header: OperationBlockHeader {
            block_number: height,
            account_key: [0u8; 32],
            reward: 0,
            fee: 0,
            protocol_version,
            protocol_available: 6,
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
        leader_signature: augecoin_crypto::signature::Ed25519Signature { bytes: [0u8; 64] },
        quorum_signatures: vec![],
        block_hash: [0u8; 64],
    }
}

fn account(number: u64, balance: u64) -> Account {
    let mut acc = Account::new(number, [0x11; 32], 1);
    acc.add_balance(balance).unwrap();
    acc
}

fn nodes_len(storage: &Storage) -> usize {
    storage.state_tree_node_count().unwrap() as usize
}

#[test]
fn incremental_matches_full_rebuild() {
    let path_a = temp_path("incr");
    let path_c = temp_path("full");

    // A: five incremental blocks (activation from height 1).
    let a = Storage::open(&path_a).unwrap();
    a.set_state_tree_activation(1);
    a.migrate_state_tree().unwrap();
    for height in 1..=5u64 {
        let mut modified = HashMap::new();
        let acc = account(height, height * 100);
        modified.insert(height, acc);
        a.commit_block_atomic_with_contracts(&modified, &block(height, 6), None, &[], false)
            .unwrap();
    }
    let root_a = a.consensus_state_root_v6().unwrap();

    // C: one block containing the same final logical state.
    let c = Storage::open(&path_c).unwrap();
    c.set_state_tree_activation(1);
    c.migrate_state_tree().unwrap();
    let mut all = HashMap::new();
    for number in 1..=5u64 {
        all.insert(number, account(number, number * 100));
    }
    c.commit_block_atomic_with_contracts(&all, &block(5, 6), None, &[], false)
        .unwrap();
    let root_c = c.consensus_state_root_v6().unwrap();

    assert_eq!(root_a, root_c, "incremental root must equal full rebuild");
    assert_ne!(root_a, [0u8; 64]);

    std::fs::remove_dir_all(&path_a).ok();
    std::fs::remove_dir_all(&path_c).ok();
}

#[test]
fn root_is_deterministic_across_databases() {
    let mut roots = Vec::new();
    let mut paths = Vec::new();
    for tag in ["det-a", "det-b"] {
        let path = temp_path(tag);
        let s = Storage::open(&path).unwrap();
        s.set_state_tree_activation(0);
        s.migrate_state_tree().unwrap();
        let mut modified = HashMap::new();
        for i in 0..32u64 {
            modified.insert(i, account(i, i + 7));
        }
        s.commit_block_atomic_with_contracts(&modified, &block(1, 6), None, &[], false)
            .unwrap();
        roots.push(s.consensus_state_root_v6().unwrap());
        paths.push(path);
    }
    assert_eq!(roots[0], roots[1]);
    for p in paths {
        std::fs::remove_dir_all(&p).ok();
    }
}

#[test]
fn reopen_reproduces_root() {
    let path = temp_path("reopen");
    let root = {
        let s = Storage::open(&path).unwrap();
        s.set_state_tree_activation(1);
        s.migrate_state_tree().unwrap();
        let mut modified = HashMap::new();
        for i in 0..16u64 {
            modified.insert(i, account(i, i * 3));
        }
        s.commit_block_atomic_with_contracts(&modified, &block(1, 6), None, &[], false)
            .unwrap();
        s.consensus_state_root_v6().unwrap()
    };

    let reopened = Storage::open(&path).unwrap();
    reopened.set_state_tree_activation(1);
    let rebuilt = reopened.migrate_state_tree().unwrap();
    assert_eq!(root, rebuilt, "reopened root must match the committed one");
    assert_eq!(
        reopened.persisted_state_tree_root().unwrap(),
        Some(root),
        "committed metadata root must be persisted"
    );

    std::fs::remove_dir_all(&path).ok();
}

#[test]
fn legacy_mode_writes_no_state_nodes() {
    let path = temp_path("legacy");
    let s = Storage::open(&path).unwrap();
    // No activation configured: legacy flat root only.
    let mut modified = HashMap::new();
    modified.insert(1u64, account(1, 100));
    s.commit_block_atomic_with_contracts(&modified, &block(1, 5), None, &[], false)
        .unwrap();
    assert_eq!(nodes_len(&s), 0, "legacy mode must not persist SMT nodes");
    assert_eq!(
        s.active_protocol_version(1),
        5,
        "legacy mode keeps protocol 5"
    );
    std::fs::remove_dir_all(&path).ok();
}

#[test]
fn block_with_wrong_protocol_version_is_rejected_when_active() {
    let path = temp_path("guard");
    let s = Storage::open(&path).unwrap();
    s.set_state_tree_activation(1);
    s.migrate_state_tree().unwrap();

    let mut modified = HashMap::new();
    modified.insert(1u64, account(1, 100));
    // Height 1 is active, so a protocol-5 block must be rejected.
    let result = s.commit_block_atomic_with_contracts(&modified, &block(1, 5), None, &[], false);
    assert!(
        result.is_err(),
        "mismatched protocol version must be rejected"
    );

    std::fs::remove_dir_all(&path).ok();
}

#[test]
fn contract_state_writes_affect_root_and_delete_restores() {
    let path = temp_path("contracts");
    let s = Storage::open(&path).unwrap();
    s.set_state_tree_activation(0);
    s.migrate_state_tree().unwrap();
    let baseline = s.consensus_state_root_v6().unwrap();

    s.commit_block_atomic_with_contracts(
        &HashMap::new(),
        &block(1, 6),
        None,
        &[(
            "contract_balances".to_string(),
            b"acct:1".to_vec(),
            Some(b"100".to_vec()),
        )],
        false,
    )
    .unwrap();
    let with_contract = s.consensus_state_root_v6().unwrap();
    assert_ne!(baseline, with_contract);

    // Deleting the same key must return to the empty-state root (height leaf
    // exists now, so compare against a rebuild, not the original baseline).
    s.commit_block_atomic_with_contracts(
        &HashMap::new(),
        &block(2, 6),
        None,
        &[("contract_balances".to_string(), b"acct:1".to_vec(), None)],
        false,
    )
    .unwrap();
    let after_delete = s.consensus_state_root_v6().unwrap();
    let rebuilt = s.rebuild_state_tree().unwrap();
    assert_eq!(after_delete, rebuilt);

    std::fs::remove_dir_all(&path).ok();
}
