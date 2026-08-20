//! Admin API-key authentication middleware.

use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

use crate::state::AppState;

/// Require a valid `x-api-key` header matching the configured admin key.
pub async fn admin_auth(State(state): State<AppState>, req: Request, next: Next) -> Response {
    let authorized = req
        .headers()
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|key| state.config.admin_key_matches(key));

    if !authorized {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "unauthorized" })),
        )
            .into_response();
    }

    next.run(req).await
}
