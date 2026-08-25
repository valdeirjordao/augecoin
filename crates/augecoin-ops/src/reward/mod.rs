//! Reward domain: append-only mirror of on-chain rewards, period-bucketed
//! summaries, and the sync worker that keeps them fresh.

mod model;
mod repo;
mod sync;

pub use model::{NewReward, RewardBucket, RewardSummary};
pub use repo::RewardRepo;
pub use sync::{RewardSync, SyncReport};
