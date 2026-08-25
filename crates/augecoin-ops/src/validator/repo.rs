//! Persistence for the validator domain (sqlx, runtime queries).
//!
//! Mirrors the license repo's conventions: TEXT columns decoded to typed enums
//! via `FromStr`, CHECK constraints at the DB layer, and every lifecycle
//! mutation recorded as an append-only event inside the same transaction.
//!
//! Every read JOINs `licenses` to surface `augeid`/`ip` from their single
//! source of truth; `validators` itself stores no binding data.

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use super::model::{Validator, ValidatorStatus};
use crate::error::{AppError, Result};

#[derive(Debug, Clone)]
pub struct NewValidator {
    pub id: Uuid,
    pub license_id: Uuid,
    pub public_key: String,
    pub os: Option<String>,
    pub cpu: Option<i32>,
    pub ram: Option<i32>,
    pub version: Option<String>,
}

/// A heartbeat sample together with the resulting validator row state.
#[derive(Debug, Clone)]
pub struct HeartbeatUpdate {
    pub cpu: Option<i32>,
    pub ram: Option<i32>,
    pub block: Option<i64>,
    pub uptime: i64,
}

/// Joined row: validator columns plus `augeid`/`ip` from the license.
#[derive(sqlx::FromRow)]
struct ValidatorRow {
    id: Uuid,
    license_id: Uuid,
    public_key: String,
    node_validator_id: Option<i64>,
    os: Option<String>,
    cpu: Option<i32>,
    ram: Option<i32>,
    version: Option<String>,
    uptime: i64,
    blocks: i64,
    blocks_lost: i64,
    leadership: i64,
    total_rewards: i64,
    last_seen: Option<DateTime<Utc>>,
    status: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl ValidatorRow {
    fn into_validator(self) -> Result<Validator> {
        Ok(Validator {
            id: self.id,
            license_id: self.license_id,
            public_key: self.public_key,
            node_validator_id: self.node_validator_id,
            os: self.os,
            cpu: self.cpu,
            ram: self.ram,
            version: self.version,
            uptime: self.uptime,
            blocks: self.blocks,
            blocks_lost: self.blocks_lost,
            leadership: self.leadership,
            total_rewards: self.total_rewards,
            last_seen: self.last_seen,
            status: self.status.parse::<ValidatorStatus>()?,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

#[derive(Clone)]
pub struct ValidatorRepo {
    pool: PgPool,
}

impl ValidatorRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Insert a validator and record its `registered` event atomically.
    pub async fn insert(&self, new: NewValidator) -> Result<Validator> {
        let mut tx = self.pool.begin().await?;

        let row: ValidatorRow = sqlx::query_as(
            r#"
            INSERT INTO validators
                (id, license_id, public_key, os, cpu, ram, version)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING *
            "#,
        )
        .bind(new.id)
        .bind(new.license_id)
        .bind(&new.public_key)
        .bind(&new.os)
        .bind(new.cpu)
        .bind(new.ram)
        .bind(&new.version)
        .fetch_one(&mut *tx)
        .await?;

        sqlx::query(
            r#"
            INSERT INTO validator_events (validator_id, event, actor, data)
            VALUES ($1, 'registered', 'system', $2::jsonb)
            "#,
        )
        .bind(new.id)
        .bind(serde_json::json!({ "public_key": new.public_key }))
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        row.into_validator()
    }

    pub async fn find_by_id(&self, id: Uuid) -> Result<Option<Validator>> {
        let row: Option<ValidatorRow> = sqlx::query_as("SELECT * FROM validators WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        row.map(ValidatorRow::into_validator).transpose()
    }

    pub async fn find_by_license_id(&self, license_id: Uuid) -> Result<Option<Validator>> {
        let row: Option<ValidatorRow> =
            sqlx::query_as("SELECT * FROM validators WHERE license_id = $1")
                .bind(license_id)
                .fetch_optional(&self.pool)
                .await?;
        row.map(ValidatorRow::into_validator).transpose()
    }

    pub async fn find_by_public_key(&self, public_key: &str) -> Result<Option<Validator>> {
        let row: Option<ValidatorRow> =
            sqlx::query_as("SELECT * FROM validators WHERE public_key = $1")
                .bind(public_key)
                .fetch_optional(&self.pool)
                .await?;
        row.map(ValidatorRow::into_validator).transpose()
    }

    /// Resolve a validator by its on-chain id (set after `validatoradd`).
    pub async fn find_by_node_id(&self, node_validator_id: i64) -> Result<Option<Validator>> {
        let row: Option<ValidatorRow> =
            sqlx::query_as("SELECT * FROM validators WHERE node_validator_id = $1")
                .bind(node_validator_id)
                .fetch_optional(&self.pool)
                .await?;
        row.map(ValidatorRow::into_validator).transpose()
    }

    pub async fn list(&self) -> Result<Vec<Validator>> {
        let rows: Vec<ValidatorRow> =
            sqlx::query_as("SELECT * FROM validators ORDER BY created_at DESC")
                .fetch_all(&self.pool)
                .await?;
        rows.into_iter().map(ValidatorRow::into_validator).collect()
    }

    pub async fn list_by_license_id(&self, license_id: Uuid) -> Result<Vec<Validator>> {
        let rows: Vec<ValidatorRow> = sqlx::query_as(
            "SELECT * FROM validators WHERE license_id = $1 ORDER BY created_at DESC",
        )
        .bind(license_id)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(ValidatorRow::into_validator).collect()
    }

    /// Record a heartbeat: update the validator's live metrics and append a
    /// heartbeat sample, atomically. Heartbeats are high-frequency and are *not*
    /// written to `validator_events`.
    pub async fn record_heartbeat(
        &self,
        validator_id: Uuid,
        update: HeartbeatUpdate,
    ) -> Result<Validator> {
        let mut tx = self.pool.begin().await?;

        let row: Option<ValidatorRow> = sqlx::query_as(
            r#"
            UPDATE validators
            SET uptime     = $1,
                cpu        = $2,
                ram        = $3,
                blocks     = $4,
                last_seen  = now(),
                updated_at = now()
            WHERE id = $5
            RETURNING *
            "#,
        )
        .bind(update.uptime)
        .bind(update.cpu)
        .bind(update.ram)
        .bind(update.block)
        .bind(validator_id)
        .fetch_optional(&mut *tx)
        .await?;

        let row = row.ok_or(AppError::NotFound)?;

        sqlx::query(
            r#"
            INSERT INTO heartbeats (id, validator_id, cpu, ram, block, uptime)
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(validator_id)
        .bind(update.cpu)
        .bind(update.ram)
        .bind(update.block)
        .bind(update.uptime)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        row.into_validator()
    }

    /// Transition a validator to `status`, recording the given audit event.
    pub async fn set_status(
        &self,
        id: Uuid,
        status: ValidatorStatus,
        event: &str,
        actor: &str,
        data: serde_json::Value,
    ) -> Result<Validator> {
        let mut tx = self.pool.begin().await?;

        let row: Option<ValidatorRow> = sqlx::query_as(
            r#"
            UPDATE validators
            SET status = $1, updated_at = now()
            WHERE id = $2
            RETURNING *
            "#,
        )
        .bind(status.as_str())
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;

        let row = row.ok_or(AppError::NotFound)?;

        sqlx::query(
            r#"
            INSERT INTO validator_events (validator_id, event, actor, data)
            VALUES ($1, $2, $3, $4::jsonb)
            "#,
        )
        .bind(id)
        .bind(event)
        .bind(actor)
        .bind(data)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        row.into_validator()
    }

    /// Set the on-chain validator id returned after `validatoradd` succeeded.
    pub async fn set_node_id(&self, id: Uuid, node_validator_id: i64) -> Result<Validator> {
        let row: Option<ValidatorRow> = sqlx::query_as(
            r#"
            UPDATE validators
            SET node_validator_id = $1, updated_at = now()
            WHERE id = $2
            RETURNING *
            "#,
        )
        .bind(node_validator_id)
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        row.ok_or(AppError::NotFound)?.into_validator()
    }

    /// Increment a validator's mirrored on-chain production counters as a block
    /// it led is attributed by the reward sync. Also refreshes `last_seen` so the
    /// validator derives as online from its chain activity.
    pub async fn record_production(&self, id: Uuid, auge_augesat: i64) -> Result<Validator> {
        let row: Option<ValidatorRow> = sqlx::query_as(
            r#"
            UPDATE validators
            SET blocks = blocks + 1,
                leadership = leadership + 1,
                total_rewards = total_rewards + $1,
                last_seen = now(),
                updated_at = now()
            WHERE id = $2
            RETURNING *
            "#,
        )
        .bind(auge_augesat)
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        row.ok_or(AppError::NotFound)?.into_validator()
    }

    /// Refresh the reported system profile (os/cpu/ram/version) on re-activation.
    pub async fn update_system(
        &self,
        id: Uuid,
        os: Option<String>,
        cpu: Option<i32>,
        ram: Option<i32>,
        version: Option<String>,
    ) -> Result<Validator> {
        let row: Option<ValidatorRow> = sqlx::query_as(
            r#"
            UPDATE validators
            SET os = $1, cpu = $2, ram = $3, version = $4, updated_at = now()
            WHERE id = $5
            RETURNING *
            "#,
        )
        .bind(os)
        .bind(cpu)
        .bind(ram)
        .bind(version)
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        row.ok_or(AppError::NotFound)?.into_validator()
    }

    /// Delete heartbeat samples older than `before`. Returns rows removed.
    pub async fn prune_heartbeats(&self, before: DateTime<Utc>) -> Result<u64> {
        let result = sqlx::query("DELETE FROM heartbeats WHERE created_at < $1")
            .bind(before)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected())
    }

    /// Refresh mirrored on-chain metrics (blocks/lost/leadership/rewards). These
    /// are reported by a node-sync job; the blockchain stays the authority.
    pub async fn set_metrics(
        &self,
        id: Uuid,
        blocks: i64,
        blocks_lost: i64,
        leadership: i64,
        total_rewards: i64,
    ) -> Result<Validator> {
        let row: Option<ValidatorRow> = sqlx::query_as(
            r#"
            UPDATE validators
            SET blocks = $1, blocks_lost = $2, leadership = $3, total_rewards = $4,
                updated_at = now()
            WHERE id = $5
            RETURNING *
            "#,
        )
        .bind(blocks)
        .bind(blocks_lost)
        .bind(leadership)
        .bind(total_rewards)
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        row.ok_or(AppError::NotFound)?.into_validator()
    }
}
