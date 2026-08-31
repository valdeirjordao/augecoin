use augecoin_contracts::{TokenCall, TokenInit};
use augecoin_core::constants::{CT_CHAIN_ID_MAINNET, MIN_FEE_AUGESAT};
use augecoin_core::operation::{
    AddressReceiverInfo, Operation, OperationPayload, OperationType, ReceiverInfo, SenderInfo,
    ValidatorAdminOp,
};
use augecoin_crypto::hdkeys::HdWallet;
use augecoin_crypto::signature::{Ed25519Signature, HybridKeyPair};

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
    #[error("insufficient balance: have {balance}, need {required} (amount + fee)")]
    InsufficientBalance { balance: u64, required: u64 },
    #[error("account response missing field: {0}")]
    MissingField(String),
    #[error("fee below minimum: {got} augesat (minimum {min})")]
    FeeTooLow { got: u64, min: u64 },
    #[error("destination required: use --to <account> or --to-address <address>")]
    MissingDestination,
    #[error("invalid AUGE address: {0}")]
    InvalidAddress(String),
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

/// SafeBox inventory (Reserved / ForSale / Owned AUGEIDs) held by a validator
/// key. Falls back to an insecure TLS client when the endpoint presents a
/// self-signed certificate (common for self-hosted nodes).
pub async fn handle_validator_inventory(
    cli: &Cli,
    public_key: &str,
    json: bool,
) -> Result<(), CommandError> {
    let params = serde_json::json!({ "validator_public_key_hex": public_key });
    let client = RpcClient::new(cli.endpoint.clone())?;
    let result = match client.call("listvalidatorinventory", params.clone()).await {
        Ok(value) => value,
        Err(crate::rpc::RpcError::Http(_)) => {
            // Self-signed or private CA on self-hosted nodes: retry without
            // certificate verification.
            let insecure = RpcClient::new_insecure(cli.endpoint.clone())?;
            insecure.call("listvalidatorinventory", params).await?
        }
        Err(e) => return Err(e.into()),
    };

    if json {
        crate::output::print_output(&result, true);
        return Ok(());
    }

    let counts = |key: &str| {
        result
            .get(key)
            .and_then(|v| v.as_array())
            .map(|a| a.len())
            .unwrap_or(0)
    };
    let first_last = |key: &str| {
        result.get(key).and_then(|v| v.as_array()).and_then(|a| {
            let first = a.first()?.as_u64()?;
            let last = a.last()?.as_u64()?;
            Some((first, last))
        })
    };
    let (rf, rl) = first_last("reserved").unwrap_or((0, 0));
    println!("SafeBox inventory for {public_key}");
    println!(
        "  reserved: {} ids{}{}",
        counts("reserved"),
        if counts("reserved") > 0 {
            " (range "
        } else {
            ""
        },
        if counts("reserved") > 0 {
            format!("{rf}..{rl})")
        } else {
            String::new()
        }
    );
    println!("  for_sale: {}", counts("for_sale"));
    println!("  owned:    {}", counts("owned"));
    if let Some(owned) = result.get("owned").and_then(|v| v.as_array()) {
        if !owned.is_empty() {
            let list: Vec<String> = owned.iter().map(|v| v.to_string()).collect();
            println!("  owned ids: {}", list.join(", "));
        }
    }
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

#[derive(Debug)]
pub struct TransferArgs {
    pub from: u64,
    pub to: Option<u64>,
    pub to_address: Option<String>,
    pub amount: u64,
    pub fee: Option<u64>,
    pub n_operation: Option<u64>,
    pub chain_id: Option<u64>,
    pub key_hex_file: String,
}

/// Parse a raw private-key file: plain 32+ byte hex, optionally in
/// `AUGECOIN_VALIDATOR_KEY_HEX=<hex>` env-file style. The first 32 bytes are
/// used as the Ed25519 seed — same convention as AUGECOIN_VALIDATOR_KEY_HEX.
fn parse_raw_key_hex(content: &str) -> Result<HybridKeyPair, CommandError> {
    let trimmed = content.trim();
    let hex_str = match trimmed.split_once('=') {
        Some((_, value)) => value.trim(),
        None => trimmed,
    };
    let bytes = hex::decode(hex_str).map_err(|e| CommandError::InvalidHex(e.to_string()))?;
    if bytes.len() < 32 {
        return Err(CommandError::InvalidKeyLength {
            expected: 32,
            got: bytes.len(),
        });
    }
    let mut seed = [0u8; 32];
    seed.copy_from_slice(&bytes[..32]);
    Ok(HybridKeyPair::from_seed(seed))
}

/// Build and sign a Transaction (account → account) or AddressTransaction
/// (account → address) operation with the given keypair. Self-describing
/// destination addresses auto-activate a new on-chain account on first receive.
fn build_signed_transfer(
    keypair: &HybridKeyPair,
    args: &TransferArgs,
    n_operation: u64,
) -> Result<Operation, CommandError> {
    let fee = args.fee.unwrap_or(MIN_FEE_AUGESAT);
    if fee < MIN_FEE_AUGESAT {
        return Err(CommandError::FeeTooLow {
            got: fee,
            min: MIN_FEE_AUGESAT,
        });
    }
    if args.to.is_none() && args.to_address.is_none() {
        return Err(CommandError::MissingDestination);
    }

    let sender = SenderInfo {
        account: args.from,
        n_operation,
        amount: args.amount,
        payload: Vec::new(),
    };

    let mut op = match &args.to_address {
        Some(addr) => {
            let address = augecoin_crypto::address::AddressHash::parse(addr)
                .ok_or_else(|| CommandError::InvalidAddress(addr.clone()))?;
            Operation {
                op_type: OperationType::AddressTransaction,
                payload: OperationPayload::AddressTransaction {
                    senders: vec![sender],
                    receivers: vec![AddressReceiverInfo {
                        address,
                        amount: args.amount,
                        payload: Vec::new(),
                    }],
                    fee,
                },
                chain_id: args.chain_id.unwrap_or(CT_CHAIN_ID_MAINNET),
                signatures: vec![dummy_signature()],
            }
        }
        None => Operation {
            op_type: OperationType::Transaction,
            payload: OperationPayload::Transaction {
                senders: vec![sender],
                receivers: vec![ReceiverInfo {
                    account: args.to.expect("validated above"),
                    amount: args.amount,
                    payload: Vec::new(),
                }],
                changers: Vec::new(),
                fee,
            },
            chain_id: args.chain_id.unwrap_or(CT_CHAIN_ID_MAINNET),
            signatures: vec![dummy_signature()],
        },
    };

    let message = op.to_bytes_stripped();
    let sig = keypair.sign(&message);
    op.signatures = vec![sig];
    Ok(op)
}

pub struct GiftAccountArgs {
    pub account: u64,
    pub recipient_public_key: String,
    pub fee: Option<u64>,
    pub n_operation: Option<u64>,
    pub chain_id: Option<u64>,
    pub key_hex_file: String,
}

pub async fn handle_send_transfer(cli: &Cli, args: TransferArgs) -> Result<(), CommandError> {
    let key_content = std::fs::read_to_string(&args.key_hex_file).map_err(CommandError::KeyFile)?;
    let keypair = parse_raw_key_hex(&key_content)?;

    let client = RpcClient::new(cli.endpoint.clone())?;

    // n_operation: use the override or query the node for the current nonce.
    let account_info = client.get_account(args.from).await?;
    let n_operation = match args.n_operation {
        Some(n) => n,
        None => account_info
            .get("n_operation")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| CommandError::MissingField("n_operation".into()))?,
    };
    let balance = account_info
        .get("balance")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| CommandError::MissingField("balance".into()))?;
    let fee = args.fee.unwrap_or(MIN_FEE_AUGESAT);
    if balance < args.amount.saturating_add(fee) {
        return Err(CommandError::InsufficientBalance {
            balance,
            required: args.amount + fee,
        });
    }

    let op = build_signed_transfer(&keypair, &args, n_operation)?;
    let result = client.send_operation(&hex::encode(op.to_bytes())).await?;
    crate::output::print_output(&result, false);
    Ok(())
}

/// Build and sign a GiftAccount operation: hands a Reserved AUGEID over to a
/// recipient Ed25519 public key. The signer must be the AUGEID's current owner
/// key (the leader key that emitted it). Reserved accounts hold no balance, so
/// the fee defaults to zero (the mempool explicitly allows zero-fee gifts for
/// Reserved ids); the recipient activates the id by signing an AcceptGift.
pub async fn handle_gift_account(cli: &Cli, args: GiftAccountArgs) -> Result<(), CommandError> {
    let key_content = std::fs::read_to_string(&args.key_hex_file).map_err(CommandError::KeyFile)?;
    let keypair = parse_raw_key_hex(&key_content)?;

    let client = RpcClient::new(cli.endpoint.clone())?;

    let n_operation = match args.n_operation {
        Some(n) => n,
        None => client
            .get_account(args.account)
            .await?
            .get("n_operation")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| CommandError::MissingField("n_operation".into()))?,
    };

    let recipient_bytes = hex::decode(&args.recipient_public_key)
        .map_err(|e| CommandError::InvalidHex(e.to_string()))?;
    if recipient_bytes.len() != ED25519_PK_LEN {
        return Err(CommandError::InvalidKeyLength {
            expected: ED25519_PK_LEN,
            got: recipient_bytes.len(),
        });
    }
    let mut recipient_public_key = [0u8; ED25519_PK_LEN];
    recipient_public_key.copy_from_slice(&recipient_bytes);

    let mut op = Operation {
        op_type: OperationType::GiftAccount,
        payload: OperationPayload::GiftAccount {
            account: args.account,
            n_operation,
            recipient_public_key,
            fee: args.fee.unwrap_or(0),
        },
        chain_id: args.chain_id.unwrap_or(CT_CHAIN_ID_MAINNET),
        signatures: vec![dummy_signature()],
    };

    let message = op.to_bytes_stripped();
    op.signatures = vec![keypair.sign(&message)];
    let result = client.send_operation(&hex::encode(op.to_bytes())).await?;
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

// ── Smart contract commands (Fase 15) ─────────────────────────────────────

fn address_to_hex(addr: u64) -> String {
    hex::encode(addr.to_be_bytes())
}

fn code_id_to_hex(code_id: u64) -> String {
    hex::encode(code_id.to_be_bytes())
}

pub async fn handle_contract_store(
    cli: &Cli,
    wasm_hex: String,
    api_key: String,
    gas_limit_hex: Option<String>,
    json: bool,
) -> Result<(), CommandError> {
    let client = RpcClient::new(cli.endpoint.clone())?;
    let result = client
        .contract_store_code(&wasm_hex, &api_key, gas_limit_hex.as_deref())
        .await?;
    crate::output::print_output(&result, json);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn handle_contract_create(
    cli: &Cli,
    code_id: u64,
    name: String,
    symbol: String,
    decimals: u8,
    initial_supply: u64,
    max_supply: u64,
    mint_enabled: bool,
    burn_enabled: bool,
    api_key: String,
    gas_limit_hex: Option<String>,
    json: bool,
) -> Result<(), CommandError> {
    let init = TokenInit {
        name,
        symbol,
        decimals,
        initial_supply,
        max_supply,
        mint_enabled,
        burn_enabled,
    };
    let instantiate_hex = hex::encode(init.to_bytes());
    let client = RpcClient::new(cli.endpoint.clone())?;
    let result = client
        .contract_create(
            &code_id_to_hex(code_id),
            &instantiate_hex,
            &api_key,
            gas_limit_hex.as_deref(),
        )
        .await?;
    crate::output::print_output(&result, json);
    Ok(())
}

pub async fn handle_contract_info(
    cli: &Cli,
    contract_id_hex: String,
    json: bool,
) -> Result<(), CommandError> {
    let client = RpcClient::new(cli.endpoint.clone())?;
    let result = client.contract_get(&contract_id_hex).await?;
    crate::output::print_output(&result, json);
    Ok(())
}

pub async fn handle_contract_balance(
    cli: &Cli,
    contract_id_hex: String,
    address: u64,
    json: bool,
) -> Result<(), CommandError> {
    let client = RpcClient::new(cli.endpoint.clone())?;
    let result = client
        .contract_balance(&contract_id_hex, &address_to_hex(address))
        .await?;
    crate::output::print_output(&result, json);
    Ok(())
}

pub async fn handle_token_info(
    cli: &Cli,
    contract_id_hex: String,
    json: bool,
) -> Result<(), CommandError> {
    let client = RpcClient::new(cli.endpoint.clone())?;
    let result = client.contract_token_info(&contract_id_hex).await?;
    crate::output::print_output(&result, json);
    Ok(())
}

pub async fn handle_token_send(
    cli: &Cli,
    contract_id_hex: String,
    to: u64,
    amount: u64,
    api_key: String,
    gas_limit_hex: Option<String>,
    json: bool,
) -> Result<(), CommandError> {
    let call_hex = hex::encode(TokenCall::Transfer { to, amount }.to_bytes());
    let client = RpcClient::new(cli.endpoint.clone())?;
    let result = client
        .contract_execute(
            &contract_id_hex,
            &call_hex,
            &api_key,
            gas_limit_hex.as_deref(),
        )
        .await?;
    crate::output::print_output(&result, json);
    Ok(())
}

pub async fn handle_token_mint(
    cli: &Cli,
    contract_id_hex: String,
    to: u64,
    amount: u64,
    api_key: String,
    gas_limit_hex: Option<String>,
    json: bool,
) -> Result<(), CommandError> {
    let call_hex = hex::encode(TokenCall::Mint { to, amount }.to_bytes());
    let client = RpcClient::new(cli.endpoint.clone())?;
    let result = client
        .contract_execute(
            &contract_id_hex,
            &call_hex,
            &api_key,
            gas_limit_hex.as_deref(),
        )
        .await?;
    crate::output::print_output(&result, json);
    Ok(())
}

pub async fn handle_token_burn(
    cli: &Cli,
    contract_id_hex: String,
    amount: u64,
    api_key: String,
    gas_limit_hex: Option<String>,
    json: bool,
) -> Result<(), CommandError> {
    let call_hex = hex::encode(TokenCall::Burn { amount }.to_bytes());
    let client = RpcClient::new(cli.endpoint.clone())?;
    let result = client
        .contract_execute(
            &contract_id_hex,
            &call_hex,
            &api_key,
            gas_limit_hex.as_deref(),
        )
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

    fn test_key_hex() -> String {
        hex::encode([7u8; 32])
    }

    #[test]
    fn raw_key_hex_accepts_plain_and_env_style() {
        let hex_str = test_key_hex();

        let kp1 = parse_raw_key_hex(&hex_str).unwrap();
        let kp2 = parse_raw_key_hex(&format!("AUGECOIN_VALIDATOR_KEY_HEX={hex_str}")).unwrap();
        let kp3 = parse_raw_key_hex(&format!("  AUGECOIN_VALIDATOR_KEY_HEX={hex_str}\n")).unwrap();

        assert_eq!(
            kp1.verifying_key().to_bytes(),
            kp2.verifying_key().to_bytes()
        );
        assert_eq!(
            kp1.verifying_key().to_bytes(),
            kp3.verifying_key().to_bytes()
        );

        // Too short (< 32 bytes) must be rejected.
        assert!(parse_raw_key_hex("aabbcc").is_err());
        assert!(parse_raw_key_hex("zzzz").is_err());
    }

    fn transfer_args() -> TransferArgs {
        TransferArgs {
            from: 10,
            to: Some(20),
            to_address: None,
            amount: 500_000,
            fee: Some(MIN_FEE_AUGESAT),
            n_operation: None,
            chain_id: None,
            key_hex_file: String::new(),
        }
    }

    #[test]
    fn signed_transfer_requires_destination() {
        let keypair = parse_raw_key_hex(&test_key_hex()).unwrap();
        let mut args = transfer_args();
        args.to = None;
        assert!(matches!(
            build_signed_transfer(&keypair, &args, 1),
            Err(CommandError::MissingDestination)
        ));
    }

    #[test]
    fn signed_transfer_to_address_roundtrips_and_verifies() {
        let keypair = parse_raw_key_hex(&test_key_hex()).unwrap();
        // Self-describing destination (v2): embeds the recipient public key so
        // the node can auto-activate an account on first receive.
        let dest_kp = parse_raw_key_hex(&hex::encode([5u8; 32])).unwrap();
        let addr = augecoin_crypto::address::derive_embedded_address(&dest_kp.verifying_key());

        let mut args = transfer_args();
        args.to = None;
        args.to_address = Some(addr.clone());
        let op = build_signed_transfer(&keypair, &args, 7).unwrap();

        assert_eq!(op.op_type, OperationType::AddressTransaction);
        assert_eq!(op.chain_id, CT_CHAIN_ID_MAINNET);

        match &op.payload {
            OperationPayload::AddressTransaction {
                senders,
                receivers,
                fee,
            } => {
                assert_eq!(senders.len(), 1);
                assert_eq!(senders[0].account, 10);
                assert_eq!(senders[0].n_operation, 7);
                assert_eq!(receivers.len(), 1);
                assert_eq!(receivers[0].amount, 500_000);
                assert_eq!(
                    receivers[0].address.to_address(),
                    addr,
                    "self-describing roundtrip"
                );
                assert!(receivers[0].address.public_key.is_some());
                assert_eq!(*fee, MIN_FEE_AUGESAT);
            }
            _ => panic!("expected AddressTransaction payload"),
        }

        let bytes = op.to_bytes();
        let restored = Operation::from_bytes(&bytes).unwrap();
        assert_eq!(restored.to_bytes(), bytes);
        assert!(
            restored.signatures[0].verify(&keypair.verifying_key(), &restored.to_bytes_stripped())
        );
    }

    #[test]
    fn transfer_rejects_invalid_address() {
        let keypair = parse_raw_key_hex(&test_key_hex()).unwrap();
        let mut args = transfer_args();
        args.to = None;
        args.to_address = Some("nao-eum-endereco".into());
        assert!(matches!(
            build_signed_transfer(&keypair, &args, 1),
            Err(CommandError::InvalidAddress(_))
        ));
    }

    #[test]
    fn signed_transfer_roundtrips_and_verifies() {
        let keypair = parse_raw_key_hex(&test_key_hex()).unwrap();
        let args = transfer_args();
        let op = build_signed_transfer(&keypair, &args, 5).unwrap();

        assert_eq!(op.op_type, OperationType::Transaction);
        assert_eq!(op.chain_id, CT_CHAIN_ID_MAINNET);
        assert_eq!(op.signatures.len(), 1);

        match &op.payload {
            OperationPayload::Transaction {
                senders,
                receivers,
                fee,
                ..
            } => {
                assert_eq!(senders.len(), 1);
                assert_eq!(senders[0].account, 10);
                assert_eq!(senders[0].n_operation, 5);
                assert_eq!(senders[0].amount, 500_000);
                assert_eq!(receivers.len(), 1);
                assert_eq!(receivers[0].account, 20);
                assert_eq!(*fee, MIN_FEE_AUGESAT);
            }
            _ => panic!("expected Transaction payload"),
        }

        let bytes = op.to_bytes();
        let restored = Operation::from_bytes(&bytes).unwrap();
        assert_eq!(restored.to_bytes(), bytes);

        let message = restored.to_bytes_stripped();
        assert!(restored.signatures[0].verify(&keypair.verifying_key(), &message));
    }

    #[test]
    fn transfer_fee_below_minimum_is_rejected() {
        let keypair = parse_raw_key_hex(&test_key_hex()).unwrap();
        let mut args = transfer_args();
        args.fee = Some(MIN_FEE_AUGESAT - 1);
        assert!(matches!(
            build_signed_transfer(&keypair, &args, 1),
            Err(CommandError::FeeTooLow { .. })
        ));
    }

    #[test]
    fn transfer_signature_depends_on_key() {
        let args = transfer_args();
        let kp_a = parse_raw_key_hex(&test_key_hex()).unwrap();
        let kp_b = parse_raw_key_hex(&hex::encode([9u8; 32])).unwrap();

        let op_a = build_signed_transfer(&kp_a, &args, 1).unwrap();
        let op_b = build_signed_transfer(&kp_b, &args, 1).unwrap();

        assert_ne!(op_a.to_bytes(), op_b.to_bytes());

        // A signature from a different key must not verify.
        assert!(!op_b.signatures[0].verify(&kp_a.verifying_key(), &op_b.to_bytes_stripped()));
    }
}
