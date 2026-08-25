//! HTTP handlers for health, the license domain and the validator domain.

use std::net::SocketAddr;

use axum::{
    extract::{ConnectInfo, Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::error::{AppError, Result};
use crate::license::{LicenseStatus, LicenseView, Plan};
use crate::release::{NewRelease, Platform};
use crate::state::AppState;
use crate::validator::{ActivateInput, HeartbeatInput, ValidatorStatus};

pub async fn health() -> Json<Value> {
    Json(json!({ "ok": true }))
}

// ── Licenses ───────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct IssueRequest {
    pub user_id: Uuid,
    /// Plan as a string so an unknown value maps to a clean `400` (not a serde
    /// 422 with a non-standard body).
    pub plan: String,
    /// Optional on-chain AUGEID number the license is bound to for activation.
    pub augeid: Option<String>,
}

pub async fn issue_license(
    State(state): State<AppState>,
    Json(body): Json<IssueRequest>,
) -> Result<(StatusCode, Json<Value>)> {
    let plan: Plan = body.plan.parse()?;
    let issued = state
        .licenses
        .issue(body.user_id, plan, body.augeid)
        .await?;
    Ok((StatusCode::CREATED, Json(serde_json::to_value(issued)?)))
}

pub async fn get_license(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<LicenseView>> {
    let license = state.licenses.get(id).await?;
    Ok(Json(LicenseView::from(&license)))
}

#[derive(Debug, Deserialize)]
pub struct LicenseListQuery {
    pub user_id: Option<Uuid>,
}

pub async fn list_licenses(
    State(state): State<AppState>,
    Query(query): Query<LicenseListQuery>,
) -> Result<Json<Value>> {
    let licenses = match query.user_id {
        Some(user_id) => state.licenses.list_by_user(user_id).await?,
        None => state.licenses.list().await?,
    };
    let views: Vec<LicenseView> = licenses.iter().map(LicenseView::from).collect();
    Ok(Json(json!({ "licenses": views })))
}

pub async fn suspend_license(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<LicenseView>> {
    let license = state.licenses.suspend(id, "admin").await?;
    Ok(Json(LicenseView::from(&license)))
}

pub async fn revoke_license(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<LicenseView>> {
    let license = state.licenses.revoke(id, "admin").await?;
    Ok(Json(LicenseView::from(&license)))
}

#[derive(Debug, Deserialize)]
pub struct ValidateRequest {
    pub license_key: String,
}

pub async fn validate_license(
    State(state): State<AppState>,
    Json(body): Json<ValidateRequest>,
) -> Result<Json<Value>> {
    let validation = state.licenses.validate(&body.license_key).await?;
    Ok(Json(serde_json::to_value(validation)?))
}

// ── Validators (public: activation + heartbeat + stats) ────────────────

#[derive(Debug, Deserialize)]
pub struct ActivateRequest {
    /// Plaintext license key (`XXXX-XXXX-…`, Crockford Base32). Preferred.
    pub license_key: Option<String>,
    /// Legacy identifier (bound AUGEID number). Kept for old clients.
    pub augeid: Option<String>,
    pub machine_id: String,
    pub public_key: String,
    pub os: Option<String>,
    pub cpu: Option<i32>,
    pub ram: Option<i32>,
    pub version: Option<String>,
}

/// Activate a license on a machine, resolving it by plaintext license key
/// (preferred) or legacy AUGEID. The client IP is taken from the TCP
/// connection, never from the request body, so a client cannot spoof it.
pub async fn activate(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(body): Json<ActivateRequest>,
) -> Result<(StatusCode, Json<Value>)> {
    if body
        .license_key
        .as_deref()
        .map(str::trim)
        .filter(|k| !k.is_empty())
        .is_none()
        && body
            .augeid
            .as_deref()
            .map(str::trim)
            .filter(|a| !a.is_empty())
            .is_none()
    {
        return Err(AppError::InvalidInput(
            "license_key or augeid is required".into(),
        ));
    }
    let response = state
        .validators
        .activate(ActivateInput {
            license_key: body.license_key,
            augeid: body.augeid,
            machine_id: body.machine_id,
            public_key: body.public_key,
            os: body.os,
            cpu: body.cpu,
            ram: body.ram,
            version: body.version,
            ip: Some(addr.ip().to_string()),
        })
        .await?;
    Ok((StatusCode::CREATED, Json(serde_json::to_value(response)?)))
}

#[derive(Debug, Deserialize)]
pub struct HeartbeatRequest {
    pub license_key: Option<String>,
    pub augeid: Option<String>,
    pub uptime: i64,
    pub cpu: Option<i32>,
    pub ram: Option<i32>,
    pub block: Option<i64>,
}

pub async fn heartbeat(
    State(state): State<AppState>,
    Json(body): Json<HeartbeatRequest>,
) -> Result<Json<Value>> {
    let response = state
        .validators
        .heartbeat(HeartbeatInput {
            license_key: body.license_key,
            augeid: body.augeid,
            uptime: body.uptime,
            cpu: body.cpu,
            ram: body.ram,
            block: body.block,
        })
        .await?;
    Ok(Json(serde_json::to_value(response)?))
}

#[derive(Debug, Deserialize)]
pub struct StatsQuery {
    pub license_key: Option<String>,
    pub augeid: Option<String>,
}

pub async fn validator_stats(
    State(state): State<AppState>,
    Query(query): Query<StatsQuery>,
) -> Result<Json<Value>> {
    let stats = if query
        .license_key
        .as_deref()
        .map(str::trim)
        .filter(|k| !k.is_empty())
        .is_some()
    {
        state
            .validators
            .stats_by_license_key(query.license_key.as_deref().unwrap_or_default())
            .await?
    } else if query
        .augeid
        .as_deref()
        .map(str::trim)
        .filter(|a| !a.is_empty())
        .is_some()
    {
        state
            .validators
            .stats_by_augeid(query.augeid.as_deref().unwrap_or_default())
            .await?
    } else {
        return Err(AppError::InvalidInput(
            "license_key or augeid is required".into(),
        ));
    };
    Ok(Json(serde_json::to_value(stats)?))
}

// ── Validators (admin) ─────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ValidatorListQuery {
    pub license_id: Option<Uuid>,
}

pub async fn list_validators(
    State(state): State<AppState>,
    Query(query): Query<ValidatorListQuery>,
) -> Result<Json<Value>> {
    let validators = match query.license_id {
        Some(license_id) => state.validators.list_by_license(license_id).await?,
        None => state.validators.list().await?,
    };
    Ok(Json(json!({ "validators": validators })))
}

pub async fn get_validator(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    let validator = state.validators.get(id).await?;
    let rewards = state.validators.rewards_for(id).await?;
    Ok(Json(json!({ "validator": validator, "rewards": rewards })))
}

/// Network-wide operational KPIs (AUGE distributed today/month, validator
/// counts), consumed by the operational dashboard.
pub async fn metrics(State(state): State<AppState>) -> Result<Json<Value>> {
    let rewards = state.validators.network_rewards().await?;
    let validators = state.validators.list().await?;
    let licenses = state.licenses.list().await?;

    let total = validators.len();
    let online = validators.iter().filter(|v| v.online).count();
    let active = validators
        .iter()
        .filter(|v| v.status == ValidatorStatus::Active)
        .count();
    let suspended = validators
        .iter()
        .filter(|v| v.status == ValidatorStatus::Suspended)
        .count();
    let pending = validators
        .iter()
        .filter(|v| v.status == ValidatorStatus::Pending)
        .count();
    let uptime_avg_seconds = if total == 0 {
        0
    } else {
        validators.iter().map(|v| v.uptime).sum::<i64>() / total as i64
    };

    let count_license = |status: LicenseStatus| -> usize {
        licenses
            .iter()
            .filter(|l| state.licenses.status_of(l) == status)
            .count()
    };

    Ok(Json(json!({
        "rewards": rewards,
        "validators": {
            "total": total,
            "online": online,
            "active": active,
            "suspended": suspended,
            "pending": pending,
            "uptime_avg_seconds": uptime_avg_seconds,
        },
        "licenses": {
            "active": count_license(LicenseStatus::Active),
            "expired": count_license(LicenseStatus::Expired),
            "suspended": count_license(LicenseStatus::Suspended),
            "revoked": count_license(LicenseStatus::Revoked),
        }
    })))
}

#[derive(Debug, Deserialize)]
pub struct ApproveRequest {
    pub activation_height: Option<u64>,
}

pub async fn approve_validator(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<ApproveRequest>,
) -> Result<Json<Value>> {
    let validator = state
        .validators
        .approve(id, body.activation_height, "admin")
        .await?;
    Ok(Json(serde_json::to_value(validator)?))
}

pub async fn suspend_validator(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    let validator = state.validators.suspend(id, "admin").await?;
    Ok(Json(serde_json::to_value(validator)?))
}

pub async fn revoke_validator(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    let validator = state.validators.revoke(id, "admin").await?;
    Ok(Json(serde_json::to_value(validator)?))
}

// ── Releases (OTA) ────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct UpdatesQuery {
    pub platform: String,
    #[serde(default)]
    pub current_version: Option<String>,
}

/// Latest release for a platform. The client verifies `signature` over
/// `artifact_hash` with its baked-in release public key before installing.
pub async fn updates(
    State(state): State<AppState>,
    Query(query): Query<UpdatesQuery>,
) -> Result<Json<Value>> {
    let platform: Platform = query.platform.parse()?;
    let release = state.releases.latest(platform).await?;
    let up_to_date = match (&query.current_version, &release) {
        (Some(current), Some(r)) => current == &r.version,
        _ => false,
    };
    Ok(Json(
        json!({ "release": release, "up_to_date": up_to_date }),
    ))
}

#[derive(Debug, Deserialize)]
pub struct PublishReleaseRequest {
    pub version: String,
    pub platform: String,
    pub artifact_url: Option<String>,
    pub artifact_hash: String,
    pub signature: String,
    pub notes: Option<String>,
}

pub async fn publish_release(
    State(state): State<AppState>,
    Json(body): Json<PublishReleaseRequest>,
) -> Result<(StatusCode, Json<Value>)> {
    let release = state
        .releases
        .publish(NewRelease {
            id: Uuid::new_v4(),
            version: body.version,
            platform: body.platform.parse()?,
            artifact_url: body.artifact_url,
            artifact_hash: body.artifact_hash,
            signature: body.signature,
            notes: body.notes,
        })
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(serde_json::to_value(crate::release::ReleaseView::from(
            &release,
        ))?),
    ))
}

pub async fn list_releases(State(state): State<AppState>) -> Result<Json<Value>> {
    let releases = state.releases.list().await?;
    Ok(Json(json!({ "releases": releases })))
}

// ── Audit ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct AuditQuery {
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}

fn default_limit() -> i64 {
    100
}

pub async fn audit(
    State(state): State<AppState>,
    Query(query): Query<AuditQuery>,
) -> Result<Json<Value>> {
    let entries = state
        .audit
        .list(query.limit.min(1000), query.offset)
        .await?;
    Ok(Json(json!({ "entries": entries })))
}

#[derive(Debug, Deserialize)]
pub struct AlertsQuery {
    #[serde(default = "default_limit")]
    pub limit: i64,
}

pub async fn alerts(
    State(state): State<AppState>,
    Query(query): Query<AlertsQuery>,
) -> Result<Json<Value>> {
    let alerts = state.alerts.list(query.limit.min(1000)).await?;
    Ok(Json(json!({ "alerts": alerts })))
}
