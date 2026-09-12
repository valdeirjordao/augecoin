//! Validator domain model: lifecycle status, persisted entity and views.
//!
//! Presence (online/offline) is derived from `last_seen`, never stored — the
//! same discipline as the license domain's `effective_status`. The lifecycle
//! `status` field captures authorization state (pending/active/suspended/
//! revoked), while `online()` answers "is it currently heartbeating?".
//!
//! Binding data (machine hash, AUGEID, IP) lives on the license row; the
//! validator row carries only operational state and the public key.

use std::fmt;
use std::str::FromStr;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::AppError;

/// Lifecycle state of a validator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ValidatorStatus {
    /// Registered but not yet authorized on-chain (awaiting admin approval).
    Pending,
    /// Authorized on-chain; participates in consensus while heartbeating.
    Active,
    /// Administratively suspended (deactivated on-chain).
    Suspended,
    /// Removed from the validator set. Terminal.
    Revoked,
}

impl ValidatorStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            ValidatorStatus::Pending => "pending",
            ValidatorStatus::Active => "active",
            ValidatorStatus::Suspended => "suspended",
            ValidatorStatus::Revoked => "revoked",
        }
    }

    /// Transitions allowed to be applied administratively.
    pub fn can_transition_to(self, next: ValidatorStatus) -> bool {
        use ValidatorStatus::*;
        match (self, next) {
            (Revoked, _) => false, // revoked is terminal
            (Pending, Active) | (Pending, Revoked) => true,
            (Active, Suspended) | (Active, Revoked) => true,
            (Suspended, Active) | (Suspended, Revoked) => true,
            _ => false,
        }
    }
}

impl FromStr for ValidatorStatus {
    type Err = AppError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "pending" => Ok(ValidatorStatus::Pending),
            "active" => Ok(ValidatorStatus::Active),
            "suspended" => Ok(ValidatorStatus::Suspended),
            "revoked" => Ok(ValidatorStatus::Revoked),
            _ => Err(AppError::InvalidInput(format!("unknown status: {s}"))),
        }
    }
}

impl fmt::Display for ValidatorStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A persisted validator row. Holds no private material — only the public key
/// and mirrored on-chain metrics. Binding data (AUGEID, IP, machine hash) lives
/// on the license row and is surfaced in [`ValidatorView`] by the service layer.
#[derive(Debug, Clone)]
pub struct Validator {
    pub id: Uuid,
    pub license_id: Uuid,
    pub public_key: String,
    pub node_validator_id: Option<i64>,
    pub os: Option<String>,
    pub cpu: Option<i32>,
    pub ram: Option<i32>,
    pub version: Option<String>,
    pub uptime: i64,
    pub blocks: i64,
    pub blocks_lost: i64,
    pub leadership: i64,
    pub total_rewards: i64,
    pub last_seen: Option<DateTime<Utc>>,
    pub status: ValidatorStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Public representation returned by the API. `augeid`/`ip` come from the
/// joined license (single source of truth); `online` is derived from
/// `last_seen` against the heartbeat window at read time.
#[derive(Debug, Clone, Serialize)]
pub struct ValidatorView {
    pub id: Uuid,
    pub license_id: Uuid,
    pub public_key: String,
    pub augeid: Option<String>,
    pub ip: Option<String>,
    pub machine_hash: Option<String>,
    pub node_validator_id: Option<i64>,
    pub os: Option<String>,
    pub cpu: Option<i32>,
    pub ram: Option<i32>,
    pub version: Option<String>,
    pub uptime: i64,
    pub blocks: i64,
    pub blocks_lost: i64,
    pub leadership: i64,
    #[serde(serialize_with = "crate::serde_util::i64_str::serialize")]
    pub total_rewards: i64,
    pub last_seen: Option<DateTime<Utc>>,
    pub status: ValidatorStatus,
    pub online: bool,
    pub license_status: crate::license::LicenseStatus,
    pub license_expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

/// True when `s` is exactly 64 lowercase/uppercase hex characters (a BLAKE3-256
/// or Ed25519 public key encoded as hex).
pub fn is_hex64(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Validate a BLAKE3-256 machine hash as a 64-char hex string.
pub fn validate_machine_id(s: &str) -> Result<(), AppError> {
    if is_hex64(s) {
        Ok(())
    } else {
        Err(AppError::InvalidInput(
            "machine_id must be a 64-character hex string (BLAKE3-256)".into(),
        ))
    }
}

/// Validate an Ed25519 public key as a 64-char hex string (32 bytes).
pub fn validate_public_key(s: &str) -> Result<(), AppError> {
    if is_hex64(s) {
        Ok(())
    } else {
        Err(AppError::InvalidInput(
            "public_key must be a 64-character hex string (Ed25519)".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_roundtrip_and_transitions() {
        assert_eq!(
            "active".parse::<ValidatorStatus>().unwrap(),
            ValidatorStatus::Active
        );
        assert_eq!(ValidatorStatus::Pending.to_string(), "pending");

        use ValidatorStatus::*;
        assert!(Pending.can_transition_to(Active));
        assert!(Active.can_transition_to(Suspended));
        assert!(Active.can_transition_to(Revoked));
        assert!(Suspended.can_transition_to(Active));
        assert!(!Revoked.can_transition_to(Active));
        assert!(!Active.can_transition_to(Active));
        assert!(!Pending.can_transition_to(Suspended));
    }

    #[test]
    fn hex64_validation() {
        assert!(is_hex64("a".repeat(64).as_str()));
        assert!(is_hex64("A".repeat(64).as_str()));
        assert!(!is_hex64("g".repeat(64).as_str())); // non-hex
        assert!(!is_hex64("a".repeat(63).as_str())); // wrong length
        assert!(!is_hex64(""));
    }

    #[test]
    fn machine_and_pubkey_validators() {
        assert!(validate_machine_id(&"ab".repeat(32)).is_ok());
        assert!(validate_machine_id("short").is_err());
        assert!(validate_public_key(&"cd".repeat(32)).is_ok());
        assert!(validate_public_key("not-a-key").is_err());
    }
}
