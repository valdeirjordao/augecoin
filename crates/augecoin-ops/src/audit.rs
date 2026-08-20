//! Unified audit trail over the immutable append-only event tables.
//!
//! Read-only. Nothing here mutates state; it merges `license_events`,
//! `validator_events` and `alerts` into a single time-ordered stream for the
//! operational panel's audit view.

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgPool;

use crate::error::Result;

/// A single merged audit entry.
#[derive(Debug, Clone, Serialize)]
pub struct AuditEntry {
    pub entity_type: String,
    pub entity_id: String,
    pub event: String,
    pub actor: String,
    pub data: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct AuditRow {
    entity_type: String,
    entity_id: String,
    event: String,
    actor: String,
    data: sqlx::types::Json<serde_json::Value>,
    created_at: DateTime<Utc>,
}

impl From<AuditRow> for AuditEntry {
    fn from(r: AuditRow) -> Self {
        AuditEntry {
            entity_type: r.entity_type,
            entity_id: r.entity_id,
            event: r.event,
            actor: r.actor,
            data: r.data.0,
            created_at: r.created_at,
        }
    }
}

#[derive(Clone)]
pub struct AuditRepo {
    pool: PgPool,
}

impl AuditRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Merged, newest-first audit stream with pagination.
    pub async fn list(&self, limit: i64, offset: i64) -> Result<Vec<AuditEntry>> {
        let rows: Vec<AuditRow> = sqlx::query_as(
            r#"
            SELECT * FROM (
                SELECT 'license'   AS entity_type, license_id::text   AS entity_id,
                       event, actor, data, created_at
                FROM license_events
                UNION ALL
                SELECT 'validator' AS entity_type, validator_id::text AS entity_id,
                       event, actor, data, created_at
                FROM validator_events
                UNION ALL
                SELECT 'alert'     AS entity_type, validator_id::text AS entity_id,
                       kind AS event, 'monitor' AS actor,
                       jsonb_build_object('message', COALESCE(message, '')) AS data,
                       created_at
                FROM alerts
            ) t
            ORDER BY created_at DESC
            LIMIT $1 OFFSET $2
            "#,
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(AuditEntry::from).collect())
    }
}
