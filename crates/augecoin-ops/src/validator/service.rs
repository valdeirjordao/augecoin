//! Validator use-cases (application layer).
//!
//! This is the bridge between the SaaS (licensing, machine binding, heartbeat)
//! and consensus. Activation registers the validator and binds the license to a
//! single machine; approval submits a `validatoradd` through the node (which
//! signs it with the network master key). The service never holds a private key.

use chrono::{Duration, Utc};
use serde::Serialize;
use uuid::Uuid;

use super::model::{
    validate_machine_id, validate_public_key, Validator, ValidatorStatus, ValidatorView,
};
use super::repo::{HeartbeatUpdate, NewValidator, ValidatorRepo};
use crate::error::{AppError, Result};
use crate::license::{License, LicenseService, LicenseStatus, LicenseView};
use crate::node::NodeClient;
use crate::reward::{RewardRepo, RewardSummary};

/// Heartbeat cadence required by the spec (30 seconds).
pub const HEARTBEAT_INTERVAL_SECS: u64 = 30;
/// A validator is `online` while its last heartbeat is within this window.
pub const ONLINE_WINDOW_SECS: i64 = 120;

/// Activation request as submitted by the desktop app.
#[derive(Debug, Clone)]
pub struct ActivateInput {
    pub license_key: String,
    pub machine_id: String,
    pub public_key: String,
    pub augeid: Option<String>,
    pub os: Option<String>,
    pub cpu: Option<i32>,
    pub ram: Option<i32>,
    pub version: Option<String>,
    pub ip: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ActivationConfig {
    pub heartbeat_interval_seconds: u64,
    pub online_window_seconds: i64,
    pub node_rpc_url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ActivationResponse {
    pub validator: ValidatorView,
    pub config: ActivationConfig,
}

/// Heartbeat request as submitted every 30 seconds.
#[derive(Debug, Clone)]
pub struct HeartbeatInput {
    pub license_key: String,
    pub uptime: i64,
    pub cpu: Option<i32>,
    pub ram: Option<i32>,
    pub block: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HeartbeatResponse {
    pub authorized: bool,
    pub status: ValidatorStatus,
    pub online: bool,
    pub uptime: i64,
    pub block: i64,
}

/// Statistics served to the wallet for a specific license.
#[derive(Debug, Clone, Serialize)]
pub struct ValidatorStats {
    pub validator: ValidatorView,
    pub license: LicenseView,
    pub rewards: RewardSummary,
    pub heartbeat_interval_seconds: u64,
    pub online_window_seconds: i64,
}

#[derive(Clone)]
pub struct ValidatorService {
    repo: ValidatorRepo,
    licenses: LicenseService,
    rewards: RewardRepo,
    node: Option<NodeClient>,
}

impl ValidatorService {
    pub fn new(
        pool: sqlx::PgPool,
        licenses: LicenseService,
        rewards: RewardRepo,
        node: Option<NodeClient>,
    ) -> Self {
        Self {
            repo: ValidatorRepo::new(pool),
            licenses,
            rewards,
            node,
        }
    }

    /// Activate a license on a machine. Idempotent for the same machine: a
    /// re-activation refreshes system info without resetting the lifecycle.
    /// A different machine or public key is rejected (anti-cloning).
    pub async fn activate(&self, input: ActivateInput) -> Result<ActivationResponse> {
        validate_machine_id(&input.machine_id)?;
        validate_public_key(&input.public_key)?;

        let license = self.licenses.resolve(&input.license_key).await?;
        if self.licenses.status_of(&license) != LicenseStatus::Active {
            return Err(AppError::LicenseNotActive);
        }

        // Anti-cloning: the license may only ever bind to one machine and one
        // public key. A mismatch here means the key leaked or was cloned.
        if let Some(bound) = &license.machine_hash {
            if bound != &input.machine_id {
                return Err(AppError::MachineMismatch);
            }
        }
        if let Some(bound) = &license.public_key {
            if bound != &input.public_key {
                return Err(AppError::PublicKeyMismatch);
            }
        }

        // A public key may not be reused across licenses.
        if let Some(existing) = self.repo.find_by_public_key(&input.public_key).await? {
            if existing.license_id != license.id {
                return Err(AppError::PublicKeyMismatch);
            }
        }

        let validator = match self.repo.find_by_license_id(license.id).await? {
            Some(existing) => {
                self.repo
                    .update_system(existing.id, input.os, input.cpu, input.ram, input.version)
                    .await?
            }
            None => {
                self.licenses
                    .bind(
                        license.id,
                        &input.machine_id,
                        &input.public_key,
                        input.augeid.as_deref(),
                        input.ip.as_deref(),
                        "activation",
                    )
                    .await?;
                self.repo
                    .insert(NewValidator {
                        id: Uuid::new_v4(),
                        license_id: license.id,
                        public_key: input.public_key,
                        os: input.os,
                        cpu: input.cpu,
                        ram: input.ram,
                        version: input.version,
                    })
                    .await?
            }
        };

        // Re-read the license so the returned view reflects the binding applied
        // above (AUGEID/IP are surfaced from the license row).
        let license = self.licenses.get(license.id).await?;
        let view = self.to_view(&validator, &license);
        Ok(ActivationResponse {
            validator: view,
            config: ActivationConfig {
                heartbeat_interval_seconds: HEARTBEAT_INTERVAL_SECS,
                online_window_seconds: ONLINE_WINDOW_SECS,
                node_rpc_url: self.node.as_ref().map(|n| n.rpc_url().to_string()),
            },
        })
    }

    /// Record a heartbeat. The license key authenticates the call; the reply
    /// tells the client whether it is still authorized to validate.
    pub async fn heartbeat(&self, input: HeartbeatInput) -> Result<HeartbeatResponse> {
        if input.uptime < 0 {
            return Err(AppError::InvalidInput("uptime must be non-negative".into()));
        }

        let license = self.licenses.resolve(&input.license_key).await?;
        let validator = self
            .repo
            .find_by_license_id(license.id)
            .await?
            .ok_or(AppError::NotFound)?;

        let authorized = self.licenses.status_of(&license) == LicenseStatus::Active;
        let validator = self
            .repo
            .record_heartbeat(
                validator.id,
                HeartbeatUpdate {
                    cpu: input.cpu,
                    ram: input.ram,
                    block: input.block,
                    uptime: input.uptime,
                },
            )
            .await?;

        Ok(HeartbeatResponse {
            authorized,
            status: validator.status,
            online: true,
            uptime: validator.uptime,
            block: validator.blocks,
        })
    }

    /// Statistics for a license, served to the wallet portal.
    pub async fn stats_by_license(&self, license_key: &str) -> Result<ValidatorStats> {
        let license = self.licenses.resolve(license_key).await?;
        let validator = self
            .repo
            .find_by_license_id(license.id)
            .await?
            .ok_or(AppError::NotFound)?;

        Ok(ValidatorStats {
            validator: self.to_view(&validator, &license),
            license: LicenseView::from(&license),
            rewards: self.rewards.summarize(validator.id).await?,
            heartbeat_interval_seconds: HEARTBEAT_INTERVAL_SECS,
            online_window_seconds: ONLINE_WINDOW_SECS,
        })
    }

    pub async fn list(&self) -> Result<Vec<ValidatorView>> {
        let validators = self.repo.list().await?;
        self.build_views(validators).await
    }

    pub async fn list_by_license(&self, license_id: Uuid) -> Result<Vec<ValidatorView>> {
        let validators = self.repo.list_by_license_id(license_id).await?;
        self.build_views(validators).await
    }

    async fn build_views(&self, validators: Vec<Validator>) -> Result<Vec<ValidatorView>> {
        let licenses = self.licenses.list().await?;
        let by_id: std::collections::HashMap<Uuid, License> =
            licenses.into_iter().map(|l| (l.id, l)).collect();
        let totals = self.rewards.totals().await?;

        Ok(validators
            .iter()
            .filter_map(|v| {
                let license = by_id.get(&v.license_id)?;
                let mut view = self.to_view(v, license);
                // The ledger is authoritative for financial figures.
                view.total_rewards = totals.get(&v.id).copied().unwrap_or(0);
                Some(view)
            })
            .collect())
    }

    pub async fn get(&self, id: Uuid) -> Result<ValidatorView> {
        let validator = self.repo.find_by_id(id).await?.ok_or(AppError::NotFound)?;
        let license = self.licenses.get(validator.license_id).await?;
        Ok(self.to_view(&validator, &license))
    }

    /// Period-bucketed rewards for a validator (individual page / wallet).
    pub async fn rewards_for(&self, id: Uuid) -> Result<RewardSummary> {
        self.rewards.summarize(id).await
    }

    /// Network-wide period-bucketed rewards (operational KPIs).
    pub async fn network_rewards(&self) -> Result<RewardSummary> {
        self.rewards.network_summary().await
    }

    /// Approve a pending validator: submit `validatoradd` to the node (which
    /// signs it with the master key) and mark it active. Requires the node
    /// bridge; consensus is never mutated directly.
    pub async fn approve(
        &self,
        id: Uuid,
        activation_height: Option<u64>,
        actor: &str,
    ) -> Result<ValidatorView> {
        let validator = self.repo.find_by_id(id).await?.ok_or(AppError::NotFound)?;
        if !validator.status.can_transition_to(ValidatorStatus::Active) {
            return Err(AppError::InvalidTransition(
                "validator cannot be approved from its current state",
            ));
        }

        let node = self.node.as_ref().ok_or(AppError::NodeNotConfigured)?;
        let height = match activation_height {
            Some(h) => h,
            None => node.block_count().await?,
        };

        let result = node.validator_add(&validator.public_key, height).await?;
        if !result.success {
            return Err(AppError::NodeError(
                result
                    .error
                    .unwrap_or_else(|| "validatoradd rejected".into()),
            ));
        }

        if let Some(node_id) = node.resolve_validator_id(&validator.public_key).await? {
            self.repo.set_node_id(id, node_id as i64).await?;
        }

        let validator = self
            .repo
            .set_status(
                id,
                ValidatorStatus::Active,
                "approved",
                actor,
                serde_json::json!({ "activation_height": height }),
            )
            .await?;
        let license = self.licenses.get(validator.license_id).await?;
        Ok(self.to_view(&validator, &license))
    }

    /// Suspend an active validator: deactivate it on-chain and mark suspended.
    /// Suspend only applies to on-chain (active) validators, so the node bridge
    /// is required to keep the SaaS state consistent with the chain.
    pub async fn suspend(&self, id: Uuid, actor: &str) -> Result<ValidatorView> {
        let validator = self.repo.find_by_id(id).await?.ok_or(AppError::NotFound)?;
        if !validator
            .status
            .can_transition_to(ValidatorStatus::Suspended)
        {
            return Err(AppError::InvalidTransition(
                "validator cannot be suspended from its current state",
            ));
        }

        let node = self.node.as_ref().ok_or(AppError::NodeNotConfigured)?;
        let node_id = self
            .node_id(&validator, node)
            .await?
            .ok_or_else(|| AppError::NodeError("validator has no on-chain id".into()))?;
        let height = node.block_count().await?;
        let result = node.validator_deactivate(node_id, height).await?;
        if !result.success {
            return Err(AppError::NodeError(
                result
                    .error
                    .unwrap_or_else(|| "validatordeactivate rejected".into()),
            ));
        }

        let validator = self
            .repo
            .set_status(
                id,
                ValidatorStatus::Suspended,
                "suspended",
                actor,
                serde_json::json!({}),
            )
            .await?;
        let license = self.licenses.get(validator.license_id).await?;
        Ok(self.to_view(&validator, &license))
    }

    /// Revoke a validator: remove it on-chain (unless it was never authorized)
    /// and mark revoked. Terminal.
    pub async fn revoke(&self, id: Uuid, actor: &str) -> Result<ValidatorView> {
        let validator = self.repo.find_by_id(id).await?.ok_or(AppError::NotFound)?;
        if !validator.status.can_transition_to(ValidatorStatus::Revoked) {
            return Err(AppError::InvalidTransition(
                "validator cannot be revoked from its current state",
            ));
        }

        // A pending validator was never authorized on-chain; revoking it is a
        // pure SaaS action. Anything else was (or is) in the on-chain set and
        // must be removed through the node bridge.
        if validator.status != ValidatorStatus::Pending {
            let node = self.node.as_ref().ok_or(AppError::NodeNotConfigured)?;
            let node_id = self
                .node_id(&validator, node)
                .await?
                .ok_or_else(|| AppError::NodeError("validator has no on-chain id".into()))?;
            let height = node.block_count().await?;
            let result = node.validator_remove(node_id, height).await?;
            if !result.success {
                return Err(AppError::NodeError(
                    result
                        .error
                        .unwrap_or_else(|| "validatorremove rejected".into()),
                ));
            }
        }

        let validator = self
            .repo
            .set_status(
                id,
                ValidatorStatus::Revoked,
                "revoked",
                actor,
                serde_json::json!({}),
            )
            .await?;
        let license = self.licenses.get(validator.license_id).await?;
        Ok(self.to_view(&validator, &license))
    }

    /// Resolve the on-chain validator id, preferring the cached value.
    async fn node_id(&self, validator: &Validator, node: &NodeClient) -> Result<Option<u64>> {
        if let Some(id) = validator.node_validator_id {
            return Ok(Some(id as u64));
        }
        node.resolve_validator_id(&validator.public_key).await
    }

    fn to_view(&self, validator: &Validator, license: &License) -> ValidatorView {
        let online = validator.last_seen.is_some_and(|seen| {
            Utc::now().signed_duration_since(seen) <= Duration::seconds(ONLINE_WINDOW_SECS)
        });
        ValidatorView {
            id: validator.id,
            license_id: validator.license_id,
            public_key: validator.public_key.clone(),
            augeid: license.augeid.clone(),
            ip: license.ip.clone(),
            machine_hash: license.machine_hash.clone(),
            node_validator_id: validator.node_validator_id,
            os: validator.os.clone(),
            cpu: validator.cpu,
            ram: validator.ram,
            version: validator.version.clone(),
            uptime: validator.uptime,
            blocks: validator.blocks,
            blocks_lost: validator.blocks_lost,
            leadership: validator.leadership,
            total_rewards: validator.total_rewards,
            last_seen: validator.last_seen,
            status: validator.status,
            online,
            license_status: self.licenses.status_of(license),
            license_expires_at: license.expires_at,
            created_at: validator.created_at,
        }
    }
}
