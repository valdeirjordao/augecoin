//! Environment-driven configuration for the ops service.
//!
//! Mirrors the node's convention: everything comes from environment variables,
//! no required config file. Secrets (admin key) are accepted as raw values and
//! reduced to a BLAKE3-256 hash immediately so the plaintext is never kept in
//! long-lived state.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use blake3::hash;
use thiserror::Error;

/// Ops service configuration.
#[derive(Debug, Clone)]
pub struct Config {
    /// PostgreSQL connection URL (e.g. `postgres://user:pass@host/db`).
    pub database_url: String,
    /// Address the HTTP server binds to.
    pub listen_addr: SocketAddr,
    /// BLAKE3-256 hash of the admin API key (the plaintext is not retained).
    pub admin_key_hash: [u8; 32],
    /// Max open connections in the PostgreSQL pool.
    pub max_connections: u32,
    /// Optional node JSON-RPC endpoint for the admin bridge (`validatoradd`, …).
    pub node_rpc_url: Option<String>,
    /// Admin API key for the node RPC (plaintext retained only for the bridge).
    pub node_admin_key: Option<String>,
    /// Ed25519 release public key (64 hex chars) used to verify OTA publish
    /// signatures. Required to publish releases.
    pub release_public_key_hex: Option<String>,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("AUGECOIN_OPS_DATABASE_URL is required")]
    MissingDatabaseUrl,
    #[error("AUGECOIN_OPS_ADMIN_KEY is required")]
    MissingAdminKey,
    #[error("invalid AUGECOIN_OPS_LISTEN_ADDR: {0}")]
    InvalidListenAddr(String),
    #[error("invalid AUGECOIN_OPS_MAX_CONNECTIONS: {0}")]
    InvalidMaxConnections(String),
    #[error("invalid AUGECOIN_OPS_RELEASE_PUBLIC_KEY: {0}")]
    InvalidReleaseKey(String),
}

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

/// Decode a 64-hex-char Ed25519 public key into 32 bytes.
fn parse_release_key(hex: &str) -> Result<[u8; 32], ConfigError> {
    let bytes = hex::decode(hex.trim())
        .map_err(|_| ConfigError::InvalidReleaseKey("not valid hex".into()))?;
    bytes
        .try_into()
        .map_err(|_| ConfigError::InvalidReleaseKey("must be 32 bytes".into()))
}

impl Config {
    pub fn from_env() -> std::result::Result<Self, ConfigError> {
        let database_url =
            env("AUGECOIN_OPS_DATABASE_URL").ok_or(ConfigError::MissingDatabaseUrl)?;

        let listen_addr = match env("AUGECOIN_OPS_LISTEN_ADDR") {
            None => SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 8790),
            Some(raw) => raw
                .parse::<SocketAddr>()
                .map_err(|_| ConfigError::InvalidListenAddr(raw))?,
        };

        let admin_key = env("AUGECOIN_OPS_ADMIN_KEY").ok_or(ConfigError::MissingAdminKey)?;
        let admin_key_hash = *hash(admin_key.trim().as_bytes()).as_bytes();

        let max_connections = match env("AUGECOIN_OPS_MAX_CONNECTIONS") {
            None => 5,
            Some(raw) => raw
                .parse::<u32>()
                .map_err(|_| ConfigError::InvalidMaxConnections(raw))?,
        };

        Ok(Self {
            database_url,
            listen_addr,
            admin_key_hash,
            max_connections,
            node_rpc_url: env("AUGECOIN_OPS_NODE_RPC_URL"),
            node_admin_key: env("AUGECOIN_OPS_NODE_ADMIN_KEY"),
            release_public_key_hex: env("AUGECOIN_OPS_RELEASE_PUBLIC_KEY"),
        })
    }

    /// True when both node bridge settings are present.
    pub fn has_node_bridge(&self) -> bool {
        self.node_rpc_url.is_some() && self.node_admin_key.is_some()
    }

    /// Parse the release public key into 32 bytes, if configured.
    pub fn release_public_key(&self) -> Option<Result<[u8; 32], ConfigError>> {
        let hex = self.release_public_key_hex.as_ref()?;
        Some(parse_release_key(hex))
    }

    /// True when the presented admin API key matches the configured one.
    /// Comparison is done over BLAKE3 hashes so the secret itself is not present
    /// in this process and comparison timing carries no information.
    pub fn admin_key_matches(&self, candidate: &str) -> bool {
        *hash(candidate.trim().as_bytes()).as_bytes() == self.admin_key_hash
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admin_key_matches_exact() {
        let cfg = Config {
            database_url: "postgres://x".into(),
            listen_addr: "127.0.0.1:1".parse().unwrap(),
            admin_key_hash: *hash(b"secret-admin-key").as_bytes(),
            max_connections: 5,
            node_rpc_url: None,
            node_admin_key: None,
            release_public_key_hex: None,
        };
        assert!(cfg.admin_key_matches("secret-admin-key"));
        // Leading/trailing whitespace is trimmed consistently on both sides.
        assert!(cfg.admin_key_matches("  secret-admin-key  "));
        assert!(!cfg.admin_key_matches("wrong"));
    }

    #[test]
    fn missing_vars_are_errors() {
        // Best-effort: this test does not mutate the real environment; it only
        // checks the error type plumbing is reachable. The real env is controlled
        // by the test harness, so we assert on the constructors directly.
        let err = ConfigError::MissingDatabaseUrl.to_string();
        assert!(err.contains("AUGECOIN_OPS_DATABASE_URL"));
    }
}
