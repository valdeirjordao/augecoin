//! Aggregated benchmark result document, serialized to JSON.

use crate::metrics::MetricsSnapshot;
use crate::modes::{BenchMode, BurstReport};
use crate::stats::LatencySummary;
use crate::LatencySample;

/// The final benchmark result written to `benchmark.json`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct BenchmarkResult {
    pub meta: BenchMeta,
    pub mode: String,
    pub bursts: Vec<BurstReport>,
    pub latency: LatencySummary,
    pub throughput: ThroughputJson,
    pub snapshots: Vec<MetricsSnapshot>,
    pub config: BenchConfigJson,
}

/// Metadata about the run.
#[derive(Debug, Clone, serde::Serialize)]
pub struct BenchMeta {
    pub started_unix: u64,
    pub finished_unix: u64,
    pub duration_secs: f64,
    pub bench_version: String,
    pub chain: String,
}

/// JSON-friendly throughput.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ThroughputJson {
    pub submitted_tps: f64,
    pub confirmed_tps: f64,
    pub submitted: u64,
    pub confirmed: u64,
    pub elapsed_secs: f64,
}

/// JSON-friendly config echo.
#[derive(Debug, Clone, serde::Serialize)]
pub struct BenchConfigJson {
    pub endpoints: Vec<String>,
    pub ops_per_burst: usize,
    pub bursts: usize,
    pub concurrency: usize,
    pub soak_hours: f64,
}

impl BenchmarkResult {
    pub fn new(
        mode: BenchMode,
        endpoints: Vec<String>,
        ops_per_burst: usize,
        bursts: usize,
        concurrency: usize,
        soak_hours: f64,
    ) -> Self {
        let started = crate::modes::now_ms() / 1000;
        Self {
            meta: BenchMeta {
                started_unix: started,
                finished_unix: started,
                duration_secs: 0.0,
                bench_version: env!("CARGO_PKG_VERSION").into(),
                chain: "augecoin-testnet".into(),
            },
            mode: mode.as_str().into(),
            bursts: Vec::new(),
            latency: LatencySummary::default(),
            throughput: ThroughputJson {
                submitted_tps: 0.0,
                confirmed_tps: 0.0,
                submitted: 0,
                confirmed: 0,
                elapsed_secs: 0.0,
            },
            snapshots: Vec::new(),
            config: BenchConfigJson {
                endpoints,
                ops_per_burst,
                bursts,
                concurrency,
                soak_hours,
            },
        }
    }

    /// Aggregate burst reports + latency samples into the final document.
    pub fn finalize(
        &mut self,
        samples: &[LatencySample],
        started_unix: u64,
        duration: std::time::Duration,
    ) {
        self.latency = LatencySummary::from_samples(samples);
        let submitted: u64 = self.bursts.iter().map(|b| b.submitted as u64).sum();
        let confirmed: u64 = self.bursts.iter().map(|b| b.confirmed as u64).sum();
        self.throughput = ThroughputJson {
            submitted_tps: submitted as f64 / duration.as_secs_f64().max(f64::MIN_POSITIVE),
            confirmed_tps: confirmed as f64 / duration.as_secs_f64().max(f64::MIN_POSITIVE),
            submitted,
            confirmed,
            elapsed_secs: duration.as_secs_f64(),
        };
        self.meta.finished_unix = crate::modes::now_ms() / 1000;
        self.meta.duration_secs = duration.as_secs_f64();
        self.meta.started_unix = started_unix;
    }
}
