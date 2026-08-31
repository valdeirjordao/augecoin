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

static TEST_COUNTER: AtomicU32 = AtomicU32::new(8000);

fn temp_path() -> String {
    format!(
        "/tmp/augecoin-e2e-{}",
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
fn end_to_end_operation_lifecycle() {
    let path = temp_path();
    let storage = Storage::open(&path).unwrap();

    // Set up: leader, sender, receiver
    let kp_leader = create_keypair(100);
    let kp_sender = create_keypair(200);
    let pk_l = kp_leader.verifying_key();

    // Leader (validator 0)
    let leader = make_account(0, &kp_leader, 50_000, 0);
    storage.put_account(&leader).unwrap();

    // Sender (account 100)
    let sender = make_account(100, &kp_sender, 500_000, 4);
    storage.put_account(&sender).unwrap();

    // Receiver (account 200)
    let kp_receiver = create_keypair(300);
    let receiver = make_account(200, &kp_receiver, 10_000, 1);
    storage.put_account(&receiver).unwrap();

    let leader_initial = storage.get_account(0).unwrap().unwrap().balance;

    // Create ops and submit to mempool
    let mut mempool = Mempool::new(MempoolConfig {
        min_fee: 1,
        max_operations: 1000,
        chain_id: 1,
        ttl_seconds: 3600,
        contract_gas_reserve: 100_000,
    });

    // Op1: sender(100) transfers to receiver(200)
    let op1 = sign_op(&kp_sender, &make_transfer_op(100, 4, 200, 1000, 50));
    mempool
        .validate_and_admit(op1, &storage, 1000)
        .expect("op1 should be admitted");
    assert_eq!(mempool.len(), 1);

    // Op2: sender(100) transfers to receiver(200)
    let op2 = sign_op(&kp_sender, &make_transfer_op(100, 5, 200, 2000, 30));
    mempool
        .validate_and_admit(op2, &storage, 1000)
        .expect("op2 should be admitted");
    assert_eq!(mempool.len(), 2);

    // Op3: receiver(200) transfers to sender(100)
    let op3 = sign_op(&kp_receiver, &make_transfer_op(200, 1, 100, 500, 10));
    mempool
        .validate_and_admit(op3, &storage, 1000)
        .expect("op3 should be admitted");
    assert_eq!(mempool.len(), 3);

    // Build block
    let pending = mempool.pending_cloned();
    assert_eq!(pending.len(), 3);

    let block_height: u64 = 42;

    let block = OperationBlock {
        header: OperationBlockHeader {
            block_number: block_height,
            account_key: pk_l.to_bytes(),
            reward: block_reward(block_height),
            fee: 0,
            protocol_version: 5,
            protocol_available: 6,
            timestamp: 1700000000 + block_height * 60,
            initial_safe_box_hash: [0u8; 64],
            operations_hash: [0u8; 64],
            block_payload: vec![],
            proof_of_work: [0u8; 32],
            previous_proof_of_work: [0u8; 32],
            leader_id: 0,
            chain_id: 1,
        },
        operations: pending,
        leader_signature: dummy_signature(),
        quorum_signatures: vec![dummy_signature(), dummy_signature(), dummy_signature()],
        block_hash: [0u8; 64],
    };

    // Execute
    let safe_box_hash = execute_block(&block, &storage).expect("block execution should succeed");
    assert_ne!(
        safe_box_hash, [0u8; 64],
        "safe_box_hash must be non-zero after execution"
    );

    // Verify sender
    let sender_after = storage.get_account(100).unwrap().unwrap();
    assert_eq!(sender_after.balance, 500_000 - 1050 - 2030 + 500);
    assert_eq!(sender_after.n_operation, 6);

    // Verify receiver
    let receiver_after = storage.get_account(200).unwrap().unwrap();
    assert_eq!(receiver_after.balance, 10_000 + 1000 + 2000 - 510);
    assert_eq!(receiver_after.n_operation, 2);

    // Verify leader got 100% block reward
    let expected_leader_reward = block_reward(block_height);
    assert!(
        storage.get_account(0).unwrap().unwrap().balance >= leader_initial + expected_leader_reward
    );

    // Verify new accounts created (CT_ACCOUNTS_PER_BLOCK)
    let start = block_height * augecoin_core::constants::CT_ACCOUNTS_PER_BLOCK;
    for i in 0..augecoin_core::constants::CT_ACCOUNTS_PER_BLOCK {
        assert!(
            storage.get_account(start + i).unwrap().is_some(),
            "new account {} must exist",
            start + i
        );
    }

    mempool.clear();
    assert!(mempool.is_empty());
    std::fs::remove_dir_all(&path).ok();
}

#[test]
fn end_to_end_multiple_blocks_in_sequence() {
    let path = temp_path();
    let storage = Storage::open(&path).unwrap();

    let kp_leader = create_keypair(600);
    let kp_alice = create_keypair(700);
    let pk_l = kp_leader.verifying_key();

    // Use accounts outside the dev range (0-4):
    //   Alice = 100, Leader = 99
    let leader = make_account(99, &kp_leader, 100_000, 0);
    storage.put_account(&leader).unwrap();

    let alice = make_account(100, &kp_alice, 1_000_000, 0);
    storage.put_account(&alice).unwrap();

    let mut prev_hash = [0x01u8; 64];
    let mut prev_safe_box_hash = [0u8; 64];

    for (alice_seq, height) in (0_u64..).zip(5..=8) {
        let mut mempool = Mempool::new(MempoolConfig {
            min_fee: 1,
            max_operations: 1000,
            chain_id: 1,
            ttl_seconds: 3600,
            contract_gas_reserve: 100_000,
        });

        let op = sign_op(&kp_alice, &make_transfer_op(100, alice_seq, 99, 100, 10));
        mempool.validate_and_admit(op, &storage, 1000).unwrap();

        let pending = mempool.pending_cloned();

        let block = OperationBlock {
            header: OperationBlockHeader {
                block_number: height,
                account_key: pk_l.to_bytes(),
                reward: block_reward(height),
                fee: 0,
                protocol_version: 5,
                protocol_available: 6,
                timestamp: 1700000000 + height * 60,
                initial_safe_box_hash: prev_safe_box_hash,
                operations_hash: [0u8; 64],
                block_payload: vec![],
                proof_of_work: [0u8; 32],
                previous_proof_of_work: prev_hash[..32].try_into().unwrap_or([0u8; 32]),
                leader_id: 99,
                chain_id: 1,
            },
            operations: pending,
            leader_signature: dummy_signature(),
            quorum_signatures: vec![dummy_signature(), dummy_signature(), dummy_signature()],
            block_hash: [0u8; 64],
        };

        prev_hash = block.hash();
        prev_safe_box_hash = execute_block(&block, &storage).unwrap();
    }

    // Alice: sent 4 ops of 100+10=110 each = 440 deducted
    // Leader receives 100% of reward + 100% of fees for each block
    let alice_after = storage.get_account(100).unwrap().unwrap();
    assert_eq!(alice_after.balance, 1_000_000 - 440);
    assert_eq!(alice_after.n_operation, 4);

    // Verify leader got transfers + 100% block rewards + fees
    let leader_after = storage.get_account(99).unwrap().unwrap();
    assert!(leader_after.balance > 100_000);

    // Verify new accounts were created for each block
    for h in 5..=8 {
        let start = h * augecoin_core::constants::CT_ACCOUNTS_PER_BLOCK;
        for i in 0..augecoin_core::constants::CT_ACCOUNTS_PER_BLOCK {
            assert!(storage.get_account(start + i).unwrap().is_some());
        }
    }

    std::fs::remove_dir_all(&path).ok();
}
