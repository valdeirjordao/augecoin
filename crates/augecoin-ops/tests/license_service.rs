//! Integration tests for the license service.
//!
//! These require a reachable PostgreSQL. Set `AUGECOIN_OPS_DATABASE_URL` to run
//! them; otherwise each test returns early (the pure unit tests in `src/` always
//! run). Tests share one database and are serialized behind a lock; each test
//! truncates the license tables before running.
//!
//! ```bash
//! export AUGECOIN_OPS_DATABASE_URL=postgres://postgres:postgres@localhost:5432/augecoin_ops_test
//! cargo test -p augecoin-ops
//! ```

use augecoin_ops::db::MIGRATOR;
use augecoin_ops::license::{LicenseKey, LicenseService, LicenseStatus, Plan};
use tokio::sync::Mutex;
use uuid::Uuid;

static DB_LOCK: Mutex<()> = Mutex::const_new(());

fn db_url() -> Option<String> {
    std::env::var("AUGECOIN_OPS_DATABASE_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
}

struct TestCtx {
    svc: LicenseService,
    _guard: tokio::sync::MutexGuard<'static, ()>,
}

async fn setup() -> Option<TestCtx> {
    let url = db_url()?;
    let guard = DB_LOCK.lock().await;
    let pool = sqlx::PgPool::connect(&url).await.ok()?;
    MIGRATOR.run(&pool).await.ok()?;
    sqlx::query("TRUNCATE licenses, license_events CASCADE")
        .execute(&pool)
        .await
        .ok()?;
    Some(TestCtx {
        svc: LicenseService::new(pool),
        _guard: guard,
    })
}

#[tokio::test]
async fn issue_then_validate_is_active() {
    let Some(ctx) = setup().await else { return };
    let user = Uuid::new_v4();

    let issued = ctx.svc.issue(user, Plan::Annual, None).await.unwrap();
    assert_eq!(issued.license_key.len(), 39); // 32 chars + 7 dashes
    assert_eq!(issued.license.plan, Plan::Annual);
    assert_eq!(issued.license.status, LicenseStatus::Active);
    assert_eq!(issued.license.user_id, user);

    let v = ctx.svc.validate(&issued.license_key).await.unwrap();
    assert!(v.valid);
    let lv = v.license.expect("known license");
    assert_eq!(lv.id, issued.license.id);
    assert_eq!(lv.license_key_prefix, issued.license.license_key_prefix);
}

#[tokio::test]
async fn validate_unknown_and_malformed_keys_are_invalid() {
    let Some(ctx) = setup().await else { return };

    // Well-formed but unknown.
    let unknown = LicenseKey::generate();
    let v = ctx.svc.validate(&unknown.display()).await.unwrap();
    assert!(!v.valid);
    assert!(v.license.is_none());

    // Malformed (wrong length / bad alphabet).
    let v = ctx.svc.validate("NOT-A-KEY").await.unwrap();
    assert!(!v.valid);
    assert!(v.license.is_none());
}

#[tokio::test]
async fn suspend_blocks_validation() {
    let Some(ctx) = setup().await else { return };
    let issued = ctx
        .svc
        .issue(Uuid::new_v4(), Plan::Monthly, None)
        .await
        .unwrap();

    ctx.svc.suspend(issued.license.id, "test").await.unwrap();

    let v = ctx.svc.validate(&issued.license_key).await.unwrap();
    assert!(!v.valid);
    assert_eq!(v.license.unwrap().status, LicenseStatus::Suspended);
}

#[tokio::test]
async fn revoke_is_terminal() {
    let Some(ctx) = setup().await else { return };
    let issued = ctx
        .svc
        .issue(Uuid::new_v4(), Plan::Monthly, None)
        .await
        .unwrap();
    let id = issued.license.id;

    ctx.svc.revoke(id, "test").await.unwrap();

    // Revoking again is rejected.
    assert!(ctx.svc.revoke(id, "test").await.is_err());
    // Suspending a revoked license is rejected.
    assert!(ctx.svc.suspend(id, "test").await.is_err());
}

#[tokio::test]
async fn get_never_returns_plaintext_key() {
    let Some(ctx) = setup().await else { return };
    let issued = ctx
        .svc
        .issue(Uuid::new_v4(), Plan::Monthly, None)
        .await
        .unwrap();

    let l = ctx.svc.get(issued.license.id).await.unwrap();
    // The hash is a 64-char hex digest, not the 32-char key.
    assert_eq!(l.license_key_hash.len(), 64);
    assert_eq!(l.license_key_prefix.len(), 4);
    // The plaintext is never derivable from the stored row.
    assert!(!l
        .license_key_hash
        .contains(&issued.license_key.replace('-', "")));
}

#[tokio::test]
async fn list_returns_issued_licenses() {
    let Some(ctx) = setup().await else { return };
    ctx.svc
        .issue(Uuid::new_v4(), Plan::Monthly, None)
        .await
        .unwrap();
    ctx.svc
        .issue(Uuid::new_v4(), Plan::Annual, None)
        .await
        .unwrap();

    let all = ctx.svc.list().await.unwrap();
    assert_eq!(all.len(), 2);
}
