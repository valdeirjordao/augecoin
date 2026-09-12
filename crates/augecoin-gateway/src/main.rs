//! augecoin-gateway — Monetized REST API Gateway for AugeCoin

use std::net::SocketAddr;
use std::sync::Arc;

use axum::{
    middleware::from_fn_with_state,
    routing::{delete, get, post},
    Json, Router,
};
use axum_server::{self, tls_rustls::RustlsConfig};
use dotenvy::dotenv;
use snafu::Snafu;
use tokio::signal;
use tower_http::cors::CorsLayer;
use tracing::{info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod auth;
mod billing;
mod db;
mod gateway_mw;
mod models;
mod rate_limit;
mod rpc_proxy;
mod webhook;

use auth::{ApiKeyAuth, JwtAuth};
use billing::{BillingEngine, DeveloperClaims};
use db::DbPool;
use gateway_mw::{authenticate_api_key, authenticate_jwt, bill_request, check_rate_limit};
use rate_limit::RateLimiter;
use rpc_proxy::RpcProxy;

// ── Shared application state ────────────────────────────────────────────────

#[derive(Clone)]
pub struct AppState {
    pub proxy: Arc<RpcProxy>,
    pub db: Arc<DbPool>,
    pub rate_limiter: Arc<RateLimiter>,
    pub billing: Arc<BillingEngine>,
    pub jwt_auth: Arc<JwtAuth>,
    pub api_key_auth: Arc<ApiKeyAuth>,
}

// ── Error type ──────────────────────────────────────────────────────────────

#[derive(Debug, Snafu)]
pub enum GatewayError {
    #[snafu(display("Failed to bind to {addr}: {source}"))]
    Bind { addr: String, source: std::io::Error },
    #[snafu(display("Database error: {source}"))]
    Database { source: sqlx::Error },
    #[snafu(display("TLS error: {source}"))]
    Tls { source: std::io::Error },
    #[snafu(display("Configuration error: {message}"))]
    Config { message: String },
}

#[tokio::main]
async fn main() -> Result<(), GatewayError> {
    dotenv().ok();

    // ── Logging ──────────────────────────────────────────────────────────────
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
            "augecoin_gateway=info,tower_http=info,axum=info".into()
        }))
        .with(tracing_subscriber::fmt::layer())
        .init();

    info!("Starting AugeCoin Gateway v{}", env!("CARGO_PKG_VERSION"));

    // ── Configuration ────────────────────────────────────────────────────────
    let gateway_port: u16 = std::env::var("GATEWAY_PORT")
        .or_else(|_| std::env::var("PORT"))
        .unwrap_or_else(|_| "8443".to_string())
        .parse()
        .expect("GATEWAY_PORT must be a valid u16");

    let gateway_host = std::env::var("GATEWAY_HOST").unwrap_or_else(|_| "0.0.0.0".to_string());

    let upstream_rpc = std::env::var("UPSTREAM_RPC_URL")
        .unwrap_or_else(|_| "http://localhost:4003".to_string());

    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://augecoin:augecoin@localhost:5432/augecoin_gateway".to_string());

    let jwt_secret = std::env::var("GATEWAY_JWT_SECRET")
        .unwrap_or_else(|_| "change-me-in-production-use-openssl-rand-hex-32".to_string());

    let admin_bootstrap_key: Option<String> = std::env::var("GATEWAY_ADMIN_BOOTSTRAP_KEY").ok();

    let tls_cert = std::env::var("GATEWAY_TLS_CERT").ok();
    let tls_key = std::env::var("GATEWAY_TLS_KEY").ok();

    // ── Database ─────────────────────────────────────────────────────────────
    info!("Connecting to database…");
    let db = db::Database::connect(&database_url)
        .await
        .map_err(|e| GatewayError::Database { source: e })?;
    db.run_migrations()
        .await
        .map_err(|e| GatewayError::Database { source: e })?;
    info!("Database ready.");

    // ── Shared state ─────────────────────────────────────────────────────────
    let pool = Arc::new(db.pool);
    let proxy = Arc::new(RpcProxy::new(&upstream_rpc));
    let rate_limiter = Arc::new(RateLimiter::default());
    let billing = Arc::new(BillingEngine::new(pool.clone()));
    let jwt_auth = Arc::new(JwtAuth::new(&jwt_secret));
    let api_key_auth = Arc::new(ApiKeyAuth::new(pool.clone()));

    if let Some(ref key) = admin_bootstrap_key {
        billing.bootstrap_admin_key(key).await;
    }

    let state = Arc::new(AppState {
        proxy: proxy.clone(),
        db: pool.clone(),
        rate_limiter: rate_limiter.clone(),
        billing: billing.clone(),
        jwt_auth: jwt_auth.clone(),
        api_key_auth: api_key_auth.clone(),
    });

    // ── Middleware wrappers ──────────────────────────────────────────────────

    let mw_auth_api_key = from_fn_with_state(state.clone(), gateway_mw::authenticate_api_key);
    let mw_check_rate = from_fn_with_state(state.clone(), gateway_mw::check_rate_limit);
    let mw_bill = from_fn_with_state(state.clone(), gateway_mw::bill_request);
    let mw_auth_jwt = from_fn_with_state(state.clone(), gateway_mw::authenticate_jwt);

    // ── Routes ───────────────────────────────────────────────────────────────
    let app = Router::new()
        // Public
        .route("/health", get(|| async { "ok" }))
        .route("/readyz", get(|| async { "ready" }))
        .route("/v1/pricing", get(|| async { Json(rate_limit::public_tiers()) }))
        .route("/v1/developers/register", post(auth::register_developer))
        .route("/v1/auth/token", post(auth::exchange_api_key))
        // JSON-RPC proxy
        .route("/", post(rpc_proxy::handle_jsonrpc))
        .route("/v1/rpc", post(rpc_proxy::handle_jsonrpc))
        // REST convenience
        .route("/v1/account/{id}", get(rpc_proxy::handle_get_account))
        .route("/v1/block/{height}", get(rpc_proxy::handle_get_block))
        .route("/v1/blocks", get(rpc_proxy::handle_list_blocks))
        .route("/v1/transactions/pending", get(rpc_proxy::handle_pendings))
        .route("/v1/node/status", get(rpc_proxy::handle_node_status))
        .route("/v1/contracts/auge20", get(rpc_proxy::handle_list_auge20))
        .route("/v1/account/{id}/balance", get(rpc_proxy::handle_balance))
        // Developer self-service
        .route("/v1/developers/me", get(auth::get_current_developer))
        .route("/v1/developers/keys", post(auth::create_api_key))
        .route("/v1/developers/keys/{id}", delete(auth::revoke_api_key))
        .route("/v1/developers/usage", get(billing::get_usage_stats))
        .route("/v1/developers/billing", get(billing::get_billing_summary))
        .route("/v1/webhooks/register", post(webhook::register_webhook))
        .route("/v1/webhooks", get(webhook::list_webhooks))
        .route("/v1/webhooks/{id}", delete(webhook::delete_webhook))
        // Prometheus metrics
        .route("/metrics", get(|| async {
            use prometheus::Encoder;
            let encoder = prometheus::TextEncoder::new();
            let metric_families = prometheus::gather();
            let mut buf = Vec::new();
            encoder.encode(&metric_families, &mut buf).unwrap();
            String::from_utf8(buf).unwrap()
        }))
        // Admin routes
        .route("/v1/admin/keys", get(auth::list_all_keys))
        .route("/v1/admin/developers", get(auth::list_all_developers))
        // Middleware layers
        .route_layer(mw_auth_api_key)
        .route_layer(mw_check_rate)
        .route_layer(mw_bill)
        .route_layer(mw_auth_jwt)
        // Global
        .layer(CorsLayer::permissive())
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .layer(tower_http::compression::CompressionLayer::new())
        .with_state(state);

    // ── Metrics ──────────────────────────────────────────────────────────────
    let _ = metrics_exporter_prometheus::PrometheusBuilder::new()
        .build_recorder();

    // ── Listen ───────────────────────────────────────────────────────────────
    let addr: SocketAddr = format!("{gateway_host}:{gateway_port}")
        .parse()
        .expect("Invalid listen address");

    info!("Gateway listening on {}", addr);
    info!("Upstream RPC: {}", upstream_rpc);
    info!("Database: {}", database_url);

    match (tls_cert, tls_key) {
        (Some(cert_path), Some(key_path)) => {
            let tls_config = RustlsConfig::from_pem_file(&cert_path, &key_path)
                .await
                .map_err(|e| GatewayError::Tls { source: std::io::Error::new(std::io::ErrorKind::Other, e) })?;
            info!("TLS enabled with cert={cert_path} key={key_path}");
            axum_server::bind_rustls(addr, tls_config)
                .serve(app.into_make_service())
                .await
                .ok();
        }
        _ => {
            warn!("Running WITHOUT TLS — not recommended for production");
            axum_server::Server::bind(addr)
                .serve(app.into_make_service())
                .await
                .map_err(|e| GatewayError::Bind { addr: addr.to_string(), source: e })?;
        }
    }

    Ok(())
}
