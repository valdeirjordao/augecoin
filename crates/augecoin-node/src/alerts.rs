use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AlertLevel {
    Info,
    Warning,
    Critical,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AlertEvent {
    Equivocation {
        validator_id: u64,
        block_height: u64,
    },
    MaliciousPeer {
        peer_id: String,
        infraction_count: u32,
        reason: String,
    },
    ValidatorAbsent {
        validator_id: u64,
        expected_height: u64,
        timeout_seconds: u64,
    },
    ReplayAttempt {
        sender_account: u64,
        n_operation: u64,
    },
}

impl AlertEvent {
    pub fn event_type_name(&self) -> &'static str {
        match self {
            AlertEvent::Equivocation { .. } => "equivocation",
            AlertEvent::MaliciousPeer { .. } => "malicious_peer",
            AlertEvent::ValidatorAbsent { .. } => "validator_absent",
            AlertEvent::ReplayAttempt { .. } => "replay_attempt",
        }
    }

    pub fn level(&self) -> AlertLevel {
        match self {
            AlertEvent::Equivocation { .. } => AlertLevel::Critical,
            AlertEvent::MaliciousPeer { .. } => AlertLevel::Warning,
            AlertEvent::ValidatorAbsent { .. } => AlertLevel::Warning,
            AlertEvent::ReplayAttempt { .. } => AlertLevel::Info,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Alert {
    pub timestamp: u64,
    pub level: AlertLevel,
    pub event: AlertEvent,
    pub detail: String,
}

impl Alert {
    pub fn new(event: AlertEvent, detail: String) -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let level = event.level();
        Alert {
            timestamp,
            level,
            event,
            detail,
        }
    }
}

pub struct AlertManager {
    history: Mutex<VecDeque<Alert>>,
    max_history: usize,
    webhook_url: Mutex<Option<String>>,
    alert_count_total: Mutex<u64>,
}

impl AlertManager {
    pub fn new(max_history: usize) -> Self {
        AlertManager {
            history: Mutex::new(VecDeque::with_capacity(max_history)),
            max_history,
            webhook_url: Mutex::new(None),
            alert_count_total: Mutex::new(0),
        }
    }

    pub fn set_webhook(&self, url: Option<String>) {
        if let Ok(mut w) = self.webhook_url.lock() {
            *w = url;
        }
    }

    pub fn webhook_url(&self) -> Option<String> {
        self.webhook_url.lock().ok().and_then(|w| w.clone())
    }

    pub fn fire_alert(&self, event: AlertEvent, detail: String) -> Alert {
        let alert = Alert::new(event, detail);

        if let Ok(mut history) = self.history.lock() {
            if history.len() >= self.max_history {
                history.pop_front();
            }
            history.push_back(alert.clone());
        }

        if let Ok(mut count) = self.alert_count_total.lock() {
            *count += 1;
        }

        eprintln!(
            "[ALERT-{}] {} | {}",
            match alert.level {
                AlertLevel::Info => "INFO",
                AlertLevel::Warning => "WARN",
                AlertLevel::Critical => "CRIT",
            },
            alert.event.event_type_name(),
            alert.detail,
        );

        alert
    }

    pub fn fire_equivocation(&self, validator_id: u64, block_height: u64, detail: &str) -> Alert {
        self.fire_alert(
            AlertEvent::Equivocation {
                validator_id,
                block_height,
            },
            detail.to_string(),
        )
    }

    pub fn fire_malicious_peer(&self, peer_id: &str, infraction_count: u32, reason: &str) -> Alert {
        self.fire_alert(
            AlertEvent::MaliciousPeer {
                peer_id: peer_id.to_string(),
                infraction_count,
                reason: reason.to_string(),
            },
            format!(
                "peer {} banned after {} infractions: {}",
                peer_id, infraction_count, reason
            ),
        )
    }

    pub fn fire_validator_absent(
        &self,
        validator_id: u64,
        expected_height: u64,
        timeout_seconds: u64,
    ) -> Alert {
        self.fire_alert(
            AlertEvent::ValidatorAbsent {
                validator_id,
                expected_height,
                timeout_seconds,
            },
            format!(
                "validator {} absent at height {} (timeout {}s)",
                validator_id, expected_height, timeout_seconds
            ),
        )
    }

    pub fn fire_replay_attempt(&self, sender_account: u64, n_operation: u64) -> Alert {
        self.fire_alert(
            AlertEvent::ReplayAttempt {
                sender_account,
                n_operation,
            },
            format!(
                "replay attempt: sender {}, n_operation {} reused",
                sender_account, n_operation
            ),
        )
    }

    pub fn recent_alerts(&self, limit: usize) -> Vec<Alert> {
        if let Ok(history) = self.history.lock() {
            history.iter().rev().take(limit).cloned().collect()
        } else {
            vec![]
        }
    }

    pub fn total_alert_count(&self) -> u64 {
        self.alert_count_total.lock().map(|c| *c).unwrap_or(0)
    }

    pub fn alerts_by_level(&self, level: AlertLevel) -> Vec<Alert> {
        if let Ok(history) = self.history.lock() {
            history
                .iter()
                .filter(|a| a.level == level)
                .cloned()
                .collect()
        } else {
            vec![]
        }
    }

    pub fn build_webhook_payload(&self, alert: &Alert) -> serde_json::Value {
        serde_json::json!({
            "source": "augecoin-node",
            "timestamp": alert.timestamp,
            "level": alert.level,
            "event": alert.event,
            "detail": alert.detail
        })
    }

    pub async fn send_webhook(&self, alert: &Alert) -> Result<(), String> {
        let url = match self.webhook_url() {
            Some(u) if !u.is_empty() => u,
            _ => return Ok(()),
        };

        let payload = self.build_webhook_payload(alert);

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .map_err(|e| format!("http client error: {e}"))?;

        let resp = client
            .post(&url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| format!("webhook request failed: {e}"))?;

        if !resp.status().is_success() {
            return Err(format!("webhook returned status {}", resp.status()));
        }

        Ok(())
    }
}

impl Default for AlertManager {
    fn default() -> Self {
        AlertManager::new(1000)
    }
}

#[derive(Debug, serde::Deserialize)]
pub struct AlertsConfig {
    pub max_history: Option<usize>,
    pub webhook_url: Option<String>,
}

impl AlertsConfig {
    pub fn from_file(path: &str) -> Result<Self, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("failed to read alerts config: {e}"))?;
        toml::from_str(&content).map_err(|e| format!("invalid alerts config: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alert_manager_creates_and_retrieves_alerts() {
        let mgr = AlertManager::new(50);

        mgr.fire_equivocation(1, 100, "validator signed two blocks at height 100");
        mgr.fire_malicious_peer("12D3KooW", 5, "repeated invalid blocks");
        mgr.fire_validator_absent(3, 200, 120);
        mgr.fire_replay_attempt(42, 7);

        let recent = mgr.recent_alerts(10);
        assert_eq!(recent.len(), 4);
        assert_eq!(mgr.total_alert_count(), 4);
    }

    #[test]
    fn alert_history_respects_max_size() {
        let mgr = AlertManager::new(5);

        for i in 0..10 {
            mgr.fire_replay_attempt(i, 0);
        }

        let recent = mgr.recent_alerts(20);
        assert_eq!(recent.len(), 5);
        assert_eq!(mgr.total_alert_count(), 10);
    }

    #[test]
    fn alert_levels_are_correct() {
        let mgr = AlertManager::new(10);

        let a1 = mgr.fire_equivocation(1, 100, "test");
        assert_eq!(a1.level, AlertLevel::Critical);

        let a2 = mgr.fire_malicious_peer("peer", 3, "test");
        assert_eq!(a2.level, AlertLevel::Warning);

        let a3 = mgr.fire_validator_absent(2, 200, 120);
        assert_eq!(a3.level, AlertLevel::Warning);

        let a4 = mgr.fire_replay_attempt(42, 7);
        assert_eq!(a4.level, AlertLevel::Info);
    }

    #[test]
    fn alert_event_type_names() {
        let e1 = AlertEvent::Equivocation {
            validator_id: 1,
            block_height: 10,
        };
        assert_eq!(e1.event_type_name(), "equivocation");

        let e2 = AlertEvent::MaliciousPeer {
            peer_id: "p".into(),
            infraction_count: 1,
            reason: "r".into(),
        };
        assert_eq!(e2.event_type_name(), "malicious_peer");

        let e3 = AlertEvent::ValidatorAbsent {
            validator_id: 1,
            expected_height: 10,
            timeout_seconds: 60,
        };
        assert_eq!(e3.event_type_name(), "validator_absent");

        let e4 = AlertEvent::ReplayAttempt {
            sender_account: 1,
            n_operation: 5,
        };
        assert_eq!(e4.event_type_name(), "replay_attempt");
    }

    #[test]
    fn alerts_by_level_filters_correctly() {
        let mgr = AlertManager::new(20);

        mgr.fire_replay_attempt(1, 1);
        mgr.fire_replay_attempt(2, 1);
        mgr.fire_equivocation(3, 100, "equiv");
        mgr.fire_malicious_peer("peer", 1, "reason");
        mgr.fire_replay_attempt(4, 1);

        let info_alerts = mgr.alerts_by_level(AlertLevel::Info);
        assert_eq!(info_alerts.len(), 3);

        let crit_alerts = mgr.alerts_by_level(AlertLevel::Critical);
        assert_eq!(crit_alerts.len(), 1);

        let warn_alerts = mgr.alerts_by_level(AlertLevel::Warning);
        assert_eq!(warn_alerts.len(), 1);
    }

    #[test]
    fn alert_has_timestamp() {
        let mgr = AlertManager::new(10);
        let a = mgr.fire_replay_attempt(1, 1);
        assert!(a.timestamp > 0);
        assert!(
            a.timestamp
                <= SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
        );
    }

    #[test]
    fn webhook_payload_is_valid_json() {
        let mgr = AlertManager::new(10);
        let alert = mgr.fire_equivocation(1, 50, "validator 1 equivocated at height 50");
        let payload = mgr.build_webhook_payload(&alert);

        assert_eq!(payload["source"], "augecoin-node");
        assert_eq!(payload["level"], "critical");
        assert_eq!(payload["detail"], "validator 1 equivocated at height 50");

        let event = &payload["event"];
        assert_eq!(event["equivocation"]["validator_id"], 1);
        assert_eq!(event["equivocation"]["block_height"], 50);

        let json_str = serde_json::to_string(&payload).unwrap();
        assert!(json_str.contains("augecoin-node"));
        assert!(json_str.contains("equivocation"));
    }

    #[test]
    fn webhook_payload_for_malicious_peer() {
        let mgr = AlertManager::new(10);
        let alert = mgr.fire_malicious_peer("12D3KooWXYZ", 10, "sending corrupted blocks");
        let payload = mgr.build_webhook_payload(&alert);

        let event = &payload["event"]["malicious_peer"];
        assert_eq!(event["peer_id"], "12D3KooWXYZ");
        assert_eq!(event["infraction_count"], 10);
    }

    #[test]
    fn webhook_url_configurable() {
        let mgr = AlertManager::new(10);
        assert!(mgr.webhook_url().is_none());

        mgr.set_webhook(Some("https://hooks.slack.com/services/TEST".to_string()));
        assert_eq!(
            mgr.webhook_url().unwrap(),
            "https://hooks.slack.com/services/TEST"
        );

        mgr.set_webhook(None);
        assert!(mgr.webhook_url().is_none());
    }

    #[test]
    fn default_alert_manager_has_capacity() {
        let mgr = AlertManager::default();

        for i in 0..2000 {
            mgr.fire_replay_attempt(i, 0);
        }

        let recent = mgr.recent_alerts(2000);
        assert!(recent.len() <= 1000);
        assert_eq!(mgr.total_alert_count(), 2000);
    }

    #[test]
    fn alert_serialization_roundtrip() {
        let alert = Alert::new(
            AlertEvent::ValidatorAbsent {
                validator_id: 5,
                expected_height: 300,
                timeout_seconds: 120,
            },
            "validator 5 missed block at height 300".to_string(),
        );

        let json = serde_json::to_string(&alert).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["level"], "warning");
        assert_eq!(parsed["detail"], "validator 5 missed block at height 300");
        assert_eq!(parsed["event"]["validator_absent"]["validator_id"], 5);
    }

    #[test]
    fn alert_detail_contains_relevant_info() {
        let mgr = AlertManager::new(10);

        let a1 = mgr.fire_equivocation(7, 150, "double sign");
        assert!(a1.detail.contains("double sign"));

        let a2 = mgr.fire_validator_absent(2, 250, 120);
        assert!(a2.detail.contains("250"));
        assert!(a2.detail.contains("120"));

        let a3 = mgr.fire_replay_attempt(99, 13);
        assert!(a3.detail.contains("99"));
        assert!(a3.detail.contains("13"));
    }
}
