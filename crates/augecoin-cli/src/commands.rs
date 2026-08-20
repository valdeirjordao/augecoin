use augecoin_core::operation::{Operation, OperationPayload, OperationType, ValidatorAdminOp};
use augecoin_crypto::hdkeys::HdWallet;
use augecoin_crypto::signature::Ed25519Signature;

const ED25519_PK_LEN: usize = 32;
const ED25519_SIG_LEN: usize = 64;

use crate::rpc::RpcClient;
use crate::Cli;

#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    #[error("admin mnemonic required for this command (provide --mnemonic, --key-file, or AUGECOIN_ADMIN_MNEMONIC)")]
    NoAdminKey,
    #[error("invalid mnemonic: {0}")]
    InvalidMnemonic(String),
    #[error("invalid hex key: {0}")]
    InvalidHex(String),
    #[error("invalid key length: expected {expected}, got {got}")]
    InvalidKeyLength { expected: usize, got: usize },
    #[error("RPC error: {0}")]
    Rpc(#[from] crate::rpc::RpcError),
    #[error("failed to read key file: {0}")]
    KeyFile(std::io::Error),
}

fn dummy_signature() -> Ed25519Signature {
    Ed25519Signature {
        bytes: [0u8; ED25519_SIG_LEN],
    }
}

fn load_mnemonic(cli: &Cli) -> Result<String, CommandError> {
    if let Some(ref m) = cli.mnemonic {
        return Ok(m.clone());
    }
    if let Some(ref path) = cli.key_file {
        let content = std::fs::read_to_string(path).map_err(CommandError::KeyFile)?;
        return Ok(content.trim().to_string());
    }
    Err(CommandError::NoAdminKey)
}

fn parse_hex_fixed<const N: usize>(hex_str: &str) -> Result<[u8; N], CommandError> {
    let bytes = hex::decode(hex_str.trim()).map_err(|e| CommandError::InvalidHex(e.to_string()))?;
    if bytes.len() != N {
        return Err(CommandError::InvalidKeyLength {
            expected: N,
            got: bytes.len(),
        });
    }
    let mut arr = [0u8; N];
    arr.copy_from_slice(&bytes);
    Ok(arr)
}

fn sign_admin_operation(
    va_op: ValidatorAdminOp,
    mnemonic: &str,
) -> Result<Operation, CommandError> {
    let wallet = HdWallet::from_mnemonic(mnemonic).map_err(CommandError::InvalidMnemonic)?;
    let keypair = wallet.derive_keypair(0);

    let mut op = Operation {
        op_type: OperationType::ValidatorAdminOp,
        payload: OperationPayload::ValidatorAdmin(va_op),
        chain_id: augecoin_core::constants::CT_CHAIN_ID_MAINNET,
        signatures: vec![dummy_signature()],
    };

    let message = op.to_bytes_stripped();
    let sig = keypair.sign(&message);
    op.signatures = vec![sig];

    Ok(op)
}

pub async fn handle_validator_list(cli: &Cli, json: bool) -> Result<(), CommandError> {
    let client = RpcClient::new(cli.endpoint.clone())?;
    let result = client.get_validator_set().await?;
    crate::output::print_output(&result, json);
    Ok(())
}

pub async fn handle_validator_add(
    cli: &Cli,
    ed25519_key: &str,
    activation_height: u64,
) -> Result<(), CommandError> {
    let mnemonic = load_mnemonic(cli)?;
    let ed_pk: [u8; ED25519_PK_LEN] = parse_hex_fixed(ed25519_key)?;

    let va_op = ValidatorAdminOp::Add {
        ed25519_public_key: ed_pk,
        activation_height,
    };

    let op = sign_admin_operation(va_op, &mnemonic)?;
    let op_hex = hex::encode(op.to_bytes());

    let client = RpcClient::new(cli.endpoint.clone())?;
    let result = client.send_operation(&op_hex).await?;
    crate::output::print_output(&result, false);
    Ok(())
}

pub async fn handle_validator_remove(
    cli: &Cli,
    validator_id: u64,
    activation_height: u64,
) -> Result<(), CommandError> {
    let mnemonic = load_mnemonic(cli)?;

    let va_op = ValidatorAdminOp::Remove {
        validator_id,
        activation_height,
    };

    let op = sign_admin_operation(va_op, &mnemonic)?;
    let op_hex = hex::encode(op.to_bytes());

    let client = RpcClient::new(cli.endpoint.clone())?;
    let result = client.send_operation(&op_hex).await?;
    crate::output::print_output(&result, false);
    Ok(())
}

pub async fn handle_validator_activate(
    cli: &Cli,
    validator_id: u64,
    activation_height: u64,
) -> Result<(), CommandError> {
    let mnemonic = load_mnemonic(cli)?;

    let va_op = ValidatorAdminOp::Activate {
        validator_id,
        activation_height,
    };

    let op = sign_admin_operation(va_op, &mnemonic)?;
    let op_hex = hex::encode(op.to_bytes());

    let client = RpcClient::new(cli.endpoint.clone())?;
    let result = client.send_operation(&op_hex).await?;
    crate::output::print_output(&result, false);
    Ok(())
}

pub async fn handle_validator_deactivate(
    cli: &Cli,
    validator_id: u64,
    activation_height: u64,
) -> Result<(), CommandError> {
    let mnemonic = load_mnemonic(cli)?;

    let va_op = ValidatorAdminOp::Deactivate {
        validator_id,
        activation_height,
    };

    let op = sign_admin_operation(va_op, &mnemonic)?;
    let op_hex = hex::encode(op.to_bytes());

    let client = RpcClient::new(cli.endpoint.clone())?;
    let result = client.send_operation(&op_hex).await?;
    crate::output::print_output(&result, false);
    Ok(())
}

pub async fn handle_status(cli: &Cli, json: bool) -> Result<(), CommandError> {
    let client = RpcClient::new(cli.endpoint.clone())?;

    let net_status = client.get_network_status().await?;
    let validator_set = client.get_validator_set().await?;

    let current_height = net_status
        .get("current_height")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let peers = net_status
        .get("peers_connected")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let syncing = net_status
        .get("syncing")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let sync_target = net_status
        .get("sync_target_height")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    let active_count = validator_set
        .get("validators")
        .and_then(|v| {
            v.as_array().map(|arr| {
                arr.iter()
                    .filter(|entry| entry.get("status").and_then(|s| s.as_str()) == Some("active"))
                    .count() as u64
            })
        })
        .unwrap_or(0);
    let total_validators = validator_set
        .get("total_validators")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let quorum_needed = if active_count > 0 {
        (active_count * 2) / 3 + 1
    } else {
        0
    };
    let quorum_healthy = active_count >= quorum_needed;

    let output = serde_json::json!({
        "network": {
            "current_height": current_height,
            "peers_connected": peers,
            "syncing": syncing,
            "sync_target_height": if syncing { sync_target } else { 0 }
        },
        "quorum": {
            "active_validators": active_count,
            "total_validators": total_validators,
            "quorum_threshold": quorum_needed,
            "healthy": quorum_healthy
        }
    });

    crate::output::print_output(&output, json);
    Ok(())
}

pub async fn handle_validator_earnings(
    cli: &Cli,
    id: Option<u64>,
    all: bool,
    json: bool,
) -> Result<(), CommandError> {
    let client = RpcClient::new(cli.endpoint.clone())?;
    let earnings = client.get_validator_earnings().await?;

    let entries = earnings
        .get("entries")
        .and_then(|v| v.as_array())
        .map(|arr| {
            let mut items: Vec<serde_json::Value> = arr
                .iter()
                .map(|entry| {
                    let validator_id = entry
                        .get("validator_id")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    let total = entry
                        .get("total_earned_auge")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    let auge = total as f64 / 100_000_000.0;
                    serde_json::json!({
                        "validator_id": validator_id,
                        "total_earned_augesat": total,
                        "total_earned_auge": format!("{:.8}", auge)
                    })
                })
                .collect::<Vec<_>>();

            items.sort_by(|a, b| {
                let b_total = b
                    .get("total_earned_augesat")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let a_total = a
                    .get("total_earned_augesat")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                b_total.cmp(&a_total)
            });

            items
        })
        .unwrap_or_default();

    let filtered: Vec<serde_json::Value> = if let Some(target_id) = id {
        entries
            .into_iter()
            .filter(|e| {
                e.get("validator_id")
                    .and_then(|v| v.as_u64())
                    .map(|v| v == target_id)
                    .unwrap_or(false)
            })
            .collect()
    } else {
        entries
    };

    let _ = all;

    crate::output::print_output(&serde_json::json!(filtered), json);
    Ok(())
}

pub async fn handle_security_equivocations(cli: &Cli, json: bool) -> Result<(), CommandError> {
    let client = RpcClient::new(cli.endpoint.clone())?;
    let proofs = client.get_equivocation_proofs().await?;
    crate::output::print_output(&proofs, json);
    Ok(())
}

pub async fn handle_security_banned_peers(cli: &Cli, json: bool) -> Result<(), CommandError> {
    let client = RpcClient::new(cli.endpoint.clone())?;
    let peers = client.get_banned_peers().await?;
    crate::output::print_output(&peers, json);
    Ok(())
}

pub async fn handle_security_alerts(cli: &Cli, tail: bool, json: bool) -> Result<(), CommandError> {
    let client = RpcClient::new(cli.endpoint.clone())?;

    if tail {
        eprintln!("monitoring alerts (Ctrl+C to stop)...");
        loop {
            let alerts = client.get_recent_alerts().await?;
            if let Some(arr) = alerts.as_array() {
                if !arr.is_empty() {
                    for alert in arr {
                        if json {
                            println!("{}", serde_json::to_string(alert).unwrap_or_default());
                        } else {
                            let timestamp =
                                alert.get("timestamp").and_then(|v| v.as_u64()).unwrap_or(0);
                            let event = alert
                                .get("event")
                                .and_then(|v| v.as_str())
                                .unwrap_or("unknown");
                            let detail = alert.get("detail").and_then(|v| v.as_str()).unwrap_or("");
                            println!("[{timestamp}] {event}: {detail}");
                        }
                    }
                }
            }
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        }
    } else {
        let alerts = client.get_recent_alerts().await?;
        crate::output::print_output(&alerts, json);
        Ok(())
    }
}

pub async fn handle_get_account(
    cli: &Cli,
    account_number: u64,
    json: bool,
) -> Result<(), CommandError> {
    let client = RpcClient::new(cli.endpoint.clone())?;
    let result = client.get_account(account_number).await?;
    crate::output::print_output(&result, json);
    Ok(())
}

pub async fn handle_get_block(
    cli: &Cli,
    block_number: u64,
    json: bool,
) -> Result<(), CommandError> {
    let client = RpcClient::new(cli.endpoint.clone())?;
    let result = client.get_block(block_number).await?;
    crate::output::print_output(&result, json);
    Ok(())
}

pub async fn handle_send_operation(cli: &Cli, op_hex: &str) -> Result<(), CommandError> {
    let client = RpcClient::new(cli.endpoint.clone())?;
    let result = client.send_operation(op_hex).await?;
    crate::output::print_output(&result, false);
    Ok(())
}

pub async fn handle_get_pendings(cli: &Cli, json: bool) -> Result<(), CommandError> {
    let client = RpcClient::new(cli.endpoint.clone())?;
    let result = client.get_pendings().await?;
    crate::output::print_output(&result, json);
    Ok(())
}

pub async fn handle_find_accounts(
    cli: &Cli,
    name: Option<String>,
    account_type: Option<u16>,
    min_balance: Option<u64>,
    max_balance: Option<u64>,
    json: bool,
) -> Result<(), CommandError> {
    let client = RpcClient::new(cli.endpoint.clone())?;
    let result = client
        .find_accounts(name.as_deref(), account_type, min_balance, max_balance)
        .await?;
    crate::output::print_output(&result, json);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::VerifyingKey;

    const TEST_MNEMONIC: &str =
        "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

    fn dummy_verifying_key() -> VerifyingKey {
        let wallet = HdWallet::from_mnemonic(TEST_MNEMONIC).unwrap();
        wallet.derive_keypair(0).verifying_key()
    }

    fn dummy_ed_hex() -> String {
        hex::encode(dummy_verifying_key().to_bytes())
    }

    #[test]
    fn admin_add_operation_is_valid() {
        let ed_hex = dummy_ed_hex();

        let va_op = ValidatorAdminOp::Add {
            ed25519_public_key: parse_hex_fixed(&ed_hex).unwrap(),
            activation_height: 100,
        };

        let op = sign_admin_operation(va_op, TEST_MNEMONIC).unwrap();

        assert_eq!(op.op_type, OperationType::ValidatorAdminOp);
        assert!(matches!(
            op.payload,
            OperationPayload::ValidatorAdmin(ValidatorAdminOp::Add { .. })
        ));

        let bytes = op.to_bytes();
        let restored = Operation::from_bytes(&bytes).unwrap();
        assert_eq!(op.payload, restored.payload);
        assert_eq!(op.signatures.len(), restored.signatures.len());
    }

    #[test]
    fn admin_remove_operation_is_valid() {
        let va_op = ValidatorAdminOp::Remove {
            validator_id: 1,
            activation_height: 200,
        };
        let op = sign_admin_operation(va_op, TEST_MNEMONIC).unwrap();
        let bytes = op.to_bytes();
        let restored = Operation::from_bytes(&bytes).unwrap();
        assert!(matches!(
            restored.payload,
            OperationPayload::ValidatorAdmin(ValidatorAdminOp::Remove { .. })
        ));
    }

    #[test]
    fn admin_activate_operation_is_valid() {
        let va_op = ValidatorAdminOp::Activate {
            validator_id: 2,
            activation_height: 300,
        };
        let op = sign_admin_operation(va_op, TEST_MNEMONIC).unwrap();
        let bytes = op.to_bytes();
        let restored = Operation::from_bytes(&bytes).unwrap();
        assert!(matches!(
            restored.payload,
            OperationPayload::ValidatorAdmin(ValidatorAdminOp::Activate { .. })
        ));
    }

    #[test]
    fn admin_deactivate_operation_is_valid() {
        let va_op = ValidatorAdminOp::Deactivate {
            validator_id: 3,
            activation_height: 400,
        };
        let op = sign_admin_operation(va_op, TEST_MNEMONIC).unwrap();
        let bytes = op.to_bytes();
        let restored = Operation::from_bytes(&bytes).unwrap();
        assert!(matches!(
            restored.payload,
            OperationPayload::ValidatorAdmin(ValidatorAdminOp::Deactivate { .. })
        ));
    }

    #[test]
    fn admin_operation_signature_verifies() {
        let ed_hex = dummy_ed_hex();

        let va_op = ValidatorAdminOp::Add {
            ed25519_public_key: parse_hex_fixed(&ed_hex).unwrap(),
            activation_height: 100,
        };
        let op = sign_admin_operation(va_op, TEST_MNEMONIC).unwrap();

        let wallet = HdWallet::from_mnemonic(TEST_MNEMONIC).unwrap();
        let admin_pk = wallet.derive_keypair(0).verifying_key();
        let message = op.to_bytes_stripped();

        assert!(op.signatures[0].verify(&admin_pk, &message));
    }

    #[test]
    fn invalid_mnemonic_is_rejected() {
        let va_op = ValidatorAdminOp::Remove {
            validator_id: 1,
            activation_height: 100,
        };
        let result = sign_admin_operation(va_op, "not a valid mnemonic at all");
        assert!(result.is_err());
    }

    #[test]
    fn invalid_hex_key_is_rejected() {
        let result = parse_hex_fixed::<32>("not-hex!!!");
        assert!(result.is_err());
    }

    #[test]
    fn wrong_hex_key_length_is_rejected() {
        let result = parse_hex_fixed::<32>("aabb");
        assert!(result.is_err());
    }

    #[test]
    fn different_mnemonic_produces_different_operation() {
        let wallet = HdWallet::from_mnemonic(TEST_MNEMONIC).unwrap();
        let pk = wallet.derive_keypair(0).verifying_key();
        let ed_hex = hex::encode(pk.to_bytes());

        let va_op = ValidatorAdminOp::Add {
            ed25519_public_key: parse_hex_fixed(&ed_hex).unwrap(),
            activation_height: 100,
        };

        let op = sign_admin_operation(va_op, TEST_MNEMONIC).unwrap();
        let bytes1 = op.to_bytes();

        let wrong_mnemonic =
            "legal winner thank year wave sausage worth useful legal winner thank yellow";
        let wallet2 = HdWallet::from_mnemonic(wrong_mnemonic).unwrap();
        let pk2 = wallet2.derive_keypair(0).verifying_key();
        let ed_hex2 = hex::encode(pk2.to_bytes());

        let va_op2 = ValidatorAdminOp::Add {
            ed25519_public_key: parse_hex_fixed(&ed_hex2).unwrap(),
            activation_height: 100,
        };

        let op2 = sign_admin_operation(va_op2, wrong_mnemonic).unwrap();
        let bytes2 = op2.to_bytes();

        assert_ne!(bytes1, bytes2);
    }
}
