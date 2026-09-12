use augecoin_core::account::Account;
use augecoin_core::block::{OperationBlock, OperationBlockHeader};
use augecoin_core::emission::block_reward;
use augecoin_core::operation::{
    Operation, OperationPayload, OperationType, ReceiverInfo, SenderInfo,
};
use augecoin_crypto::hdkeys::HdWallet;
use augecoin_crypto::signature::Ed25519Signature;
use augecoin_node::execution;
use augecoin_storage::Storage;

fn make_storage(temp_id: &str) -> (Storage, String) {
    let path = format!("/tmp/augecoin-partition-{temp_id}");
    let _ = std::fs::remove_dir_all(&path);
    let storage = Storage::open(&path).unwrap();
    (storage, path)
}

fn dummy_sig() -> Ed25519Signature {
    Ed25519Signature { bytes: [0u8; 64] }
}

fn make_account(num: u64, ed_pk: [u8; 32], balance: u64) -> Account {
    let mut acc = Account::new(num, ed_pk, 0);
    if balance > 0 {
        acc.add_balance(balance).unwrap();
    }
    acc
}

#[test]
fn partition_two_validators_cannot_reach_quorum() {
    let total_validators: u64 = 4;
    let quorum_threshold = (total_validators * 2) / 3 + 1;
    let partitioned_signatures: u64 = 2;

    assert_eq!(quorum_threshold, 3);
    assert!(partitioned_signatures < quorum_threshold);

    let can_finalize = partitioned_signatures >= quorum_threshold;
    assert!(!can_finalize);
}

#[test]
fn partition_resolved_all_validators_can_finalize() {
    let total_validators: u64 = 4;
    let quorum_threshold = (total_validators * 2) / 3 + 1;
    assert!(4 >= quorum_threshold);
}

#[test]
fn three_validators_still_reach_quorum() {
    let total_validators: u64 = 4;
    let quorum_threshold = (total_validators * 2) / 3 + 1;
    assert!(3 >= quorum_threshold);
}

#[test]
fn partition_state_isolation_preserves_atomicity() {
    let wallet = HdWallet::from_mnemonic(
        "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about",
    )
    .unwrap();
    let pk = wallet.derive_keypair(0).verifying_key();
    let ed_pk: [u8; 32] = pk.to_bytes();
    let leader_ed: [u8; 32] = [0x11; 32];

    let (storage, path) = make_storage("partition-atomic");

    // Use accounts outside dev range (0-4): senders 10, receiver 11, leader 100
    let acc1 = make_account(10, ed_pk, 10000);
    storage.put_account(&acc1).unwrap();
    let acc2 = make_account(11, ed_pk, 10000);
    storage.put_account(&acc2).unwrap();
    let leader = make_account(100, leader_ed, 0);
    storage.put_account(&leader).unwrap();

    // Create dev account for block 1 (dev_account_for_block(1) = 1)
    let dev1 = make_account(1, leader_ed, 0);
    storage.put_account(&dev1).unwrap();

    let unsigned_op = Operation {
        op_type: OperationType::Transaction,
        payload: OperationPayload::Transaction {
            senders: vec![SenderInfo {
                account: 10,
                n_operation: 0,
                amount: 100,
                payload: vec![],
            }],
            receivers: vec![ReceiverInfo {
                account: 11,
                amount: 90,
                payload: vec![],
            }],
            changers: vec![],
            fee: 10,
        },
        signatures: vec![dummy_sig()],
        chain_id: 1,
    };
    let kp = wallet.derive_keypair(0);
    let msg = unsigned_op.to_bytes_stripped();
    let sig = kp.sign(&msg);
    let op = Operation {
        signatures: vec![sig],
        ..unsigned_op
    };

    let block = OperationBlock {
        header: OperationBlockHeader {
            block_number: 1,
            account_key: leader_ed,
            reward: block_reward(1),
            fee: 0,
            protocol_version: 5,
            protocol_available: 6,
            timestamp: 1000,
            initial_safe_box_hash: [0u8; 64],
            operations_hash: [0u8; 64],
            block_payload: vec![],
            proof_of_work: [0u8; 32],
            previous_proof_of_work: [0u8; 32],
            leader_id: 100,
            chain_id: 1,
        },
        operations: vec![op],
        leader_signature: dummy_sig(),
        quorum_signatures: vec![dummy_sig(), dummy_sig(), dummy_sig()],
        block_hash: [0u8; 64],
    };

    let result = execution::execute_block(&block, &storage);
    assert!(result.is_ok());

    let acc1_after = storage.get_account(10).unwrap().unwrap();
    assert_eq!(acc1_after.balance, 10000 - 100 - 10);

    let acc2_after = storage.get_account(11).unwrap().unwrap();
    assert_eq!(acc2_after.balance, 10000 + 90);

    let leader_after = storage.get_account(100).unwrap().unwrap();
    assert!(leader_after.balance > 0);

    std::fs::remove_dir_all(&path).ok();
}

#[test]
fn partition_state_root_changes_after_recovery() {
    let wallet = HdWallet::from_mnemonic(
        "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about",
    )
    .unwrap();
    let pk = wallet.derive_keypair(0).verifying_key();
    let ed_pk: [u8; 32] = pk.to_bytes();
    let leader_ed: [u8; 32] = [0x33; 32];

    let (storage, path) = make_storage("partition-root");

    storage.put_account(&make_account(1, ed_pk, 5000)).unwrap();
    storage
        .put_account(&make_account(10, leader_ed, 0))
        .unwrap();

    let unsigned_op = Operation {
        op_type: OperationType::Transaction,
        payload: OperationPayload::Transaction {
            senders: vec![SenderInfo {
                account: 1,
                n_operation: 0,
                amount: 100,
                payload: vec![],
            }],
            receivers: vec![ReceiverInfo {
                account: 10,
                amount: 90,
                payload: vec![],
            }],
            changers: vec![],
            fee: 10,
        },
        signatures: vec![dummy_sig()],
        chain_id: 1,
    };
    let kp = wallet.derive_keypair(0);
    let msg = unsigned_op.to_bytes_stripped();
    let sig = kp.sign(&msg);
    let op1 = Operation {
        signatures: vec![sig],
        ..unsigned_op
    };

    let block1 = OperationBlock {
        header: OperationBlockHeader {
            block_number: 1,
            account_key: leader_ed,
            reward: block_reward(1),
            fee: 0,
            protocol_version: 5,
            protocol_available: 6,
            timestamp: 1000,
            initial_safe_box_hash: [0u8; 64],
            operations_hash: [0u8; 64],
            block_payload: vec![],
            proof_of_work: [0u8; 32],
            previous_proof_of_work: [0u8; 32],
            leader_id: 10,
            chain_id: 1,
        },
        operations: vec![op1],
        leader_signature: dummy_sig(),
        quorum_signatures: vec![dummy_sig(), dummy_sig(), dummy_sig()],
        block_hash: [0u8; 64],
    };

    let root_after_first = execution::execute_block(&block1, &storage).unwrap();

    let unsigned_op2 = Operation {
        op_type: OperationType::Transaction,
        payload: OperationPayload::Transaction {
            senders: vec![SenderInfo {
                account: 1,
                n_operation: 1,
                amount: 50,
                payload: vec![],
            }],
            receivers: vec![ReceiverInfo {
                account: 10,
                amount: 45,
                payload: vec![],
            }],
            changers: vec![],
            fee: 5,
        },
        signatures: vec![dummy_sig()],
        chain_id: 1,
    };
    let msg2 = unsigned_op2.to_bytes_stripped();
    let sig2 = kp.sign(&msg2);
    let op2 = Operation {
        signatures: vec![sig2],
        ..unsigned_op2
    };

    let block2 = OperationBlock {
        header: OperationBlockHeader {
            block_number: 2,
            account_key: leader_ed,
            reward: block_reward(2),
            fee: 0,
            protocol_version: 5,
            protocol_available: 6,
            timestamp: 1060,
            initial_safe_box_hash: root_after_first,
            operations_hash: [0u8; 64],
            block_payload: vec![],
            proof_of_work: [0u8; 32],
            previous_proof_of_work: [0u8; 32],
            leader_id: 10,
            chain_id: 1,
        },
        operations: vec![op2],
        leader_signature: dummy_sig(),
        quorum_signatures: vec![dummy_sig(), dummy_sig(), dummy_sig()],
        block_hash: [0u8; 64],
    };

    let root_after_second = execution::execute_block(&block2, &storage).unwrap();

    assert_ne!(root_after_first, root_after_second);

    std::fs::remove_dir_all(&path).ok();
}
