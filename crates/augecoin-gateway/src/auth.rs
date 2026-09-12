//! Auth middleware — API key (x-api-key) + JWT Bearer token

use std::sync::Arc;

use axum::{
    body::Body,
    extract::{Json, Path, State},
    http::request::Parts,
    http::{Request, StatusCode},
    middleware::Next,
    response::Response,
};
use jsonwebtoken::{decode, DecodingKey, Validation, Algorithm};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use tracing::warn;
use uuid::Uuid;

use crate::billing::DeveloperClaims;
use crate::db::DbPool;
use crate::models::ApiKeyRow;

// ─── JWT claims ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JwtClaims {
    pub sub: String,
    pub tier: String,
    pub exp: i64,
    pub iat: i64,
    pub scope: Vec<String>,
}

// Make DeveloperClaims extractable as a handler parameter
impl<S> axum::extract::FromRequestParts<S> for DeveloperClaims
where
    S: Send + Sync,
{
    type Rejection = (StatusCode, &'static str);

    async fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<DeveloperClaims>()
            .cloned()
            .ok_or((StatusCode::UNAUTHORIZED, "Missing developer claims"))
    }
}

// ─── Auth service structs ────────────────────────────────────────────────────

#[derive(Clone)]
pub struct JwtAuth {
    pub secret: String,
}

impl JwtAuth {
    pub fn new(secret: &str) -> Self {
        Self { secret: secret.to_string() }
    }
}

#[derive(Clone)]
pub struct ApiKeyAuth {
    pub db: Arc<DbPool>,
}

impl ApiKeyAuth {
    pub fn new(db: Arc<DbPool>) -> Self {
        Self { db }
    }
}

// ─── Middleware: API key ─────────────────────────────────────────────────────

pub async fn authenticate_api_key(
    State(state): State<Arc<crate::AppState>>,
    mut req: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let api_key = req
        .headers()
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let row: Option<ApiKeyRow> = sqlx::query_as(
        r#"SELECT id, developer_id, key_prefix, key_hash, tier, label, scopes, revoked, last_used_at, created_at
           FROM api_keys WHERE key_hash = encode(sha256($1::bytea), 'hex') AND revoked = false"#,
    )
    .bind(api_key)
    .fetch_optional(state.db.as_ref())
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let row = row.ok_or(StatusCode::UNAUTHORIZED)?;

    let db_clone = state.db.clone();
    let key_id = row.id;
    tokio::spawn(async move {
        let _ = sqlx::query("UPDATE api_keys SET last_used_at = now() WHERE id = $1")
            .bind(key_id)
            .execute(db_clone.as_ref())
            .await;
    });

    let claims = DeveloperClaims {
        developer_id: row.developer_id,
        api_key_id: Some(row.id),
        tier: row.tier,
        scopes: row.scopes.unwrap_or_default(),
    };

    req.extensions_mut().insert(claims);
    Ok(next.run(req).await)
}

// ─── Middleware: JWT ─────────────────────────────────────────────────────────

pub async fn authenticate_jwt(
    State(state): State<Arc<crate::AppState>>,
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

    let validation = Validation::new(Algorithm::HS256);
    let claims = decode::<JwtClaims>(
        token,
        &DecodingKey::from_secret(state.jwt_auth.secret.as_bytes()),
        &validation,
    );

    let claims = match claims {
        Ok(c) => c.claims,
        Err(_) => {
            warn!("Invalid JWT presented");
            return Err(Response::builder()
                .status(StatusCode::UNAUTHORIZED)
                .body(Body::from(r#"{"error":"invalid_token"}"#))
                .unwrap());
        }
    };

    req.extensions_mut().insert(DeveloperClaims {
        developer_id: Uuid::new_v4(),
        api_key_id: None,
        tier: claims.tier,
        scopes: claims.scope,
    });

    Ok(next.run(req).await)
}

// ─── Developer registration ──────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    pub name: String,
    pub email: String,
    pub company: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct RegisterResponse {
    pub developer_id: String,
    pub api_key: String,
    pub tier: &'static str,
    pub created_at: String,
}

pub async fn register_developer(
    State(state): State<Arc<crate::AppState>>,
    Json(req): Json<RegisterRequest>,
) -> Result<Json<RegisterResponse>, StatusCode> {
    let dev_id = Uuid::new_v4();

    sqlx::query(
        r#"INSERT INTO developers (id, name, email, company, tier, created_at)
           VALUES ($1, $2, $3, $4, 'free', now())"#,
    )
    .bind(dev_id)
    .bind(req.name)
    .bind(req.email)
    .bind(req.company)
    .execute(state.db.as_ref())
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let api_key = state.billing.generate_api_key(&dev_id.to_string(), "default").await;

    let row = sqlx::query("SELECT created_at FROM developers WHERE id = $1")
        .bind(dev_id)
        .fetch_one(state.db.as_ref())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let created_at_str: String = row.get(0);
    let created_at = chrono::DateTime::parse_from_rfc3339(&created_at_str)
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .unwrap_or_else(|_| chrono::Utc::now());

    Ok(Json(RegisterResponse {
        developer_id: dev_id.to_string(),
        api_key,
        tier: "free",
        created_at: created_at.to_rfc3339(),
    }))
}

pub async fn exchange_api_key() {
    // TODO: OAuth / SSO integration
}

pub async fn get_current_developer(
    State(state): State<Arc<crate::AppState>>,
    claims: DeveloperClaims,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let row = sqlx::query(
        r#"SELECT id, name, email, company, tier, created_at
           FROM developers WHERE id = $1"#,
    )
    .bind(claims.developer_id)
    .fetch_optional(state.db.as_ref())
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let row = row.ok_or(StatusCode::NOT_FOUND)?;
    let id_val: Uuid = row.try_get(0).unwrap_or_default();
    let name_val: String = row.try_get(1).unwrap_or_default();
    let email_val: String = row.try_get(2).unwrap_or_default();
    let company_val: Option<String> = row.try_get(3).unwrap_or(None);
    let tier_val: String = row.try_get(4).unwrap_or_default();
    let ca: chrono::DateTime<chrono::Utc> = row.try_get(5).unwrap_or_else(|_| chrono::Utc::now());

    Ok(Json(serde_json::json!({
        "id": id_val.to_string(),
        "name": name_val,
        "email": email_val,
        "company": company_val,
        "tier": tier_val,
        "created_at": ca.to_rfc3339(),
    })))
}

pub async fn create_api_key(
    State(state): State<Arc<crate::AppState>>,
    claims: DeveloperClaims,
    Json(req): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let label = req.get("label").and_then(|v| v.as_str()).unwrap_or("unnamed");
    let api_key = state.billing.generate_api_key(&claims.developer_id.to_string(), label).await;
    Ok(Json(serde_json::json!({ "api_key": api_key, "label": label })))
}

pub async fn revoke_api_key(
    State(state): State<Arc<crate::AppState>>,
    claims: DeveloperClaims,
    Path(key_id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    sqlx::query("UPDATE api_keys SET revoked = true WHERE id = $1 AND developer_id = $2")
        .bind(key_id)
        .bind(claims.developer_id.to_string())
        .execute(state.db.as_ref())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn list_all_keys(
    State(state): State<Arc<crate::AppState>>,
    claims: DeveloperClaims,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let rows: Vec<ApiKeyRow> = sqlx::query_as(
        r#"SELECT id, developer_id, key_prefix, key_hash, tier, label, scopes, revoked, last_used_at, created_at
           FROM api_keys WHERE developer_id = $1"#,
    )
    .bind(claims.developer_id.to_string())
    .fetch_all(state.db.as_ref())
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(serde_json::json!(rows)))
}

pub async fn list_all_developers(
    State(state): State<Arc<crate::AppState>>,
    claims: DeveloperClaims,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let rows: Vec<crate::models::DeveloperRow> = sqlx::query_as(
        "SELECT id, name, email, company, tier, created_at, updated_at FROM developers",
    )
    .fetch_all(state.db.as_ref())
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(serde_json::json!(rows)))
}
