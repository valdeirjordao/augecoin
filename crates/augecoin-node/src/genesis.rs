use augecoin_core::account::Account;
use augecoin_crypto::signature::HybridKeyPair;
use augecoin_storage::Storage;
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Deserialize, Clone)]
pub struct GenesisConfig {
    pub genesis: GenesisBalances,
    #[serde(default)]
    pub accounts: Vec<GenesisAccount>,
    /// Shared genesis admin key (Caminho A): when present together with
    /// `validators`, every node booting non-dev builds the SAME initial
    /// ValidatorSet from this file instead of a self-only set.
    #[serde(default)]
    pub admin: Option<GenesisKey>,
    /// Initial validators, ids assigned in declaration order starting at 1.
    #[serde(default)]
    pub validators: Vec<GenesisKey>,
}

/// A 32-byte Ed25519 public key declared in the genesis file.
#[derive(Debug, Deserialize, Clone)]
pub struct GenesisKey {
    pub ed25519_public_key_hex: String,
}

impl GenesisKey {
    pub fn pubkey(&self, role: &str) -> Result<[u8; 32], String> {
        let bytes = hex::decode(self.ed25519_public_key_hex.trim())
            .map_err(|e| format!("{role} public key is not hex: {e}"))?;
        let key: [u8; 32] = bytes
            .try_into()
            .map_err(|_| format!("{role} public key must contain 32 bytes"))?;
        ed25519_dalek::VerifyingKey::from_bytes(&key)
            .map_err(|_| format!("{role} public key is not a valid Ed25519 key"))?;
        if key == [0u8; 32] {
            return Err(format!("{role} public key cannot be zero"));
        }
        Ok(key)
    }
}

impl GenesisConfig {
    /// Shared initial validator set declared by this file: ids start at 1 in
    /// declaration order. `None` when the file declares no validators (legacy
    /// self-only bootstrap).
    pub fn shared_validator_set(
        &self,
    ) -> Result<Option<(ed25519_dalek::VerifyingKey, Vec<(u64, [u8; 32])>)>, String> {
        if self.validators.is_empty() {
            return Ok(None);
        }
        let members = self
            .validators
            .iter()
            .enumerate()
            .map(|(i, v)| Ok((i as u64 + 1, v.pubkey("validator")?)))
            .collect::<Result<Vec<(u64, [u8; 32])>, String>>()?;
        let admin = match &self.admin {
            Some(a) => ed25519_dalek::VerifyingKey::from_bytes(&a.pubkey("admin")?)
                .map_err(|e| format!("admin key error: {e}"))?,
            None => return Err("genesis declares [[validators]] but no [admin] key".to_string()),
        };
        Ok(Some((admin, members)))
    }
}

/// Load the genesis config if a path is configured; `None` means no genesis
/// file (node boots with legacy defaults).
pub fn load_optional(path: Option<&Path>) -> Result<Option<GenesisConfig>, String> {
    match path {
        None => Ok(None),
        Some(p) => Ok(Some(GenesisConfig::load(p)?)),
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct GenesisBalances {
    pub chain_id: u64,
    pub treasury_balance: u64,
    pub faucet_balance: u64,
    pub validator_balance: u64,
    pub admin_balance: u64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct GenesisAccount {
    pub role: String,
    pub account_number: u64,
    pub balance: Option<u64>,
    pub nonce: Option<u64>,
    pub public_key_hex: Option<String>,
    pub private_key_file: Option<String>,
}

impl GenesisConfig {
    pub fn load(path: &Path) -> Result<Self, String> {
        let raw = std::fs::read_to_string(path)
            .map_err(|e| format!("cannot read genesis config '{}': {e}", path.display()))?;
        toml::from_str(&raw).map_err(|e| format!("invalid genesis config: {e}"))
    }
}

fn public_key(account: &GenesisAccount) -> Result<[u8; 32], String> {
    let bytes = if let Some(hex) = &account.public_key_hex {
        hex::decode(hex.trim())
            .map_err(|e| format!("{} public key is not hex: {e}", account.role))?
    } else if let Some(path) = &account.private_key_file {
        let raw = std::fs::read_to_string(path).map_err(|e| {
            format!(
                "cannot read {} private key file '{}': {e}",
                account.role, path
            )
        })?;
        let decoded = hex::decode(raw.trim())
            .map_err(|e| format!("{} private key is not hex: {e}", account.role))?;
        if decoded.len() < 32 {
            return Err(format!(
                "{} private key must contain 32 bytes",
                account.role
            ));
        }
        HybridKeyPair::from_seed(decoded[..32].try_into().unwrap())
            .verifying_key()
            .to_bytes()
            .to_vec()
    } else {
        return Err(format!(
            "{} needs public_key_hex or private_key_file",
            account.role
        ));
    };

    let key: [u8; 32] = bytes
        .try_into()
        .map_err(|_| format!("{} public key must contain 32 bytes", account.role))?;
    ed25519_dalek::VerifyingKey::from_bytes(&key)
        .map_err(|_| format!("{} public key is not a valid Ed25519 key", account.role))?;
    if key == [0u8; 32] {
        return Err(format!("{} public key cannot be zero", account.role));
    }
    Ok(key)
}

/// Initialize configured accounts once. Existing accounts are never overwritten.
pub fn initialize(storage: &Storage, config: Option<&GenesisConfig>) -> Result<(), String> {
    let Some(config) = config else {
        return Ok(());
    };
    let _configured_chain_id = config.genesis.chain_id;
    for entry in &config.accounts {
        if storage
            .get_account(entry.account_number)
            .map_err(|e| e.to_string())?
            .is_some()
        {
            continue;
        }
        let key = public_key(entry)?;
        let mut account = Account::new(entry.account_number, key, 0);
        account.balance = entry.balance.unwrap_or(match entry.role.as_str() {
            "treasury" => config.genesis.treasury_balance,
            "faucet" => config.genesis.faucet_balance,
            "validator_treasury" => config.genesis.validator_balance,
            "admin" => config.genesis.admin_balance,
            _ => 0,
        });
        account.n_operation = entry.nonce.unwrap_or(0);
        storage.put_account(&account).map_err(|e| e.to_string())?;
        println!(
            "[genesis] initialized role={} account={} balance={} key={}",
            entry.role,
            entry.account_number,
            account.balance,
            hex::encode(key)
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use augecoin_crypto::signature::HybridKeyPair;

    #[test]
    fn rejects_zero_key() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("genesis.toml");
        std::fs::write(&path, "[genesis]\nchain_id=2\ntreasury_balance=1\nfaucet_balance=1\nvalidator_balance=1\nadmin_balance=0\n[[accounts]]\nrole='faucet'\naccount_number=1\nbalance=1\npublic_key_hex='0000000000000000000000000000000000000000000000000000000000000000'\n").unwrap();
        let storage = Storage::open(dir.path().join("db")).unwrap();
        assert!(initialize(&storage, Some(&path)).is_err());
    }

    #[test]
    fn derives_public_key_from_external_seed() {
        let dir = tempfile::tempdir().unwrap();
        let key = dir.path().join("key.hex");
        std::fs::write(&key, hex::encode([7u8; 32])).unwrap();
        let expected = HybridKeyPair::from_seed([7u8; 32])
            .verifying_key()
            .to_bytes();
        let path = dir.path().join("genesis.toml");
        std::fs::write(&path, format!("[genesis]\nchain_id=2\ntreasury_balance=1\nfaucet_balance=1\nvalidator_balance=1\nadmin_balance=0\n[[accounts]]\nrole='faucet'\naccount_number=1\nprivate_key_file='{}'\n", key.display())).unwrap();
        let storage = Storage::open(dir.path().join("db")).unwrap();
        initialize(&storage, Some(&path)).unwrap();
        assert_eq!(
            storage
                .get_account(1)
                .unwrap()
                .unwrap()
                .account_info
                .account_key
                .ed25519_public_key,
            expected
        );
    }
}
