use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, thiserror::Error)]
pub enum RpcError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("JSON parsing failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("JSON-RPC error {code}: {message}")]
    Rpc { code: i32, message: String },
}

#[derive(Debug, Serialize)]
struct JsonRpcRequest {
    jsonrpc: String,
    method: String,
    params: Value,
    id: Value,
}

#[derive(Debug, Deserialize)]
struct JsonRpcResponse {
    #[allow(dead_code)]
    jsonrpc: String,
    result: Option<Value>,
    error: Option<JsonRpcErrorObj>,
    #[allow(dead_code)]
    id: Value,
}

#[derive(Debug, Deserialize)]
struct JsonRpcErrorObj {
    code: i32,
    message: String,
}

pub struct RpcClient {
    endpoint: String,
    client: reqwest::Client,
}

impl RpcClient {
    pub fn new(endpoint: String) -> Result<Self, RpcError> {
        let client = reqwest::Client::builder()
            .danger_accept_invalid_certs(false)
            .build()?;
        Ok(RpcClient { endpoint, client })
    }

    #[allow(dead_code)]
    pub fn new_insecure(endpoint: String) -> Result<Self, RpcError> {
        let client = reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .build()?;
        Ok(RpcClient { endpoint, client })
    }

    pub async fn call(&self, method: &str, params: Value) -> Result<Value, RpcError> {
        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            method: method.to_string(),
            params,
            id: Value::Number(1.into()),
        };

        let response = self
            .client
            .post(&self.endpoint)
            .json(&request)
            .send()
            .await?;

        let rpc_response: JsonRpcResponse = response.json().await?;

        if let Some(err) = rpc_response.error {
            return Err(RpcError::Rpc {
                code: err.code,
                message: err.message,
            });
        }

        Ok(rpc_response.result.unwrap_or(Value::Null))
    }

    pub async fn get_validator_set(&self) -> Result<Value, RpcError> {
        self.call("getValidatorSet", Value::Null).await
    }

    pub async fn get_network_status(&self) -> Result<Value, RpcError> {
        self.call("getNetworkStatus", Value::Null).await
    }

    pub async fn get_validator_earnings(&self) -> Result<Value, RpcError> {
        self.call("getValidatorEarnings", Value::Null).await
    }

    pub async fn get_equivocation_proofs(&self) -> Result<Value, RpcError> {
        self.call("getEquivocationProofs", Value::Null).await
    }

    pub async fn get_banned_peers(&self) -> Result<Value, RpcError> {
        self.call("getBannedPeers", Value::Null).await
    }

    pub async fn get_recent_alerts(&self) -> Result<Value, RpcError> {
        self.call("getRecentAlerts", Value::Null).await
    }

    pub async fn send_operation(&self, op_hex: &str) -> Result<Value, RpcError> {
        self.call("sendOperation", serde_json::json!({ "op_hex": op_hex }))
            .await
    }

    pub async fn get_account(&self, account_number: u64) -> Result<Value, RpcError> {
        self.call(
            "getAccount",
            serde_json::json!({ "account_number": account_number }),
        )
        .await
    }

    pub async fn get_block(&self, block_number: u64) -> Result<Value, RpcError> {
        self.call(
            "getBlock",
            serde_json::json!({ "block_number": block_number }),
        )
        .await
    }

    pub async fn get_pendings(&self) -> Result<Value, RpcError> {
        self.call("getPendings", Value::Null).await
    }

    pub async fn find_accounts(
        &self,
        name: Option<&str>,
        account_type: Option<u16>,
        min_balance: Option<u64>,
        max_balance: Option<u64>,
    ) -> Result<Value, RpcError> {
        self.call(
            "findAccounts",
            serde_json::json!({
                "name": name,
                "type": account_type,
                "min_balance": min_balance,
                "max_balance": max_balance,
            }),
        )
        .await
    }
}
