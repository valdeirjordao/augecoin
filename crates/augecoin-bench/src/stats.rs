//! Statistical helpers: latency percentiles and throughput aggregation.

use crate::LatencySample;

/// Percentile latency summary.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct LatencySummary {
    pub count: usize,
    pub p50_ms: u64,
    pub p95_ms: u64,
    pub p99_ms: u64,
    pub min_ms: u64,
    pub max_ms: u64,
    pub mean_ms: f64,
}

impl LatencySummary {
    /// Compute percentiles from a slice of latency samples (in ms).
    pub fn from_samples(samples: &[LatencySample]) -> Self {
        let mut vals: Vec<u64> = samples.iter().map(|s| s.latency_ms()).collect();
        vals.sort_unstable();
        let count = vals.len();
        if count == 0 {
            return Self::default();
        }
        let pct = |p: f64| -> u64 {
            let idx = ((p * count as f64) as usize).min(count - 1);
            vals[idx]
        };
        let sum: u64 = vals.iter().sum();
        Self {
            count,
            p50_ms: pct(0.50),
            p95_ms: pct(0.95),
            p99_ms: pct(0.99),
            min_ms: vals[0],
            max_ms: vals[count - 1],
            mean_ms: sum as f64 / count as f64,
        }
    }
}

/// Throughput over a duration.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct Throughput {
    /// Operations successfully submitted per second.
    pub submitted_tps: f64,
    /// Operations confirmed on-chain per second.
    pub confirmed_tps: f64,
    /// Total submitted.
    pub submitted: u64,
    /// Total confirmed.
    pub confirmed: u64,
    /// Elapsed wall-clock time (seconds).
    pub elapsed_secs: f64,
}

impl Throughput {
    pub fn compute(submitted: u64, confirmed: u64, elapsed: std::time::Duration) -> Self {
        let secs = elapsed.as_secs_f64().max(f64::MIN_POSITIVE);
        Self {
            submitted_tps: submitted as f64 / secs,
            confirmed_tps: confirmed as f64 / secs,
            submitted,
            confirmed,
            elapsed_secs: secs,
        }
    }
}
