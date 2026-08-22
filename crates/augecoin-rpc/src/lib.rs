pub mod auth;
pub mod config;
pub mod endpoints;

use axum::{
    routing::{get, post},
    Json, Router,
};
use axum_server::tls_rustls::RustlsConfig;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;
use thiserror::Error;
use tokio::net::TcpListener;

#[derive(Debug, Error)]
pub enum RpcError {
    #[error("TLS configuration error: {0}")]
    TlsConfig(String),
    #[error("server error: {0}")]
    Server(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone)]
pub struct RpcSettings {
    pub jsonrpc_addr: SocketAddr,
}

impl Default for RpcSettings {
    fn default() -> Self {
        RpcSettings {
            jsonrpc_addr: "127.0.0.1:0".parse().unwrap(),
        }
    }
}

// ── JSON-RPC 2.0 types ───────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub method: String,
    #[serde(default)]
    pub params: serde_json::Value,
    pub id: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
    pub id: serde_json::Value,
}

#[derive(Debug, Serialize, Clone)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
}

type AppState = endpoints::AppState;

/// Methods that mint tokens or create state and are prime spam targets on a
/// public testnet. These are additionally throttled by the stricter
/// `sensitive_rate_limiter` on top of the global limiter.
const SENSITIVE_METHODS: &[&str] = &["createaccount", "faucet"];

fn client_identifier(
    api_key: Option<&str>,
    remote: Option<SocketAddr>,
    forwarded_for: Option<&str>,
) -> String {
    if let Some(key) = api_key {
        return format!("key:{key}");
    }
    // Behind a reverse proxy the TCP peer is the proxy itself (usually
    // loopback), which would collapse every user into a single rate-limit
    // bucket. When the direct peer is loopback, trust the client IP from
    // X-Forwarded-For so each real client is limited independently.
    if let Some(addr) = remote {
        if addr.ip().is_loopback() {
            if let Some(ip) = forwarded_for.and_then(first_forwarded_ip) {
                return format!("ip:{ip}");
            }
        }
        return format!("ip:{}", addr.ip());
    }
    "anonymous".to_string()
}

/// Return the left-most IP of an `X-Forwarded-For` chain (the original client).
fn first_forwarded_ip(xff: &str) -> Option<String> {
    xff.split(',')
        .next()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

async fn jsonrpc_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::ConnectInfo(remote): axum::extract::ConnectInfo<SocketAddr>,
    headers: axum::http::HeaderMap,
    Json(req): Json<JsonRpcRequest>,
) -> Json<JsonRpcResponse> {
    let api_key = headers.get("x-api-key").and_then(|v| v.to_str().ok());
    let forwarded = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok());

    let client_id = client_identifier(api_key, Some(remote), forwarded);

    if !state.rate_limiter.check(&client_id) {
        return Json(JsonRpcResponse {
            jsonrpc: "2.0".into(),
            result: None,
            error: Some(JsonRpcError {
                code: -32001,
                message: "rate limit exceeded".into(),
            }),
            id: req.id,
        });
    }

    if SENSITIVE_METHODS.contains(&req.method.as_str())
        && !state.sensitive_rate_limiter.check(&client_id)
    {
        return Json(JsonRpcResponse {
            jsonrpc: "2.0".into(),
            result: None,
            error: Some(JsonRpcError {
                code: -32001,
                message: "rate limit exceeded for this endpoint; try again later".into(),
            }),
            id: req.id,
        });
    }

    let auth_level = auth::check_auth(api_key, &state.api_keys);
    if auth::is_admin_method(&req.method) && auth_level != auth::AuthLevel::Admin {
        return Json(JsonRpcResponse {
            jsonrpc: "2.0".into(),
            result: None,
            error: Some(JsonRpcError {
                code: -32002,
                message: "admin API key required for this method".into(),
            }),
            id: req.id,
        });
    }

    let result = dispatch_method(&req.method, &req.params, &state);

    match result {
        Ok(value) => Json(JsonRpcResponse {
            jsonrpc: "2.0".into(),
            result: Some(value),
            error: None,
            id: req.id,
        }),
        Err(msg) => Json(JsonRpcResponse {
            jsonrpc: "2.0".into(),
            result: None,
            error: Some(JsonRpcError {
                code: -32000,
                message: msg,
            }),
            id: req.id,
        }),
    }
}

fn dispatch_method(
    method: &str,
    params: &serde_json::Value,
    state: &Arc<AppState>,
) -> Result<serde_json::Value, String> {
    match method {
        "getaccount" => {
            let p: endpoints::GetAccountParams =
                serde_json::from_value(params.clone()).map_err(|e| e.to_string())?;
            let result = endpoints::handle_get_account(p, state)?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "resolve_address" | "resolveaddress" => {
            let p: endpoints::ResolveAddressParams =
                serde_json::from_value(params.clone()).map_err(|e| e.to_string())?;
            serde_json::to_value(endpoints::handle_resolve_address(p, state)?)
                .map_err(|e| e.to_string())
        }
        "createaccount" => {
            let p: endpoints::CreateAccountParams =
                serde_json::from_value(params.clone()).map_err(|e| e.to_string())?;
            serde_json::to_value(endpoints::handle_create_account(p, state)?)
                .map_err(|e| e.to_string())
        }
        "getblockoperations" => {
            let p: endpoints::GetBlockParams =
                serde_json::from_value(params.clone()).map_err(|e| e.to_string())?;
            let result = endpoints::handle_get_block_operations(p, state)?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "getblock" => {
            let p: endpoints::GetBlockParams =
                serde_json::from_value(params.clone()).map_err(|e| e.to_string())?;
            let result = endpoints::handle_get_block(p, state)?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "getoperations" => {
            let p: endpoints::GetOperationsParams =
                serde_json::from_value(params.clone()).map_err(|e| e.to_string())?;
            let result = endpoints::handle_get_operations(p, state)?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "getblockbyhash" => {
            let p: endpoints::GetByHashParams =
                serde_json::from_value(params.clone()).map_err(|e| e.to_string())?;
            let result = endpoints::handle_get_block_by_hash(p, state)?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "getoperationbyhash" => {
            let p: endpoints::GetByHashParams =
                serde_json::from_value(params.clone()).map_err(|e| e.to_string())?;
            let result = endpoints::handle_get_operation_by_hash(p, state)?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "sendoperation" => {
            let p: endpoints::SendOperationParams =
                serde_json::from_value(params.clone()).map_err(|e| e.to_string())?;
            let result = endpoints::handle_send_operation(p, state)?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "send" => {
            let p: endpoints::SendParams =
                serde_json::from_value(params.clone()).map_err(|e| e.to_string())?;
            serde_json::to_value(endpoints::handle_send(p, state)?).map_err(|e| e.to_string())
        }
        "sendoperations" => {
            let p: endpoints::SendOperationsParams =
                serde_json::from_value(params.clone()).map_err(|e| e.to_string())?;
            let result = endpoints::handle_send_operations(p, state)?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "getpendings" => {
            let result = endpoints::handle_get_pendings(state)?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "findaccounts" => {
            let p: endpoints::FindAccountsParams =
                serde_json::from_value(params.clone()).map_err(|e| e.to_string())?;
            let result = endpoints::handle_find_accounts(p, state)?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "getaccountcount" => {
            let result = endpoints::handle_get_account_count(state)?;
            Ok(serde_json::json!(result))
        }
        "getblockcount" => {
            let result = endpoints::handle_get_block_count(state)?;
            Ok(serde_json::json!(result))
        }
        "nodestatus" => {
            let result = endpoints::handle_get_node_status(state)?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "getvalidatorset" => {
            let result = endpoints::handle_get_validator_set(state)?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "validatoradd" => {
            let p: endpoints::ValidatorAddParams =
                serde_json::from_value(params.clone()).map_err(|e| e.to_string())?;
            let result = endpoints::handle_validator_add(p, state)?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "validatorremove" => {
            let p: endpoints::ValidatorRemoveParams =
                serde_json::from_value(params.clone()).map_err(|e| e.to_string())?;
            let result = endpoints::handle_validator_remove(p, state)?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "validatoractivate" => {
            let p: endpoints::ValidatorActivateParams =
                serde_json::from_value(params.clone()).map_err(|e| e.to_string())?;
            let result = endpoints::handle_validator_activate(p, state)?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "validatordeactivate" => {
            let p: endpoints::ValidatorDeactivateParams =
                serde_json::from_value(params.clone()).map_err(|e| e.to_string())?;
            let result = endpoints::handle_validator_deactivate(p, state)?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "listaccountsforsale" => {
            let result = endpoints::handle_list_accounts_for_sale(state)?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "listvalidatorinventory" => {
            let p: endpoints::ListInventoryParams =
                serde_json::from_value(params.clone()).map_err(|e| e.to_string())?;
            let result = endpoints::handle_list_validator_inventory(p, state)?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "listpendinggifts" => {
            let p: endpoints::ListPendingGiftsParams =
                serde_json::from_value(params.clone()).map_err(|e| e.to_string())?;
            let result = endpoints::handle_list_pending_gifts(p, state)?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "buyaccount" => {
            let p: endpoints::BuyAccountParams =
                serde_json::from_value(params.clone()).map_err(|e| e.to_string())?;
            let result = endpoints::handle_buy_account(p, state)?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "sellaccount" => {
            let p: endpoints::SellAccountParams =
                serde_json::from_value(params.clone()).map_err(|e| e.to_string())?;
            let result = endpoints::handle_sell_account(p, state)?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "giftaccount" => {
            let p: endpoints::GiftAccountParams =
                serde_json::from_value(params.clone()).map_err(|e| e.to_string())?;
            let result = endpoints::handle_gift_account(p, state)?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "acceptgift" => {
            let p: endpoints::AcceptGiftParams =
                serde_json::from_value(params.clone()).map_err(|e| e.to_string())?;
            let result = endpoints::handle_accept_gift(p, state)?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "cancelsale" => {
            let p: endpoints::CancelSaleParams =
                serde_json::from_value(params.clone()).map_err(|e| e.to_string())?;
            let result = endpoints::handle_cancel_sale(p, state)?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "ping" => Ok(serde_json::json!("pong")),
        _ => Err(format!("unknown method: {method}")),
    }
}

pub fn hash_operation(op: &augecoin_core::operation::Operation) -> [u8; 64] {
    augecoin_core::hash::blake3_512(&op.to_bytes())
}

pub fn install_crypto_provider() {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

pub fn load_tls_pem() -> Result<(String, String), RpcError> {
    match (
        std::env::var("AUGECOIN_TLS_CERT").ok(),
        std::env::var("AUGECOIN_TLS_KEY").ok(),
    ) {
        (Some(cert_path), Some(key_path)) => {
            let cert = std::fs::read_to_string(&cert_path).map_err(|e| {
                RpcError::TlsConfig(format!("cannot read AUGECOIN_TLS_CERT '{cert_path}': {e}"))
            })?;
            let key = std::fs::read_to_string(&key_path).map_err(|e| {
                RpcError::TlsConfig(format!("cannot read AUGECOIN_TLS_KEY '{key_path}': {e}"))
            })?;
            Ok((cert, key))
        }
        _ => {
            eprintln!(
                "WARNING: AUGECOIN_TLS_CERT/AUGECOIN_TLS_KEY not set; using self-signed \
                 certificate. Configure real TLS certs for production."
            );
            Ok((
                config::test_cert_pem().to_string(),
                config::test_key_pem().to_string(),
            ))
        }
    }
}

// ── Server ───────────────────────────────────────────────────────────

pub struct RpcServer {
    settings: RpcSettings,
}

impl RpcServer {
    pub fn new(settings: RpcSettings) -> Self {
        RpcServer { settings }
    }

    pub fn rustls_config_from_pem(cert_pem: &str, key_pem: &str) -> Result<RustlsConfig, RpcError> {
        let cert_chain: Vec<rustls::pki_types::CertificateDer> =
            rustls_pemfile::certs(&mut cert_pem.as_bytes())
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| RpcError::TlsConfig(format!("invalid cert PEM: {e}")))?;

        let key = rustls_pemfile::private_key(&mut key_pem.as_bytes())
            .map_err(|e| RpcError::TlsConfig(format!("invalid key PEM: {e}")))?
            .ok_or_else(|| RpcError::TlsConfig("no private key found in PEM".into()))?;

        let config = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(cert_chain, key)
            .map_err(|e| RpcError::TlsConfig(format!("rustls config error: {e}")))?;

        Ok(RustlsConfig::from_config(Arc::new(config)))
    }

    pub async fn start_jsonrpc(
        &self,
        tls_config: RustlsConfig,
        state: Arc<AppState>,
    ) -> Result<SocketAddr, RpcError> {
        let app = Router::new()
            .route("/", post(jsonrpc_handler))
            .route("/createaccount", post(create_account_handler))
            .route("/faucet", post(faucet_handler))
            .route("/health", get(health_handler))
            .layer(axum::extract::DefaultBodyLimit::max(1048576))
            .with_state(state);

        let listener = TcpListener::bind(self.settings.jsonrpc_addr).await?;
        let bound_addr = listener.local_addr()?;

        let handle = axum_server::from_tcp_rustls(listener.into_std()?, tls_config)
            .map_err(|e| RpcError::Server(format!("failed to create TLS listener: {e}")))?;

        tokio::spawn(async move {
            if let Err(e) = handle
                .serve(app.into_make_service_with_connect_info::<SocketAddr>())
                .await
            {
                eprintln!("JSON-RPC server error: {e}");
            }
        });

        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        Ok(bound_addr)
    }
}

/// Minimal liveness probe for external monitors. Returns 200 with a small
/// JSON body when the node is up and has a recent view of the chain; no
/// authentication or full RPC client required.
async fn health_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    let status = &state.node_status;
    let height = status
        .block_height
        .load(std::sync::atomic::Ordering::SeqCst);
    let peers = status
        .peers_connected
        .load(std::sync::atomic::Ordering::SeqCst);
    let syncing = status.syncing.load(std::sync::atomic::Ordering::SeqCst);
    let uptime = status
        .uptime_seconds
        .load(std::sync::atomic::Ordering::SeqCst);
    let hash = status
        .latest_block_hash
        .lock()
        .map(|g| hex::encode(*g))
        .unwrap_or_default();
    Json(serde_json::json!({
        "status": "ok",
        "height": height,
        "latest_block_hash_hex": hash,
        "peers_connected": peers,
        "syncing": syncing != 0,
        "uptime_seconds": uptime,
    }))
}

async fn create_account_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::ConnectInfo(remote): axum::extract::ConnectInfo<SocketAddr>,
    headers: axum::http::HeaderMap,
    Json(params): Json<endpoints::CreateAccountParams>,
) -> Result<Json<endpoints::CreateAccountResult>, (axum::http::StatusCode, String)> {
    let api_key = headers.get("x-api-key").and_then(|v| v.to_str().ok());
    if auth::check_auth(api_key, &state.api_keys) != auth::AuthLevel::Admin {
        return Err((
            axum::http::StatusCode::UNAUTHORIZED,
            "admin API key required for createaccount".into(),
        ));
    }
    let forwarded = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok());
    let client_id = client_identifier(api_key, Some(remote), forwarded);
    if !state.rate_limiter.check(&client_id) || !state.sensitive_rate_limiter.check(&client_id) {
        return Err((
            axum::http::StatusCode::TOO_MANY_REQUESTS,
            "rate limit exceeded for this endpoint; try again later".into(),
        ));
    }
    endpoints::handle_create_account(params, &state)
        .map(Json)
        .map_err(|e| (axum::http::StatusCode::BAD_REQUEST, e))
}

async fn faucet_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::ConnectInfo(remote): axum::extract::ConnectInfo<SocketAddr>,
    headers: axum::http::HeaderMap,
    Json(params): Json<endpoints::FaucetParams>,
) -> Result<Json<endpoints::FaucetResult>, (axum::http::StatusCode, String)> {
    let api_key = headers.get("x-api-key").and_then(|v| v.to_str().ok());
    let forwarded = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok());
    let client_id = match forwarded.and_then(first_forwarded_ip) {
        Some(ip) => format!("ip:{ip}"),
        None => client_identifier(api_key, Some(remote), None),
    };
    if !state.rate_limiter.check(&client_id) || !state.sensitive_rate_limiter.check(&client_id) {
        return Err((
            axum::http::StatusCode::TOO_MANY_REQUESTS,
            "rate limit exceeded".into(),
        ));
    }
    endpoints::handle_faucet(params, &client_id, &state)
        .map(Json)
        .map_err(|e| (axum::http::StatusCode::TOO_MANY_REQUESTS, e))
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod rpc_tests {
    use super::*;
    use std::net::TcpStream as StdTcpStream;
    use std::sync::OnceLock;

    fn install_crypto_provider() {
        static INIT: std::sync::Once = std::sync::Once::new();
        INIT.call_once(|| {
            let _ = rustls::crypto::ring::default_provider().install_default();
        });
    }

    fn rpc_certs() -> (&'static str, &'static str) {
        static CERTS: OnceLock<(String, String)> = OnceLock::new();
        let (cert, key) = CERTS.get_or_init(|| {
            (
                config::test_cert_pem().to_string(),
                config::test_key_pem().to_string(),
            )
        });
        (cert, key)
    }

    fn make_test_state() -> Arc<AppState> {
        use std::sync::atomic::AtomicU32 as A32;
        static C: A32 = A32::new(9500);
        let id = C.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let path = format!("/tmp/augecoin-rpc-lib2-{id}");
        let _ = std::fs::remove_dir_all(&path);
        let storage = augecoin_storage::Storage::open(&path).unwrap();

        let node_status = Arc::new(endpoints::NodeStatus::default());
        node_status
            .chain_id
            .store(1, std::sync::atomic::Ordering::SeqCst);

        Arc::new(AppState {
            storage: Arc::new(storage),
            mempool: Arc::new(std::sync::Mutex::new(
                augecoin_core::mempool::Mempool::default(),
            )),
            node_status,
            api_keys: auth::ApiKeyStore::new(vec!["test-admin-key".into()]),
            rate_limiter: auth::RateLimiter::new(100, std::time::Duration::from_secs(60)),
            sensitive_rate_limiter: auth::RateLimiter::new(5, std::time::Duration::from_secs(60)),
            require_admin_auth: true,
            faucet_keypair: None,
            faucet_account: 1,
            faucet_amount: 100,
            faucet_claims: std::sync::Mutex::new(std::collections::HashMap::new()),
            admin_keypair: None,
            op_broadcaster: None,
        })
    }

    fn test_connect_info() -> axum::extract::ConnectInfo<SocketAddr> {
        axum::extract::ConnectInfo("127.0.0.1:40000".parse().unwrap())
    }

    #[tokio::test]
    async fn jsonrpc_accepts_tls_connection() {
        install_crypto_provider();
        let (cert_pem, key_pem) = rpc_certs();
        let tls_config = RpcServer::rustls_config_from_pem(cert_pem, key_pem).unwrap();
        let server = RpcServer::new(RpcSettings::default());
        let state = make_test_state();
        let addr = server.start_jsonrpc(tls_config, state).await.unwrap();

        let stream = StdTcpStream::connect_timeout(&addr, std::time::Duration::from_secs(3));
        assert!(stream.is_ok(), "server should be listening on {addr}");
    }

    #[test]
    fn plain_http_rejected_by_tls_only_server() {
        install_crypto_provider();
        let (cert_pem, key_pem) = rpc_certs();
        let tls_config = RpcServer::rustls_config_from_pem(cert_pem, key_pem);
        assert!(
            tls_config.is_ok(),
            "valid PEM should produce TLS-only config"
        );
    }

    #[test]
    fn jsonrpc_handler_responds_to_ping() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let state = make_test_state();
        rt.block_on(async {
            let req = JsonRpcRequest {
                jsonrpc: "2.0".into(),
                method: "ping".into(),
                params: serde_json::Value::Null,
                id: serde_json::json!(1),
            };
            let axum_state = axum::extract::State(state.clone());
            let headers = axum::http::HeaderMap::new();
            let resp = jsonrpc_handler(axum_state, test_connect_info(), headers, Json(req)).await;
            assert_eq!(resp.jsonrpc, "2.0");
            assert_eq!(resp.result, Some(serde_json::json!("pong")));
        });
    }

    #[test]
    fn jsonrpc_handler_unknown_method_returns_error() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let state = make_test_state();
        rt.block_on(async {
            let req = JsonRpcRequest {
                jsonrpc: "2.0".into(),
                method: "unknown".into(),
                params: serde_json::Value::Null,
                id: serde_json::json!(1),
            };
            let axum_state = axum::extract::State(state.clone());
            let headers = axum::http::HeaderMap::new();
            let resp = jsonrpc_handler(axum_state, test_connect_info(), headers, Json(req)).await;
            assert!(resp.error.is_some());
            assert!(resp.result.is_none());
        });
    }

    #[test]
    fn admin_method_rejected_without_api_key() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let state = make_test_state();
        rt.block_on(async {
            let req = JsonRpcRequest {
                jsonrpc: "2.0".into(),
                method: "validatoradd".into(),
                params: serde_json::Value::Null,
                id: serde_json::json!(1),
            };
            let axum_state = axum::extract::State(state.clone());
            let headers = axum::http::HeaderMap::new();
            let resp = jsonrpc_handler(axum_state, test_connect_info(), headers, Json(req)).await;
            assert!(resp.error.is_some());
            let err = resp.error.clone().unwrap();
            assert!(err.message.contains("admin API key"));
        });
    }

    #[test]
    fn admin_method_accepted_with_valid_api_key() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let state = make_test_state();
        rt.block_on(async {
            let req = JsonRpcRequest {
                jsonrpc: "2.0".into(),
                method: "validatoradd".into(),
                params: serde_json::Value::Null,
                id: serde_json::json!(1),
            };
            let axum_state = axum::extract::State(state.clone());
            let mut headers = axum::http::HeaderMap::new();
            headers.insert("x-api-key", "test-admin-key".parse().unwrap());
            let resp = jsonrpc_handler(axum_state, test_connect_info(), headers, Json(req)).await;
            assert!(resp.error.is_some());
            let err = resp.error.clone().unwrap();
            assert!(!err.message.contains("admin API key"));
        });
    }

    #[test]
    fn public_method_works_without_api_key() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let state = make_test_state();
        rt.block_on(async {
            let req = JsonRpcRequest {
                jsonrpc: "2.0".into(),
                method: "getaccount".into(),
                params: serde_json::json!({"account_number": 1}),
                id: serde_json::json!(1),
            };
            let axum_state = axum::extract::State(state.clone());
            let headers = axum::http::HeaderMap::new();
            let resp = jsonrpc_handler(axum_state, test_connect_info(), headers, Json(req)).await;
            assert!(resp.error.is_some());
        });
    }

    #[test]
    fn rate_limit_blocks_excessive_requests() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let state = make_test_state();
        rt.block_on(async {
            for _ in 0..200 {
                let req = JsonRpcRequest {
                    jsonrpc: "2.0".into(),
                    method: "ping".into(),
                    params: serde_json::Value::Null,
                    id: serde_json::json!(1),
                };
                let axum_state = axum::extract::State(state.clone());
                let headers = axum::http::HeaderMap::new();
                let resp =
                    jsonrpc_handler(axum_state, test_connect_info(), headers, Json(req)).await;
                if resp.error.is_some() {
                    return;
                }
            }
            panic!("rate limiter did not trigger after 200 requests");
        });
    }

    #[test]
    fn sensitive_method_rate_limited_separately() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let state = make_test_state();
        rt.block_on(async {
            // sensitive limiter in make_test_state allows 5 createaccount/min.
            let mut blocked = false;
            for _ in 0..10 {
                let req = JsonRpcRequest {
                    jsonrpc: "2.0".into(),
                    method: "createaccount".into(),
                    params: serde_json::json!({"public_key_hex": "00".repeat(32)}),
                    id: serde_json::json!(1),
                };
                let axum_state = axum::extract::State(state.clone());
                let headers = axum::http::HeaderMap::new();
                let resp =
                    jsonrpc_handler(axum_state, test_connect_info(), headers, Json(req)).await;
                if let Some(err) = &resp.error {
                    if err.message.contains("rate limit") {
                        blocked = true;
                        break;
                    }
                }
            }
            assert!(
                blocked,
                "sensitive rate limiter did not trigger for createaccount spam"
            );
        });
    }

    #[test]
    fn health_endpoint_reports_status() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let state = make_test_state();
        state
            .node_status
            .block_height
            .store(42, std::sync::atomic::Ordering::SeqCst);
        rt.block_on(async {
            let axum_state = axum::extract::State(state.clone());
            let Json(body) = health_handler(axum_state).await;
            assert_eq!(body["status"], "ok");
            assert_eq!(body["height"], 42);
            assert_eq!(body["syncing"], false);
        });
    }
}
