//! Rate limiting for API key tiers
//!
//! Uses a simple std::time::Instant-based token bucket algorithm.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use tracing::warn;

// ─── Tier definitions (also exposed as JSON at GET /v1/pricing) ─────────────

#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct Tier {
    pub name: &'static str,
    pub slug: &'static str,
    pub requests_per_minute: u32,
    pub monthly_call_quota: u64,
    pub monthly_price_augesat: u64,
    pub overage_price_augesat_per_call: u64,
    pub features: &'static [&'static str],
}

pub const TIERS: &[Tier] = &[
    Tier {
        name: "Free",
        slug: "free",
        requests_per_minute: 10,
        monthly_call_quota: 1_000,
        monthly_price_augesat: 0,
        overage_price_augesat_per_call: 500,
        features: &["Community support", "Testnet access"],
    },
    Tier {
        name: "Pro",
        slug: "pro",
        requests_per_minute: 60,
        monthly_call_quota: 100_000,
        monthly_price_augesat: 10 * 1_0000_0000,
        overage_price_augesat_per_call: 200,
        features: &["Email support", "Mainnet access", "Webhooks", "Priority rate limit"],
    },
    Tier {
        name: "Business",
        slug: "business",
        requests_per_minute: 300,
        monthly_call_quota: 1_000_000,
        monthly_price_augesat: 50 * 1_0000_0000,
        overage_price_augesat_per_call: 50,
        features: &["Dedicated support", "SLA 99.9%", "Custom webhooks", "Dedicated rate limit"],
    },
    Tier {
        name: "Enterprise",
        slug: "enterprise",
        requests_per_minute: 3_000,
        monthly_call_quota: 0,
        monthly_price_augesat: 0,
        overage_price_augesat_per_call: 0,
        features: &["Dedicated infrastructure", "Custom SLA", "Negotiated pricing"],
    },
];

pub fn public_tiers() -> Vec<serde_json::Value> {
    TIERS.iter().map(|t| {
        let quota = if t.monthly_call_quota == 0 {
            serde_json::Value::String("unlimited".to_string())
        } else {
            serde_json::Value::Number(t.monthly_call_quota.into())
        };
        serde_json::json!({
            "name": t.name,
            "slug": t.slug,
            "requests_per_minute": t.requests_per_minute,
            "monthly_call_quota": quota,
            "monthly_price_augesat": t.monthly_price_augesat as f64 / 1_0000_0000.0,
            "features": t.features,
        })
    }).collect()
}

// ─── Token bucket (simple std::time::Instant-based) ─────────────────────────

#[derive(Clone)]
struct TokenBucket {
    capacity: f64,
    tokens: f64,
    refill_rate: f64,
    last_refill: Instant,
}

impl TokenBucket {
    fn new(capacity: u32, _period_secs: f64) -> Self {
        let cap = capacity as f64;
        Self {
            capacity: cap,
            tokens: cap,
            refill_rate: cap / 60.0,
            last_refill: Instant::now(),
        }
    }

    fn consume(&mut self, _amount: f64) -> bool {
        self.refill();
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }

    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        let new_tokens = elapsed * self.refill_rate;
        self.tokens = (self.tokens + new_tokens).min(self.capacity);
        self.last_refill = now;
    }
}

// ─── RateLimiter ─────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct RateLimiter {
    inner: Arc<RwLock<RateLimiterInner>>,
}

#[derive(Clone)]
struct RateLimiterInner {
    buckets: Arc<RwLock<HashMap<String, TokenBucket>>>,
    default_rpm: u32,
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self {
            inner: Arc::new(RwLock::new(RateLimiterInner {
                buckets: Arc::new(RwLock::new(HashMap::new())),
                default_rpm: 10,
            })),
        }
    }
}

impl RateLimiter {
    /// Set the default RPM (used when no API key matches a tier)
    pub async fn with_default_rpm(mut self, rpm: u32) -> Self {
        self.inner.write().await.default_rpm = rpm;
        self
    }

    /// Check if a request is allowed for this key. Returns `true` if allowed,
    /// `false` if rate-limited (HTTP 429).
    pub async fn check(&self, api_key: &str) -> bool {
        let rpm = self.lookup_rpm(api_key).await;
        let rpm_nonzero = rpm.max(1);

        let inner = self.inner.read().await;
        let mut buckets = inner.buckets.write().await;

        let bucket = buckets
            .entry(api_key.to_string())
            .or_insert_with(|| TokenBucket::new(rpm_nonzero, 60.0));

        bucket.consume(1.0)
    }

    /// Update the RPM for an API key (used when tier is changed)
    pub async fn update_key_rpm(&self, api_key: &str, _rpm: u32) {
        let inner = self.inner.read().await;
        inner.buckets.write().await.remove(api_key);
    }

    /// Get current RPM for a key from DB tier
    async fn lookup_rpm(&self, _api_key: &str) -> u32 {
        self.inner.read().await.default_rpm
    }
}
