//! Scale benchmark for the consensus state-root hot path.
//!
//! This test is intentionally *not* part of the default fast suite: it opens a
//! storage directory with a production-scale account count and measures the
//! cost that dominates block commit latency.
//!
//! Run against a copied node data dir (never against a live one):
//!   AUGECOIN_BENCH_DB=/tmp/opencode/bench-db \
//!   cargo test --release -p augecoin-node --test bench_state_root_scale \
//!     -- --nocapture --test-threads=1
//!
//! If `AUGECOIN_BENCH_DB` is unset the test seeds a synthetic state of
//! `AUGECOIN_BENCH_ACCOUNTS` accounts (default 50_000) into a temporary dir.

use augecoin_core::account::Account;
use augecoin_core::block::{OperationBlock, OperationBlockHeader};
use augecoin_core::emission::block_reward;
use augecoin_storage::Storage;
use std::collections::HashMap;
use std::time::{Duration, Instant};

const WARMUP: usize = 2;
const ITERS: usize = 7;

fn median(mut v: Vec<Duration>) -> Duration {
    v.sort();
    v[v.len() / 2]
}

fn dummy_block(height: u64) -> OperationBlock {
    OperationBlock {
        header: OperationBlockHeader {
            block_number: height,
            account_key: [0u8; 32],
            reward: block_reward(height),
            fee: 0,
            protocol_version: 5,
            protocol_available: 6,
            timestamp: 1000,
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

/// Seed `n` accounts in a single atomic commit so the fixture is not O(n^2)
/// (each `put_account` would otherwise recompute the SafeBox Merkle hash).
fn seed_accounts(storage: &Storage, n: u64) {
    let mut modified: HashMap<u64, Account> = HashMap::with_capacity(n as usize);
    for i in 1..=n {
        let mut acc = Account::new(i, [0x11; 32], 1);
        acc.add_balance(1_000).unwrap();
        modified.insert(i, acc);
    }
    storage
        .commit_block_atomic_with_contracts(&modified, &dummy_block(1), None, &[], false)
        .expect("seed commit");
    // Drop the resident SafeBox cache so measurements start from persisted state.
    let _ = storage;
}

fn open_bench_storage() -> (Storage, Option<String>) {
    match std::env::var("AUGECOIN_BENCH_DB") {
        Ok(path) if !path.trim().is_empty() => {
            let storage = Storage::open(&path).expect("open bench db");
            // Match the production node configuration: only throttle full
            // SafeBox snapshots, never the in-memory/consensus state.
            storage.set_safebox_snapshot_interval(
                augecoin_core::constants::SAFEBOX_SNAPSHOT_INTERVAL_MAINNET,
            );
            (storage, None)
        }
        _ => {
            let n: u64 = std::env::var("AUGECOIN_BENCH_ACCOUNTS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(50_000);
            let path = format!("/tmp/augecoin-bench-scale-{n}");
            let _ = std::fs::remove_dir_all(&path);
            let storage = Storage::open(&path).expect("open seeded db");
            storage.set_safebox_snapshot_interval(
                augecoin_core::constants::SAFEBOX_SNAPSHOT_INTERVAL_MAINNET,
            );
            seed_accounts(&storage, n);
            (storage, Some(path))
        }
    }
}

#[test]
fn bench_consensus_state_root_scale() {
    let (storage, cleanup) = open_bench_storage();

    // Account count + raw byte volume the root scan has to touch.
    let (account_count, account_bytes) = storage
        .iter_accounts()
        .map(|v| {
            let bytes: usize = v.iter().map(|a| a.to_bytes().len()).sum();
            (v.len() as u64, bytes)
        })
        .unwrap_or((0, 0));
    println!(
        "\n=== consensus state-root scale bench ===\naccounts={account_count} account_bytes={account_bytes} ({:.1} MiB)",
        account_bytes as f64 / (1024.0 * 1024.0)
    );

    // 1) consensus_state_root(): the value carried between block headers.
    let mut root_samples = Vec::new();
    let mut first_root = [0u8; 64];
    for iter in 0..(WARMUP + ITERS) {
        let start = Instant::now();
        let root = storage.consensus_state_root().unwrap();
        let elapsed = start.elapsed();
        if iter == 0 {
            first_root = root;
        }
        if iter >= WARMUP {
            root_samples.push(elapsed);
        }
    }
    let root_avg = root_samples.iter().sum::<Duration>() / root_samples.len() as u32;
    println!(
        "consensus_state_root           median={:>10.2?}  avg={:>10.2?}",
        median(root_samples),
        root_avg
    );

    // 2) SafeBox load (rebuild from accounts CF) then Merkle hash recompute.
    let load_start = Instant::now();
    let _ = storage.safebox().unwrap();
    println!(
        "SafeBox load (rebuild from CF)      {:>10.2?}",
        load_start.elapsed()
    );

    let mut sb_samples = Vec::new();
    for iter in 0..(WARMUP + ITERS) {
        let sb = storage.safebox().unwrap();
        let start = Instant::now();
        let h = sb.compute_safe_box_hash();
        let elapsed = start.elapsed();
        assert_ne!(h, [0u8; 64]);
        if iter >= WARMUP {
            sb_samples.push(elapsed);
        }
    }
    println!(
        "SafeBox::compute_safe_box_hash  median={:>10.2?}",
        median(sb_samples)
    );

    // 3) Full per-block commit path with an empty block (no operations).
    let mut exec_samples = Vec::new();
    for iter in 0..(WARMUP + ITERS) {
        let block = dummy_block(100 + iter as u64);
        let start = Instant::now();
        let res = augecoin_node::execution::execute_block(&block, &storage);
        let elapsed = start.elapsed();
        assert!(res.is_ok(), "execute_block failed: {res:?}");
        if iter >= WARMUP {
            exec_samples.push(elapsed);
        }
    }
    println!(
        "execute_block (empty block, commit) median={:>10.2?}",
        median(exec_samples)
    );

    println!(
        "\n=> one full block currently performs ~2 consensus_state_root scans + 1 SafeBox Merkle rebuild"
    );
    println!("first root = {}", hex::encode(first_root));

    drop(storage);
    if let Some(path) = cleanup {
        let _ = std::fs::remove_dir_all(&path);
    }
}
