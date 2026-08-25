use augecoin_core::account::Account;
use augecoin_core::block::{OperationBlock, OperationBlockHeader};
use augecoin_core::emission::block_reward;
use augecoin_core::mempool::{Mempool, MempoolConfig};
use augecoin_core::operation::{
    Operation, OperationPayload, OperationType, ReceiverInfo, SenderInfo,
};
use augecoin_crypto::signature::{Ed25519KeyPair, Ed25519Signature};
use augecoin_node::execution::execute_block;
use augecoin_storage::Storage;
use std::sync::atomic::{AtomicU32, Ordering};

static TEST_COUNTER: AtomicU32 = AtomicU32::new(9000);

fn temp_path() -> String {
    format!(
        "/tmp/augecoin-det-{}",
        TEST_COUNTER.fetch_add(1, Ordering::SeqCst)
    )
}

fn create_keypair(seed: u64) -> Ed25519KeyPair {
    use augecoin_crypto::hdkeys::HdWallet;
    let seed_bytes = seed.to_be_bytes();
    let mut full_seed = [0u8; 64];
    full_seed[..8].copy_from_slice(&seed_bytes);
    HdWallet::from_seed(&full_seed).derive_keypair(0)
}

fn dummy_signature() -> Ed25519Signature {
    Ed25519Signature { bytes: [0u8; 64] }
}

fn sign_op(kp: &Ed25519KeyPair, op: &Operation) -> Operation {
    let message = op.to_bytes_stripped();
    let sig = kp.sign(&message);
    Operation {
        signatures: vec![sig],
        ..op.clone()
    }
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
        signatures: vec![dummy_signature()],
        chain_id: 1,
    }
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

#[test]
fn independent_nodes_produce_same_state_root() {
    let path_a = temp_path();
    let path_b = temp_path();

    let storage_a = Storage::open(&path_a).unwrap();
    let storage_b = Storage::open(&path_b).unwrap();

    let kp_leader = create_keypair(5000);
    let kp_alice = create_keypair(5001);
    let pk_leader = kp_leader.verifying_key();

    let leader = make_account(50, &kp_leader, 100_000, 0);
    let alice = make_account(100, &kp_alice, 1_000_000, 0);
    let bob = make_account(200, &kp_alice, 10_000, 0);

    for storage in [&storage_a, &storage_b] {
        storage.put_account(&leader).unwrap();
        storage.put_account(&alice).unwrap();
        storage.put_account(&bob).unwrap();
    }

    let checksum_before_a = storage_a.verify_checksum();
    let checksum_before_b = storage_b.verify_checksum();
    assert_eq!(
        checksum_before_a, checksum_before_b,
        "initial state checksums must match"
    );

    let block_count: u64 = 10;

    for height in 1..=block_count {
        let alice_n_op = height - 1;
        let mut mempool = Mempool::new(MempoolConfig {
            min_fee: 1,
            max_operations: 1000,
            chain_id: 1,
            ttl_seconds: 3600,
        });

        let op = sign_op(&kp_alice, &make_transfer_op(100, alice_n_op, 200, 500, 10));
        mempool.validate_and_admit(op, &storage_a, 1000).unwrap();

        let pending = mempool.pending_cloned();

        let block = OperationBlock {
            header: OperationBlockHeader {
                block_number: height,
                account_key: pk_leader.to_bytes(),
                reward: block_reward(height),
                fee: 0,
                protocol_version: 5,
                protocol_available: 6,
                timestamp: 1700000000 + height * 60,
                initial_safe_box_hash: [0u8; 64],
                operations_hash: [0u8; 64],
                block_payload: vec![],
                proof_of_work: [0u8; 32],
                previous_proof_of_work: [0u8; 32],
                leader_id: 50,
                chain_id: 1,
            },
            operations: pending,
            leader_signature: dummy_signature(),
            quorum_signatures: vec![dummy_signature(), dummy_signature(), dummy_signature()],
            block_hash: [0u8; 64],
        };

        execute_block(&block, &storage_a)
            .unwrap_or_else(|e| panic!("execution failed on A at height {height}: {e}"));
        execute_block(&block, &storage_b)
            .unwrap_or_else(|e| panic!("execution failed on B at height {height}: {e}"));
    }

    let checksum_a = storage_a.verify_checksum();
    let checksum_b = storage_b.verify_checksum();
    assert_eq!(
        checksum_a, checksum_b,
        "state checksums must match after {block_count} blocks"
    );
    assert_ne!(
        checksum_a, checksum_before_a,
        "checksum should have changed after execution"
    );

    let safebox_a = storage_a
        .get_safe_box()
        .unwrap()
        .expect("safebox A must exist");
    let safebox_b = storage_b
        .get_safe_box()
        .unwrap()
        .expect("safebox B must exist");
    assert_eq!(
        safebox_a.header.safe_box_hash, safebox_b.header.safe_box_hash,
        "safe_box_hash must match"
    );

    for acct_num in [50u64, 100, 200] {
        let acc_a = storage_a
            .get_account(acct_num)
            .unwrap()
            .unwrap_or_else(|| panic!("account {acct_num} must exist on A"));
        let acc_b = storage_b
            .get_account(acct_num)
            .unwrap()
            .unwrap_or_else(|| panic!("account {acct_num} must exist on B"));
        assert_eq!(
            acc_a.balance, acc_b.balance,
            "balance of account {acct_num} must match"
        );
        assert_eq!(
            acc_a.n_operation, acc_b.n_operation,
            "n_operation of account {acct_num} must match"
        );
    }

    for i in 0..augecoin_core::constants::CT_ACCOUNTS_PER_BLOCK {
        let num = block_count * augecoin_core::constants::CT_ACCOUNTS_PER_BLOCK + i;
        let acc_a = storage_a.get_account(num).unwrap();
        let acc_b = storage_b.get_account(num).unwrap();
        assert_eq!(
            acc_a.is_some(),
            acc_b.is_some(),
            "new account {num} existence must match"
        );
    }

    std::fs::remove_dir_all(&path_a).ok();
    std::fs::remove_dir_all(&path_b).ok();
}
