//! Latency / outcome metrics for the AUGECOIN stress harness (Fase 18).
//!
//! Records per-operation latency (microseconds) in an HdrHistogram and tracks
//! accepted/rejected counts plus a rejection-by-reason breakdown, so the
//! harness can report throughput, p50/p95/p99 latency and rejection causes.

use std::collections::HashMap;

use hdrhistogram::Histogram;

pub struct Metrics {
    latencies: Histogram<u64>,
    accepted: u64,
    rejected: u64,
    by_reason: HashMap<String, u64>,
}

impl Metrics {
    pub fn new() -> Self {
        // Highest trackable value: 60s in microseconds (fits our scenarios).
        let latencies = Histogram::<u64>::new(3).expect("valid histogram bounds");
        Metrics {
            latencies,
            accepted: 0,
            rejected: 0,
            by_reason: HashMap::new(),
        }
    }

    pub fn record(&mut self, latency_us: u64, accepted: bool, reason: Option<String>) {
        let _ = self.latencies.record(latency_us);
        if accepted {
            self.accepted += 1;
        } else {
            self.rejected += 1;
            let key = classify(reason.unwrap_or_else(|| "unknown".into()));
            *self.by_reason.entry(key).or_insert(0) += 1;
        }
    }

    pub fn total(&self) -> u64 {
        self.accepted + self.rejected
    }

    pub fn accepted(&self) -> u64 {
        self.accepted
    }

    pub fn rejected(&self) -> u64 {
        self.rejected
    }

    /// Latency at the given quantile (0.0..=1.0) in microseconds.
    pub fn percentile_us(&self, q: f64) -> u64 {
        self.latencies.value_at_quantile(q)
    }

    pub fn by_reason(&self) -> &HashMap<String, u64> {
        &self.by_reason
    }
}

/// Buckets a raw node error string into a stable reason category.
pub fn classify(error: String) -> String {
    let e = error.to_lowercase();
    if e.contains("gas") {
        "gas".into()
    } else if e.contains("n_operation") {
        "n_operation".into()
    } else if e.contains("balance") {
        "balance".into()
    } else if e.contains("rate") {
        "rate_limit".into()
    } else if e.contains("admin") || e.contains("api key") || e.contains("auth") {
        "auth".into()
    } else {
        "other".into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentiles_match_fed_distribution() {
        let mut m = Metrics::new();
        // 100 samples: 100us, 200us, ..., 10000us
        for i in 1u64..=100 {
            m.record(i * 100, true, None);
        }
        assert_eq!(m.total(), 100);
        assert_eq!(m.accepted(), 100);
        assert_eq!(m.rejected(), 0);

        // p50 ~= 5000us, p95 ~= 9500us, p99 ~= 9900us (HdrHistogram is approximate).
        assert!(m.percentile_us(0.5) >= 4800 && m.percentile_us(0.5) <= 5200);
        assert!(m.percentile_us(0.95) >= 9200);
        assert!(m.percentile_us(0.99) >= 9700);
    }

    #[test]
    fn counts_accepted_and_rejected() {
        let mut m = Metrics::new();
        m.record(100, true, None);
        m.record(100, false, Some("insufficient balance".into()));
        m.record(100, false, Some("gas limit exceeded".into()));
        m.record(100, false, Some("n_operation mismatch".into()));
        assert_eq!(m.accepted(), 1);
        assert_eq!(m.rejected(), 3);
    }

    #[test]
    fn classifies_rejection_reasons() {
        let mut m = Metrics::new();
        m.record(10, false, Some("insufficient balance".into()));
        m.record(10, false, Some("gas limit exceeded".into()));
        m.record(10, false, Some("n_operation mismatch".into()));
        m.record(10, false, Some("rate limited".into()));
        m.record(10, false, Some("admin API key required".into()));
        m.record(10, false, Some("weird failure".into()));

        assert_eq!(m.by_reason().get("balance"), Some(&1));
        assert_eq!(m.by_reason().get("gas"), Some(&1));
        assert_eq!(m.by_reason().get("n_operation"), Some(&1));
        assert_eq!(m.by_reason().get("rate_limit"), Some(&1));
        assert_eq!(m.by_reason().get("auth"), Some(&1));
        assert_eq!(m.by_reason().get("other"), Some(&1));
    }
}
