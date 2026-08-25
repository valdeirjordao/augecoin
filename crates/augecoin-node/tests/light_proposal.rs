//! Light-proposal transport tests.
//!
//! Verifies that a validator reconstructs the *exact* byte-for-byte
//! `OperationBlock` the leader proposed from the light `ProposalMessage`
//! (header + Merkle root + tx hashes), and that tampered proposals are
//! rejected by the Merkle-root / hash checks.

use augecoin_core::account::Account;
use augecoin_core::mempool::{Mempool, MempoolConfig};
use augecoin_core::operation::{
    Operation, OperationPayload, OperationType, ReceiverInfo, SenderInfo,
};
use augecoin_core::proposal::{op_hash, ProposalMessage};
use augecoin_crypto::hdkeys::HdWallet;
use augecoin_crypto::signature::{Ed25519KeyPair, Ed25519Signature};
use augecoin_node::consensus::{
    create_dev_validator_set_and_keys, reconstruct_from_proposal, ConsensusEngine,
};
use augecoin_storage::Storage;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

static TEST_COUNTER: AtomicU32 = AtomicU32::new(7000);

fn temp_path() -> String {
    format!(
        "/tmp/augecoin-light-proposal-{}",
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

fn make_transfer_op(sender: u64, n_op: u64, to: u64, amount: u64, fee: u64) -> Operation {
    Operation {
        op_type: OperationType::Transaction,
        payload: OperationPayload::Transaction {
            senders: vec![SenderInfo {
                account: sender,
                n_operation: n_op,
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

#[test]
fn leader_and_validator_reconstruct_identical_block() {
    let storage = Arc::new(Storage::open(temp_path()).unwrap());
    let sender_kp = create_keypair(777);
    let receiver_kp = create_keypair(778);

    let sender = make_account(100, &sender_kp, 100_000, 0);
    storage.put_account(&sender).unwrap();
    let receiver = make_account(200, &receiver_kp, 0, 0);
    storage.put_account(&receiver).unwrap();

    let mempool = Arc::new(Mutex::new(Mempool::new(MempoolConfig {
        min_fee: 0,
        max_operations: 20_000,
        chain_id: 1,
        ttl_seconds: 0,
    })));

    // Admit a chain of transfers from a single sender.
    for n in 0..100u64 {
        let op = sign_op(&sender_kp, &make_transfer_op(100, n, 200, 1, 0));
        mempool
            .lock()
            .unwrap()
            .validate_and_admit(op, storage.as_ref(), 1_000)
            .expect("op should be admitted");
    }

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

    // Leader builds the block and the light proposal.
    let block = engine
        .build_block(1, [0u8; 64], [0u8; 64])
        .expect("block should build");
    assert!(!block.operations.is_empty());
    let proposal = engine.build_light_proposal(&block);
    assert_eq!(proposal.merkle_root, block.header.operations_hash);
    assert_eq!(proposal.tx_hashes.len(), block.operations.len());

    // A validator resolves every tx from its mempool (O(1) hash lookups) and
    // reconstructs the exact same block.
    let resolved: Vec<Operation> = proposal
        .tx_hashes
        .iter()
        .map(|h| {
            mempool
                .lock()
                .unwrap()
                .get_by_hash(h)
                .map(|a| (*a).clone())
                .expect("op should be resident")
        })
        .collect();

    let reconstructed = reconstruct_from_proposal(&proposal, resolved).expect("reconstruct");
    assert_eq!(
        reconstructed.to_bytes(),
        block.to_bytes(),
        "reconstructed block must be byte-for-byte identical to the leader's block"
    );
    assert_eq!(reconstructed.hash(), block.hash());
}

#[test]
fn tampered_merkle_root_is_rejected() {
    let storage = Arc::new(Storage::open(temp_path()).unwrap());
    let sender_kp = create_keypair(777);
    let receiver_kp = create_keypair(778);
    storage
        .put_account(&make_account(100, &sender_kp, 100_000, 0))
        .unwrap();
    storage
        .put_account(&make_account(200, &receiver_kp, 0, 0))
        .unwrap();

    let mempool = Arc::new(Mutex::new(Mempool::new(MempoolConfig {
        min_fee: 0,
        max_operations: 20_000,
        chain_id: 1,
        ttl_seconds: 0,
    })));
    let op = sign_op(&sender_kp, &make_transfer_op(100, 0, 200, 1, 0));
    mempool
        .lock()
        .unwrap()
        .validate_and_admit(op.clone(), storage.as_ref(), 1_000)
        .unwrap();

    let (validator_set, keys, _admin) = create_dev_validator_set_and_keys(4);
    let engine = ConsensusEngine::new(
        0,
        4,
        keys[0].1.clone(),
        validator_set,
        storage.clone(),
        mempool.clone(),
        15,
    );
    let block = engine.build_block(1, [0u8; 64], [0u8; 64]).unwrap();
    let mut proposal = engine.build_light_proposal(&block);

    // Tamper: replace a tx hash with a bogus one — the recomputed Merkle root
    // no longer matches, so reconstruction must fail.
    proposal.tx_hashes[0] = [0xFFu8; 64];
    let resolved = vec![op];
    assert!(reconstruct_from_proposal(&proposal, resolved).is_err());
}

#[test]
fn proposal_message_serialization_roundtrip() {
    let mut header = augecoin_core::block::OperationBlockHeader {
        block_number: 1,
        account_key: [0u8; 32],
        reward: 725_000_000,
        fee: 0,
        protocol_version: 5,
        protocol_available: 6,
        timestamp: 1_700_000_000,
        initial_safe_box_hash: [0u8; 64],
        operations_hash: [0u8; 64],
        block_payload: vec![],
        proof_of_work: [0u8; 32],
        previous_proof_of_work: [0u8; 32],
        leader_id: 0,
        chain_id: 1,
    };
    header.operations_hash = [0xABu8; 64];
    let proposal = ProposalMessage {
        header,
        merkle_root: [0xABu8; 64],
        tx_hashes: vec![[1u8; 64], [2u8; 64]],
        leader_signature: Ed25519Signature { bytes: [9u8; 64] },
    };
    let bytes = proposal.to_bytes();
    let back = ProposalMessage::from_bytes(&bytes).unwrap();
    assert_eq!(proposal, back);

    // op_hash is stable for the same op.
    let op = make_transfer_op(1, 0, 2, 1, 0);
    assert_eq!(op_hash(&op), op_hash(&op));
}
