//! # augecoin-bench
//!
//! AUGECOIN-native benchmarking harness, conceptually inspired by Hyperledger
//! Caliper but designed entirely around the official AUGECOIN JSON-RPC
//! surface. It drives real on-chain operations (transfer, marketplace,
//! gift, rename) against one or many PoA validators using Tokio + reqwest +
//! [`futures::stream::FuturesUnordered`] for thousands of concurrent calls.
//!
//! It measures:
//! - submitted TPS and confirmed TPS (via on-chain nonce / block scans)
//! - P50 / P95 / P99 latencies
//! - block time and mempool growth
//! - CPU / RAM and RocksDB behaviour around `commit_block_atomic` (exposed
//!   through the existing Prometheus metrics of each validator)
//!
//! Modes: `transfer`, `marketplace`, `gift`, `rename`, `mixed`.
//! Outputs: `benchmark.json`, `benchmark.csv`, `benchmark.html`.
//! Distributed execution across multiple PoA validators and 24h soak runs
//! are supported.
//!
//! This crate never touches the consensus critical path (`execute_block` →
//! `SafeBox` → `commit_block_atomic`): it only submits operations over RPC
//! and reads public metrics, exactly like an external wallet.

#![forbid(unsafe_code)]

pub mod client;
pub mod exporters;
pub mod metrics;
pub mod modes;
pub mod ops;
pub mod report;
pub mod soak;
pub mod stats;

use std::time::Duration;

/// Re-export commonly used types.
pub use client::{BenchClient, ClientConfig};
pub use report::BenchmarkResult;

/// Default RPC path on each validator (TLS-terminated JSON-RPC).
pub const DEFAULT_RPC_PATH: &str = "";
/// Default Prometheus metrics path on each validator.
pub const DEFAULT_METRICS_PATH: &str = "/metrics";

/// A single validated timing sample.
#[derive(Debug, Clone, Copy)]
pub struct LatencySample {
    pub submitted_at_ms: u64,
    pub confirmed_at_ms: u64,
    pub op_index: usize,
    pub mode: &'static str,
}

impl LatencySample {
    /// End-to-end latency from submission to on-chain confirmation.
    pub fn latency_ms(&self) -> u64 {
        self.confirmed_at_ms.saturating_sub(self.submitted_at_ms)
    }
}

/// Duration constant used for RPC/network timeouts.
pub const DEFAULT_RPC_TIMEOUT: Duration = Duration::from_secs(30);
