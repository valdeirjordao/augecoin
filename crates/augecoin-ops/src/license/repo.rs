//! Persistence for the license domain (sqlx, runtime queries).
//!
//! `plan` and `status` are TEXT columns in PostgreSQL; they are decoded to
//! strings here and converted to the typed enums via `FromStr`, which keeps the
//! SQL mapping simple while the database CHECK constraints still enforce
//! integrity at the storage layer.

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use super::model::{License, LicenseStatus, Plan};
use crate::error::{AppError, Result};

#[derive(Debug, Clone)]
pub struct NewLicense {
    pub id: Uuid,
    pub license_key_hash: String,
    pub license_key_prefix: String,
    pub user_id: Uuid,
    pub plan: Plan,
    pub expires_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct LicenseRow {
    id: Uuid,
    license_key_hash: String,
    license_key_prefix: String,
    user_id: Uuid,
    plan: String,
    augeid: Option<String>,
    machine_hash: Option<String>,
    public_key: Option<String>,
    ip: Option<String>,
    expires_at: DateTime<Utc>,
    status: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl LicenseRow {
    fn into_license(self) -> Result<License> {
        Ok(License {
            id: self.id,
            license_key_hash: self.license_key_hash,
            license_key_prefix: self.license_key_prefix,
            user_id: self.user_id,
            plan: self.plan.parse::<Plan>()?,
            augeid: self.augeid,
            machine_hash: self.machine_hash,
            public_key: self.public_key,
            ip: self.ip,
            expires_at: self.expires_at,
            status: self.status.parse::<LicenseStatus>()?,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

#[derive(Clone)]
pub struct LicenseRepo {
    pool: PgPool,
}

impl LicenseRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Insert a new license and record its `issued` event atomically.
    pub async fn insert(&self, new: NewLicense) -> Result<License> {
        let mut tx = self.pool.begin().await?;

        let row: LicenseRow = sqlx::query_as(
            r#"
            INSERT INTO licenses
                (id, license_key_hash, license_key_prefix, user_id, plan, expires_at)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING *
            "#,
        )
        .bind(new.id)
        .bind(&new.license_key_hash)
        .bind(&new.license_key_prefix)
        .bind(new.user_id)
        .bind(new.plan.as_str())
        .bind(new.expires_at)
        .fetch_one(&mut *tx)
        .await?;

        sqlx::query(
            r#"
            INSERT INTO license_events (license_id, event, actor, data)
            VALUES ($1, 'issued', $2, '{}'::jsonb)
            "#,
        )
        .bind(new.id)
        .bind("system")
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        row.into_license()
    }

    pub async fn find_by_id(&self, id: Uuid) -> Result<Option<License>> {
        let row: Option<LicenseRow> = sqlx::query_as("SELECT * FROM licenses WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        row.map(LicenseRow::into_license).transpose()
    }

    pub async fn find_by_key_hash(&self, hash_hex: &str) -> Result<Option<License>> {
        let row: Option<LicenseRow> =
            sqlx::query_as("SELECT * FROM licenses WHERE license_key_hash = $1")
                .bind(hash_hex)
                .fetch_optional(&self.pool)
                .await?;
        row.map(LicenseRow::into_license).transpose()
    }

    /// Transition a license to `status` and record the given audit event, atomically.
    pub async fn set_status(
        &self,
        id: Uuid,
        status: LicenseStatus,
        event: &str,
        actor: &str,
    ) -> Result<License> {
        let mut tx = self.pool.begin().await?;

        let row: Option<LicenseRow> = sqlx::query_as(
            r#"
            UPDATE licenses
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
            INSERT INTO license_events (license_id, event, actor, data)
            VALUES ($1, $2, $3, $4::jsonb)
            "#,
        )
        .bind(id)
        .bind(event)
        .bind(actor)
        .bind(serde_json::json!({ "status": status.as_str() }))
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        row.into_license()
    }

    pub async fn list(&self) -> Result<Vec<License>> {
        let rows: Vec<LicenseRow> =
            sqlx::query_as("SELECT * FROM licenses ORDER BY created_at DESC")
                .fetch_all(&self.pool)
                .await?;
        rows.into_iter().map(LicenseRow::into_license).collect()
    }

    pub async fn find_by_user_id(&self, user_id: Uuid) -> Result<Vec<License>> {
        let rows: Vec<LicenseRow> =
            sqlx::query_as("SELECT * FROM licenses WHERE user_id = $1 ORDER BY created_at DESC")
                .bind(user_id)
                .fetch_all(&self.pool)
                .await?;
        rows.into_iter().map(LicenseRow::into_license).collect()
    }

    /// Bind machine/public-key/AUGEID/IP onto a license and record the event.
    ///
    /// Called once on first activation. The machine hash and public key are the
    /// anti-cloning anchors: any later activation presenting a different value
    /// is rejected before this method is reached.
    pub async fn bind(
        &self,
        id: Uuid,
        machine_hash: &str,
        public_key: &str,
        augeid: Option<&str>,
        ip: Option<&str>,
        actor: &str,
    ) -> Result<License> {
        let mut tx = self.pool.begin().await?;

        let row: Option<LicenseRow> = sqlx::query_as(
            r#"
            UPDATE licenses
            SET machine_hash = $1, public_key = $2, augeid = COALESCE($3, augeid),
                ip = $4, updated_at = now()
            WHERE id = $5
            RETURNING *
            "#,
        )
        .bind(machine_hash)
        .bind(public_key)
        .bind(augeid)
        .bind(ip)
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;

        let row = row.ok_or(AppError::NotFound)?;

        sqlx::query(
            r#"
            INSERT INTO license_events (license_id, event, actor, data)
            VALUES ($1, 'activated', $2, $3::jsonb)
            "#,
        )
        .bind(id)
        .bind(actor)
        .bind(serde_json::json!({
            "public_key": public_key,
            "ip": ip,
        }))
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        row.into_license()
    }
}
