//! Release (OTA) domain model.

use std::str::FromStr;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::AppError;

/// Target platform of an installer artifact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    Windows,
    Linux,
}

impl Platform {
    pub fn as_str(self) -> &'static str {
        match self {
            Platform::Windows => "windows",
            Platform::Linux => "linux",
        }
    }
}

impl FromStr for Platform {
    type Err = AppError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "windows" => Ok(Platform::Windows),
            "linux" => Ok(Platform::Linux),
            _ => Err(AppError::InvalidInput(format!("unknown platform: {s}"))),
        }
    }
}

impl std::fmt::Display for Platform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Validate a BLAKE3-256 artifact hash (64 hex chars).
pub fn validate_artifact_hash(s: &str) -> Result<(), AppError> {
    if s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(AppError::InvalidInput(
            "artifact_hash must be a 64-character hex string (BLAKE3-256)".into(),
        ))
    }
}

/// Validate an Ed25519 signature (128 hex chars = 64 bytes).
pub fn validate_signature(s: &str) -> Result<(), AppError> {
    if s.len() == 128 && s.bytes().all(|b| b.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(AppError::InvalidInput(
            "signature must be a 128-character hex string (Ed25519)".into(),
        ))
    }
}

/// A persisted release row.
#[derive(Debug, Clone)]
pub struct Release {
    pub id: Uuid,
    pub version: String,
    pub platform: Platform,
    pub artifact_url: Option<String>,
    pub artifact_hash: String,
    pub signature: String,
    pub notes: Option<String>,
    pub published_at: DateTime<Utc>,
}

/// Public representation returned by `GET /updates`.
#[derive(Debug, Clone, Serialize)]
pub struct ReleaseView {
    pub version: String,
    pub platform: Platform,
    pub artifact_url: Option<String>,
    pub artifact_hash: String,
    pub signature: String,
    pub notes: Option<String>,
    pub published_at: DateTime<Utc>,
}

impl From<&Release> for ReleaseView {
    fn from(r: &Release) -> Self {
        ReleaseView {
            version: r.version.clone(),
            platform: r.platform,
            artifact_url: r.artifact_url.clone(),
            artifact_hash: r.artifact_hash.clone(),
            signature: r.signature.clone(),
            notes: r.notes.clone(),
            published_at: r.published_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_roundtrip() {
        assert_eq!("linux".parse::<Platform>().unwrap(), Platform::Linux);
        assert_eq!(Platform::Windows.to_string(), "windows");
        assert!("macos".parse::<Platform>().is_err());
    }

    #[test]
    fn hash_and_signature_validation() {
        assert!(validate_artifact_hash(&"ab".repeat(32)).is_ok());
        assert!(validate_artifact_hash("short").is_err());
        assert!(validate_signature(&"cd".repeat(64)).is_ok());
        assert!(validate_signature("not-a-sig").is_err());
    }
}
