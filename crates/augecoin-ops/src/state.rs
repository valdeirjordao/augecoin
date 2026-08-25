//! Shared application state injected into HTTP handlers.
use sqlx::PgPool;

use crate::audit::AuditRepo;
use crate::config::Config;
use crate::license::LicenseService;
use crate::monitor::AlertRepo;
use crate::node::NodeClient;
use crate::release::ReleaseService;
use crate::reward::RewardRepo;
use crate::validator::ValidatorService;

#[derive(Clone)]
pub struct AppState {
    pub config: Config,
    pub licenses: LicenseService,
    pub validators: ValidatorService,
    pub rewards: RewardRepo,
    pub releases: ReleaseService,
    pub audit: AuditRepo,
    pub alerts: AlertRepo,
}

impl AppState {
    pub fn new(config: Config, pool: PgPool) -> Self {
        let licenses = LicenseService::new(pool.clone());
        let rewards = RewardRepo::new(pool.clone());
        let node = if config.has_node_bridge() {
            Some(NodeClient::new(
                config.node_rpc_url.clone().expect("checked"),
                config.node_admin_key.clone().expect("checked"),
            ))
        } else {
            None
        };
        let validators =
            ValidatorService::new(pool.clone(), licenses.clone(), rewards.clone(), node);

        // A malformed release key fails fast at startup rather than at first
        // publish; a missing key simply leaves publishing disabled.
        let release_public_key = match config.release_public_key() {
            Some(Ok(key)) => Some(key),
            Some(Err(_)) => {
                tracing::error!(
                    "invalid AUGECOIN_OPS_RELEASE_PUBLIC_KEY; release publishing disabled"
                );
                None
            }
            None => None,
        };
        let releases = ReleaseService::new(pool.clone(), release_public_key);
        let audit = AuditRepo::new(pool.clone());
        let alerts = AlertRepo::new(pool.clone());

        Self {
            config,
            licenses,
            validators,
            rewards,
            releases,
            audit,
            alerts,
        }
    }
}
