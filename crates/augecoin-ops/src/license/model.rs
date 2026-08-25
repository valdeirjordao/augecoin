//! License domain model: plans, statuses and the persisted entity.

use std::fmt;
use std::str::FromStr;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::AppError;

/// Subscription plan. Determines the license duration at issuance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Plan {
    Monthly,
    Semiannual,
    Annual,
    Lifetime,
}

impl Plan {
    pub fn as_str(self) -> &'static str {
        match self {
            Plan::Monthly => "monthly",
            Plan::Semiannual => "semiannual",
            Plan::Annual => "annual",
            Plan::Lifetime => "lifetime",
        }
    }

    /// License validity in days. Deterministic, calendar-independent.
    pub fn duration_days(self) -> i64 {
        match self {
            Plan::Monthly => 30,
            Plan::Semiannual => 180,
            Plan::Annual => 365,
            // Effectively never expires; used for permanent/genesis operators.
            Plan::Lifetime => 36500,
        }
    }
}

impl FromStr for Plan {
    type Err = AppError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "monthly" => Ok(Plan::Monthly),
            "semiannual" => Ok(Plan::Semiannual),
            "annual" => Ok(Plan::Annual),
            "lifetime" => Ok(Plan::Lifetime),
            _ => Err(AppError::InvalidInput(format!("unknown plan: {s}"))),
        }
    }
}

impl fmt::Display for Plan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Lifecycle state of a license.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LicenseStatus {
    Active,
    Expired,
    Suspended,
    Revoked,
}

impl LicenseStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            LicenseStatus::Active => "active",
            LicenseStatus::Expired => "expired",
            LicenseStatus::Suspended => "suspended",
            LicenseStatus::Revoked => "revoked",
        }
    }

    /// Transitions allowed to be applied administratively (see `LicenseService`).
    pub fn can_transition_to(self, next: LicenseStatus) -> bool {
        use LicenseStatus::*;
        match (self, next) {
            (Revoked, _) => false, // revoked is terminal
            (Active, Expired) => true,
            (Active, Suspended) | (Active, Revoked) => true,
            (Expired, Suspended) | (Expired, Revoked) => true,
            (Suspended, Active) | (Suspended, Expired) | (Suspended, Revoked) => true,
            (Expired, Active) => false, // re-activation is a renewal, not a status flip
            (same, _) if same == next => false,
            _ => false,
        }
    }
}

impl FromStr for LicenseStatus {
    type Err = AppError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "active" => Ok(LicenseStatus::Active),
            "expired" => Ok(LicenseStatus::Expired),
            "suspended" => Ok(LicenseStatus::Suspended),
            "revoked" => Ok(LicenseStatus::Revoked),
            _ => Err(AppError::InvalidInput(format!("unknown status: {s}"))),
        }
    }
}

impl fmt::Display for LicenseStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A persisted license row. Never contains the plaintext key.
#[derive(Debug, Clone)]
pub struct License {
    pub id: Uuid,
    pub license_key_hash: String,
    pub license_key_prefix: String,
    pub user_id: Uuid,
    pub plan: Plan,
    pub augeid: Option<String>,
    pub machine_hash: Option<String>,
    pub public_key: Option<String>,
    pub ip: Option<String>,
    pub expires_at: DateTime<Utc>,
    pub status: LicenseStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// The public representation returned by the API (hash never leaks).
#[derive(Debug, Clone, Serialize)]
pub struct LicenseView {
    pub id: Uuid,
    pub license_key_prefix: String,
    pub user_id: Uuid,
    pub plan: Plan,
    pub augeid: Option<String>,
    pub machine_hash: Option<String>,
    pub public_key: Option<String>,
    pub ip: Option<String>,
    pub expires_at: DateTime<Utc>,
    pub status: LicenseStatus,
    pub created_at: DateTime<Utc>,
}

impl From<&License> for LicenseView {
    fn from(l: &License) -> Self {
        LicenseView {
            id: l.id,
            license_key_prefix: l.license_key_prefix.clone(),
            user_id: l.user_id,
            plan: l.plan,
            augeid: l.augeid.clone(),
            machine_hash: l.machine_hash.clone(),
            public_key: l.public_key.clone(),
            ip: l.ip.clone(),
            expires_at: l.expires_at,
            status: l.status,
            created_at: l.created_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_durations() {
        assert_eq!(Plan::Monthly.duration_days(), 30);
        assert_eq!(Plan::Semiannual.duration_days(), 180);
        assert_eq!(Plan::Annual.duration_days(), 365);
    }

    #[test]
    fn plan_roundtrip() {
        assert_eq!("monthly".parse::<Plan>().unwrap(), Plan::Monthly);
        assert_eq!(Plan::Annual.to_string(), "annual");
        assert!("daily".parse::<Plan>().is_err());
    }

    #[test]
    fn status_transition_rules() {
        use LicenseStatus::*;
        assert!(Active.can_transition_to(Suspended));
        assert!(Active.can_transition_to(Revoked));
        assert!(Active.can_transition_to(Expired));
        assert!(Suspended.can_transition_to(Active));
        assert!(!Revoked.can_transition_to(Active)); // terminal
        assert!(!Expired.can_transition_to(Active)); // needs renewal
        assert!(!Active.can_transition_to(Active));
    }

    #[test]
    fn status_roundtrip() {
        assert_eq!(
            "revoked".parse::<LicenseStatus>().unwrap(),
            LicenseStatus::Revoked
        );
        assert_eq!(LicenseStatus::Suspended.to_string(), "suspended");
    }
}
