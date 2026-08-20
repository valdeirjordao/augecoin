//! Machine ID: a stable, non-clonable identifier derived from the hardware.
//!
//! The raw hardware identifiers are NEVER persisted — only the BLAKE3-256 hash
//! is produced and (optionally) stored. A persisted random "install id" is
//! mixed in so the result is stable across reboots but changes if the app data
//! directory (OS user account) is cloned to another machine.

use std::path::PathBuf;

fn data_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("augecoin-validator")
}

/// Persistent random install id (created once, stored in the app data dir).
fn install_id() -> String {
    let dir = data_dir();
    std::fs::create_dir_all(&dir).ok();
    let path = dir.join("install-id");
    if let Ok(existing) = std::fs::read_to_string(&path) {
        let id = existing.trim().to_string();
        if !id.is_empty() {
            return id;
        }
    }
    let mut bytes = [0u8; 32];
    use rand::RngCore;
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    let id = hex::encode(bytes);
    // Best effort: non-fatal if the file cannot be written.
    let _ = std::fs::write(&path, &id);
    id
}

/// Best-effort hardware fingerprint (never persisted raw).
fn hardware_info() -> String {
    let mut parts: Vec<String> = Vec::new();

    // Linux: stable machine id + CPU model + MAC addresses.
    for f in ["/etc/machine-id", "/var/lib/dbus/machine-id"] {
        if let Ok(s) = std::fs::read_to_string(f) {
            parts.push(s.trim().to_string());
            break;
        }
    }
    if let Ok(cpuinfo) = std::fs::read_to_string("/proc/cpuinfo") {
        for line in cpuinfo.lines() {
            if line.starts_with("model name")
                || line.starts_with("Hardware")
                || line.starts_with("Serial")
            {
                parts.push(line.to_string());
            }
        }
    }
    if let Ok(entries) = std::fs::read_dir("/sys/class/net") {
        let mut macs: Vec<String> = Vec::new();
        for entry in entries.flatten() {
            if let Ok(addr) = std::fs::read_to_string(entry.path().join("address")) {
                macs.push(addr.trim().to_string());
            }
        }
        macs.sort();
        macs.dedup();
        parts.extend(macs);
    }

    // Cross-platform fallbacks (hostname, OS, arch).
    parts.push(std::env::consts::OS.to_string());
    parts.push(std::env::consts::ARCH.to_string());
    parts.push(std::env::var("HOSTNAME").unwrap_or_default());
    parts.push(std::env::var("COMPUTERNAME").unwrap_or_default());

    parts
        .into_iter()
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// BLAKE3-256(install_id || hardware_info) as a 64-char hex string.
pub fn machine_id() -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(install_id().as_bytes());
    hasher.update(b"\x00");
    hasher.update(hardware_info().as_bytes());
    hasher.finalize().to_hex().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn machine_id_is_stable_and_hex64() {
        let a = machine_id();
        let b = machine_id();
        assert_eq!(a, b);
        assert_eq!(a.len(), 64);
        assert!(a.bytes().all(|c| c.is_ascii_hexdigit()));
    }
}
