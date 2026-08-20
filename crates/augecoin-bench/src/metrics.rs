//! System and node metrics collection.
//!
//! Two sources are merged:
//! - **Prometheus** endpoints already exposed by each validator
//!   (`augecoin_block_height`, `augecoin_block_time_seconds`,
//!   `augecoin_mempool_size`, `augecoin_rocksdb_log_size`,
//!   `augecoin_safebox_bytes`, `augecoin_snapshot_duration_seconds`, ...).
//! - **Local `/proc`** parsing for the bench process itself (CPU %, RSS RAM)
//!   when running on the same host as the validator.

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::time::Duration;

/// Parsed Prometheus text exposition into metric name → (type, samples).
pub fn parse_prometheus(body: &str) -> HashMap<String, Vec<(String, f64)>> {
    let mut out: HashMap<String, Vec<(String, f64)>> = HashMap::new();
    let mut current: Option<String> = None;
    for line in body.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            if let Some(name) = line.strip_prefix("# HELP ") {
                current = name.split_whitespace().next().map(|s| s.to_string());
            }
            continue;
        }
        // sample: metric_name{labels} value
        let Some(name) = current
            .clone()
            .or_else(|| line.split(['{', ' ']).next().map(|s| s.to_string()))
        else {
            continue;
        };
        // Extract value: last whitespace-separated token.
        let value = line
            .split_whitespace()
            .last()
            .and_then(|v| v.parse::<f64>().ok());
        if let Some(v) = value {
            out.entry(name).or_default().push((String::new(), v));
        }
        current = None;
    }
    out
}

/// Look up the latest value of a Prometheus gauge by name.
pub fn gauge_value(parsed: &HashMap<String, Vec<(String, f64)>>, name: &str) -> Option<f64> {
    parsed
        .get(name)
        .and_then(|samples| samples.last())
        .map(|(_, v)| *v)
}

/// Process CPU and memory usage read from `/proc`.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct ProcStats {
    pub cpu_percent: f64,
    pub rss_mb: f64,
}

/// Read this process's CPU% and RSS from `/proc/self/stat` (Linux).
pub fn read_proc_stats() -> Result<ProcStats> {
    let stat = std::fs::read_to_string("/proc/self/stat").context("read /proc/self/stat")?;
    // Fields are space-separated after the comm field which may contain spaces
    // (enclosed in parens). Find the last ')' then split the rest.
    let close = stat.rfind(')').context("malformed stat")?;
    let rest = &stat[close + 1..];
    let fields: Vec<&str> = rest.split_whitespace().collect();
    // After comm: utime(14) stime(15) ... starttime(22) vsize(23) rss(24)
    // (1-indexed within the full stat; here `fields[0]` is field 3).
    let utime: u64 = fields.get(11).and_then(|s| s.parse().ok()).unwrap_or(0);
    let stime: u64 = fields.get(12).and_then(|s| s.parse().ok()).unwrap_or(0);
    let starttime: u64 = fields.get(19).and_then(|s| s.parse().ok()).unwrap_or(0);
    let rss_pages: u64 = fields.get(21).and_then(|s| s.parse().ok()).unwrap_or(0);

    let page_size = 4096u64;
    let rss_mb = rss_pages as f64 * page_size as f64 / (1024.0 * 1024.0);

    // CPU percent since process start: (utime+stime)/clock_ticks / uptime.
    let hz = 100u64; // typical USER_HZ
    let total_ticks = utime + stime;
    let uptime = std::fs::read_to_string("/proc/uptime")
        .ok()
        .and_then(|s| s.split_whitespace().next().map(|v| v.parse::<f64>().ok()))
        .flatten()
        .unwrap_or(0.0);
    let run_secs = if starttime > 0 {
        let ticks = total_ticks as f64 / hz as f64;
        // approximate: total_ticks relative, not wall; use ticks as proxy
        ticks
    } else {
        0.0
    };
    let cpu_percent = if uptime > 0.0 {
        (total_ticks as f64 / hz as f64) / uptime * 100.0
    } else {
        0.0
    };
    let _ = run_secs;

    Ok(ProcStats {
        cpu_percent,
        rss_mb,
    })
}

/// A snapshot of node + system metrics at a point in time.
#[derive(Debug, Clone, serde::Serialize)]
pub struct MetricsSnapshot {
    pub timestamp_unix: u64,
    pub block_height: u64,
    pub block_time_seconds: f64,
    pub mempool_size: u64,
    pub transactions_total: u64,
    pub blocks_total: u64,
    pub rocksdb_log_size: u64,
    pub safebox_bytes: u64,
    pub snapshot_duration_seconds: f64,
    pub cpu_percent: f64,
    pub rss_mb: f64,
    pub peers_connected: u64,
}

impl MetricsSnapshot {
    /// Build a snapshot from scraped Prometheus bodies and optional /proc stats.
    pub fn from_bodies(
        bodies: &[(String, String)],
        proc: Option<&ProcStats>,
        now_unix: u64,
    ) -> Self {
        // Aggregate gauge values across validators (sum for gauges is wrong,
        // but we report the max/mean as a simple aggregate; here we use mean).
        let mut acc = MetricsAggregator::default();
        for (_ep, body) in bodies {
            let parsed = parse_prometheus(body);
            acc.push(parsed);
        }
        let (cpu, rss) = proc
            .map(|p| (p.cpu_percent, p.rss_mb))
            .unwrap_or((0.0, 0.0));
        MetricsSnapshot {
            timestamp_unix: now_unix,
            block_height: acc.mean("augecoin_block_height") as u64,
            block_time_seconds: acc.mean("augecoin_block_time_seconds"),
            mempool_size: acc.mean("augecoin_mempool_size") as u64,
            transactions_total: acc.sum("augecoin_transactions_total") as u64,
            blocks_total: acc.sum("augecoin_blocks_total") as u64,
            rocksdb_log_size: acc.mean("augecoin_rocksdb_log_size") as u64,
            safebox_bytes: acc.mean("augecoin_safebox_bytes") as u64,
            snapshot_duration_seconds: acc.mean("augecoin_snapshot_duration_seconds"),
            cpu_percent: cpu,
            rss_mb: rss,
            peers_connected: acc.mean("augecoin_peers_connected") as u64,
        }
    }
}

#[derive(Default)]
struct MetricsAggregator {
    sums: HashMap<String, f64>,
    counts: HashMap<String, usize>,
}

impl MetricsAggregator {
    fn push(&mut self, parsed: HashMap<String, Vec<(String, f64)>>) {
        for (name, samples) in parsed {
            if let Some((_, v)) = samples.last() {
                *self.sums.entry(name.clone()).or_insert(0.0) += v;
                *self.counts.entry(name).or_insert(0) += 1;
            }
        }
    }

    fn mean(&self, name: &str) -> f64 {
        let c = self.counts.get(name).copied().unwrap_or(0) as f64;
        if c == 0.0 {
            0.0
        } else {
            self.sums.get(name).copied().unwrap_or(0.0) / c
        }
    }

    fn sum(&self, name: &str) -> f64 {
        self.sums.get(name).copied().unwrap_or(0.0)
    }
}

/// Measure the delta of a counter between two snapshots over a duration.
pub fn rate_per_sec(start: f64, end: f64, secs: f64) -> f64 {
    if secs <= 0.0 {
        0.0
    } else {
        (end - start).max(0.0) / secs
    }
}

/// Convenience: sleep helper.
pub async fn sleep(secs: u64) {
    tokio::time::sleep(Duration::from_secs(secs)).await;
}
