//! Long-duration (soak) test support.
//!
//! Runs a bench load continuously for a configurable number of hours (default
//! 24), collecting periodic metric snapshots and per-cycle burst reports so
//! drift, mempool growth, and RocksDB behaviour can be observed over time.

use crate::client::BenchClient;
use crate::metrics::{read_proc_stats, MetricsSnapshot};
use crate::modes::{run_burst, BenchAccount, BenchMode};
use crate::report::BenchmarkResult;
use anyhow::{Context, Result};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// A single soak cycle summary.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SoakCycle {
    pub cycle: usize,
    pub started_unix: u64,
    pub snapshot: MetricsSnapshot,
    pub report: Option<crate::modes::BurstReport>,
}

/// Run the soak test.
///
/// `hours` (default 24) determines the total duration. Every `interval_secs`
/// a burst of `ops_per_burst` operations is fired and metrics are sampled.
pub async fn run_soak(
    client: &BenchClient,
    mode: BenchMode,
    accounts: &[BenchAccount],
    ops_per_burst: usize,
    hours: f64,
    interval_secs: u64,
    result: &mut BenchmarkResult,
) -> Result<Vec<SoakCycle>> {
    let total = Duration::from_secs_f64(hours * 3600.0);
    let started = Instant::now();
    let mut cycles = Vec::new();
    let mut cycle = 0usize;

    while started.elapsed() < total {
        cycle += 1;
        let started_unix = now_unix();

        // Sample metrics before the burst.
        let bodies = client.scrape_all_metrics().await.unwrap_or_default();
        let proc = read_proc_stats().ok();
        let snap = MetricsSnapshot::from_bodies(&bodies, proc.as_ref(), started_unix);
        result.snapshots.push(snap.clone());

        // Fire the burst.
        let report = run_burst(
            mode,
            client,
            accounts,
            ops_per_burst,
            Duration::from_secs(120),
        )
        .await
        .context("soak burst failed")?;
        result.bursts.push(report.clone());

        cycles.push(SoakCycle {
            cycle,
            started_unix,
            snapshot: snap,
            report: Some(report),
        });

        // Sleep until the next interval.
        let remaining = total.saturating_sub(started.elapsed());
        if remaining.is_zero() {
            break;
        }
        tokio::time::sleep(Duration::from_secs(interval_secs).min(remaining)).await;
    }

    Ok(cycles)
}

/// Current unix seconds.
pub fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
