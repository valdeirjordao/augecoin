//! Presence monitoring: staleness tiers and automatic PoA removal.
//!
//! The spec defines heartbeat staleness tiers: ≤2 min green (online), >2 min
//! yellow (warning), >5 min red (critical), >10 min removal from the PoA set.
//! The monitor runs periodically, records one unresolved alert per
//! (validator, tier) and, past the removal threshold, suspends the validator
//! through the node bridge (which signs the `validatordeactivate` op).

use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::Result;
use crate::validator::{ValidatorService, ValidatorStatus};

/// Staleness thresholds (seconds), matching the spec.
pub const ONLINE_SECS: i64 = 120;
pub const WARNING_SECS: i64 = 300;
pub const REMOVAL_SECS: i64 = 600;

/// Staleness tier derived from `last_seen`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresenceTier {
    Online,
    Warning,
    Critical,
    Removal,
}

impl PresenceTier {
    /// Alert kind persisted for this tier (online produces none).
    pub fn kind(self) -> Option<&'static str> {
        match self {
            PresenceTier::Online => None,
            PresenceTier::Warning => Some("offline_warning"),
            PresenceTier::Critical => Some("offline_critical"),
            PresenceTier::Removal => Some("poa_removal"),
        }
    }

    pub fn message(self) -> &'static str {
        match self {
            PresenceTier::Online => "validator online",
            PresenceTier::Warning => "validator heartbeat stale (>2 min)",
            PresenceTier::Critical => "validator heartbeat stale (>5 min)",
            PresenceTier::Removal => "validator heartbeat stale (>10 min); removing from PoA",
        }
    }
}

/// Pure tier classification from a heartbeat age in seconds.
pub fn tier_from_age(age_secs: i64) -> PresenceTier {
    if age_secs <= ONLINE_SECS {
        PresenceTier::Online
    } else if age_secs <= WARNING_SECS {
        PresenceTier::Warning
    } else if age_secs <= REMOVAL_SECS {
        PresenceTier::Critical
    } else {
        PresenceTier::Removal
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct MonitorReport {
    pub checked: u64,
    pub new_alerts: u64,
    pub removed: u64,
    pub removal_failed: u64,
}

#[derive(Clone)]
pub struct AlertRepo {
    pool: PgPool,
}

/// An alert row as served by `GET /alerts`.
#[derive(Debug, Clone, Serialize)]
pub struct Alert {
    pub id: Uuid,
    pub validator_id: Uuid,
    pub kind: String,
    pub message: Option<String>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct AlertRow {
    id: Uuid,
    validator_id: Uuid,
    kind: String,
    message: Option<String>,
    resolved_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
}

impl From<AlertRow> for Alert {
    fn from(r: AlertRow) -> Self {
        Alert {
            id: r.id,
            validator_id: r.validator_id,
            kind: r.kind,
            message: r.message,
            resolved_at: r.resolved_at,
            created_at: r.created_at,
        }
    }
}

impl AlertRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Alerts, unresolved first then newest-first.
    pub async fn list(&self, limit: i64) -> Result<Vec<Alert>> {
        let rows: Vec<AlertRow> = sqlx::query_as(
            r#"
            SELECT id, validator_id, kind, message, resolved_at, created_at
            FROM alerts
            ORDER BY (resolved_at IS NULL) DESC, created_at DESC
            LIMIT $1
            "#,
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(Alert::from).collect())
    }

    /// Open an unresolved alert of `kind` for the validator. Returns false when
    /// one is already open (dedup via the partial unique index).
    async fn open(&self, validator_id: Uuid, kind: &str, message: &str) -> Result<bool> {
        let result = sqlx::query(
            r#"
            INSERT INTO alerts (id, validator_id, kind, message)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (validator_id, kind) WHERE resolved_at IS NULL DO NOTHING
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(validator_id)
        .bind(kind)
        .bind(message)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    /// Resolve every open alert for a validator (it recovered).
    async fn resolve_all(&self, validator_id: Uuid) -> Result<()> {
        sqlx::query(
            "UPDATE alerts SET resolved_at = now() WHERE validator_id = $1 AND resolved_at IS NULL",
        )
        .bind(validator_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

#[derive(Clone)]
pub struct Monitor {
    validators: ValidatorService,
    alerts: AlertRepo,
}

impl Monitor {
    pub fn new(pool: PgPool, validators: ValidatorService) -> Self {
        Self {
            validators,
            alerts: AlertRepo::new(pool),
        }
    }

    /// One pass over active validators: classify staleness, record alerts, and
    /// suspend validators past the removal threshold.
    pub async fn tick(&self) -> Result<MonitorReport> {
        let validators = self.validators.list().await?;
        let now = Utc::now();
        let mut report = MonitorReport::default();

        for v in validators {
            if v.status != ValidatorStatus::Active {
                continue;
            }
            let Some(last_seen) = v.last_seen else {
                continue;
            };
            let age = now.signed_duration_since(last_seen).num_seconds();
            let tier = tier_from_age(age);
            report.checked += 1;

            match tier {
                PresenceTier::Online => {
                    self.alerts.resolve_all(v.id).await?;
                }
                PresenceTier::Warning | PresenceTier::Critical | PresenceTier::Removal => {
                    let kind = tier.kind().expect("non-online tier has a kind");
                    let opened = self.alerts.open(v.id, kind, tier.message()).await?;
                    if opened {
                        report.new_alerts += 1;
                    }

                    if tier == PresenceTier::Removal {
                        match self.validators.suspend(v.id, "monitor").await {
                            Ok(_) => report.removed += 1,
                            Err(e) => {
                                report.removal_failed += 1;
                                tracing::warn!(validator = %v.id, error = %e, "PoA removal failed");
                            }
                        }
                    }
                }
            }
        }

        Ok(report)
    }

    /// Run the monitor forever, ticking every `interval`.
    pub fn spawn(self, interval: Duration) {
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(interval);
            loop {
                tick.tick().await;
                match self.tick().await {
                    Ok(report)
                        if report.checked > 0 || report.new_alerts > 0 || report.removed > 0 =>
                    {
                        tracing::info!(
                            checked = report.checked,
                            new_alerts = report.new_alerts,
                            removed = report.removed,
                            removal_failed = report.removal_failed,
                            "monitor tick"
                        );
                    }
                    Ok(_) => {}
                    Err(e) => tracing::error!(error = %e, "monitor tick failed"),
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tier_classification_matches_spec() {
        assert_eq!(tier_from_age(0), PresenceTier::Online);
        assert_eq!(tier_from_age(120), PresenceTier::Online);
        assert_eq!(tier_from_age(121), PresenceTier::Warning);
        assert_eq!(tier_from_age(300), PresenceTier::Warning);
        assert_eq!(tier_from_age(301), PresenceTier::Critical);
        assert_eq!(tier_from_age(600), PresenceTier::Critical);
        assert_eq!(tier_from_age(601), PresenceTier::Removal);
        assert_eq!(tier_from_age(86_400), PresenceTier::Removal);
    }

    #[test]
    fn kinds_are_stable() {
        assert_eq!(PresenceTier::Online.kind(), None);
        assert_eq!(PresenceTier::Warning.kind(), Some("offline_warning"));
        assert_eq!(PresenceTier::Critical.kind(), Some("offline_critical"));
        assert_eq!(PresenceTier::Removal.kind(), Some("poa_removal"));
    }
}
