//! HTTP API: router + admin API-key authentication.

mod auth;
mod handlers;

use axum::{
    middleware,
    routing::{get, post},
    Router,
};
use tower_http::{
    limit::RequestBodyLimitLayer,
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    trace::TraceLayer,
};

use crate::state::AppState;

/// Body limit for JSON payloads (licenses are small; heartbeat/activation stay
/// well under this bound too).
const MAX_BODY_BYTES: usize = 16 * 1024;

pub fn router(state: AppState) -> Router {
    // Administrative surface: gated by the shared-secret API key.
    let admin = Router::new()
        .route(
            "/v1/licenses",
            post(handlers::issue_license).get(handlers::list_licenses),
        )
        .route("/v1/licenses/{id}", get(handlers::get_license))
        .route("/v1/licenses/{id}/suspend", post(handlers::suspend_license))
        .route("/v1/licenses/{id}/revoke", post(handlers::revoke_license))
        .route("/v1/validators", get(handlers::list_validators))
        .route("/v1/validators/{id}", get(handlers::get_validator))
        .route(
            "/v1/validators/{id}/approve",
            post(handlers::approve_validator),
        )
        .route(
            "/v1/validators/{id}/suspend",
            post(handlers::suspend_validator),
        )
        .route(
            "/v1/validators/{id}/revoke",
            post(handlers::revoke_validator),
        )
        .route("/metrics", get(handlers::metrics))
        .route(
            "/releases",
            post(handlers::publish_release).get(handlers::list_releases),
        )
        .route("/audit", get(handlers::audit))
        .route("/alerts", get(handlers::alerts))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            auth::admin_auth,
        ))
        .with_state(state.clone());

    // Public surface: health, license validation and the validator-facing API
    // (activation, heartbeat, stats) called by the desktop app and the wallet.
    let public = Router::new()
        .route("/health", get(handlers::health))
        .route("/v1/licenses/validate", post(handlers::validate_license))
        .route("/activate", post(handlers::activate))
        .route("/validator/heartbeat", post(handlers::heartbeat))
        .route("/validator/stats", get(handlers::validator_stats))
        .route("/updates", get(handlers::updates))
        .with_state(state);

    admin
        .merge(public)
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
        .layer(PropagateRequestIdLayer::x_request_id())
        .layer(RequestBodyLimitLayer::new(MAX_BODY_BYTES))
        .layer(TraceLayer::new_for_http())
}
