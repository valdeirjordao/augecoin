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
        // Operator tool: self-hosted nodes commonly present self-signed
        // certificates (e.g. https://127.0.0.1:9005, https://augeco.in:19403),
        // so certificate verification is disabled by default here. Use
        // `new_strict` if verification is required.
        let client = reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .build()?;
        Ok(RpcClient { endpoint, client })
    }

    pub fn new_strict(endpoint: String) -> Result<Self, RpcError> {
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
        self.call_with_api_key(method, params, "").await
    }

    /// JSON-RPC call authenticated with the node admin API key (`x-api-key`
    /// header) — required by admin methods such as contract_create/execute.
    pub async fn call_with_api_key(
        &self,
        method: &str,
        params: Value,
        api_key: &str,
    ) -> Result<Value, RpcError> {
        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            method: method.to_string(),
            params,
            id: Value::Number(1.into()),
        };

        let response = self
            .client
            .post(&self.endpoint)
            .header("x-api-key", api_key)
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
        self.call("getvalidatorset", Value::Null).await
    }

    pub async fn get_network_status(&self) -> Result<Value, RpcError> {
        self.call("nodestatus", Value::Null).await
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
        self.call("sendoperation", serde_json::json!({ "hex": op_hex }))
            .await
    }

    pub async fn get_account(&self, account_number: u64) -> Result<Value, RpcError> {
        self.call(
            "getaccount",
            serde_json::json!({ "account_number": account_number }),
        )
        .await
    }

    pub async fn get_block(&self, block_number: u64) -> Result<Value, RpcError> {
        self.call(
            "getblock",
            serde_json::json!({ "block_number": block_number }),
        )
        .await
    }

    pub async fn get_pendings(&self) -> Result<Value, RpcError> {
        self.call("getpendings", Value::Null).await
    }

    pub async fn find_accounts(
        &self,
        name: Option<&str>,
        account_type: Option<u16>,
        min_balance: Option<u64>,
        max_balance: Option<u64>,
    ) -> Result<Value, RpcError> {
        self.call(
            "findaccounts",
            serde_json::json!({
                "name": name,
                "type": account_type,
                "min_balance": min_balance,
                "max_balance": max_balance,
            }),
        )
        .await
    }

    // ── Smart contract RPC methods (Fase 15) ─────────────────────────────

    pub async fn contract_get(&self, contract_id_hex: &str) -> Result<Value, RpcError> {
        self.call(
            "contract_get",
            serde_json::json!({ "contract_id_hex": contract_id_hex }),
        )
        .await
    }

    pub async fn contract_balance(
        &self,
        contract_id_hex: &str,
        address_hex: &str,
    ) -> Result<Value, RpcError> {
        self.call(
            "contract_balance",
            serde_json::json!({ "contract_id_hex": contract_id_hex, "address_hex": address_hex }),
        )
        .await
    }

    pub async fn contract_token_info(&self, contract_id_hex: &str) -> Result<Value, RpcError> {
        self.call(
            "contract_token_info",
            serde_json::json!({ "contract_id_hex": contract_id_hex }),
        )
        .await
    }

    pub async fn contract_store_code(
        &self,
        wasm_hex: &str,
        api_key: &str,
        gas_limit_hex: Option<&str>,
    ) -> Result<Value, RpcError> {
        self.call_with_api_key(
            "contract_store_code",
            serde_json::json!({
                "wasm_hex": wasm_hex,
                "sender_hex": "auto",
                "gas_limit_hex": gas_limit_hex,
            }),
            api_key,
        )
        .await
    }

    pub async fn contract_create(
        &self,
        code_id_hex: &str,
        instantiate_hex: &str,
        api_key: &str,
        gas_limit_hex: Option<&str>,
    ) -> Result<Value, RpcError> {
        self.call_with_api_key(
            "contract_create",
            serde_json::json!({
                "code_id_hex": code_id_hex,
                "instantiate_hex": instantiate_hex,
                "sender_hex": "auto",
                "gas_limit_hex": gas_limit_hex,
            }),
            api_key,
        )
        .await
    }

    pub async fn contract_execute(
        &self,
        contract_id_hex: &str,
        call_hex: &str,
        api_key: &str,
        gas_limit_hex: Option<&str>,
    ) -> Result<Value, RpcError> {
        self.call_with_api_key(
            "contract_execute",
            serde_json::json!({
                "contract_id_hex": contract_id_hex,
                "call_hex": call_hex,
                "sender_hex": "auto",
                "gas_limit_hex": gas_limit_hex,
            }),
            api_key,
        )
        .await
    }
}
