//! Integration tests for the validator service.
//!
//! These require a reachable PostgreSQL. Set `AUGECOIN_OPS_DATABASE_URL` to run
//! them; otherwise each test returns early (the pure unit tests in `src/` always
//! run). Tests share one database and are serialized behind a lock; each test
//! truncates the license/validator tables before running.
//!
//! ```bash
//! export AUGECOIN_OPS_DATABASE_URL=postgres://postgres:postgres@localhost:5432/augecoin_ops_test
//! cargo test -p augecoin-ops
//! ```

use std::net::SocketAddr;
use std::sync::Arc;

use augecoin_ops::db::MIGRATOR;
use augecoin_ops::license::{LicenseService, LicenseStatus, Plan};
use augecoin_ops::node::NodeClient;
use augecoin_ops::reward::RewardRepo;
use augecoin_ops::validator::{ValidatorService, ValidatorStatus};
use axum::{extract::State, response::IntoResponse, routing::post, Json, Router};
use serde_json::{json, Value};
use tokio::sync::Mutex;
use uuid::Uuid;

static DB_LOCK: Mutex<()> = Mutex::const_new(());

fn db_url() -> Option<String> {
    std::env::var("AUGECOIN_OPS_DATABASE_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
}

fn hex64(seed: u8) -> String {
    format!("{:02x}", seed).repeat(32)
}

/// A minimal JSON-RPC node mock: echoes `validatoradd` success, resolves the
/// on-chain id for the added public key, and accepts deactivate/remove.
#[derive(Clone, Default)]
struct MockNode {
    added_pubkey: Arc<Mutex<Option<String>>>,
}

async fn mock_handler(State(mock): State<MockNode>, Json(body): Json<Value>) -> impl IntoResponse {
    let method = body.get("method").and_then(|m| m.as_str()).unwrap_or("");
    let result = match method {
        "validatoradd" => {
            let pk = body["params"]["ed25519_public_key_hex"]
                .as_str()
                .unwrap_or("")
                .to_string();
            *mock.added_pubkey.lock().await = Some(pk);
            json!({ "success": true, "error": null })
        }
        "validatorremove" | "validatordeactivate" | "validatoractivate" => {
            json!({ "success": true, "error": null })
        }
        "getvalidatorset" => {
            let pk = mock.added_pubkey.lock().await.clone().unwrap_or_default();
            json!({ "active": [{ "id": 42, "ed25519_public_key": pk }] })
        }
        "getblockcount" => json!(100),
        _ => json!({}),
    };
    Json(json!({ "jsonrpc": "2.0", "id": 1, "result": result }))
}

async fn spawn_mock_node() -> (SocketAddr, MockNode) {
    let mock = MockNode::default();
    let app = Router::new()
        .route("/", post(mock_handler))
        .with_state(mock.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (addr, mock)
}

struct TestCtx {
    svc: ValidatorService,
    licenses: LicenseService,
    _guard: tokio::sync::MutexGuard<'static, ()>,
}

async fn setup(node: Option<NodeClient>) -> Option<TestCtx> {
    let url = db_url()?;
    let guard = DB_LOCK.lock().await;
    let pool = sqlx::PgPool::connect(&url).await.ok()?;
    MIGRATOR.run(&pool).await.ok()?;
    sqlx::query(
        "TRUNCATE licenses, license_events, validators, validator_events, heartbeats, validator_rewards, sync_state CASCADE",
    )
    .execute(&pool)
    .await
    .ok()?;
    let licenses = LicenseService::new(pool.clone());
    let rewards = RewardRepo::new(pool.clone());
    let svc = ValidatorService::new(pool, licenses.clone(), rewards, node);
    Some(TestCtx {
        svc,
        licenses,
        _guard: guard,
    })
}

fn activate_input(augeid: &str, machine: u8, pubkey: u8) -> augecoin_ops::validator::ActivateInput {
    augecoin_ops::validator::ActivateInput {
        license_key: None,
        augeid: Some(augeid.to_string()),
        machine_id: hex64(machine),
        public_key: hex64(pubkey),
        os: Some("linux".to_string()),
        cpu: Some(32),
        ram: Some(48),
        version: Some("1.0.0".to_string()),
        ip: Some("203.0.113.10".to_string()),
    }
}

fn activate_by_key<'a>(
    key: &'a str,
    machine: u8,
    pubkey: u8,
) -> augecoin_ops::validator::ActivateInput {
    augecoin_ops::validator::ActivateInput {
        license_key: Some(key.to_string()),
        augeid: None,
        machine_id: hex64(machine),
        public_key: hex64(pubkey),
        os: Some("linux".to_string()),
        cpu: Some(32),
        ram: Some(48),
        version: Some("1.0.0".to_string()),
        ip: Some("203.0.113.10".to_string()),
    }
}

#[tokio::test]
async fn activate_registers_pending_validator_and_binds_license() {
    let Some(ctx) = setup(None).await else { return };
    let _issued = ctx
        .licenses
        .issue(Uuid::new_v4(), Plan::Annual, None)
        .await
        .unwrap();
    let input = activate_input("AUGE123", 0x11, 0x22);

    let resp = ctx.svc.activate(input).await.unwrap();
    assert_eq!(resp.validator.status, ValidatorStatus::Pending);
    assert_eq!(resp.validator.public_key, hex64(0x22));
    assert_eq!(resp.validator.augeid.as_deref(), Some("AUGE123"));
    assert_eq!(resp.config.heartbeat_interval_seconds, 30);

    // The license is now bound to the machine and public key.
    let license = ctx.licenses.resolve_by_augeid("AUGE123").await.unwrap();
    assert_eq!(license.machine_hash.as_deref(), Some(hex64(0x11).as_str()));
    assert_eq!(license.public_key.as_deref(), Some(hex64(0x22).as_str()));
}

#[tokio::test]
async fn reactivation_on_same_machine_is_idempotent() {
    let Some(ctx) = setup(None).await else { return };
    let _issued = ctx
        .licenses
        .issue(Uuid::new_v4(), Plan::Annual, None)
        .await
        .unwrap();

    let first = ctx
        .svc
        .activate(activate_input("AUGE123", 0x11, 0x22))
        .await
        .unwrap();
    let second = ctx
        .svc
        .activate(activate_input("AUGE123", 0x11, 0x22))
        .await
        .unwrap();

    assert_eq!(first.validator.id, second.validator.id);
    assert_eq!(second.validator.status, ValidatorStatus::Pending);
    assert_eq!(ctx.svc.list().await.unwrap().len(), 1);
}

#[tokio::test]
async fn reactivation_on_another_machine_is_rejected() {
    let Some(ctx) = setup(None).await else { return };
    let _issued = ctx
        .licenses
        .issue(Uuid::new_v4(), Plan::Annual, None)
        .await
        .unwrap();

    ctx.svc
        .activate(activate_input("AUGE123", 0x11, 0x22))
        .await
        .unwrap();
    let err = ctx
        .svc
        .activate(activate_input("AUGE123", 0x99, 0x22))
        .await
        .unwrap_err();
    assert!(matches!(err, augecoin_ops::AppError::MachineMismatch));
}

#[tokio::test]
async fn activation_with_different_pubkey_is_rejected() {
    let Some(ctx) = setup(None).await else { return };
    let _issued = ctx
        .licenses
        .issue(Uuid::new_v4(), Plan::Annual, None)
        .await
        .unwrap();

    ctx.svc
        .activate(activate_input("AUGE123", 0x11, 0x22))
        .await
        .unwrap();
    let err = ctx
        .svc
        .activate(activate_input("AUGE123", 0x11, 0xAA))
        .await
        .unwrap_err();
    assert!(matches!(err, augecoin_ops::AppError::PublicKeyMismatch));
}

#[tokio::test]
async fn activation_with_suspended_license_is_forbidden() {
    let Some(ctx) = setup(None).await else { return };
    let issued = ctx
        .licenses
        .issue(Uuid::new_v4(), Plan::Annual, None)
        .await
        .unwrap();
    ctx.licenses
        .suspend(issued.license.id, "test")
        .await
        .unwrap();

    let err = ctx
        .svc
        .activate(activate_input("AUGE123", 0x11, 0x22))
        .await
        .unwrap_err();
    assert!(matches!(err, augecoin_ops::AppError::LicenseNotActive));
}

#[tokio::test]
async fn heartbeat_updates_metrics_and_marks_online() {
    let Some(ctx) = setup(None).await else { return };
    let _issued = ctx
        .licenses
        .issue(Uuid::new_v4(), Plan::Annual, None)
        .await
        .unwrap();
    ctx.svc
        .activate(activate_input("AUGE123", 0x11, 0x22))
        .await
        .unwrap();

    let hb = ctx
        .svc
        .heartbeat(augecoin_ops::validator::HeartbeatInput {
            license_key: None,
            augeid: Some("AUGE123".to_string()),
            uptime: 86400,
            cpu: Some(32),
            ram: Some(48),
            block: Some(325000),
        })
        .await
        .unwrap();

    assert!(hb.authorized);
    assert!(hb.online);
    assert_eq!(hb.uptime, 86400);
    assert_eq!(hb.block, 325000);

    let stats = ctx.svc.stats_by_augeid("AUGE123").await.unwrap();
    assert_eq!(stats.validator.uptime, 86400);
    assert_eq!(stats.validator.blocks, 325000);
    assert!(stats.validator.online);
    assert_eq!(stats.license.status, LicenseStatus::Active);
}

#[tokio::test]
async fn heartbeat_with_unknown_license_is_rejected() {
    let Some(ctx) = setup(None).await else { return };
    let err = ctx
        .svc
        .heartbeat(augecoin_ops::validator::HeartbeatInput {
            license_key: None,
            augeid: Some("UNKNOWN".to_string()),
            uptime: 10,
            cpu: None,
            ram: None,
            block: None,
        })
        .await
        .unwrap_err();
    assert!(matches!(err, augecoin_ops::AppError::InvalidLicenseKey));
}

#[tokio::test]
async fn activate_and_heartbeat_by_license_key() {
    let Some(ctx) = setup(None).await else { return };
    let issued = ctx
        .licenses
        .issue(Uuid::new_v4(), Plan::Annual, None)
        .await
        .unwrap();

    // No AUGEID anywhere: the plaintext key alone resolves the license.
    let resp = ctx
        .svc
        .activate(activate_by_key(&issued.license_key, 0x11, 0x22))
        .await
        .unwrap();
    assert_eq!(resp.validator.status, ValidatorStatus::Pending);
    assert_eq!(resp.validator.augeid, None);

    let hb = ctx
        .svc
        .heartbeat(augecoin_ops::validator::HeartbeatInput {
            license_key: Some(issued.license_key.clone()),
            augeid: None,
            uptime: 120,
            cpu: None,
            ram: None,
            block: Some(42),
        })
        .await
        .unwrap();
    assert!(hb.authorized);

    let stats = ctx
        .svc
        .stats_by_license_key(&issued.license_key)
        .await
        .unwrap();
    assert!(stats.validator.online);
    assert_eq!(stats.validator.blocks, 42);
}

#[tokio::test]
async fn approve_without_node_is_unavailable() {
    let Some(ctx) = setup(None).await else { return };
    let _issued = ctx
        .licenses
        .issue(Uuid::new_v4(), Plan::Annual, None)
        .await
        .unwrap();
    let resp = ctx
        .svc
        .activate(activate_input("AUGE123", 0x11, 0x22))
        .await
        .unwrap();

    let err = ctx
        .svc
        .approve(resp.validator.id, None, "admin")
        .await
        .unwrap_err();
    assert!(matches!(err, augecoin_ops::AppError::NodeNotConfigured));
}

#[tokio::test]
async fn approve_suspend_revoke_full_lifecycle_with_mock_node() {
    let (addr, _mock) = spawn_mock_node().await;
    let node = NodeClient::new(format!("http://{addr}/"), "node-key".to_string());
    let Some(ctx) = setup(Some(node)).await else {
        return;
    };

    let _issued = ctx
        .licenses
        .issue(Uuid::new_v4(), Plan::Annual, None)
        .await
        .unwrap();
    let resp = ctx
        .svc
        .activate(activate_input("AUGE123", 0x11, 0x22))
        .await
        .unwrap();
    let id = resp.validator.id;

    // Approve → active, with the on-chain id resolved from the mock.
    let approved = ctx.svc.approve(id, Some(500), "admin").await.unwrap();
    assert_eq!(approved.status, ValidatorStatus::Active);
    assert_eq!(approved.node_validator_id, Some(42));

    // Suspend → suspended.
    let suspended = ctx.svc.suspend(id, "admin").await.unwrap();
    assert_eq!(suspended.status, ValidatorStatus::Suspended);

    // Revoke → revoked (terminal); a second revoke is rejected.
    let revoked = ctx.svc.revoke(id, "admin").await.unwrap();
    assert_eq!(revoked.status, ValidatorStatus::Revoked);
    assert!(ctx.svc.revoke(id, "admin").await.is_err());
}

#[tokio::test]
async fn pending_can_be_revoked_locally_and_is_terminal() {
    let Some(ctx) = setup(None).await else { return };
    let _issued = ctx
        .licenses
        .issue(Uuid::new_v4(), Plan::Annual, None)
        .await
        .unwrap();
    let resp = ctx
        .svc
        .activate(activate_input("AUGE123", 0x11, 0x22))
        .await
        .unwrap();

    // Revoking a pending validator is a pure SaaS action (no node required).
    let revoked = ctx.svc.revoke(resp.validator.id, "admin").await.unwrap();
    assert_eq!(revoked.status, ValidatorStatus::Revoked);

    // Terminal: cannot approve or revoke again.
    assert!(ctx
        .svc
        .approve(resp.validator.id, None, "admin")
        .await
        .is_err());
    assert!(ctx.svc.revoke(resp.validator.id, "admin").await.is_err());
}
