//! Release use-cases (application layer).
//!
//! Publishing verifies the Ed25519 signature over the artifact hash against the
//! configured release public key, so a compromised admin credential cannot push
//! an unsigned (tampered) artifact. The release *private* key lives in the build
//! pipeline — never on this server. Clients re-verify the same signature with
//! their baked-in public key before installing.

use super::model::{validate_artifact_hash, validate_signature, Platform, Release, ReleaseView};
use super::repo::{NewRelease, ReleaseRepo};
use crate::error::{AppError, Result};

use ed25519_dalek::Verifier;

#[derive(Clone)]
pub struct ReleaseService {
    repo: ReleaseRepo,
    /// Ed25519 release public key (32 bytes), used to verify publish signatures.
    release_public_key: Option<[u8; 32]>,
}

impl ReleaseService {
    pub fn new(pool: sqlx::PgPool, release_public_key: Option<[u8; 32]>) -> Self {
        Self {
            repo: ReleaseRepo::new(pool),
            release_public_key,
        }
    }

    /// Publish a release after validating formats and, when a release public key
    /// is configured, verifying the signature over the artifact hash.
    pub async fn publish(&self, new: NewRelease) -> Result<Release> {
        validate_artifact_hash(&new.artifact_hash)?;
        validate_signature(&new.signature)?;

        let key = self
            .release_public_key
            .ok_or(AppError::ReleaseKeyNotConfigured)?;

        let hash_bytes: [u8; 32] = hex::decode(&new.artifact_hash)
            .map_err(|_| AppError::InvalidInput("invalid artifact_hash hex".into()))?
            .try_into()
            .map_err(|_| AppError::InvalidInput("invalid artifact_hash length".into()))?;
        let sig_bytes: [u8; 64] = hex::decode(&new.signature)
            .map_err(|_| AppError::InvalidInput("invalid signature hex".into()))?
            .try_into()
            .map_err(|_| AppError::InvalidInput("invalid signature length".into()))?;

        let verifying = ed25519_dalek::VerifyingKey::from_bytes(&key)
            .map_err(|_| AppError::InvalidInput("invalid release public key".into()))?;
        let signature = ed25519_dalek::Signature::from_bytes(&sig_bytes);

        // The signature covers the raw 32-byte artifact hash.
        if verifying.verify(&hash_bytes, &signature).is_err() {
            return Err(AppError::InvalidSignature);
        }

        self.repo.insert(new).await
    }

    /// Latest release for a platform (the OTA candidate), if any.
    pub async fn latest(&self, platform: Platform) -> Result<Option<ReleaseView>> {
        Ok(self
            .repo
            .latest_for(platform)
            .await?
            .map(|r| ReleaseView::from(&r)))
    }

    pub async fn list(&self) -> Result<Vec<ReleaseView>> {
        Ok(self
            .repo
            .list()
            .await?
            .iter()
            .map(ReleaseView::from)
            .collect())
    }
}
