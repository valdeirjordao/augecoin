use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct ApiKeyStore {
    keys: HashMap<String, KeyInfo>,
}

#[derive(Debug, Clone)]
struct KeyInfo {
    is_admin: bool,
}

impl KeyInfo {
    fn new(is_admin: bool) -> Self {
        KeyInfo { is_admin }
    }
}

impl ApiKeyStore {
    pub fn new(admin_keys: Vec<String>) -> Self {
        let mut keys = HashMap::new();
        for key in admin_keys {
            keys.insert(key, KeyInfo::new(true));
        }
        ApiKeyStore { keys }
    }

    pub fn empty() -> Self {
        ApiKeyStore {
            keys: HashMap::new(),
        }
    }

    pub fn is_valid(&self, key: &str) -> bool {
        self.keys.contains_key(key)
    }

    pub fn is_admin(&self, key: &str) -> bool {
        self.keys.get(key).is_some_and(|info| info.is_admin)
    }
}

#[derive(Debug)]
struct ClientBucket {
    tokens: u32,
    last_refill: Instant,
}

impl ClientBucket {
    fn new(max_tokens: u32) -> Self {
        ClientBucket {
            tokens: max_tokens,
            last_refill: Instant::now(),
        }
    }

    fn try_consume(&mut self, max_tokens: u32, refill_rate: Duration) -> bool {
        let now = Instant::now();
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
    clients: Mutex<HashMap<String, ClientBucket>>,
    max_tokens: u32,
    refill_rate: Duration,
}

impl RateLimiter {
    pub fn new(max_requests: u32, per_interval: Duration) -> Self {
        RateLimiter {
            clients: Mutex::new(HashMap::new()),
            max_tokens: max_requests,
            refill_rate: per_interval.div_f64(max_requests as f64),
        }
    }

    pub fn check(&self, client_id: &str) -> bool {
        let mut clients = self.clients.lock().unwrap();
        let bucket = clients
            .entry(client_id.to_string())
            .or_insert_with(|| ClientBucket::new(self.max_tokens));
        bucket.try_consume(self.max_tokens, self.refill_rate)
    }
}

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
];

pub fn is_admin_method(method: &str) -> bool {
    ADMIN_METHODS.contains(&method)
}

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
        let store = ApiKeyStore::empty();
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
}
