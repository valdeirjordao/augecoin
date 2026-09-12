//! Middleware for the AugeCoin Gateway

use std::sync::Arc;

use axum::{
    body::Body,
    extract::State,
    http::Request,
    middleware::Next,
    response::Response,
};
use crate::AppState;
use crate::rate_limit::RateLimiter;
use crate::models::ApiKeyRow;

// ─── Auth middleware (API key) ───────────────────────────────────────────────

pub async fn authenticate_api_key(
    State(state): State<Arc<AppState>>,
    mut req: Request<Body>,
    next: Next,
) -> Result<Response, axum::http::StatusCode> {
    let api_key = req
        .headers()
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .ok_or(axum::http::StatusCode::UNAUTHORIZED)?;

    let row: Option<ApiKeyRow> = sqlx::query_as(
        r#"SELECT id, developer_id, key_prefix, key_hash, tier, label, scopes, revoked, last_used_at, created_at
           FROM api_keys WHERE key_hash = encode(sha256($1::bytea), 'hex') AND revoked = false"#,
    )
    .bind(api_key)
    .fetch_optional(state.db.as_ref())
    .await
    .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

    let row = row.ok_or(axum::http::StatusCode::UNAUTHORIZED)?;

    let db_clone = state.db.clone();
    let key_id = row.id;
    tokio::spawn(async move {
        let _ = sqlx::query("UPDATE api_keys SET last_used_at = now() WHERE id = $1")
            .bind(key_id)
            .execute(db_clone.as_ref())
            .await;
    });

    let claims = crate::billing::DeveloperClaims {
        developer_id: row.developer_id,
        api_key_id: Some(row.id),
        tier: row.tier,
        scopes: row.scopes.unwrap_or_default(),
    };

    req.extensions_mut().insert(claims);
    Ok(next.run(req).await)
}

// ─── Auth middleware (JWT) ───────────────────────────────────────────────────

pub async fn authenticate_jwt(
    State(state): State<Arc<AppState>>,
    mut req: Request<Body>,
    next: Next,
) -> Result<Response, Response> {
    let auth_header = req
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok());

    let token = match auth_header {
        Some(h) if h.starts_with("Bearer ") => &h[7..],
        _ => return Ok(next.run(req).await),
    };

    use jsonwebtoken::{decode, DecodingKey, Validation, Algorithm};
    use crate::auth::JwtClaims;
    use uuid::Uuid;

    let validation = Validation::new(Algorithm::HS256);
    let claims = decode::<JwtClaims>(
        token,
        &DecodingKey::from_secret(state.jwt_auth.secret.as_bytes()),
        &validation,
    );

    let claims = match claims {
        Ok(c) => c.claims,
        Err(_) => {
            tracing::warn!("Invalid JWT presented");
            return Err(Response::builder()
                .status(axum::http::StatusCode::UNAUTHORIZED)
                .body(Body::from(r#"{"error":"invalid_token"}"#))
                .unwrap());
        }
    };

    req.extensions_mut().insert(crate::billing::DeveloperClaims {
        developer_id: Uuid::new_v4(),
        api_key_id: None,
        tier: claims.tier,
        scopes: claims.scope,
    });

    Ok(next.run(req).await)
}

// ─── Rate limit middleware ──────────────────────────────────────────────────

pub async fn check_rate_limit(
    State(state): State<Arc<AppState>>,
    req: Request<Body>,
    next: Next,
) -> Result<Response, axum::http::StatusCode> {
    let api_key = req
        .headers()
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("anonymous");

    let allowed = state.rate_limiter.check(api_key).await;
    if !allowed {
        return Err(axum::http::StatusCode::TOO_MANY_REQUESTS);
    }

    Ok(next.run(req).await)
}

// ─── Billing middleware ─────────────────────────────────────────────────────

pub async fn bill_request(
    State(state): State<Arc<AppState>>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let rpc_method = req
        .headers()
        .get("x-augecoin-rpc-method")
        .and_then(|v| v.to_str().ok())
        .unwrap_or(req.uri().path())
        .to_string();

    let claims = req.extensions().get::<crate::billing::DeveloperClaims>().cloned();

    if let Some(claims) = claims {
        let dev_id = claims.developer_id;
        let tier = claims.tier.clone();
        let api_key_id = claims.api_key_id;
        let billing = state.billing.clone();
        let rpc = rpc_method.clone();
        tokio::spawn(async move {
            billing.record_call(&dev_id, &api_key_id, &rpc, &tier).await;
        });
    }

    next.run(req).await
}
