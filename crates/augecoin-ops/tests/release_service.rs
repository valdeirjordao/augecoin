//! Integration tests for the release (OTA) service and its signature check.
//!
//! Requires PostgreSQL via `AUGECOIN_OPS_DATABASE_URL`; otherwise returns early.

use tokio::sync::Mutex;
use uuid::Uuid;

use augecoin_ops::db::MIGRATOR;
use augecoin_ops::error::AppError;
use augecoin_ops::release::{NewRelease, Platform, ReleaseService};

use ed25519_dalek::Signer;

static DB_LOCK: Mutex<()> = Mutex::const_new(());

fn db_url() -> Option<String> {
    std::env::var("AUGECOIN_OPS_DATABASE_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
}

fn artifact_hash() -> [u8; 32] {
    let mut h = [0u8; 32];
    for (i, b) in h.iter_mut().enumerate() {
        *b = i as u8;
    }
    h
}

struct TestCtx {
    svc: ReleaseService,
    _guard: tokio::sync::MutexGuard<'static, ()>,
}

async fn setup(key: Option<[u8; 32]>) -> Option<TestCtx> {
    let url = db_url()?;
    let guard = DB_LOCK.lock().await;
    let pool = sqlx::PgPool::connect(&url).await.ok()?;
    MIGRATOR.run(&pool).await.ok()?;
    sqlx::query("TRUNCATE releases CASCADE")
        .execute(&pool)
        .await
        .ok()?;
    Some(TestCtx {
        svc: ReleaseService::new(pool, key),
        _guard: guard,
    })
}

fn new_release(hash_hex: &str, sig_hex: &str) -> NewRelease {
    NewRelease {
        id: Uuid::new_v4(),
        version: "1.2.3".into(),
        platform: Platform::Linux,
        artifact_url: Some("https://cdn.augeco.in/validator/1.2.3/linux.deb".into()),
        artifact_hash: hash_hex.into(),
        signature: sig_hex.into(),
        notes: Some("release".into()),
    }
}

#[tokio::test]
async fn publish_requires_release_key() {
    let Some(ctx) = setup(None).await else { return };
    let err = ctx
        .svc
        .publish(new_release(&"ab".repeat(32), &"cd".repeat(64)))
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::ReleaseKeyNotConfigured));
}

#[tokio::test]
async fn publish_with_valid_signature_and_latest_lookup() {
    let signing = ed25519_dalek::SigningKey::from_bytes(&[7u8; 32]);
    let vk = signing.verifying_key();
    let hash = artifact_hash();
    let sig = signing.sign(&hash);

    let Some(ctx) = setup(Some(vk.to_bytes())).await else {
        return;
    };
    let release = ctx
        .svc
        .publish(new_release(
            &hex::encode(hash),
            &hex::encode(sig.to_bytes()),
        ))
        .await
        .unwrap();
    assert_eq!(release.version, "1.2.3");

    let latest = ctx.svc.latest(Platform::Linux).await.unwrap().unwrap();
    assert_eq!(latest.version, "1.2.3");
    assert_eq!(latest.artifact_hash, hex::encode(hash));
}

#[tokio::test]
async fn publish_with_invalid_signature_is_rejected() {
    let signing = ed25519_dalek::SigningKey::from_bytes(&[7u8; 32]);
    let vk = signing.verifying_key();
    let hash = artifact_hash();
    // Sign a *different* message, or use a wrong key, so verification fails.
    let wrong_sig = signing.sign(&[0u8; 32]);

    let Some(ctx) = setup(Some(vk.to_bytes())).await else {
        return;
    };
    let err = ctx
        .svc
        .publish(new_release(
            &hex::encode(hash),
            &hex::encode(wrong_sig.to_bytes()),
        ))
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::InvalidSignature));
}

#[tokio::test]
async fn latest_is_none_when_no_release_for_platform() {
    let Some(ctx) = setup(Some([1u8; 32])).await else {
        return;
    };
    assert!(ctx.svc.latest(Platform::Windows).await.unwrap().is_none());
}
