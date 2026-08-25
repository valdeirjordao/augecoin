//! Tauri commands: activation, heartbeat, dashboard and OTA.
//!
//! The private key is generated on-device, stored locally (0600) and only ever
//! passed to the node process as an env var. The ops backend receives only the
//! public key and the machine hash — never the private key.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

use augecoin_crypto::signature::Ed25519KeyPair;
use serde::Serialize;

use crate::machine;
use crate::ops::{OpsClient, RewardSummary, ValidatorView};
use crate::state::{self, SavedState};

static OPS: OnceLock<OpsClient> = OnceLock::new();
static HEARTBEAT_STARTED: AtomicBool = AtomicBool::new(false);
static NODE_SPAWNED: AtomicBool = AtomicBool::new(false);

fn ops() -> &'static OpsClient {
    OPS.get_or_init(OpsClient::new)
}

const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Release public key used to verify OTA artifacts. Set at build time via the
/// `AUGECOIN_RELEASE_PUBLIC_KEY_HEX` environment variable; an empty value
/// disables OTA installation (never install unsigned binaries).
const RELEASE_PUBLIC_KEY_HEX: &str = match option_env!("AUGECOIN_RELEASE_PUBLIC_KEY_HEX") {
    Some(v) => v,
    None => "",
};

#[derive(Debug, Serialize)]
pub struct MachineInfo {
    pub machine_id: String,
}

#[derive(Debug, Serialize)]
pub struct ActivationSummary {
    pub state: SavedState,
    pub validator: ValidatorView,
    pub heartbeat_interval_seconds: u64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct NodeStatus {
    pub running: bool,
    pub syncing: bool,
    pub current_height: i64,
    pub peers_connected: i64,
    pub validator_id: i64,
    pub chain_id: i64,
    pub uptime_seconds: i64,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct DashboardData {
    pub state: SavedState,
    pub validator: ValidatorView,
    pub rewards: RewardSummary,
    pub node: NodeStatus,
}

#[derive(Debug, Serialize)]
pub struct UpdateInfo {
    pub current_version: String,
    pub latest_version: Option<String>,
    pub up_to_date: bool,
    pub artifact_url: Option<String>,
    pub signature_valid: bool,
}

// ── Machine ID ─────────────────────────────────────────────────────────

#[tauri::command]
pub fn machine_id() -> Result<MachineInfo, String> {
    Ok(MachineInfo {
        machine_id: machine::machine_id(),
    })
}

// ── State ──────────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_state() -> Result<SavedState, String> {
    Ok(state::load_state())
}

#[tauri::command]
pub fn clear_state() -> Result<(), String> {
    state::clear_state()
}

// ── Activation ─────────────────────────────────────────────────────────

#[tauri::command]
pub async fn activate(license_key: String) -> Result<ActivationSummary, String> {
    let key = license_key.trim().to_string();
    if key.is_empty() {
        return Err("Chave de licença é obrigatória".into());
    }

    // Generate the validator keypair on-device. The seed never leaves here.
    let kp = Ed25519KeyPair::generate();
    let seed_hex = hex::encode(kp.signing_key_bytes());
    let public_key_hex = hex::encode(kp.verifying_key().to_bytes());

    let machine_id = machine::machine_id();
    let (os, cpu, ram) = system_info();

    let response = ops()
        .activate(&crate::ops::ActivatePayload {
            license_key: &key,
            augeid: None,
            machine_id: &machine_id,
            public_key: &public_key_hex,
            os: Some(&os),
            cpu,
            ram,
            version: APP_VERSION,
        })
        .await?;

    let state = SavedState {
        license_key: key,
        augeid: None,
        public_key: public_key_hex,
        machine_id,
        activated_at: Some(chrono::Utc::now().to_rfc3339()),
        app_version: Some(APP_VERSION.to_string()),
    };
    state::save_state(&state)?;
    state::save_private_key(&seed_hex)?;

    spawn_node(&seed_hex);
    start_heartbeat();

    Ok(ActivationSummary {
        state,
        validator: response.validator,
        heartbeat_interval_seconds: response.config.heartbeat_interval_seconds.max(30),
    })
}

fn system_info() -> (String, Option<i32>, Option<i32>) {
    let os = format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH);
    let cpu = std::thread::available_parallelism()
        .ok()
        .map(|n| n.get() as i32);
    let ram = total_ram_gb();
    (os, cpu, ram)
}

fn total_ram_gb() -> Option<i32> {
    // Linux: /proc/meminfo MemTotal (kB).
    std::fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("MemTotal:"))
                .and_then(|l| l.split_whitespace().nth(1)?.parse::<u64>().ok())
        })
        .map(|kb| (kb / 1024 / 1024) as i32)
}

// ── Node spawn ─────────────────────────────────────────────────────────

fn find_node_bin() -> Option<std::path::PathBuf> {
    if let Ok(b) = std::env::var("AUGECOIN_NODE_BIN") {
        if !b.is_empty() {
            return Some(std::path::PathBuf::from(b));
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        let dir = exe.parent()?;
        let sidecar = dir.join("augecoin-node");
        if sidecar.exists() {
            return Some(sidecar);
        }
    }
    let dev = std::path::PathBuf::from("./augecoin-node");
    if dev.exists() {
        return Some(dev);
    }
    None
}

fn spawn_node(seed_hex: &str) {
    if NODE_SPAWNED.swap(true, Ordering::SeqCst) {
        return;
    }
    let Some(bin) = find_node_bin() else {
        tracing_node_unavailable();
        return;
    };

    let data_dir = state::data_dir().join("node");
    let _ = std::fs::create_dir_all(&data_dir);

    let mut cmd = std::process::Command::new(bin);
    cmd.env("AUGECOIN_VALIDATOR_KEY_HEX", seed_hex)
        .env("AUGECOIN_DATA_DIR", &data_dir)
        .env("AUGECOIN_RPC_PORT", "9005")
        .env("AUGECOIN_P2P_PORT", "9200")
        .stdin(std::process::Stdio::null());

    // Extra node config (genesis, bootnodes, chain id, …) supplied by the
    // operator via a `node.env` file in the data dir (KEY=VALUE lines).
    if let Ok(extra) = std::fs::read_to_string(state::data_dir().join("node.env")) {
        for line in extra.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some((k, v)) = line.split_once('=') {
                cmd.env(k.trim(), v.trim());
            }
        }
    }

    match cmd.spawn() {
        Ok(_child) => {
            // Fire-and-forget: the node runs independently of the UI.
            let _ = _child;
        }
        Err(e) => {
            eprintln!("[validator] failed to spawn node: {e}");
        }
    }
}

fn tracing_node_unavailable() {
    eprintln!("[validator] augecoin-node binary not found (set AUGECOIN_NODE_BIN)");
}

// ── Heartbeat ──────────────────────────────────────────────────────────

/// Resume a previously activated validator (called on app startup): re-spawns
/// the node with the stored seed and restarts the heartbeat loop.
pub fn resume() {
    let state = state::load_state();
    if !state.is_activated() {
        return;
    }
    if let Some(seed) = state::load_private_key() {
        spawn_node(&seed);
        start_heartbeat();
    }
}

fn start_heartbeat() {
    if HEARTBEAT_STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
        interval.tick().await; // skip the immediate first tick
        loop {
            interval.tick().await;
            let state = state::load_state();
            if !state.is_activated() {
                continue;
            }
            let node = query_node_status().await;
            let uptime = node.uptime_seconds;
            let block = if node.running {
                Some(node.current_height)
            } else {
                None
            };
            let _ = ops()
                .heartbeat(
                    Some(state.license_key.as_str()),
                    state.augeid.as_deref(),
                    uptime,
                    None,
                    None,
                    block,
                )
                .await;
        }
    });
}

// ── Node RPC (local) ───────────────────────────────────────────────────

fn node_rpc_port() -> String {
    std::env::var("AUGECOIN_NODE_RPC_PORT").unwrap_or_else(|_| "9005".into())
}

async fn node_rpc(method: &str, params: serde_json::Value) -> Result<serde_json::Value, String> {
    let url = format!("http://127.0.0.1:{}/", node_rpc_port());
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| e.to_string())?;
    let res = client
        .post(&url)
        .json(&serde_json::json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params }))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let body: serde_json::Value = res.json().await.map_err(|e| e.to_string())?;
    if let Some(err) = body.get("error") {
        return Err(err
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("rpc error")
            .to_string());
    }
    body.get("result")
        .cloned()
        .ok_or_else(|| "missing result".into())
}

async fn query_node_status() -> NodeStatus {
    let mut status = NodeStatus::default();
    match node_rpc("nodestatus", serde_json::json!({})).await {
        Ok(v) => {
            status.running = true;
            status.syncing = v.get("syncing").and_then(|s| s.as_bool()).unwrap_or(false);
            status.current_height = v
                .get("current_height")
                .and_then(|n| n.as_i64())
                .unwrap_or(0);
            status.peers_connected = v
                .get("peers_connected")
                .and_then(|n| n.as_i64())
                .unwrap_or(0);
            status.validator_id = v.get("validator_id").and_then(|n| n.as_i64()).unwrap_or(-1);
            status.chain_id = v.get("chain_id").and_then(|n| n.as_i64()).unwrap_or(0);
            status.uptime_seconds = v
                .get("uptime_seconds")
                .and_then(|n| n.as_i64())
                .unwrap_or(0);
        }
        Err(e) => {
            status.error = Some(e);
        }
    }
    status
}

// ── Dashboard ──────────────────────────────────────────────────────────

#[tauri::command]
pub async fn get_dashboard() -> Result<DashboardData, String> {
    let state = state::load_state();
    if !state.is_activated() {
        return Err("validador ainda não ativado".into());
    }

    let stats = ops()
        .stats(Some(state.license_key.as_str()), state.augeid.as_deref())
        .await?;
    let node = query_node_status().await;

    Ok(DashboardData {
        state,
        validator: stats.validator,
        rewards: stats.rewards,
        node,
    })
}

#[tauri::command]
pub async fn node_status() -> Result<NodeStatus, String> {
    Ok(query_node_status().await)
}

// ── OTA ────────────────────────────────────────────────────────────────

fn verify_release_signature(
    public_key_hex: &str,
    artifact_hash_hex: &str,
    signature_hex: &str,
) -> bool {
    if public_key_hex.is_empty() || public_key_hex.len() != 64 {
        return false;
    }
    let Ok(pk) = hex::decode(public_key_hex) else {
        return false;
    };
    let Ok(hash) = hex::decode(artifact_hash_hex) else {
        return false;
    };
    let Ok(sig) = hex::decode(signature_hex) else {
        return false;
    };
    let (Ok(pk_arr), Ok(hash_arr), Ok(sig_arr)) = (
        <[u8; 32]>::try_from(pk.as_slice()),
        <[u8; 32]>::try_from(hash.as_slice()),
        <[u8; 64]>::try_from(sig.as_slice()),
    ) else {
        return false;
    };
    use ed25519_dalek::Verifier;
    let Ok(vk) = ed25519_dalek::VerifyingKey::from_bytes(&pk_arr) else {
        return false;
    };
    let signature = ed25519_dalek::Signature::from_bytes(&sig_arr);
    vk.verify(&hash_arr, &signature).is_ok()
}

#[tauri::command]
pub async fn check_update(current_version: String) -> Result<UpdateInfo, String> {
    let platform = if cfg!(target_os = "windows") {
        "windows"
    } else {
        "linux"
    };
    let res = ops().updates(platform, &current_version).await?;

    let Some(release) = res.release else {
        return Ok(UpdateInfo {
            current_version,
            latest_version: None,
            up_to_date: true,
            artifact_url: None,
            signature_valid: false,
        });
    };

    let signature_valid = verify_release_signature(
        RELEASE_PUBLIC_KEY_HEX,
        &release.artifact_hash,
        &release.signature,
    );

    Ok(UpdateInfo {
        current_version,
        latest_version: Some(release.version),
        up_to_date: res.up_to_date,
        artifact_url: release.artifact_url,
        signature_valid,
    })
}
