//! Integration tests for the bench harness (stats, ops, exporters).

use augecoin_bench::stats::{LatencySummary, Throughput};
use augecoin_bench::LatencySample;
use std::time::Duration;

fn sample(ms: u64) -> LatencySample {
    LatencySample {
        submitted_at_ms: 1000,
        confirmed_at_ms: 1000 + ms,
        op_index: 0,
        mode: "test",
    }
}

#[test]
fn latency_percentiles_are_computed() {
    let samples: Vec<LatencySample> = (1..=100).map(|i| sample(i * 10)).collect();
    let s = LatencySummary::from_samples(&samples);
    assert_eq!(s.count, 100);
    assert_eq!(s.p50_ms, 510);
    assert_eq!(s.p95_ms, 960);
    assert_eq!(s.p99_ms, 1000);
    assert_eq!(s.min_ms, 10);
    assert_eq!(s.max_ms, 1000);
}

#[test]
fn empty_latency_is_default() {
    let s = LatencySummary::from_samples(&[]);
    assert_eq!(s.count, 0);
    assert_eq!(s.p50_ms, 0);
}

#[test]
fn throughput_computes_tps() {
    let tp = Throughput::compute(100, 50, Duration::from_secs(10));
    assert!((tp.submitted_tps - 10.0).abs() < 0.001);
    assert!((tp.confirmed_tps - 5.0).abs() < 0.001);
    assert_eq!(tp.submitted, 100);
    assert_eq!(tp.confirmed, 50);
}

#[test]
fn ops_serialize_and_sign() {
    use augecoin_bench::ops;
    use augecoin_crypto::signature::HybridKeyPair;

    let key = HybridKeyPair::from_seed([7u8; 32]);
    let op = ops::make_transfer(&key, 1, 0, 2, 100);
    assert_eq!(op.signatures.len(), 1);

    let hex = ops::op_to_hex(&op);
    assert!(!hex.is_empty());

    // Round-trip the serialized bytes.
    let bytes = hex::decode(&hex).unwrap();
    let decoded = augecoin_core::operation::Operation::from_bytes(&bytes).unwrap();
    assert_eq!(decoded.signatures.len(), 1);
    assert_eq!(
        decoded.op_type(),
        augecoin_core::operation::OperationType::Transaction
    );
}

#[test]
fn prometheus_parser_reads_gauges() {
    let body = concat!(
        "# HELP augecoin_block_height Current block height\n",
        "# TYPE augecoin_block_height gauge\n",
        "augecoin_block_height 42\n",
        "augecoin_mempool_size 7\n",
    );
    let parsed = augecoin_bench::metrics::parse_prometheus(body);
    assert_eq!(
        augecoin_bench::metrics::gauge_value(&parsed, "augecoin_block_height"),
        Some(42.0)
    );
    assert_eq!(
        augecoin_bench::metrics::gauge_value(&parsed, "augecoin_mempool_size"),
        Some(7.0)
    );
}

#[test]
fn html_exporter_renders_chart() {
    let mut result = augecoin_bench::report::BenchmarkResult::new(
        augecoin_bench::modes::BenchMode::Transfer,
        vec!["https://127.0.0.1:9005".into()],
        1000,
        1,
        256,
        0.0,
    );
    result.bursts.push(augecoin_bench::modes::BurstReport {
        mode: "transfer".into(),
        requested: 1000,
        submitted: 1000,
        confirmed: 1000,
        submitted_tps: 100.0,
        confirmed_tps: 80.0,
        elapsed_secs: 10.0,
        p50_ms: 100,
        p95_ms: 200,
        p99_ms: 300,
    });
    result.finalize(&[], 0, Duration::from_secs(10));

    let dir = tempfile::tempdir().unwrap();
    augecoin_bench::exporters::export_all(&result, dir.path()).unwrap();

    let html = std::fs::read_to_string(dir.path().join("benchmark.html")).unwrap();
    assert!(html.contains("AUGECOIN Benchmark"));
    assert!(html.contains("<svg"));
    let csv = std::fs::read_to_string(dir.path().join("benchmark.csv")).unwrap();
    assert!(csv.contains("mode"));
    let json = std::fs::read_to_string(dir.path().join("benchmark.json")).unwrap();
    assert!(json.contains("\"mode\""));
}
