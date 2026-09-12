//! Shared data models for the gateway

use chrono::{DateTime, NaiveDate, Utc};
use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct ApiKeyRow {
    pub id: Uuid,
    pub developer_id: Uuid,
    pub key_prefix: String,
    pub key_hash: String,
    pub tier: String,
    pub label: String,
    pub scopes: Option<Vec<String>>,
    pub revoked: bool,
    pub last_used_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct DeveloperRow {
    pub id: Uuid,
    pub name: String,
    pub email: String,
    pub company: Option<String>,
    pub tier: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct UsageRow {
    pub developer_id: Uuid,
    pub period_start: NaiveDate,
    pub call_count: i64,
    pub last_method: Option<String>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct InvoiceRow {
    pub id: Uuid,
    pub developer_id: Uuid,
    pub period_start: NaiveDate,
    pub period_end: NaiveDate,
    pub total_augesat: i64,
    pub status: String,
    pub paid_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}
