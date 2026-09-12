//! Billing engine — tracks API usage per developer and calculates invoices.

use std::sync::Arc;

use axum::extract::{Json, State};
use axum::http::StatusCode;
use chrono::{DateTime, Datelike, NaiveDate, Utc};
use serde::Serialize;
use sqlx::PgPool;
use tracing::{info, warn};
use uuid::Uuid;

use crate::db::DbPool;

// ─── Types ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct DeveloperClaims {
    pub developer_id: Uuid,
    pub api_key_id: Option<Uuid>,
    pub tier: String,
    pub scopes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UsageStats {
    pub developer_id: Uuid,
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub calls_this_period: u64,
    pub quota: u64,
    pub overage_calls: u64,
    pub flat_fee_augesat: u64,
    pub overage_cost_augesat: u64,
    pub total_augesat: u64,
    pub tier: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct BillingSummary {
    pub current_period: UsageStats,
    pub previous_invoices: Vec<Invoice>,
    pub total_outstanding_augesat: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Invoice {
    pub id: String,
    pub period_start: NaiveDate,
    pub period_end: NaiveDate,
    pub calls: u64,
    pub total_augesat: u64,
    pub status: InvoiceStatus,
    pub paid_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InvoiceStatus {
    Pending,
    Paid,
    Overdue,
    Cancelled,
}

// ─── Helpers ────────────────────────────────────────────────────────────────

fn period_start_now() -> DateTime<Utc> {
    let now = Utc::now();
    NaiveDate::from_ymd_opt(now.year(), now.month(), 1)
        .expect("valid date")
        .and_hms_opt(0, 0, 0)
        .expect("valid time")
        .and_utc()
}

fn period_start_end(start: DateTime<Utc>) -> DateTime<Utc> {
    let mut y = start.year();
    let mut m = start.month() + 1;
    if m > 12 {
        m = 1;
        y += 1;
    }
    NaiveDate::from_ymd_opt(y, m, 1)
        .expect("valid date")
        .and_hms_opt(0, 0, 0)
        .expect("valid time")
        .and_utc()
}

// ─── BillingEngine ───────────────────────────────────────────────────────────

pub struct BillingEngine {
    db: Arc<DbPool>,
}

impl BillingEngine {
    pub fn new(db: Arc<DbPool>) -> Self {
        Self { db }
    }

    // ─── API key generation ─────────────────────────────────────────────────

    pub async fn generate_api_key(&self, developer_id: &str, label: &str) -> String {
        use rand::Rng;
        let full_key = format!(
            "aug_{}.{}",
            uuid::Uuid::new_v4().simple().to_string(),
            rand::thread_rng()
                .sample_iter(&rand::distributions::Alphanumeric)
                .take(32)
                .map(char::from)
                .collect::<String>()
        );

        let prefix = full_key.chars().take(12).collect::<String>();
        let key_hash = sha256(&full_key);
        let dev_uuid = Uuid::parse_str(developer_id).unwrap_or_else(|_| Uuid::new_v4());

        sqlx::query(
            r#"INSERT INTO api_keys (id, developer_id, key_prefix, key_hash, label, tier, created_at)
               VALUES ($1, $2, $3, $4, $5, 'free', now())"#,
        )
        .bind(Uuid::new_v4())
        .bind(dev_uuid)
        .bind(prefix)
        .bind(key_hash)
        .bind(label)
        .execute(self.db.as_ref())
        .await
        .expect("Failed to insert API key");

        info!("Generated API key for developer {} ({})", developer_id, label);
        full_key
    }

    pub async fn bootstrap_admin_key(&self, key: &str) {
        let key_hash = sha256(key);

        let exists: Option<String> = sqlx::query_scalar("SELECT id FROM api_keys WHERE key_hash = $1")
            .bind(&key_hash)
            .fetch_optional(self.db.as_ref())
            .await
            .ok()
            .flatten();

        if exists.is_none() {
            let dev_id = Uuid::new_v4();
            sqlx::query(
                r#"INSERT INTO developers (id, name, email, tier, created_at)
                   VALUES ($1, 'Admin (bootstrap)', 'admin@augecoin.io', 'enterprise', now())"#,
            )
            .bind(dev_id)
            .execute(self.db.as_ref())
            .await
            .expect("Bootstrap admin insert failed");

            sqlx::query(
                r#"INSERT INTO api_keys (id, developer_id, key_prefix, key_hash, label, tier, scopes, created_at)
                   VALUES ($1, $2, $3, $4, 'bootstrap-admin', 'enterprise', '["admin"]', now())"#,
            )
            .bind(Uuid::new_v4())
            .bind(dev_id)
            .bind("aug_admin")
            .bind(key_hash)
            .execute(self.db.as_ref())
            .await
            .expect("Bootstrap admin key insert failed");
            warn!("Bootstrapped admin API key (GATEWAY_ADMIN_BOOTSTRAP_KEY was set)");
        }
    }

    // ─── Usage tracking ──────────────────────────────────────────────────────

    pub async fn record_call(
        &self,
        developer_id: &Uuid,
        api_key_id: &Option<Uuid>,
        method: &str,
        tier: &str,
    ) {
        let period_start = period_start_now();

        sqlx::query(
            r#"INSERT INTO api_usage (developer_id, api_key_id, period_start, call_count, last_method, updated_at)
               VALUES ($1, $2, $3, 1, $4, now())
               ON CONFLICT (developer_id, period_start) DO UPDATE
               SET call_count = api_usage.call_count + 1, last_method = $4, updated_at = now()"#,
        )
        .bind(developer_id)
        .bind(api_key_id)
        .bind(period_start)
        .bind(method)
        .execute(self.db.as_ref())
        .await
        .ok();

        let quota = self.get_tier_quota(tier);
        let current_count: Option<i64> = sqlx::query_scalar(
            "SELECT call_count FROM api_usage WHERE developer_id = $1 AND period_start = $2",
        )
        .bind(developer_id)
        .bind(period_start)
        .fetch_optional(self.db.as_ref())
        .await
        .ok()
        .flatten()
        .unwrap_or(Some(0));

        let current_count = current_count.unwrap_or(0);

        if quota > 0 && current_count > quota as i64 {
            let overage_calls = current_count - quota as i64;
            let tier_info = self.lookup_tier(tier);
            let cost_augesat = tier_info.overage_price_augesat_per_call;

            sqlx::query(
                r#"INSERT INTO billing_events (id, developer_id, api_key_id, event_type, amount_augesat, overage_calls, method, created_at)
                   VALUES ($1, $2, $3, 'overage_charge', $4, $5, $6, now())"#,
            )
            .bind(Uuid::new_v4())
            .bind(developer_id)
            .bind(api_key_id)
            .bind(cost_augesat as i64)
            .bind(overage_calls)
            .bind(method)
            .execute(self.db.as_ref())
            .await
            .ok();
        }
    }

    pub async fn get_usage(&self, developer_id: &Uuid) -> Result<UsageStats, sqlx::Error> {
        let period_start = period_start_now();
        let period_end = period_start_end(period_start);

        let calls: Option<i64> = sqlx::query_scalar(
            r#"SELECT call_count FROM api_usage WHERE developer_id = $1 AND period_start = $2"#,
        )
        .bind(developer_id)
        .bind(period_start)
        .fetch_optional(self.db.as_ref())
        .await?;

        let calls = calls.unwrap_or(0) as u64;

        let tier: String = sqlx::query_scalar("SELECT tier FROM developers WHERE id = $1")
            .bind(developer_id)
            .fetch_one(self.db.as_ref())
            .await?;

        let tier_info = self.lookup_tier(&tier);
        let quota = tier_info.monthly_call_quota;
        let overage_calls = if quota > 0 && calls > quota { calls - quota } else { 0 };
        let overage_cost = overage_calls * tier_info.overage_price_augesat_per_call;
        let flat_fee = tier_info.monthly_price_augesat;

        Ok(UsageStats {
            developer_id: *developer_id,
            period_start,
            period_end,
            calls_this_period: calls,
            quota,
            overage_calls,
            flat_fee_augesat: flat_fee,
            overage_cost_augesat: overage_cost,
            total_augesat: flat_fee + overage_cost,
            tier,
        })
    }

    pub async fn get_summary(&self, developer_id: &Uuid) -> Result<BillingSummary, sqlx::Error> {
        let current = self.get_usage(developer_id).await?;

        let invoices: Vec<crate::models::InvoiceRow> = sqlx::query_as(
            r#"SELECT id, period_start, period_end, total_augesat, status, paid_at, created_at
               FROM invoices WHERE developer_id = $1 ORDER BY period_start DESC LIMIT 12"#,
        )
        .bind(developer_id)
        .fetch_all(self.db.as_ref())
        .await?;

        let outstanding: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(total_augesat), 0) FROM invoices WHERE developer_id = $1 AND status = 'pending'",
        )
        .bind(developer_id)
        .fetch_one(self.db.as_ref())
        .await?;

        Ok(BillingSummary {
            current_period: current,
            previous_invoices: invoices
                .into_iter()
                .map(|r| Invoice {
                    id: r.id.to_string(),
                    period_start: r.period_start,
                    period_end: r.period_end,
                    calls: 0,
                    total_augesat: r.total_augesat as u64,
                    status: match r.status.as_str() {
                        "paid" => InvoiceStatus::Paid,
                        "overdue" => InvoiceStatus::Overdue,
                        "cancelled" => InvoiceStatus::Cancelled,
                        _ => InvoiceStatus::Pending,
                    },
                    paid_at: r.paid_at,
                })
                .collect(),
            total_outstanding_augesat: outstanding,
        })
    }

    fn lookup_tier(&self, slug: &str) -> crate::rate_limit::Tier {
        crate::rate_limit::TIERS
            .iter()
            .find(|t| t.slug == slug)
            .copied()
            .unwrap_or(crate::rate_limit::TIERS[0])
    }

    fn get_tier_quota(&self, slug: &str) -> u64 {
        self.lookup_tier(slug).monthly_call_quota
    }
}

use sha2::{Digest, Sha256};

fn sha256(s: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(s.as_bytes());
    let result = hasher.finalize();
    hex::encode(result)
}

// ─── Axum handler functions ──────────────────��──────────────────────────────

pub async fn get_usage_stats(
    State(state): State<Arc<crate::AppState>>,
    claims: DeveloperClaims,
) -> Result<Json<UsageStats>, StatusCode> {
    let stats = state
        .billing
        .get_usage(&claims.developer_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(stats))
}

pub async fn get_billing_summary(
    State(state): State<Arc<crate::AppState>>,
    claims: DeveloperClaims,
) -> Result<Json<BillingSummary>, StatusCode> {
    let summary = state
        .billing
        .get_summary(&claims.developer_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(summary))
}
