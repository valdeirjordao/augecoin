//! Phase-disaggregated micro benchmark for the block execution hot path.
//!
//! Run with:
//!   cargo test --release -p augecoin-node --test bench_phases -- --nocapture --test-threads=1
//!
//! Everything is reported as the *median* of N measured iterations after a
//! warmup, so the numbers are stable enough to compare before/after a change.

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
use std::time::{Duration, Instant};

const OP_COUNT: u64 = 1000;
const WARMUP: usize = 3;
const ITERS: usize = 11;

fn dummy_sig() -> Ed25519Signature {
    Ed25519Signature { bytes: [0u8; 64] }
}

fn median(mut v: Vec<Duration>) -> Duration {
    v.sort();
    v[v.len() / 2]
}

fn make_storage(temp_id: &str) -> (Storage, String) {
    let path = format!("/tmp/augecoin-bench-{temp_id}");
    let _ = std::fs::remove_dir_all(&path);
    let storage = Storage::open(&path).unwrap();
    (storage, path)
}

fn build_fixture(op_count: u64, seed_accounts: u64) -> (Storage, String, OperationBlock, HdWallet) {
    let wallet = HdWallet::from_mnemonic(
        "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about",
    )
    .unwrap();
    let pk = wallet.derive_keypair(0).verifying_key();
    let ed_pk: [u8; 32] = pk.to_bytes();
    let leader_ed: [u8; 32] = [0xAA; 32];

    let (storage, path) = make_storage(&format!("{}-{seed_accounts}", op_count));
    // Match the node's steady-state configuration (mainnet default). Without
    // this the storage layer snapshots the whole SafeBox on every commit,
    // which is not how a production node runs.
    storage.set_safebox_snapshot_interval(augecoin_core::constants::SAFEBOX_SNAPSHOT_INTERVAL_MAINNET);

    for i in 1..=op_count {
        let mut acc = Account::new(i, ed_pk, 1);
        acc.add_balance(10_000_000).unwrap();
        storage.put_account(&acc).unwrap();
    }
    // Extra dormant accounts to make the state-root scan reflect a realistic
    // account count rather than only the accounts a block touches.
    for i in (op_count + 1)..=(op_count + seed_accounts) {
        let mut acc = Account::new(i, ed_pk, 1);
        acc.add_balance(1_000).unwrap();
        storage.put_account(&acc).unwrap();
    }
    let mut leader = Account::new(10000, leader_ed, 1);
    leader.add_balance(0).unwrap();
    storage.put_account(&leader).unwrap();

    let kp = wallet.derive_keypair(0);
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
                    amount: 100,
                    payload: vec![],
                }],
                changers: vec![],
                fee: 5,
            },
            signatures: vec![dummy_sig()],
            chain_id: 1,
        };
        let msg = unsigned.to_bytes_stripped();
        let sig = kp.sign(&msg);
        ops.push(Operation {
            signatures: vec![sig],
            ..unsigned
        });
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

    (storage, path, block, wallet)
}

/// Fresh storage per iteration: `execute_block` commits, so each measured run
/// must start from an identical pre-state.
fn time_execute_block(op_count: u64, seed_accounts: u64) -> Duration {
    let mut samples = Vec::new();
    for iter in 0..(WARMUP + ITERS) {
        let (storage, path, block, _w) = build_fixture(op_count, seed_accounts);
        let start = Instant::now();
        let res = execution::execute_block(&block, &storage);
        let elapsed = start.elapsed();
        assert!(res.is_ok(), "execute_block failed: {res:?}");
        drop(storage);
        std::fs::remove_dir_all(&path).ok();
        if iter >= WARMUP {
            samples.push(elapsed);
        }
    }
    median(samples)
}

/// Dry-run validation (what every validator does before voting).
fn time_validate_only(op_count: u64, seed_accounts: u64) -> Duration {
    let mut samples = Vec::new();
    for iter in 0..(WARMUP + ITERS) {
        let (storage, path, block, _w) = build_fixture(op_count, seed_accounts);
        let start = Instant::now();
        let res = execution::validate_block_execution(&block, &storage);
        let elapsed = start.elapsed();
        assert!(res.is_ok(), "validate_block_execution failed: {res:?}");
        drop(storage);
        std::fs::remove_dir_all(&path).ok();
        if iter >= WARMUP {
            samples.push(elapsed);
        }
    }
    median(samples)
}

fn time_state_root(seed_accounts: u64) -> Duration {
    let (storage, path, _block, _w) = build_fixture(1, seed_accounts);
    let mut samples = Vec::new();
    for iter in 0..(WARMUP + ITERS) {
        let start = Instant::now();
        let root = storage.consensus_state_root().unwrap();
        let elapsed = start.elapsed();
        assert_ne!(root, [0u8; 64]);
        if iter >= WARMUP {
            samples.push(elapsed);
        }
    }
    drop(storage);
    std::fs::remove_dir_all(&path).ok();
    median(samples)
}

fn time_merkle(op_count: u64) -> Duration {
    let (_storage, path, block, _w) = build_fixture(op_count, 0);
    let mut samples = Vec::new();
    for iter in 0..(WARMUP + ITERS) {
        let start = Instant::now();
        let root = block.compute_operations_merkle_root();
        let elapsed = start.elapsed();
        assert_ne!(root, [0u8; 64]);
        if iter >= WARMUP {
            samples.push(elapsed);
        }
    }
    std::fs::remove_dir_all(&path).ok();
    median(samples)
}

fn time_block_hash(op_count: u64) -> Duration {
    let (_storage, path, block, _w) = build_fixture(op_count, 0);
    let mut samples = Vec::new();
    for iter in 0..(WARMUP + ITERS) {
        let start = Instant::now();
        let h = block.hash();
        let elapsed = start.elapsed();
        assert_ne!(h, [0u8; 64]);
        if iter >= WARMUP {
            samples.push(elapsed);
        }
    }
    std::fs::remove_dir_all(&path).ok();
    median(samples)
}

/// Steady-state cost of the SafeBox Merkle root, which is recomputed on every
/// block commit (`commit_safebox_incremental` -> `update_hash`).
fn time_safebox_hash(accounts: u64) -> Duration {
    use augecoin_core::safe_box::SafeBox;
    let mut sb = SafeBox::new(5, 0);
    for i in 1..=accounts {
        let mut acc = Account::new(i, [7u8; 32], 1);
        acc.add_balance(1).unwrap();
        sb.add_account(acc);
    }
    let mut samples = Vec::new();
    for iter in 0..(WARMUP + ITERS) {
        let start = Instant::now();
        let h = sb.compute_safe_box_hash();
        let elapsed = start.elapsed();
        assert_ne!(h, [0u8; 64]);
        if iter >= WARMUP {
            samples.push(elapsed);
        }
    }
    median(samples)
}

/// `execute_block` against a *warm* SafeBox cache (steady state of a running
/// node). The fixture is rebuilt each iteration because `execute_block`
/// commits, but `storage.safebox()` is called first so the O(n) SafeBox
/// rebuild from RocksDB happens outside the timed window.
fn time_execute_block_warm(op_count: u64, seed_accounts: u64) -> Duration {
    let mut samples = Vec::new();
    for iter in 0..(WARMUP + ITERS) {
        let (storage, path, block, _w) = build_fixture(op_count, seed_accounts);
        // Pre-load the resident SafeBox cache (outside the timed section).
        let _ = storage.safebox().unwrap();
        let start = Instant::now();
        let res = execution::execute_block(&block, &storage);
        let elapsed = start.elapsed();
        assert!(res.is_ok(), "execute_block failed: {res:?}");
        drop(storage);
        std::fs::remove_dir_all(&path).ok();
        if iter >= WARMUP {
            samples.push(elapsed);
        }
    }
    median(samples)
}

/// The full per-block path a validator runs: dry-run validation (to sign) and
/// then commit. Both currently re-verify every operation signature.
fn time_validator_path(op_count: u64, seed_accounts: u64) -> Duration {
    let mut samples = Vec::new();
    for iter in 0..(WARMUP + ITERS) {
        let (storage, path, block, _w) = build_fixture(op_count, seed_accounts);
        let _ = storage.safebox().unwrap();
        let start = Instant::now();
        let v = execution::validate_block_execution(&block, &storage);
        assert!(v.is_ok(), "validate failed: {v:?}");
        let c = execution::execute_block(&block, &storage);
        assert!(c.is_ok(), "commit failed: {c:?}");
        let elapsed = start.elapsed();
        drop(storage);
        std::fs::remove_dir_all(&path).ok();
        if iter >= WARMUP {
            samples.push(elapsed);
        }
    }
    median(samples)
}

/// Raw Ed25519 verify cost, to attribute the signature-verification share.
fn time_ed25519_verify(iters: usize) -> Duration {
    use augecoin_crypto::signature::Ed25519Signature;
    let wallet = HdWallet::from_mnemonic(
        "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about",
    )
    .unwrap();
    let kp = wallet.derive_keypair(0);
    let pk = kp.verifying_key();
    let msg = [42u8; 128];
    let sig = kp.sign(&msg);
    let mut samples = Vec::new();
    for _ in 0..(WARMUP + ITERS) {
        let start = Instant::now();
        for _ in 0..iters {
            let ok = sig.verify(&pk, &msg);
            assert!(ok);
        }
        let elapsed = start.elapsed();
        samples.push(elapsed);
    }
    let total = median(samples);
    println!(
        "      (raw ed25519 verify: {:.2?} total / {iters} = {:.2?} per verify)",
        total,
        total / iters as u32
    );
    total
}

/// Full SafeBox serialization, i.e. what a snapshot writes every
/// `snapshot_interval` blocks.
fn time_safebox_to_bytes(accounts: u64) -> Duration {
    use augecoin_core::safe_box::SafeBox;
    let mut sb = SafeBox::new(5, 0);
    for i in 1..=accounts {
        let mut acc = Account::new(i, [7u8; 32], 1);
        acc.add_balance(1).unwrap();
        sb.add_account(acc);
    }
    let mut samples = Vec::new();
    for iter in 0..(WARMUP + ITERS) {
        let start = Instant::now();
        let bytes = sb.to_bytes();
        let elapsed = start.elapsed();
        assert!(!bytes.is_empty());
        if iter >= WARMUP {
            samples.push(elapsed);
        }
    }
    median(samples)
}

#[test]
fn bench_block_execution_phases() {
    println!("\n=== AUGE block-execution phase benchmark ===");
    println!("ops={OP_COUNT} warmup={WARMUP} iters={ITERS} (median)\n");

    let exec_small = time_execute_block(OP_COUNT, 0);
    let exec_big = time_execute_block(OP_COUNT, 20_000);
    let exec_warm_small = time_execute_block_warm(OP_COUNT, 0);
    let exec_warm_big = time_execute_block_warm(OP_COUNT, 20_000);
    let val_small = time_validate_only(OP_COUNT, 0);
    let val_big = time_validate_only(OP_COUNT, 20_000);
    let root_small = time_state_root(0);
    let root_big = time_state_root(20_000);
    let sb_small = time_safebox_hash(1_000);
    let sb_big = time_safebox_hash(20_000);
    let merkle = time_merkle(OP_COUNT);
    let bhash = time_block_hash(OP_COUNT);
    let validator_path = time_validator_path(OP_COUNT, 20_000);
    let sb_to_bytes = time_safebox_to_bytes(20_000);
    let _ed = time_ed25519_verify(1000);

    let rows: [(&str, Duration); 14] = [
        ("execute_block COLD (1k ops, 1k accts)", exec_small),
        ("execute_block COLD (1k ops, 21k accts)", exec_big),
        ("execute_block WARM (1k ops, 1k accts)", exec_warm_small),
        ("execute_block WARM (1k ops, 21k accts)", exec_warm_big),
        ("VALIDATOR PATH validate+exec (21k)", validator_path),
        ("validate_block_execution (1k/1k)", val_small),
        ("validate_block_execution (1k/21k)", val_big),
        ("consensus_state_root (1k accts)", root_small),
        ("consensus_state_root (21k accts)", root_big),
        ("SafeBox::compute_safe_box_hash (1k)", sb_small),
        ("SafeBox::compute_safe_box_hash (21k)", sb_big),
        ("SafeBox::to_bytes snapshot (21k)", sb_to_bytes),
        ("compute_operations_merkle_root (1k)", merkle),
        ("block.hash() (1k ops)", bhash),
    ];

    println!("{:<42} {:>12}", "phase", "median");
    println!("{}", "-".repeat(56));
    for (name, d) in rows {
        println!("{name:<42} {d:>12.2?}");
    }

    let tps = OP_COUNT as f64 / exec_warm_small.as_secs_f64();
    let tps_big = OP_COUNT as f64 / exec_warm_big.as_secs_f64();
    println!("\nWARM execute_block throughput (1k accts):  {tps:.0} ops/s");
    println!("WARM execute_block throughput (21k accts): {tps_big:.0} ops/s");
    println!(
        "SafeBox Merkle share of WARM execute_block (21k): {:.1}%",
        100.0 * sb_big.as_secs_f64() / exec_warm_big.as_secs_f64()
    );
    println!(
        "SafeBox Merkle share of WARM execute_block (1k): {:.1}%",
        100.0 * sb_small.as_secs_f64() / exec_warm_small.as_secs_f64()
    );
}
