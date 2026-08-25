//! AUGECOIN benchmark CLI.
//!
//! Example:
//! ```text
//! augecoin-bench --endpoints https://127.0.0.1:9005 --mode transfer \
//!     --ops 1000 --bursts 5 --concurrency 256 --out ./bench-out
//! ```

use anyhow::{anyhow, Result};
use augecoin_bench::client::{BenchClient, ClientConfig};
use augecoin_bench::exporters;
use augecoin_bench::metrics::{read_proc_stats, MetricsSnapshot};
use augecoin_bench::modes::{ensure_accounts, run_burst, BenchAccount, BenchMode};
use augecoin_bench::report::BenchmarkResult;
use augecoin_bench::soak::run_soak;
use clap::Parser;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[derive(Parser, Debug, Clone)]
#[command(name = "augecoin-bench", version, about = "AUGECOIN benchmark harness")]
struct Cli {
    /// Validator RPC endpoints (comma separated; distributed across all).
    #[arg(long, value_delimiter = ',', env = "AUGECOIN_BENCH_ENDPOINTS")]
    endpoints: Vec<String>,

    /// Benchmark mode: transfer|marketplace|gift|rename|mixed.
    #[arg(long, default_value = "transfer")]
    mode: String,

    /// Operations per burst.
    #[arg(long, default_value_t = 1000)]
    ops: usize,

    /// Number of bursts (1 run).
    #[arg(long, default_value_t = 1)]
    bursts: usize,

    /// Max concurrency for each burst (fan-out size).
    #[arg(long, default_value_t = 256)]
    concurrency: usize,

    /// Output directory for benchmark.{json,csv,html}.
    #[arg(long, default_value = "./bench-out")]
    out: PathBuf,

    /// Optional API key (x-api-key header).
    #[arg(long)]
    api_key: Option<String>,

    /// Comma-separated funded account numbers to drive load.
    #[arg(long, value_delimiter = ',')]
    accounts: Vec<u64>,

    /// Prometheus metrics endpoints (comma separated). These live on a
    /// different port than the JSON-RPC endpoint. When omitted, they are
    /// derived from the RPC endpoints using the AUGECOIN node port scheme
    /// (metrics port = 9100 + (rpc_port - 9005) * 10, plain HTTP).
    #[arg(long, value_delimiter = ',', env = "AUGECOIN_BENCH_METRICS_ENDPOINTS")]
    metrics_endpoints: Vec<String>,

    /// Soak mode: run for N hours (default 24) instead of a finite number of bursts.
    #[arg(long)]
    soak_hours: Option<f64>,

    /// Soak sampling interval in seconds.
    #[arg(long, default_value_t = 60)]
    soak_interval: u64,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    if cli.endpoints.is_empty() {
        return Err(anyhow!("at least one --endpoints is required"));
    }
    let mode: BenchMode = cli.mode.parse()?;

    let metrics_endpoints = if cli.metrics_endpoints.is_empty() {
        cli.endpoints
            .iter()
            .map(|ep| derive_metrics_endpoint(ep))
            .collect()
    } else {
        cli.metrics_endpoints.clone()
    };

    let config = ClientConfig::new(cli.endpoints.clone())
        .with_api_key(cli.api_key.clone())
        .with_metrics_endpoints(metrics_endpoints)
        .with_timeout(Duration::from_secs(60));
    let client = BenchClient::new(config)?;

    // Derive funded accounts. When explicit accounts are given, use them;
    // otherwise fall back to the 4 dev-validator accounts (0..3), which are
    // the only accounts the benchmark controls keys for.
    let accounts = if !cli.accounts.is_empty() {
        let mut out = Vec::new();
        for n in &cli.accounts {
            let key = derive_dev_key(*n);
            out.push(BenchAccount { key, account: *n });
        }
        out
    } else {
        (0..4)
            .map(|id| BenchAccount {
                key: derive_dev_key(id),
                account: id,
            })
            .collect()
    };
    if accounts.is_empty() {
        return Err(anyhow!("no accounts available to drive load"));
    }

    ensure_accounts(&client, &accounts).await?;

    let mut result = BenchmarkResult::new(
        mode,
        cli.endpoints.clone(),
        cli.ops,
        cli.bursts,
        cli.concurrency,
        cli.soak_hours.unwrap_or(0.0),
    );

    let started_unix = now_unix();
    let start = Instant::now();

    if let Some(hours) = cli.soak_hours {
        let hours = if hours <= 0.0 { 24.0 } else { hours };
        println!(
            "[bench] soak mode: {}h, interval {}s, mode {}, {} ops/burst",
            hours,
            cli.soak_interval,
            mode.as_str(),
            cli.ops
        );
        let _cycles = run_soak(
            &client,
            mode,
            &accounts,
            cli.ops,
            hours,
            cli.soak_interval,
            &mut result,
        )
        .await?;
        let duration = start.elapsed();
        result.finalize(&[], started_unix, duration);
    } else {
        for burst in 0..cli.bursts {
            println!(
                "[bench] burst {}/{}: mode={}, ops={}, concurrency={}",
                burst + 1,
                cli.bursts,
                mode.as_str(),
                cli.ops,
                cli.concurrency
            );
            let report =
                run_burst(mode, &client, &accounts, cli.ops, Duration::from_secs(120)).await?;
            println!(
                "[bench]   submitted={} confirmed={} (env {:.2}/conf {:.2} tps, p50={}ms p95={}ms p99={}ms)",
                report.submitted,
                report.confirmed,
                report.submitted_tps,
                report.confirmed_tps,
                report.p50_ms,
                report.p95_ms,
                report.p99_ms
            );
            result.bursts.push(report);

            // Sample metrics each burst.
            let bodies = client.scrape_all_metrics().await.unwrap_or_default();
            let proc = read_proc_stats().ok();
            let snap = MetricsSnapshot::from_bodies(&bodies, proc.as_ref(), now_unix());
            result.snapshots.push(snap);
        }
        let duration = start.elapsed();
        result.finalize(&[], started_unix, duration);
    }

    exporters::export_all(&result, &cli.out)?;
    println!(
        "[bench] done in {:.2}s -> {}",
        result.meta.duration_secs,
        cli.out.display()
    );
    println!(
        "[bench] TPS submitted={:.2} confirmed={:.2} | latency p50={}ms p95={}ms p99={}ms",
        result.throughput.submitted_tps,
        result.throughput.confirmed_tps,
        result.latency.p50_ms,
        result.latency.p95_ms,
        result.latency.p99_ms
    );
    Ok(())
}

/// Derive the deterministic dev-validator key for a given validator id,
/// mirroring the node's `create_dev_validator_key` scheme exactly.
fn derive_dev_key(vid: u64) -> augecoin_crypto::signature::HybridKeyPair {
    let mut seed = [0u8; 64];
    seed[0] = 0xAB;
    seed[1..9].copy_from_slice(&vid.to_be_bytes());
    augecoin_crypto::hdkeys::HdWallet::from_seed(&seed).derive_keypair(0)
}

/// Derive the Prometheus metrics endpoint for a given RPC endpoint.
///
/// The AUGECOIN node exposes JSON-RPC on `9005 + validator_id` and Prometheus
/// metrics on `9100 + validator_id * 10`, over plain HTTP (no TLS). This
/// mirrors that scheme so the benchmark scrapes the correct port.
fn derive_metrics_endpoint(rpc_endpoint: &str) -> String {
    const RPC_BASE: u16 = 9005;
    const METRICS_BASE: u16 = 9100;

    let port = rpc_endpoint
        .rsplit(':')
        .next()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(RPC_BASE);
    let metrics_port = if port >= RPC_BASE {
        METRICS_BASE + (port - RPC_BASE) * 10
    } else {
        METRICS_BASE
    };

    // Extract the host part (strip scheme and port).
    let host = rpc_endpoint
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .split(':')
        .next()
        .unwrap_or("127.0.0.1");
    format!("http://{host}:{metrics_port}")
}

/// Current unix seconds.
fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Suppress unused import warnings in the binary.
#[allow(dead_code)]
fn _silence(_: &PathBuf) {}
