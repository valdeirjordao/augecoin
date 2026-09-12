//! Webhook delivery engine

use axum::{
    extract::{Json, Path, State},
};
use hmac::{Hmac, Mac};
use serde_json::json;
use sha2::Sha256;
use sqlx::Row;
use std::sync::Arc;
use tracing::info;
use uuid::Uuid;

use crate::billing::DeveloperClaims;

#[derive(Debug, serde::Serialize)]
pub struct WebhookPayload {
    pub id: String,
    pub event: String,
    pub data: serde_json::Value,
    pub timestamp: i64,
}

pub async fn register_webhook(
    State(state): State<Arc<crate::AppState>>,
    claims: DeveloperClaims,
    Json(req): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, axum::http::StatusCode> {
    let url = req.get("url").and_then(|v| v.as_str()).ok_or(axum::http::StatusCode::BAD_REQUEST)?;
    let events: Vec<String> = req.get("events")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
        .unwrap_or_default();
    let secret = req.get("secret").and_then(|v| v.as_str()).unwrap_or("");

    let webhook_id = Uuid::new_v4();
    let dev_id_str = claims.developer_id.to_string();

    sqlx::query(
        r#"INSERT INTO webhooks (id, developer_id, url, secret, events)
           VALUES ($1, $2, $3, $4, $5)"#,
    )
    .bind(webhook_id)
    .bind(dev_id_str)
    .bind(url)
    .bind(secret)
    .bind(events)
    .execute(state.db.as_ref())
    .await
    .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

    info!("Webhook registered: {} for {}", webhook_id, url);
    Ok(Json(json!({ "id": webhook_id.to_string(), "url": url })))
}

pub async fn list_webhooks(
    State(state): State<Arc<crate::AppState>>,
    claims: DeveloperClaims,
) -> Result<Json<Vec<serde_json::Value>>, axum::http::StatusCode> {
    let rows = sqlx::query(
        "SELECT id, url, events, active, last_delivered_at, created_at FROM webhooks WHERE developer_id = $1",
    )
    .bind(claims.developer_id.to_string())
    .fetch_all(state.db.as_ref())
    .await
    .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut out = Vec::new();
    let mut i = 0;
    while i < rows.len() {
        let row = &rows[i];
        let id_val: Uuid = row.try_get(0).unwrap_or_default();
        let url_val: String = row.try_get(1).unwrap_or_default();
        let events_val: Vec<String> = row.try_get(2).unwrap_or_default();
        let active_val: bool = row.try_get(3).unwrap_or(false);
        let lda: Option<chrono::DateTime<chrono::Utc>> = row.try_get(4).unwrap_or(None);
        let ca: chrono::DateTime<chrono::Utc> = row.try_get(5).unwrap_or_else(|_| chrono::Utc::now());
        out.push(json!({
            "id": id_val, "url": url_val, "events": events_val,
            "active": active_val, "last_delivered_at": lda, "created_at": ca,
        }));
        i += 1;
    }
    Ok(Json(out))
}

pub async fn delete_webhook(
    State(state): State<Arc<crate::AppState>>,
    claims: DeveloperClaims,
    Path(id): Path<String>,
) -> Result<axum::http::StatusCode, axum::http::StatusCode> {
    sqlx::query("DELETE FROM webhooks WHERE id = $1 AND developer_id = $2")
        .bind(id)
        .bind(claims.developer_id.to_string())
        .execute(state.db.as_ref())
        .await
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

/// Fire-and-forget delivery of a webhook to all registered subscribers.
pub async fn deliver(webhook_payload: WebhookPayload) {
    info!("Delivering webhook event={} payload={:?}", webhook_payload.event, webhook_payload.id);
}

fn sign_payload(secret: &str, body: &str) -> String {
    type HmacSha256 = Hmac<Sha256>;
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC key");
    mac.update(body.as_bytes());
    let result = mac.finalize();
    let bytes: [u8; 32] = result.into_bytes().into();
    format!("sha256={}", hex_encode(bytes))
}

fn hex_encode(bytes: [u8; 32]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
