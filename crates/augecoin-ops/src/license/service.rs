//! License use-cases (application layer).

use chrono::{Duration, Utc};
use serde::Serialize;
use uuid::Uuid;

use super::key::LicenseKey;
use super::model::{License, LicenseStatus, LicenseView, Plan};
use super::repo::{LicenseRepo, NewLicense};
use crate::error::{AppError, Result};

/// A freshly issued license together with its one-time plaintext key.
#[derive(Debug, Serialize)]
pub struct IssuedLicense {
    pub license: LicenseView,
    /// Plaintext license key. Shown exactly once at issuance; never persisted.
    pub license_key: String,
}

/// Result of validating a license key.
#[derive(Debug, Serialize)]
pub struct Validation {
    pub valid: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub license: Option<LicenseView>,
}

#[derive(Clone)]
pub struct LicenseService {
    repo: LicenseRepo,
}

impl LicenseService {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self {
            repo: LicenseRepo::new(pool),
        }
    }

    /// Issue a license for `user_id` under `plan`. Returns the full key once.
    pub async fn issue(&self, user_id: Uuid, plan: Plan) -> Result<IssuedLicense> {
        let key = LicenseKey::generate();
        let expires_at = Utc::now() + Duration::days(plan.duration_days());
        let license = self
            .repo
            .insert(NewLicense {
                id: Uuid::new_v4(),
                license_key_hash: key.hash_hex(),
                license_key_prefix: key.prefix().to_string(),
                user_id,
                plan,
                expires_at,
            })
            .await?;

        Ok(IssuedLicense {
            license: LicenseView::from(&license),
            license_key: key.display(),
        })
    }

    pub async fn get(&self, id: Uuid) -> Result<License> {
        self.repo.find_by_id(id).await?.ok_or(AppError::NotFound)
    }

    /// Resolve a license by its plaintext key (for activation/heartbeat). Does
    /// not leak whether the key exists: unknown or malformed keys map to
    /// [`AppError::InvalidLicenseKey`], a uniform `404`-safe error.
    pub async fn resolve(&self, key: &str) -> Result<License> {
        let parsed = LicenseKey::parse(key).map_err(|_| AppError::InvalidLicenseKey)?;
        self.repo
            .find_by_key_hash(&parsed.hash_hex())
            .await?
            .ok_or(AppError::InvalidLicenseKey)
    }

    /// Effective status of a license, deriving `expired` from `expires_at`.
    pub fn status_of(&self, license: &License) -> LicenseStatus {
        effective_status(license)
    }

    /// Bind machine/public-key/AUGEID/IP onto a license (first activation).
    pub async fn bind(
        &self,
        id: Uuid,
        machine_hash: &str,
        public_key: &str,
        augeid: Option<&str>,
        ip: Option<&str>,
        actor: &str,
    ) -> Result<License> {
        self.repo
            .bind(id, machine_hash, public_key, augeid, ip, actor)
            .await
    }

    pub async fn list(&self) -> Result<Vec<License>> {
        self.repo.list().await
    }

    pub async fn list_by_user(&self, user_id: Uuid) -> Result<Vec<License>> {
        self.repo.find_by_user_id(user_id).await
    }

    /// Validate a license key without leaking whether it exists. `valid` is
    /// true only when the key maps to a license whose effective status is
    /// `active` (i.e. not expired by date, not suspended, not revoked).
    pub async fn validate(&self, key: &str) -> Result<Validation> {
        let parsed = match LicenseKey::parse(key) {
            Ok(k) => k,
            Err(_) => {
                return Ok(Validation {
                    valid: false,
                    license: None,
                })
            }
        };

        let license = self.repo.find_by_key_hash(&parsed.hash_hex()).await?;
        match license {
            None => Ok(Validation {
                valid: false,
                license: None,
            }),
            Some(l) => {
                let valid = effective_status(&l) == LicenseStatus::Active;
                Ok(Validation {
                    valid,
                    license: Some(LicenseView::from(&l)),
                })
            }
        }
    }

    pub async fn suspend(&self, id: Uuid, actor: &str) -> Result<License> {
        let current = self.repo.find_by_id(id).await?.ok_or(AppError::NotFound)?;
        if !current.status.can_transition_to(LicenseStatus::Suspended) {
            return Err(AppError::InvalidTransition(
                "license cannot be suspended from its current state",
            ));
        }
        self.repo
            .set_status(id, LicenseStatus::Suspended, "suspended", actor)
            .await
    }

    pub async fn revoke(&self, id: Uuid, actor: &str) -> Result<License> {
        let current = self.repo.find_by_id(id).await?.ok_or(AppError::NotFound)?;
        if !current.status.can_transition_to(LicenseStatus::Revoked) {
            return Err(AppError::InvalidTransition(
                "license cannot be revoked from its current state",
            ));
        }
        self.repo
            .set_status(id, LicenseStatus::Revoked, "revoked", actor)
            .await
    }
}

/// Expiration is derived from `expires_at`, not stored as a discrete status, so
/// a license left in `active` past its expiry is reported as `expired`.
fn effective_status(l: &License) -> LicenseStatus {
    if l.status == LicenseStatus::Active && l.expires_at <= Utc::now() {
        LicenseStatus::Expired
    } else {
        l.status
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effective_status_flips_active_to_expired_when_past_due() {
        let past = Utc::now() - Duration::hours(1);
        let future = Utc::now() + Duration::hours(1);
        let base = |expires_at, status| License {
            id: Uuid::new_v4(),
            license_key_hash: "h".into(),
            license_key_prefix: "AAAA".into(),
            user_id: Uuid::new_v4(),
            plan: Plan::Monthly,
            augeid: None,
            machine_hash: None,
            public_key: None,
            ip: None,
            expires_at,
            status,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        assert_eq!(
            effective_status(&base(future, LicenseStatus::Active)),
            LicenseStatus::Active
        );
        assert_eq!(
            effective_status(&base(past, LicenseStatus::Active)),
            LicenseStatus::Expired
        );
        // Suspended/revoked are not derived from time.
        assert_eq!(
            effective_status(&base(past, LicenseStatus::Suspended)),
            LicenseStatus::Suspended
        );
    }
}
