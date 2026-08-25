//! Persistence for the release (OTA) domain.

use sqlx::PgPool;
use uuid::Uuid;

use super::model::{Platform, Release};
use crate::error::Result;

#[derive(Debug, Clone)]
pub struct NewRelease {
    pub id: Uuid,
    pub version: String,
    pub platform: Platform,
    pub artifact_url: Option<String>,
    pub artifact_hash: String,
    pub signature: String,
    pub notes: Option<String>,
}

#[derive(sqlx::FromRow)]
struct ReleaseRow {
    id: Uuid,
    version: String,
    platform: String,
    artifact_url: Option<String>,
    artifact_hash: String,
    signature: String,
    notes: Option<String>,
    published_at: chrono::DateTime<chrono::Utc>,
}

impl ReleaseRow {
    fn into_release(self) -> Result<Release> {
        Ok(Release {
            id: self.id,
            version: self.version,
            platform: self.platform.parse::<Platform>()?,
            artifact_url: self.artifact_url,
            artifact_hash: self.artifact_hash,
            signature: self.signature,
            notes: self.notes,
            published_at: self.published_at,
        })
    }
}

#[derive(Clone)]
pub struct ReleaseRepo {
    pool: PgPool,
}

impl ReleaseRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn insert(&self, new: NewRelease) -> Result<Release> {
        let row: ReleaseRow = sqlx::query_as(
            r#"
            INSERT INTO releases
                (id, version, platform, artifact_url, artifact_hash, signature, notes)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING *
            "#,
        )
        .bind(new.id)
        .bind(&new.version)
        .bind(new.platform.as_str())
        .bind(&new.artifact_url)
        .bind(&new.artifact_hash)
        .bind(&new.signature)
        .bind(&new.notes)
        .fetch_one(&self.pool)
        .await?;
        row.into_release()
    }

    /// Latest release for a platform (the OTA candidate).
    pub async fn latest_for(&self, platform: Platform) -> Result<Option<Release>> {
        let row: Option<ReleaseRow> = sqlx::query_as(
            r#"
            SELECT * FROM releases
            WHERE platform = $1
            ORDER BY published_at DESC
            LIMIT 1
            "#,
        )
        .bind(platform.as_str())
        .fetch_optional(&self.pool)
        .await?;
        row.map(ReleaseRow::into_release).transpose()
    }

    pub async fn list(&self) -> Result<Vec<Release>> {
        let rows: Vec<ReleaseRow> =
            sqlx::query_as("SELECT * FROM releases ORDER BY published_at DESC")
                .fetch_all(&self.pool)
                .await?;
        rows.into_iter().map(ReleaseRow::into_release).collect()
    }

    pub async fn find_by_version_platform(
        &self,
        version: &str,
        platform: Platform,
    ) -> Result<Option<Release>> {
        let row: Option<ReleaseRow> =
            sqlx::query_as("SELECT * FROM releases WHERE version = $1 AND platform = $2")
                .bind(version)
                .bind(platform.as_str())
                .fetch_optional(&self.pool)
                .await?;
        row.map(ReleaseRow::into_release).transpose()
    }

    pub async fn find_by_id(&self, id: Uuid) -> Result<Option<Release>> {
        let row: Option<ReleaseRow> = sqlx::query_as("SELECT * FROM releases WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        row.map(ReleaseRow::into_release).transpose()
    }
}
