//! Local state persistence for the validator desktop.
//!
//! Layout under the OS data dir (`~/.local/share/augecoin-validator` on Linux):
//!   state.json      — activation state (public info only)
//!   validator.key   — Ed25519 seed (hex), 0600 permissions; the private key
//!                     NEVER leaves this machine and is only fed to the node
//!                     process via env var.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub fn data_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("augecoin-validator")
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SavedState {
    pub license_key: String,
    pub augeid: Option<String>,
    pub public_key: String,
    pub machine_id: String,
    pub activated_at: Option<String>,
    pub app_version: Option<String>,
}

impl SavedState {
    /// True when a license has been activated on this machine.
    pub fn is_activated(&self) -> bool {
        self.augeid.is_some() && !self.public_key.is_empty()
    }
}

pub fn state_path() -> PathBuf {
    data_dir().join("state.json")
}

pub fn key_path() -> PathBuf {
    data_dir().join("validator.key")
}

pub fn load_state() -> SavedState {
    let path = state_path();
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_state(state: &SavedState) -> Result<(), String> {
    let dir = data_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let json = serde_json::to_string_pretty(state).map_err(|e| e.to_string())?;
    std::fs::write(state_path(), json).map_err(|e| e.to_string())
}

/// Persist the Ed25519 seed (hex) with owner-only permissions.
pub fn save_private_key(seed_hex: &str) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let dir = data_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = key_path();
    std::fs::write(&path, seed_hex).map_err(|e| e.to_string())?;
    let mut perms = std::fs::metadata(&path)
        .map_err(|e| e.to_string())?
        .permissions();
    perms.set_mode(0o600);
    std::fs::set_permissions(&path, perms).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn load_private_key() -> Option<String> {
    std::fs::read_to_string(key_path())
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Wipe the local state (deactivation / reset).
pub fn clear_state() -> Result<(), String> {
    let _ = std::fs::remove_file(state_path());
    let _ = std::fs::remove_file(key_path());
    Ok(())
}
