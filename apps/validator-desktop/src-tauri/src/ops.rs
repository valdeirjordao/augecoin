//! Client for the operational backend (augecoin-ops) public endpoints.
//!
//! The desktop uses only the *public* surface: `/activate`, `/validator/heartbeat`,
//! `/validator/stats` and `/updates`. No admin key is ever present on the client.
//! TLS is mandatory in production; in dev the URL can point anywhere.

use serde::{Deserialize, Serialize};

const DEFAULT_OPS_URL: &str = "https://operacional.augeco.in/api";

fn ops_url() -> String {
    std::env::var("AUGECOIN_OPS_API_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_OPS_URL.to_string())
}

#[derive(Clone)]
pub struct OpsClient {
    http: reqwest::Client,
    base: String,
}

#[derive(Debug, Serialize)]
pub struct ActivatePayload<'a> {
    pub license_key: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub augeid: Option<&'a str>,
    pub machine_id: &'a str,
    pub public_key: &'a str,
    pub os: Option<&'a str>,
    pub cpu: Option<i32>,
    pub ram: Option<i32>,
    pub version: &'a str,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ValidatorStatus {
    #[default]
    Pending,
    Active,
    Suspended,
    Revoked,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ValidatorView {
    pub id: String,
    pub public_key: String,
    #[serde(default)]
    pub augeid: Option<String>,
    #[serde(default)]
    pub ip: Option<String>,
    #[serde(default)]
    pub os: Option<String>,
    #[serde(default)]
    pub online: bool,
    #[serde(default)]
    pub status: ValidatorStatus,
    #[serde(default)]
    pub uptime: i64,
    #[serde(default)]
    pub blocks: i64,
    #[serde(default)]
    pub leadership: i64,
    #[serde(default)]
    pub total_rewards: String,
    #[serde(default)]
    pub license_expires_at: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RewardBucket {
    #[serde(default)]
    pub auge: String,
    #[serde(default)]
    pub augeids: i64,
    #[serde(default)]
    pub blocks: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RewardSummary {
    #[serde(default)]
    pub hour: RewardBucket,
    #[serde(default)]
    pub day: RewardBucket,
    #[serde(default)]
    pub week: RewardBucket,
    #[serde(default)]
    pub month: RewardBucket,
    #[serde(default)]
    pub year: RewardBucket,
    #[serde(default)]
    pub total: RewardBucket,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ActivationConfig {
    #[serde(default)]
    pub heartbeat_interval_seconds: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ActivationResponse {
    pub validator: ValidatorView,
    pub config: ActivationConfig,
}

#[derive(Debug, Deserialize)]
pub struct StatsResponse {
    pub validator: ValidatorView,
    pub rewards: RewardSummary,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Release {
    pub version: String,
    #[serde(default)]
    pub artifact_url: Option<String>,
    pub artifact_hash: String,
    pub signature: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdatesResponse {
    #[serde(default)]
    pub release: Option<Release>,
    #[serde(default)]
    pub up_to_date: bool,
}

impl OpsClient {
    pub fn new() -> Self {
        Self {
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(15))
                .build()
                .expect("reqwest client"),
            base: ops_url(),
        }
    }

    pub async fn activate(
        &self,
        payload: &ActivatePayload<'_>,
    ) -> Result<ActivationResponse, String> {
        let res = self
            .http
            .post(format!("{}/activate", self.base))
            .json(payload)
            .send()
            .await
            .map_err(|e| format!("activate request failed: {e}"))?;
        let status = res.status();
        let body = res.text().await.map_err(|e| e.to_string())?;
        if !status.is_success() {
            let msg = serde_json::from_str::<serde_json::Value>(&body)
                .ok()
                .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(String::from))
                .unwrap_or_else(|| format!("HTTP {status}"));
            return Err(msg);
        }
        serde_json::from_str(&body).map_err(|e| format!("bad activate response: {e}"))
    }

    /// Heartbeat identified by license key (preferred) or legacy AUGEID.
    pub async fn heartbeat(
        &self,
        license_key: Option<&str>,
        augeid: Option<&str>,
        uptime: i64,
        cpu: Option<i32>,
        ram: Option<i32>,
        block: Option<i64>,
    ) -> Result<bool, String> {
        let mut payload = serde_json::Map::new();
        if let Some(k) = license_key.filter(|k| !k.is_empty()) {
            payload.insert("license_key".into(), serde_json::Value::String(k.into()));
        }
        if let Some(a) = augeid.filter(|a| !a.is_empty()) {
            payload.insert("augeid".into(), serde_json::Value::String(a.into()));
        }
        payload.insert("uptime".into(), serde_json::Value::from(uptime));
        if let Some(cpu) = cpu {
            payload.insert("cpu".into(), serde_json::Value::from(cpu));
        }
        if let Some(ram) = ram {
            payload.insert("ram".into(), serde_json::Value::from(ram));
        }
        if let Some(block) = block {
            payload.insert("block".into(), serde_json::Value::from(block));
        }
        let res = self
            .http
            .post(format!("{}/validator/heartbeat", self.base))
            .json(&serde_json::Value::Object(payload))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let status = res.status();
        let body: serde_json::Value = res.json().await.unwrap_or(serde_json::Value::Null);
        if !status.is_success() {
            return Err(body
                .get("error")
                .and_then(|e| e.as_str())
                .unwrap_or("heartbeat failed")
                .to_string());
        }
        Ok(body
            .get("authorized")
            .and_then(|v| v.as_bool())
            .unwrap_or(false))
    }

    /// Dashboard statistics, identified by license key (preferred) or legacy
    /// AUGEID.
    pub async fn stats(
        &self,
        license_key: Option<&str>,
        augeid: Option<&str>,
    ) -> Result<StatsResponse, String> {
        let mut query = Vec::new();
        if let Some(k) = license_key.filter(|k| !k.is_empty()) {
            query.push(("license_key", k));
        } else if let Some(a) = augeid.filter(|a| !a.is_empty()) {
            query.push(("augeid", a));
        }
        let res = self
            .http
            .get(format!("{}/validator/stats", self.base))
            .query(&query)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let status = res.status();
        let body = res.text().await.map_err(|e| e.to_string())?;
        if !status.is_success() {
            return Err(format!("stats failed: HTTP {status}"));
        }
        serde_json::from_str(&body).map_err(|e| format!("bad stats response: {e}"))
    }

    pub async fn updates(
        &self,
        platform: &str,
        current_version: &str,
    ) -> Result<UpdatesResponse, String> {
        let res = self
            .http
            .get(format!("{}/updates", self.base))
            .query(&[("platform", platform), ("current_version", current_version)])
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let body: UpdatesResponse = res.json().await.map_err(|e| e.to_string())?;
        Ok(body)
    }
}
