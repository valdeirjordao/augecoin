use serde::Deserialize;
use std::time::{Duration, Instant};
use tracing::{info, warn};

// ─── In-memory cache (L1) ──────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct KeyInfo {
    pub is_admin: bool,
}

#[derive(Debug)]
pub struct ApiKeyStore {
    /// Bootstrap admin keys from env — always valid, never evicted.
    admin_keys: std::collections::HashSet<String>,
    /// L1 cache: key_hash → KeyInfo. Populated from DB at startup,
    /// refreshed periodically in the background.
    cache: dashmap::DashMap<String, KeyInfo>,
    /// Database pool for background refresh (optional — None = memory-only mode).
    pool: Option<sqlx::PgPool>,
    /// How often the background task refreshes the cache from DB.
    #[allow(dead_code)]
    refresh_interval: Duration,
}

impl ApiKeyStore {
    /// Create a store backed by PostgreSQL. Keys are loaded from DB at startup
    /// and refreshed every `refresh_interval`. Bootstrap admin keys are always valid.
    pub async fn new_with_pool(
        pool: sqlx::PgPool,
        bootstrap_admin_keys: Vec<String>,
        refresh_interval: Duration,
    ) -> Self {
        let store = ApiKeyStore {
            admin_keys: bootstrap_admin_keys.iter().cloned().collect(),
            cache: dashmap::DashMap::new(),
            pool: Some(pool),
            refresh_interval,
        };

        // Initial load from DB
        store.refresh_from_db().await;

        // Spawn background refresh
        let cache = store.cache.clone();
        let pool_clone = store.pool.as_ref().unwrap().clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(refresh_interval);
            loop {
                interval.tick().await;
                match refresh_cache(&pool_clone, &cache).await {
                    Ok(count) => info!("rpc auth: refreshed {} API keys from DB", count),
                    Err(e) => warn!("rpc auth: DB refresh failed: {}", e),
                }
            }
        });

        store
    }

    /// Create a memory-only store (legacy behavior). Useful for tests
    /// or environments without PostgreSQL.
    pub fn new(admin_keys: Vec<String>) -> Self {
        let admin_set: std::collections::HashSet<String> = admin_keys.iter().cloned().collect();
        let cache = dashmap::DashMap::new();
        for key in &admin_set {
            cache.insert(key.clone(), KeyInfo { is_admin: true });
        }
        ApiKeyStore {
            admin_keys: admin_set,
            cache,
            pool: None,
            refresh_interval: Duration::from_secs(60),
        }
    }

    /// Check if a key is valid (present and not revoked).
    /// This is synchronous — reads from the L1 cache.
    pub fn is_valid(&self, key: &str) -> bool {
        if self.admin_keys.contains(key) {
            return true;
        }
        self.cache.contains_key(key)
    }

    /// Check if a key has admin privileges.
    pub fn is_admin(&self, key: &str) -> bool {
        if self.admin_keys.contains(key) {
            return true;
        }
        self.cache.get(key).map(|v| v.is_admin).unwrap_or(false)
    }

    /// Force a cache refresh from the database.
    pub async fn refresh_from_db(&self) {
        if let Some(pool) = &self.pool {
            let _ = refresh_cache(pool, &self.cache).await;
        }
    }
}

/// Load all active API keys from PostgreSQL into the DashMap cache.
/// Revoked keys are evicted. New keys are added.
async fn refresh_cache(
    pool: &sqlx::PgPool,
    cache: &dashmap::DashMap<String, KeyInfo>,
) -> Result<usize, sqlx::Error> {
    let rows = sqlx::query_as::<_, (String, bool)>(
        "SELECT key_hash, is_admin FROM rpc_api_keys WHERE revoked = false",
    )
    .fetch_all(pool)
    .await?;

    // Evict all existing entries and repopulate from DB snapshot.
    cache.clear();
    for (hash, is_admin) in rows.iter() {
        cache.insert(hash.clone(), KeyInfo { is_admin: *is_admin });
    }

    Ok(rows.len())
}

// ─── Rate limiter (DashMap — lock-free sharded map) ───────────────────

#[derive(Debug)]
struct ClientBucket {
    tokens: u32,
    last_refill: Instant,
    last_seen: Instant,
}

impl ClientBucket {
    fn new(max_tokens: u32) -> Self {
        let now = Instant::now();
        ClientBucket {
            tokens: max_tokens,
            last_refill: now,
            last_seen: now,
        }
    }

    fn try_consume(&mut self, max_tokens: u32, refill_rate: Duration) -> bool {
        let now = Instant::now();
        self.last_seen = now;
        let elapsed = now.duration_since(self.last_refill);

        let refill_count = (elapsed.as_nanos() / refill_rate.as_nanos()) as u32;
        if refill_count > 0 {
            self.tokens = (self.tokens + refill_count).min(max_tokens);
            self.last_refill = now;
        }

        if self.tokens > 0 {
            self.tokens -= 1;
            true
        } else {
            false
        }
    }
}

#[derive(Debug)]
pub struct RateLimiter {
    /// Lock-free sharded map. Each shard has its own Mutex, so concurrent
    /// requests to different clients never contend. Default shard count
    /// is 64 (good for up to ~50k concurrent clients).
    clients: dashmap::DashMap<String, ClientBucket>,
    max_tokens: u32,
    refill_rate: Duration,
}

const MAX_CLIENT_BUCKETS: usize = 10_000;
const CLIENT_BUCKET_IDLE_TTL: Duration = Duration::from_secs(15 * 60);

impl RateLimiter {
    pub fn new(max_requests: u32, per_interval: Duration) -> Self {
        let max_requests = max_requests.max(1);
        RateLimiter {
            clients: dashmap::DashMap::new(),
            max_tokens: max_requests,
            refill_rate: per_interval.div_f64(max_requests as f64),
        }
    }

    pub fn check(&self, client_id: &str) -> bool {
        let now = Instant::now();

        // Remove idle buckets. DashMap::retain runs per-shard, no global lock.
        self.clients
            .retain(|_, bucket| now.duration_since(bucket.last_seen) < CLIENT_BUCKET_IDLE_TTL);

        // Enforce max client count to prevent memory growth.
        if !self.clients.contains_key(client_id) && self.clients.len() >= MAX_CLIENT_BUCKETS {
            return false;
        }

        // entry().or_insert() is atomic per shard — no global lock contention.
        let mut entry = self
            .clients
            .entry(client_id.to_string())
            .or_insert_with(|| ClientBucket::new(self.max_tokens));

        entry.try_consume(self.max_tokens, self.refill_rate)
    }
}

// ─── Auth levels ──────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthLevel {
    Public,
    Admin,
}

#[derive(Debug, Deserialize)]
pub struct AuthHeaders {
    #[serde(rename = "x-api-key")]
    pub api_key: Option<String>,
}

pub fn check_auth(api_key: Option<&str>, store: &ApiKeyStore) -> AuthLevel {
    match api_key {
        Some(key) if store.is_admin(key) => AuthLevel::Admin,
        Some(key) if store.is_valid(key) => AuthLevel::Public,
        _ => AuthLevel::Public,
    }
}

pub const ADMIN_METHODS: &[&str] = &[
    "createaccount",
    "validatoradd",
    "validatorremove",
    "validatoractivate",
    "validatordeactivate",
    "contract_store_code",
    "contract_create",
    "contract_execute",
];

pub fn is_admin_method(method: &str) -> bool {
    ADMIN_METHODS.contains(&method)
}

// ─── Tests ───────────────────────────────────────────────��─────────────

#[cfg(test)]
mod auth_tests {
    use super::*;

    #[test]
    fn api_key_store_validates_known_keys() {
        let store = ApiKeyStore::new(vec!["secret123".into()]);
        assert!(store.is_valid("secret123"));
        assert!(store.is_admin("secret123"));
        assert!(!store.is_valid("wrong-key"));
    }

    #[test]
    fn empty_store_rejects_all() {
        let store = ApiKeyStore::new(Vec::new());
        assert!(!store.is_valid("any-key"));
        assert!(!store.is_admin("any-key"));
    }

    #[test]
    fn check_auth_returns_public_without_key() {
        let store = ApiKeyStore::new(vec!["admin-key".into()]);
        assert_eq!(check_auth(None, &store), AuthLevel::Public);
    }

    #[test]
    fn check_auth_returns_admin_with_valid_key() {
        let store = ApiKeyStore::new(vec!["admin-key".into()]);
        assert_eq!(check_auth(Some("admin-key"), &store), AuthLevel::Admin);
    }

    #[test]
    fn check_auth_returns_public_with_invalid_key() {
        let store = ApiKeyStore::new(vec!["admin-key".into()]);
        assert_eq!(check_auth(Some("wrong"), &store), AuthLevel::Public);
    }

    #[test]
    fn admin_methods_are_recognized() {
        assert!(is_admin_method("validatoradd"));
        assert!(is_admin_method("validatorremove"));
        assert!(!is_admin_method("getaccount"));
        assert!(!is_admin_method("ping"));
    }

    #[test]
    fn rate_limiter_allows_up_to_limit() {
        let limiter = RateLimiter::new(3, Duration::from_secs(60));
        let client = "127.0.0.1";
        assert!(limiter.check(client));
        assert!(limiter.check(client));
        assert!(limiter.check(client));
    }

    #[test]
    fn rate_limiter_blocks_after_limit() {
        let limiter = RateLimiter::new(2, Duration::from_secs(60));
        let client = "192.168.1.1";
        assert!(limiter.check(client));
        assert!(limiter.check(client));
        assert!(!limiter.check(client));
    }

    #[test]
    fn rate_limiter_separates_clients() {
        let limiter = RateLimiter::new(2, Duration::from_secs(60));
        assert!(limiter.check("client-a"));
        assert!(limiter.check("client-a"));
        assert!(!limiter.check("client-a"));

        assert!(limiter.check("client-b"));
        assert!(limiter.check("client-b"));
    }

    #[test]
    fn rate_limiter_removes_idle_buckets() {
        let limiter = RateLimiter::new(1, Duration::from_secs(60));
        assert!(limiter.check("idle-client"));
        {
            let mut bucket = limiter.clients.get_mut("idle-client").unwrap();
            bucket.last_seen = Instant::now() - CLIENT_BUCKET_IDLE_TTL - Duration::from_secs(1);
        }

        assert!(limiter.check("active-client"));
        assert!(!limiter.clients.contains_key("idle-client"));
        assert!(limiter.clients.contains_key("active-client"));
    }
}
