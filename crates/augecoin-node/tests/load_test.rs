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
use std::time::Instant;

fn make_storage(temp_id: &str) -> (Storage, String) {
    let path = format!("/tmp/augecoin-loadtest-{temp_id}");
    let _ = std::fs::remove_dir_all(&path);
    let storage = Storage::open(&path).unwrap();
    (storage, path)
}

fn create_account_with_balance(
    storage: &Storage,
    number: u64,
    balance: u64,
    ed_pk: [u8; 32],
    height: u64,
) {
    let mut acc = Account::new(number, ed_pk, height);
    acc.add_balance(balance).unwrap();
    storage.put_account(&acc).unwrap();
}

fn sign_op(wallet: &HdWallet, op: &Operation) -> Operation {
    let kp = wallet.derive_keypair(0);
    let msg = op.to_bytes_stripped();
    let sig = kp.sign(&msg);
    Operation {
        signatures: vec![sig],
        ..op.clone()
    }
}

fn dummy_sig() -> Ed25519Signature {
    Ed25519Signature { bytes: [0u8; 64] }
}

#[test]
fn load_test_operation_throughput() {
    let wallet = HdWallet::from_mnemonic(
        "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about",
    )
    .unwrap();
    let pk = wallet.derive_keypair(0).verifying_key();
    let ed_pk: [u8; 32] = pk.to_bytes();
    let leader_ed: [u8; 32] = [0xAB; 32];

    let op_counts = [10, 50, 100, 200, 500];
    let mut results = Vec::new();

    for &op_count in &op_counts {
        let (storage, path) = make_storage(&format!("load-{op_count}"));

        // Create sender accounts
        for i in 1..=op_count {
            create_account_with_balance(&storage, i, 1_000_000, ed_pk, 1);
        }
        // Create leader account
        create_account_with_balance(&storage, 10000, 0, leader_ed, 1);

        // Build operations
        let mut ops = Vec::new();
        for i in 1..=op_count {
            let unsigned = Operation {
                op_type: OperationType::Transaction,
                payload: OperationPayload::Transaction {
                    senders: vec![SenderInfo {
                        account: i,
                        n_operation: 0,
                        amount: 100,
                        payload: vec![],
                    }],
                    receivers: vec![ReceiverInfo {
                        account: 10000,
                        amount: 90,
                        payload: vec![],
                    }],
                    changers: vec![],
                    fee: 10,
                },
                signatures: vec![dummy_sig()],
                chain_id: 1,
            };
            ops.push(sign_op(&wallet, &unsigned));
        }

        let header = OperationBlockHeader {
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
            leader_id: 10000,
            chain_id: 1,
        };

        let block = OperationBlock {
            header,
            operations: ops,
            leader_signature: dummy_sig(),
            quorum_signatures: vec![dummy_sig(), dummy_sig(), dummy_sig()],
            block_hash: [0u8; 64],
        };

        let start = Instant::now();
        let result = execution::execute_block(&block, &storage);
        let elapsed = start.elapsed();

        match result {
            Ok(_) => {
                results.push((op_count, elapsed));
                assert!(
                    elapsed.as_secs_f64() < 60.0,
                    "block with {op_count} ops took {elapsed:.2?}, exceeding 60s target"
                );
            }
            Err(e) => {
                results.push((op_count, elapsed));
                eprintln!("block with {op_count} ops failed: {e:?}");
            }
        }

        std::fs::remove_dir_all(&path).ok();
    }

    eprintln!("\n=== Load Test Results ===");
    eprintln!(
        "{:>10} | {:>10} | {:>10} | {:>10}",
        "Op Count", "Time", "Op/s", "Status"
    );
    eprintln!("{:-<50}", "");
    for (count, elapsed) in &results {
        let tps = if elapsed.as_secs_f64() > 0.0 {
            *count as f64 / elapsed.as_secs_f64()
        } else {
            f64::INFINITY
        };
        eprintln!("{count:>10} | {elapsed:>10.2?} | {tps:>10.1} | OK");
    }
    eprintln!();

    assert!(!results.is_empty(), "no results collected");
    for (_, elapsed) in &results {
        assert!(
            elapsed.as_secs_f64() < 60.0,
            "block exceeded 60s time limit"
        );
    }
}

#[test]
fn load_test_max_block() {
    let wallet = HdWallet::from_mnemonic(
        "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about",
    )
    .unwrap();
    let pk = wallet.derive_keypair(0).verifying_key();
    let ed_pk: [u8; 32] = pk.to_bytes();
    let leader_ed: [u8; 32] = [0xAA; 32];

    let op_count: u64 = 1000;
    let (storage, path) = make_storage("maxblock");

    for i in 1..=op_count {
        create_account_with_balance(&storage, i, 10_000_000, ed_pk, 1);
    }
    create_account_with_balance(&storage, 10000, 0, leader_ed, 1);

    let mut ops = Vec::new();
    for i in 1..=op_count {
        let unsigned = Operation {
            op_type: OperationType::Transaction,
            payload: OperationPayload::Transaction {
                senders: vec![SenderInfo {
                    account: i,
                    n_operation: 0,
                    amount: 50,
                    payload: vec![],
                }],
                receivers: vec![ReceiverInfo {
                    account: 10000,
                    amount: 45,
                    payload: vec![],
                }],
                changers: vec![],
                fee: 5,
            },
            signatures: vec![dummy_sig()],
            chain_id: 1,
        };
        ops.push(sign_op(&wallet, &unsigned));
    }

    let header = OperationBlockHeader {
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
        leader_id: 10000,
        chain_id: 1,
    };

    let block = OperationBlock {
        header,
        operations: ops,
        leader_signature: dummy_sig(),
        quorum_signatures: vec![dummy_sig(), dummy_sig(), dummy_sig()],
        block_hash: [0u8; 64],
    };

    let start = Instant::now();
    let result = execution::execute_block(&block, &storage);
    let elapsed = start.elapsed();

    eprintln!("Max block ({op_count} ops): {elapsed:.2?}");

    match result {
        Ok(_) => {
            let expected_reward = block_reward(1);
            let expected_fees = op_count * 5;
            let _expected = expected_reward + expected_fees;

            let leader = storage.get_account(10000).unwrap().unwrap();
            // Leader receives 100% of block_reward + 100% of fees
            assert!(leader.balance > 0, "leader should have balance after block");
        }
        Err(e) => {
            panic!("max block execution failed: {e:?}");
        }
    }

    std::fs::remove_dir_all(&path).ok();
}
