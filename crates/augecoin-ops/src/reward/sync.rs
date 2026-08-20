//! Rewards sync worker.
//!
//! Walks the chain from a persisted cursor to the node tip, attributing each
//! block's reward (reward + fees) and newly-emitted AUGEIDs to the validator
//! that led it (matched by `leader_id` → `validators.node_validator_id`).
//!
//! Idempotent by construction: rows carry `UNIQUE(validator_id, block_number)`
//! and are inserted with `ON CONFLICT DO NOTHING`, so a crash mid-batch only
//! re-walks (and re-skips) already-processed blocks.

use std::time::Duration;

use chrono::{DateTime, Utc};
use sqlx::PgPool;

use super::model::NewReward;
use super::repo::RewardRepo;
use crate::error::Result;
use crate::node::NodeClient;
use crate::validator::ValidatorRepo;

/// Consensus rule: every block emits exactly this many AUGEIDs to the leader.
const AUGEIDS_PER_BLOCK: i32 = 10;
/// Default batch size per sync tick.
const DEFAULT_BATCH: u64 = 500;
/// Cursor key in the `sync_state` table.
const CURSOR_KEY: &str = "reward_last_block";

/// Outcome of a single sync tick (for logs and observability).
#[derive(Debug, Clone, Copy, Default)]
pub struct SyncReport {
    pub processed: u64,
    pub attributed: u64,
    pub skipped: u64,
    pub tip: u64,
}

#[derive(Clone)]
pub struct RewardSync {
    node: NodeClient,
    rewards: RewardRepo,
    validators: ValidatorRepo,
    pool: PgPool,
}

impl RewardSync {
    pub fn new(
        node: NodeClient,
        rewards: RewardRepo,
        validators: ValidatorRepo,
        pool: PgPool,
    ) -> Self {
        Self {
            node,
            rewards,
            validators,
            pool,
        }
    }

    /// Process up to `max_blocks` blocks behind the tip, resuming from the
    /// persisted cursor. Returns a report for observability.
    pub async fn sync_once(&self, max_blocks: u64) -> Result<SyncReport> {
        let cursor = self.read_cursor().await?;
        let tip = self.node.block_count().await?;
        if tip as i64 <= cursor {
            return Ok(SyncReport {
                tip,
                ..Default::default()
            });
        }

        let start = cursor + 1;
        let end = (start + max_blocks as i64 - 1).min(tip as i64);

        let mut report = SyncReport {
            tip,
            ..Default::default()
        };

        for height in start..=end {
            let block = self.node.get_block(height as u64).await?;
            let auge = block.reward.saturating_add(block.fee) as i64;
            let block_ts =
                DateTime::<Utc>::from_timestamp(block.timestamp as i64, 0).unwrap_or_else(Utc::now);

            match self
                .validators
                .find_by_node_id(block.leader_id as i64)
                .await?
            {
                Some(validator) => {
                    let inserted = self
                        .rewards
                        .insert(NewReward {
                            validator_id: validator.id,
                            block_number: height,
                            leader_id: block.leader_id as i64,
                            auge,
                            fees: block.fee as i64,
                            augeids: AUGEIDS_PER_BLOCK,
                            block_ts,
                        })
                        .await?;
                    if inserted {
                        report.attributed += 1;
                    }
                }
                // A leader not present in the SaaS (e.g. genesis validators)
                // has no row to attribute; it is skipped, not an error.
                None => report.skipped += 1,
            }

            report.processed += 1;
            self.write_cursor(height).await?;
        }

        Ok(report)
    }

    /// Run the sync loop forever, ticking every `interval`.
    pub fn spawn(self, interval: Duration) {
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(interval);
            loop {
                tick.tick().await;
                match self.sync_once(DEFAULT_BATCH).await {
                    Ok(report) if report.processed > 0 => {
                        tracing::info!(
                            processed = report.processed,
                            attributed = report.attributed,
                            skipped = report.skipped,
                            tip = report.tip,
                            "reward sync tick"
                        );
                    }
                    Ok(_) => {}
                    Err(e) => {
                        tracing::error!(error = %e, "reward sync failed");
                    }
                }
            }
        });
    }

    async fn read_cursor(&self) -> Result<i64> {
        let row: Option<(String,)> = sqlx::query_as("SELECT value FROM sync_state WHERE key = $1")
            .bind(CURSOR_KEY)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.and_then(|(v,)| v.parse::<i64>().ok()).unwrap_or(-1))
    }

    async fn write_cursor(&self, block: i64) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO sync_state (key, value, updated_at)
            VALUES ($1, $2, now())
            ON CONFLICT (key) DO UPDATE SET value = $2, updated_at = now()
            "#,
        )
        .bind(CURSOR_KEY)
        .bind(block.to_string())
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
