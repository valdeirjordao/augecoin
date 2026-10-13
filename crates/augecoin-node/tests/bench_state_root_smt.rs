//! Benchmark: legacy flat consensus root vs incremental SMT state root.
//!
//! Measures, on the same storage instance and state size:
//!   * cost of the legacy `consensus_state_root()` scan (O(state));
//!   * one-time SMT migration/rebuild cost (O(state));
//!   * per-block incremental commit cost for k touched keys (O(k·log n));
//!   * persisted SMT node count.
//!
//! Run:
//!   AUGECOIN_BENCH_ACCOUNTS=50000 cargo test --release -p augecoin-node \
//!     --test bench_state_root_smt -- --nocapture --test-threads=1

use augecoin_core::account::Account;
use augecoin_core::block::{OperationBlock, OperationBlockHeader};
use augecoin_storage::Storage;
use std::collections::HashMap;
use std::time::{Duration, Instant};

const WARMUP: usize = 2;
const ITERS: usize = 7;
const TOUCHED_PER_BLOCK: u64 = 16;

fn median(mut v: Vec<Duration>) -> Duration {
    v.sort();
    v[v.len() / 2]
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

struct Timer;

impl Timer {
    fn run<F: FnMut()>(iters: usize, mut f: F) -> Duration {
        let mut samples = Vec::with_capacity(iters);
        for i in 0..(WARMUP + iters) {
            let start = Instant::now();
            f();
            let elapsed = start.elapsed();
            if i >= WARMUP {
                samples.push(elapsed);
            }
        }
        median(samples)
    }
}

#[test]
fn bench_flat_vs_smt() {
    let n: u64 = std::env::var("AUGECOIN_BENCH_ACCOUNTS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(20_000);
    let path = format!("/tmp/augecoin-bench-smt-{n}");
    let _ = std::fs::remove_dir_all(&path);
    let storage = Storage::open(&path).expect("open bench db");

    // Seed n accounts in a single commit.
    let mut seed = HashMap::with_capacity(n as usize);
    for i in 1..=n {
        let mut acc = Account::new(i, [0x11; 32], 1);
        acc.add_balance(1_000).unwrap();
        seed.insert(i, acc);
    }
    let seed_start = Instant::now();
    storage
        .commit_block_atomic_with_contracts(&seed, &block(1, 5), None, &[], false)
        .expect("seed");
    let seed_elapsed = seed_start.elapsed();

    // --- Legacy flat root scan ---
    let flat = Timer::run(ITERS, || {
        let _ = storage.consensus_state_root().unwrap();
    });

    // --- SMT migration / full rebuild ---
    let migrate = Instant::now();
    let root_v6 = storage.rebuild_state_tree().unwrap();
    let migrate_elapsed = migrate.elapsed();
    assert_ne!(root_v6, [0u8; 64]);

    // --- Cached SMT root read ---
    let cached = Timer::run(ITERS, || {
        let _ = storage.consensus_state_root_v6().unwrap();
    });

    // --- Incremental per-block commit (TOUCHED_PER_BLOCK accounts) ---
    storage.set_state_tree_activation(1);
    let mut height = 100u64;
    let incremental = Timer::run(ITERS, || {
        height += 1;
        let mut modified = HashMap::new();
        for offset in 0..TOUCHED_PER_BLOCK {
            let num = (offset % n) + 1;
            let mut acc = Account::new(num, [0x11; 32], 1);
            acc.add_balance(2_000 + height).unwrap();
            modified.insert(num, acc);
        }
        storage
            .commit_block_atomic_with_contracts(&modified, &block(height, 6), None, &[], false)
            .unwrap();
    });

    let nodes = storage.state_tree_node_count().unwrap();
    println!("\n=== flat vs SMT state root (accounts={n}) ===");
    println!("seed (single block, flat)          {seed_elapsed:>12.3?}");
    println!("legacy flat consensus_state_root   {flat:>12.3?}");
    println!("SMT full rebuild (migration)       {migrate_elapsed:>12.3?}");
    println!("SMT cached root read               {cached:>12.3?}");
    println!("SMT incremental commit (k={TOUCHED_PER_BLOCK})    {incremental:>12.3?}");
    println!("persisted state_nodes count        {nodes:>12}");
    println!(
        "speedup (flat / incremental)       {:>12.1}x",
        flat.as_secs_f64() / incremental.as_secs_f64().max(1e-9)
    );

    drop(storage);
    let _ = std::fs::remove_dir_all(&path);
}

/// Isolated SMT scaling: prove per-update cost is O(k · log n), independent of
/// the total state size, and that storage is O(n).
#[test]
fn bench_smt_update_scaling() {
    use augecoin_storage::state_tree::{self, MemNodeBackend, TreeChange};

    let n: u64 = std::env::var("AUGECOIN_BENCH_ACCOUNTS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(200_000);

    let build: Vec<TreeChange> = (1..=n)
        .map(|i| TreeChange::put(b"accounts", &i.to_be_bytes(), b"v"))
        .collect();
    let mut backend = MemNodeBackend::default();
    let start = Instant::now();
    let _root = state_tree::update(&mut backend, build);
    let rebuild = start.elapsed();
    let nodes = backend.nodes.len();

    println!("\n=== SMT scaling (n={n}) ===");
    println!("full rebuild        {rebuild:>12.3?}   nodes={nodes}");

    for k in [1u64, 16, 256, 4096] {
        // Fresh backend per k: a real node only ever advances its root forward,
        // so rewinding to an old root on a mutated backend is not a valid case.
        let base: Vec<TreeChange> = (1..=n)
            .map(|i| TreeChange::put(b"accounts", &i.to_be_bytes(), b"v"))
            .collect();
        let mut backend_k = MemNodeBackend::default();
        let base_root = state_tree::update(&mut backend_k, base);
        let changes: Vec<TreeChange> = (0..k)
            .map(|offset| {
                let i = (offset % n) + 1;
                TreeChange::put(b"accounts", &i.to_be_bytes(), b"v2")
            })
            .collect();
        let start = Instant::now();
        let (new_root, stats) =
            state_tree::update_from_root_with_stats(&mut backend_k, Some(base_root), changes);
        let elapsed = start.elapsed();
        println!(
            "k={k:<5}            {elapsed:>12.3?}   reads={} writes={} deletes={}",
            stats.nodes_read, stats.nodes_written, stats.nodes_deleted
        );
        let _ = new_root;
    }

    // Doubling n should roughly double rebuild time and node count (linearity).
    let n2 = n * 2;
    let build2: Vec<TreeChange> = (1..=n2)
        .map(|i| TreeChange::put(b"accounts", &i.to_be_bytes(), b"v"))
        .collect();
    let mut backend2 = MemNodeBackend::default();
    let start = Instant::now();
    let _ = state_tree::update(&mut backend2, build2);
    println!(
        "full rebuild n*2    {:>12.3?}   nodes={}",
        start.elapsed(),
        backend2.nodes.len()
    );
}
