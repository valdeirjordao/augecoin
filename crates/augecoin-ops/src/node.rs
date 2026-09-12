//! Minimal JSON-RPC client for the AUGECOIN node's administrative surface.
//!
//! The ops service is a *bridge*, never a consensus actor. When an admin
//! approves/suspends/revokes a validator, this client asks the node to sign the
//! corresponding `ValidatorAdminOp` with the node's own admin key and submit it
//! through consensus. The ops service never holds the network master key and
//! never mutates consensus directly.

use serde::Deserialize;
use serde_json::{json, Value};

use crate::error::{AppError, Result};

/// JSON-RPC 2.0 request id counter (per-client, monotonic).
#[derive(Debug, Clone)]
pub struct NodeClient {
    http: reqwest::Client,
    rpc_url: String,
    admin_key: String,
}

/// Result shape returned by the node's admin endpoints (`validatoradd`, etc.).
#[derive(Debug, Clone, Deserialize)]
pub struct AdminResult {
    pub success: bool,
    #[serde(default)]
    pub error: Option<String>,
}

/// A validator entry as reported by `getvalidatorset`.
#[derive(Debug, Clone, Deserialize)]
pub struct ChainValidator {
    pub id: u64,
    #[serde(alias = "ed25519_public_key")]
    pub ed25519_public_key_hex: String,
}

/// A block header as reported by `getblock` (subset used for reward sync).
#[derive(Debug, Clone, Deserialize)]
pub struct BlockInfo {
    pub block_number: u64,
    pub reward: u64,
    pub fee: u64,
    pub timestamp: u64,
    pub leader_id: u64,
}

impl NodeClient {
    pub fn new(rpc_url: String, admin_key: String) -> Self {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            // The node serves JSON-RPC over TLS with a self-signed certificate
            // (see augecoin-rpc `load_tls_pem` fallback). Accept it for the
            // loopback bridge; the admin key still authenticates the caller.
            .danger_accept_invalid_certs(true)
            .danger_accept_invalid_hostnames(true)
            .build()
            .expect("reqwest client");
        Self {
            http,
            rpc_url,
            admin_key,
        }
    }

    /// The configured node RPC endpoint (surfaced to activations).
    pub fn rpc_url(&self) -> &str {
        &self.rpc_url
    }

    async fn call(&self, method: &str, params: Value) -> Result<Value> {
        let res = self
            .http
            .post(&self.rpc_url)
            .header("x-api-key", &self.admin_key)
            .header("content-type", "application/json")
            .json(&json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": method,
                "params": params,
            }))
            .send()
            .await
            .map_err(|e| AppError::NodeUnreachable(e.to_string()))?;

        let body: Value = res
            .json()
            .await
            .map_err(|e| AppError::NodeUnreachable(format!("invalid node response: {e}")))?;

        if let Some(err) = body.get("error") {
            let msg = err
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("node RPC error")
                .to_string();
            return Err(AppError::NodeError(msg));
        }
        body.get("result")
            .cloned()
            .ok_or_else(|| AppError::NodeError("node response missing 'result'".into()))
    }

    async fn admin(&self, method: &str, params: Value) -> Result<AdminResult> {
        let result = self.call(method, params).await?;
        serde_json::from_value(result)
            .map_err(|e| AppError::NodeError(format!("unexpected node result for {method}: {e}")))
    }

    pub async fn validator_add(
        &self,
        ed25519_public_key_hex: &str,
        activation_height: u64,
    ) -> Result<AdminResult> {
        self.admin(
            "validatoradd",
            json!({ "ed25519_public_key_hex": ed25519_public_key_hex, "activation_height": activation_height }),
        )
        .await
    }

    pub async fn validator_remove(
        &self,
        validator_id: u64,
        activation_height: u64,
    ) -> Result<AdminResult> {
        self.admin(
            "validatorremove",
            json!({ "validator_id": validator_id, "activation_height": activation_height }),
        )
        .await
    }

    pub async fn validator_activate(
        &self,
        validator_id: u64,
        activation_height: u64,
    ) -> Result<AdminResult> {
        self.admin(
            "validatoractivate",
            json!({ "validator_id": validator_id, "activation_height": activation_height }),
        )
        .await
    }

    pub async fn validator_deactivate(
        &self,
        validator_id: u64,
        activation_height: u64,
    ) -> Result<AdminResult> {
        self.admin(
            "validatordeactivate",
            json!({ "validator_id": validator_id, "activation_height": activation_height }),
        )
        .await
    }

    /// Current block height, used as a sensible `activation_height` default.
    pub async fn block_count(&self) -> Result<u64> {
        let result = self.call("getblockcount", json!({})).await?;
        result
            .as_u64()
            .or_else(|| result.as_str().and_then(|s| s.parse().ok()))
            .ok_or_else(|| AppError::NodeError("node returned a non-numeric block count".into()))
    }

    /// Fetch a block header for reward attribution.
    pub async fn get_block(&self, block_number: u64) -> Result<BlockInfo> {
        let result = self
            .call("getblock", json!({ "block_number": block_number }))
            .await?;
        serde_json::from_value(result)
            .map_err(|e| AppError::NodeError(format!("unexpected block header: {e}")))
    }

    /// Resolve the on-chain validator id for a public key, if the node has it.
    pub async fn resolve_validator_id(&self, ed25519_public_key_hex: &str) -> Result<Option<u64>> {
        let result = self.call("getvalidatorset", json!({})).await?;
        let set = result.get("active").cloned().unwrap_or(Value::Null);
        let validators: Vec<ChainValidator> = serde_json::from_value(set)
            .map_err(|e| AppError::NodeError(format!("unexpected validatorset: {e}")))?;
        Ok(validators
            .into_iter()
            .find(|v| {
                v.ed25519_public_key_hex
                    .eq_ignore_ascii_case(ed25519_public_key_hex)
            })
            .map(|v| v.id))
    }
}
