//! Reward domain model: the append-only mirror of on-chain rewards and the
//! period-bucketed summaries served to dashboards.

use serde::Serialize;

/// Reward metrics for a single time bucket.
#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct RewardBucket {
    /// Coins credited to the leader (block reward + fees), in augesat.
    #[serde(serialize_with = "crate::serde_util::i64_str::serialize")]
    pub auge: i64,
    /// Number of new AUGEIDs (accounts) owned by the leader.
    pub augeids: i64,
    /// Fees credited to the leader, in augesat (subset of `auge`).
    #[serde(serialize_with = "crate::serde_util::i64_str::serialize")]
    pub fees: i64,
    /// Number of blocks the validator led in this bucket.
    pub blocks: i64,
}

/// Period-bucketed reward summary for a validator or for the whole network.
#[derive(Debug, Clone, Default, Serialize)]
pub struct RewardSummary {
    pub hour: RewardBucket,
    pub day: RewardBucket,
    pub week: RewardBucket,
    pub month: RewardBucket,
    pub year: RewardBucket,
    pub total: RewardBucket,
}

/// A reward ledger row to persist (built by the sync worker).
#[derive(Debug, Clone)]
pub struct NewReward {
    pub validator_id: uuid::Uuid,
    pub block_number: i64,
    pub leader_id: i64,
    pub auge: i64,
    pub fees: i64,
    pub augeids: i32,
    pub block_ts: chrono::DateTime<chrono::Utc>,
}
