//! RPC Proxy — forwards JSON-RPC 2.0 requests to the upstream AugeCoin node.

use axum::{
    extract::{Json, Path, Query, State},
    http::StatusCode,
};
use serde_json::{json, Value};
use std::sync::Arc;
use tracing::{debug, error, warn};

use crate::AppState;
use crate::billing::DeveloperClaims;

// ─── Helpers ────────────────────────────────────────────────────────────────

fn proxy(state: &AppState) -> &Arc<RpcProxy> {
    &state.proxy
}

// ─── RPC Proxy State ────────────────────────────────────────────────────────

pub struct RpcProxy {
    upstream_url: String,
    client: reqwest::Client,
}

impl RpcProxy {
    pub fn new(upstream_rpc: &str) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("reqwest client build");

        Self {
            upstream_url: upstream_rpc.trim_end_matches('/').to_string(),
            client,
        }
    }
}

// ─── JSON-RPC handler ────────────────────────────────────────────────────────

#[derive(Debug, serde::Deserialize)]
pub struct RpcBody {
    #[serde(rename = "jsonrpc")]
    pub jsonrpc: Option<String>,
    pub id: Option<Value>,
    pub method: Option<String>,
    pub params: Option<Vec<Value>>,
}

// Admin-only methods — never forwarded on behalf of a developer
const ADMIN_METHODS: &[&str] = &[
    "createaccount",
    "validatoradd",
    "validatorremove",
    "validatoractivate",
    "validatordeactivate",
    "contract_store_code",
    "contract_create",
    "contract_execute",
];

pub async fn handle_jsonrpc(
    State(state): State<Arc<AppState>>,
    claims: DeveloperClaims,
    Json(body): Json<RpcBody>,
) -> Result<Json<Value>, StatusCode> {
    let method = body.method.unwrap_or_default();

    // Block admin methods for non-admin tiers
    if ADMIN_METHODS.contains(&method.as_str()) && !claims.scopes.contains(&"admin".to_string()) {
        return Ok(Json(json!({
            "jsonrpc": "2.0",
            "id": body.id,
            "error": { "code": -32601, "message": "Method not allowed for your tier. Upgrade to Business." }
        })));
    }

    let upstream = format!("{}/", proxy(&state).upstream_url);
    debug!("Proxying {} → {}", method, upstream);

    let req_body = json!({
        "jsonrpc": body.jsonrpc.unwrap_or_else(|| "2.0".to_string()),
        "id": body.id,
        "method": method,
        "params": body.params.unwrap_or_default(),
    });

    let resp = proxy(&state)
        .client
        .post(&upstream)
        .header("content-type", "application/json")
        .header("x-augecoin-developer-id", claims.developer_id.to_string())
        .json(&req_body)
        .send()
        .await
        .map_err(|e| {
            error!("Upstream RPC error: {}", e);
            StatusCode::BAD_GATEWAY
        })?;

    let status = resp.status();
    let body: Value = resp.json().await.map_err(|_| StatusCode::BAD_GATEWAY)?;

    if status.is_success() {
        Ok(Json(body))
    } else {
        warn!("Upstream returned {}: {:?}", status, body);
        Ok(Json(body))
    }
}

// ─── REST convenience endpoints ───────���─────────────────────────────────────

pub async fn handle_get_account(
    State(state): State<Arc<AppState>>,
    claims: DeveloperClaims,
    Path(account_id): Path<i64>,
) -> Result<Json<Value>, StatusCode> {
    call_upstream(&state, &claims, "getaccount", vec![account_id.into()]).await
}

pub async fn handle_get_block(
    State(state): State<Arc<AppState>>,
    claims: DeveloperClaims,
    Path(height): Path<i64>,
) -> Result<Json<Value>, StatusCode> {
    call_upstream(&state, &claims, "getblock", vec![height.into()]).await
}

#[derive(Debug, serde::Deserialize)]
pub struct ListBlocksQuery {
    pub from_block: Option<i64>,
    pub limit: Option<i64>,
}

pub async fn handle_list_blocks(
    State(state): State<Arc<AppState>>,
    claims: DeveloperClaims,
    Query(q): Query<ListBlocksQuery>,
) -> Result<Json<Value>, StatusCode> {
    let _ = q;
    call_upstream(&state, &claims, "getblockcount", vec![]).await
}

pub async fn handle_pendings(
    State(state): State<Arc<AppState>>,
    claims: DeveloperClaims,
) -> Result<Json<Value>, StatusCode> {
    call_upstream(&state, &claims, "getpendings", vec![0i64.into(), 50i64.into()]).await
}

pub async fn handle_node_status(
    State(state): State<Arc<AppState>>,
    claims: DeveloperClaims,
) -> Result<Json<Value>, StatusCode> {
    call_upstream(&state, &claims, "nodestatus", vec![]).await
}

pub async fn handle_list_auge20(
    State(state): State<Arc<AppState>>,
    claims: DeveloperClaims,
) -> Result<Json<Value>, StatusCode> {
    call_upstream(&state, &claims, "contract_list_auge20", vec![]).await
}

pub async fn handle_balance(
    State(state): State<Arc<AppState>>,
    claims: DeveloperClaims,
    Path(account_id): Path<i64>,
) -> Result<Json<Value>, StatusCode> {
    let account = call_upstream(&state, &claims, "getaccount", vec![account_id.into()]).await?;
    Ok(Json(json!({
        "account_id": account_id,
        "balance_augesat": account.0.get("balance"),
        "balance_auge": account.0.get("balance").map(|v| (v.as_u64().unwrap_or(0) as f64) / 1e8)
    })))
}

// ─── Shared upstream caller ──────────────────────────────────────────────────

async fn call_upstream(
    state: &Arc<AppState>,
    claims: &DeveloperClaims,
    method: &str,
    params: Vec<Value>,
) -> Result<Json<Value>, StatusCode> {
    let req_body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": method,
        "params": params,
    });

    let dev_id = claims.developer_id.clone();
    let resp = proxy(state)
        .client
        .post(format!("{}/", proxy(state).upstream_url))
        .header("content-type", "application/json")
        .header("x-augecoin-developer-id", dev_id.to_string())
        .json(&req_body)
        .send()
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?;

    let body: Value = resp.json().await.map_err(|_| StatusCode::BAD_GATEWAY)?;
    Ok(Json(body))
}
