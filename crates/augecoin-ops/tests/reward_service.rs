//! Integration tests for the reward ledger and its aggregation.
//!
//! These require a reachable PostgreSQL. Set `AUGECOIN_OPS_DATABASE_URL` to run
//! them; otherwise each test returns early. Serialized behind a lock; each test
//! truncates the reward/validator tables before running.

use chrono::Utc;
use tokio::sync::Mutex;
use uuid::Uuid;

use augecoin_ops::db::MIGRATOR;
use augecoin_ops::license::{LicenseService, Plan};
use augecoin_ops::reward::{NewReward, RewardRepo};
use augecoin_ops::validator::{ActivateInput, ValidatorService};

static DB_LOCK: Mutex<()> = Mutex::const_new(());

fn db_url() -> Option<String> {
    std::env::var("AUGECOIN_OPS_DATABASE_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
}

fn hex64(seed: u8) -> String {
    format!("{:02x}", seed).repeat(32)
}

struct TestCtx {
    svc: ValidatorService,
    licenses: LicenseService,
    rewards: RewardRepo,
    _guard: tokio::sync::MutexGuard<'static, ()>,
}

async fn setup() -> Option<TestCtx> {
    let url = db_url()?;
    let guard = DB_LOCK.lock().await;
    let pool = sqlx::PgPool::connect(&url).await.ok()?;
    MIGRATOR.run(&pool).await.ok()?;
    sqlx::query("TRUNCATE licenses, validators, validator_rewards, sync_state CASCADE")
        .execute(&pool)
        .await
        .ok()?;
    let licenses = LicenseService::new(pool.clone());
    let rewards = RewardRepo::new(pool.clone());
    let svc = ValidatorService::new(pool, licenses.clone(), rewards.clone(), None);
    Some(TestCtx {
        svc,
        licenses,
        rewards,
        _guard: guard,
    })
}

/// Issue a license and activate it, returning the validator id.
async fn activate_validator(ctx: &TestCtx, machine: u8, pubkey: u8) -> Uuid {
    let _issued = ctx
        .licenses
        .issue(Uuid::new_v4(), Plan::Annual, Some("AUGE1".into()))
        .await
        .unwrap();
    let resp = ctx
        .svc
        .activate(ActivateInput {
            license_key: None,
            augeid: Some("AUGE1".into()),
            machine_id: hex64(machine),
            public_key: hex64(pubkey),
            os: Some("linux".into()),
            cpu: Some(32),
            ram: Some(48),
            version: Some("1.0.0".into()),
            ip: Some("203.0.113.10".into()),
        })
        .await
        .unwrap();
    resp.validator.id
}

#[tokio::test]
async fn reward_ledger_aggregates_into_period_buckets() {
    let Some(ctx) = setup().await else { return };
    let vid = activate_validator(&ctx, 0x11, 0x22).await;

    // Attribute 2 blocks: 7.25 AUGE + fees each, 3 AUGEIDs each.
    let now = Utc::now();
    for (block, fee) in [(1i64, 1_000_000i64), (2i64, 2_000_000i64)] {
        ctx.rewards
            .insert(NewReward {
                validator_id: vid,
                block_number: block,
                leader_id: 42,
                auge: 725_000_000 + fee,
                fees: fee,
                augeids: 10,
                block_ts: now,
            })
            .await
            .unwrap();
    }

    let summary = ctx.rewards.summarize(vid).await.unwrap();
    assert_eq!(summary.total.auge, 2 * 725_000_000 + 3_000_000);
    assert_eq!(summary.total.fees, 3_000_000);
    assert_eq!(summary.total.augeids, 20);
    assert_eq!(summary.total.blocks, 2);
    // The 2 blocks happened "now", so they land in the hour/day buckets.
    assert_eq!(summary.hour.blocks, 2);
    assert_eq!(summary.day.blocks, 2);
}

#[tokio::test]
async fn reward_insert_is_idempotent() {
    let Some(ctx) = setup().await else { return };
    let vid = activate_validator(&ctx, 0x11, 0x22).await;

    let reward = NewReward {
        validator_id: vid,
        block_number: 1,
        leader_id: 42,
        auge: 725_000_000,
        fees: 0,
        augeids: 10,
        block_ts: Utc::now(),
    };

    assert!(ctx.rewards.insert(reward.clone()).await.unwrap());
    // Re-inserting the same block is a no-op.
    assert!(!ctx.rewards.insert(reward).await.unwrap());

    let summary = ctx.rewards.summarize(vid).await.unwrap();
    assert_eq!(summary.total.auge, 725_000_000);
    assert_eq!(summary.total.blocks, 1);
}

#[tokio::test]
async fn network_summary_aggregates_across_validators() {
    let Some(ctx) = setup().await else { return };
    for (m, p) in [(0x11u8, 0x22u8), (0x33u8, 0x44u8)] {
        let vid = activate_validator(&ctx, m, p).await;
        ctx.rewards
            .insert(NewReward {
                validator_id: vid,
                block_number: 1,
                leader_id: 42,
                auge: 725_000_000,
                fees: 0,
                augeids: 10,
                block_ts: Utc::now(),
            })
            .await
            .unwrap();
    }

    let net = ctx.rewards.network_summary().await.unwrap();
    assert_eq!(net.total.auge, 2 * 725_000_000);
    assert_eq!(net.total.blocks, 2);
    assert_eq!(net.total.augeids, 20);

    let totals = ctx.rewards.totals().await.unwrap();
    assert_eq!(totals.len(), 2);
    assert!(totals.values().all(|v| *v == 725_000_000));
}

#[tokio::test]
async fn empty_ledger_summarizes_to_zero() {
    let Some(ctx) = setup().await else { return };
    let vid = activate_validator(&ctx, 0x11, 0x22).await;

    let summary = ctx.rewards.summarize(vid).await.unwrap();
    assert_eq!(summary.total.auge, 0);
    assert_eq!(summary.total.augeids, 0);
    assert_eq!(summary.total.blocks, 0);
}
