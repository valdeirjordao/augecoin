//! Persistence for the reward ledger and its period-bucketed aggregates.
//!
//! Rows are appended idempotently (`ON CONFLICT DO NOTHING`); aggregates are
//! computed in SQL from the block timestamp (`block_ts`) so the dashboard reads
//! are a single indexed query per validator, with no in-process math to drift.

use sqlx::PgPool;

use super::model::{NewReward, RewardBucket, RewardSummary};
use crate::error::Result;

/// Period bucket names in the order they appear in the aggregate query.
const PERIODS: [&str; 5] = ["hour", "day", "week", "month", "year"];

fn period_condition(period: &str) -> &'static str {
    match period {
        "hour" => "block_ts >= now() - interval '1 hour'",
        "day" => "block_ts >= date_trunc('day', now())",
        "week" => "block_ts >= date_trunc('week', now())",
        "month" => "block_ts >= date_trunc('month', now())",
        "year" => "block_ts >= date_trunc('year', now())",
        _ => unreachable!("known period"),
    }
}

/// Build the 24-column aggregate SELECT (5 buckets × 4 metrics + total).
fn aggregate_select(where_clause: &str) -> String {
    let mut cols: Vec<String> = Vec::with_capacity(24);
    for p in PERIODS {
        let cond = period_condition(p);
        cols.push(format!(
            "COALESCE(SUM(auge) FILTER (WHERE {cond}), 0)::bigint AS {p}_auge"
        ));
        cols.push(format!(
            "COALESCE(SUM(augeids) FILTER (WHERE {cond}), 0)::bigint AS {p}_augeids"
        ));
        cols.push(format!(
            "COALESCE(SUM(fees) FILTER (WHERE {cond}), 0)::bigint AS {p}_fees"
        ));
        cols.push(format!(
            "COALESCE(COUNT(*) FILTER (WHERE {cond}), 0)::bigint AS {p}_blocks"
        ));
    }
    cols.push("COALESCE(SUM(auge), 0)::bigint AS total_auge".into());
    cols.push("COALESCE(SUM(augeids), 0)::bigint AS total_augeids".into());
    cols.push("COALESCE(SUM(fees), 0)::bigint AS total_fees".into());
    cols.push("COALESCE(COUNT(*), 0)::bigint AS total_blocks".into());
    format!(
        "SELECT {} FROM validator_rewards {where_clause}",
        cols.join(", ")
    )
}

#[derive(sqlx::FromRow)]
struct SummaryRow {
    hour_auge: i64,
    hour_augeids: i64,
    hour_fees: i64,
    hour_blocks: i64,
    day_auge: i64,
    day_augeids: i64,
    day_fees: i64,
    day_blocks: i64,
    week_auge: i64,
    week_augeids: i64,
    week_fees: i64,
    week_blocks: i64,
    month_auge: i64,
    month_augeids: i64,
    month_fees: i64,
    month_blocks: i64,
    year_auge: i64,
    year_augeids: i64,
    year_fees: i64,
    year_blocks: i64,
    total_auge: i64,
    total_augeids: i64,
    total_fees: i64,
    total_blocks: i64,
}

impl SummaryRow {
    fn into_summary(self) -> RewardSummary {
        RewardSummary {
            hour: RewardBucket {
                auge: self.hour_auge,
                augeids: self.hour_augeids,
                fees: self.hour_fees,
                blocks: self.hour_blocks,
            },
            day: RewardBucket {
                auge: self.day_auge,
                augeids: self.day_augeids,
                fees: self.day_fees,
                blocks: self.day_blocks,
            },
            week: RewardBucket {
                auge: self.week_auge,
                augeids: self.week_augeids,
                fees: self.week_fees,
                blocks: self.week_blocks,
            },
            month: RewardBucket {
                auge: self.month_auge,
                augeids: self.month_augeids,
                fees: self.month_fees,
                blocks: self.month_blocks,
            },
            year: RewardBucket {
                auge: self.year_auge,
                augeids: self.year_augeids,
                fees: self.year_fees,
                blocks: self.year_blocks,
            },
            total: RewardBucket {
                auge: self.total_auge,
                augeids: self.total_augeids,
                fees: self.total_fees,
                blocks: self.total_blocks,
            },
        }
    }
}

#[derive(Clone)]
pub struct RewardRepo {
    pool: PgPool,
}

impl RewardRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Append a reward row. Returns `false` when the row already existed (the
    /// UNIQUE(validator_id, block_number) guard), making re-sync idempotent.
    pub async fn insert(&self, reward: NewReward) -> Result<bool> {
        let result = sqlx::query(
            r#"
            INSERT INTO validator_rewards
                (id, validator_id, block_number, leader_id, auge, fees, augeids, block_ts)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (validator_id, block_number) DO NOTHING
            "#,
        )
        .bind(uuid::Uuid::new_v4())
        .bind(reward.validator_id)
        .bind(reward.block_number)
        .bind(reward.leader_id)
        .bind(reward.auge)
        .bind(reward.fees)
        .bind(reward.augeids)
        .bind(reward.block_ts)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    /// Period-bucketed summary for a single validator.
    pub async fn summarize(&self, validator_id: uuid::Uuid) -> Result<RewardSummary> {
        let sql = aggregate_select("WHERE validator_id = $1");
        let row: SummaryRow = sqlx::query_as(&sql)
            .bind(validator_id)
            .fetch_one(&self.pool)
            .await?;
        Ok(row.into_summary())
    }

    /// Period-bucketed summary across the whole network (operational KPIs).
    pub async fn network_summary(&self) -> Result<RewardSummary> {
        let sql = aggregate_select("");
        let row: SummaryRow = sqlx::query_as(&sql).fetch_one(&self.pool).await?;
        Ok(row.into_summary())
    }

    /// Total AUGE (augesat) attributed per validator, for list views.
    pub async fn totals(&self) -> Result<std::collections::HashMap<uuid::Uuid, i64>> {
        let rows: Vec<(uuid::Uuid, i64)> = sqlx::query_as(
            r#"
            SELECT validator_id, SUM(auge)::bigint
            FROM validator_rewards
            GROUP BY validator_id
            "#,
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().collect())
    }
}
