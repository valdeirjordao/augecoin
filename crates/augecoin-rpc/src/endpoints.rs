use augecoin_consensus::validator::{ValidatorSet, ValidatorStatus};
use augecoin_core::account::Account;
use augecoin_core::account::AccountState;
use augecoin_core::block::OperationBlockHeader;
use augecoin_core::mempool::Mempool;
use augecoin_core::operation::{Operation, OperationPayload, ReceiverInfo, SenderInfo};
use augecoin_storage::Storage;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, AtomicU64};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::auth::{ApiKeyStore, RateLimiter};

fn valid_address(address: &str) -> bool {
    augecoin_crypto::address::validate_address(address)
}

/// Shared runtime status snapshot, updated by the node main loop and
/// read by the RPC layer. All fields use interior mutability so that
/// the node can update the snapshot without re-locking the AppState.
#[derive(Debug)]
pub struct NodeStatus {
    pub block_height: AtomicU64,
    pub latest_block_hash: std::sync::Mutex<[u8; 64]>,
    pub peers_connected: AtomicU32,
    /// Peers in the gossipsub mesh of the consensus topics (the peers that
    /// actually exchange consensus messages).
    pub peers_gossipsub_consensus: AtomicU32,
    /// Peers known via Kademlia DHT discovery (not necessarily connected).
    pub peers_kademlia_total: AtomicU32,
    pub syncing: AtomicU64,
    pub sync_target_height: AtomicU64,
    pub mempool_size: AtomicU64,
    pub current_round: AtomicU64,
    pub current_view: AtomicU64,
    pub validator_id: AtomicU64,
    pub chain_id: AtomicU64,
    pub uptime_seconds: AtomicU64,
    pub last_consensus_error: std::sync::Mutex<Option<String>>,
}

impl Default for NodeStatus {
    fn default() -> Self {
        NodeStatus {
            block_height: AtomicU64::new(0),
            latest_block_hash: std::sync::Mutex::new([0u8; 64]),
            peers_connected: AtomicU32::new(0),
            peers_gossipsub_consensus: AtomicU32::new(0),
            peers_kademlia_total: AtomicU32::new(0),
            syncing: AtomicU64::new(0),
            sync_target_height: AtomicU64::new(0),
            mempool_size: AtomicU64::new(0),
            current_round: AtomicU64::new(0),
            current_view: AtomicU64::new(0),
            validator_id: AtomicU64::new(0),
            chain_id: AtomicU64::new(0),
            uptime_seconds: AtomicU64::new(0),
            last_consensus_error: std::sync::Mutex::new(None),
        }
    }
}

pub struct AppState {
    pub storage: Arc<Storage>,
    pub mempool: Arc<std::sync::Mutex<Mempool>>,
    pub node_status: Arc<NodeStatus>,
    pub api_keys: ApiKeyStore,
    pub rate_limiter: RateLimiter,
    /// Stricter limiter applied to spam-sensitive endpoints
    /// (`createaccount`, `faucet`) on top of the global limiter.
    pub sensitive_rate_limiter: RateLimiter,
    pub require_admin_auth: bool,
    pub faucet_keypair: Option<augecoin_crypto::signature::HybridKeyPair>,
    pub faucet_account: u64,
    pub faucet_amount: u64,
    pub faucet_claims: std::sync::Mutex<HashMap<String, u64>>,
    pub admin_keypair: Option<augecoin_crypto::signature::HybridKeyPair>,
    /// When set, operations admitted through RPC are broadcast to the P2P
    /// network so every validator's mempool converges (otherwise only the
    /// node that received the RPC call would ever see the operation).
    pub op_broadcaster: Option<crossbeam_channel::Sender<Vec<u8>>>,
}

impl AppState {
    /// Gossip a freshly admitted operation to the rest of the network.
    pub fn broadcast_op(&self, op_bytes: Vec<u8>) {
        if let Some(ref tx) = self.op_broadcaster {
            let _ = tx.send(op_bytes);
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateAccountParams {
    /// The Reserved AUGEID number to activate. Pass `0` to auto-select the
    /// first available Reserved AUGEID (recommended for wallet on-boarding).
    pub account_number: u64,
    pub public_key_hex: String,
    #[serde(default)]
    pub metadata_hex: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CreateAccountResult {
    pub accepted: bool,
    /// "pending" (operation submitted to consensus), "exists" (account already registered)
    /// or "rejected" (operation not admitted).
    pub status: String,
    pub op_hash_hex: Option<String>,
    /// Only known immediately when the account already exists; otherwise the
    /// number is assigned by consensus when the block commits (see getaccount).
    pub account_number: Option<u64>,
    pub address: String,
    pub public_key: String,
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct FaucetParams {
    #[serde(default)]
    pub address: Option<String>,
    /// Destination AUGEID (account number). Payments target AUGEIDs, not
    /// addresses.
    #[serde(default)]
    pub account_number: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct FaucetResult {
    pub success: bool,
    pub tx_hash: Option<String>,
    pub error: Option<String>,
}

fn default_max() -> u64 {
    100
}

// ── getaccount ───────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct GetAccountParams {
    #[serde(default)]
    pub account_number: Option<u64>,
    #[serde(default)]
    pub address: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AccountInfoResponse {
    pub account_number: u64,
    pub balance: u64,
    pub n_operation: u64,
    pub name: Option<String>,
    pub account_type: u16,
    pub account_data_hex: String,
    pub account_seal_hex: String,
    pub state: String,
    pub updated_on_block_passive_mode: u64,
    pub updated_on_block_active_mode: u64,
    pub locked_until_block: u64,
    pub price: u64,
    pub account_to_pay: u64,
    pub account_key_ed_hex: String,
    pub address: String,
}

// ── getblockoperations / getblock ────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct GetBlockParams {
    pub block_number: u64,
}

#[derive(Debug, Serialize)]
pub struct BlockHeaderInfo {
    pub block_number: u64,
    pub account_key_hex: String,
    pub reward: u64,
    pub fee: u64,
    pub protocol_version: u16,
    pub protocol_available: u16,
    pub timestamp: u64,
    pub initial_safe_box_hash_hex: String,
    pub operations_hash_hex: String,
    pub block_hash_hex: String,
    pub block_payload_hex: String,
    pub proof_of_work_hex: String,
    pub previous_proof_of_work_hex: String,
    pub leader_id: u64,
}

#[derive(Debug, Serialize)]
pub struct BlockOperationsResponse {
    pub block: BlockHeaderInfo,
    pub operations: Vec<OperationInfo>,
}

#[derive(Debug, Serialize)]
pub struct OperationInfo {
    pub op_type: u8,
    pub op_type_name: String,
    pub signatures_count: usize,
    pub op_hash_hex: String,
    pub payload: serde_json::Value,
}

// ── sendoperation ─────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct SendOperationParams {
    pub hex: String,
}

/// Unified send envelope. The wallet signs locally and supplies `hex`; `to`
/// and `amount` are retained for clients that build the operation themselves.
#[derive(Debug, Deserialize)]
pub struct SendParams {
    #[serde(default)]
    pub hex: Option<String>,
    #[serde(default)]
    pub to: Option<String>,
    #[serde(default)]
    pub amount: Option<u64>,
}

pub fn handle_send(params: SendParams, state: &AppState) -> Result<SendOperationResult, String> {
    if let Some(hex) = params.hex {
        return handle_send_operation(SendOperationParams { hex }, state);
    }
    let _ = (params.to, params.amount);
    Err("send requires a locally signed operation in 'hex'; private keys are never sent to the node".into())
}

#[derive(Debug, Serialize)]
pub struct SendOperationResult {
    pub accepted: bool,
    pub op_hash_hex: String,
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ResolveAddressParams {
    pub address: String,
}

#[derive(Debug, Serialize)]
pub struct ResolveAddressResult {
    pub exists: bool,
    pub account: Option<u64>,
    pub public_key: Option<String>,
}

pub fn handle_resolve_address(
    params: ResolveAddressParams,
    state: &AppState,
) -> Result<ResolveAddressResult, String> {
    let hash = augecoin_crypto::address::AddressHash::parse(params.address.trim())
        .ok_or_else(|| "invalid AUGE address".to_string())?;
    let account = state
        .storage
        .resolve_address(&hash)
        .map_err(|e| e.to_string())?;
    Ok(ResolveAddressResult {
        exists: account.is_some(),
        account,
        public_key: None,
    })
}

#[derive(Debug, Deserialize)]
pub struct SendOperationsParams {
    /// Ordered operations from one sender.
    pub operations: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct SendOperationsResult {
    pub accepted: usize,
    pub rejected: usize,
    pub errors: Vec<String>,
}

const OPERATION_BATCH_MARKER: u8 = 0xff;

fn encode_operation_batch(operations: &[Vec<u8>]) -> Vec<u8> {
    let capacity = 1 + 4 + operations.iter().map(|op| 4 + op.len()).sum::<usize>();
    let mut encoded = Vec::with_capacity(capacity);
    encoded.push(OPERATION_BATCH_MARKER);
    encoded.extend_from_slice(&(operations.len() as u32).to_be_bytes());
    for operation in operations {
        encoded.extend_from_slice(&(operation.len() as u32).to_be_bytes());
        encoded.extend_from_slice(operation);
    }
    encoded
}

// ── getoperations ────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct GetOperationsParams {
    pub block_number: u64,
    #[serde(default)]
    pub start: u64,
    /// Optional page size. Omitted keeps the legacy full-block response.
    pub limit: Option<u64>,
}

// ── getblockbyhash / getoperationbyhash ──────────────────────────────

/// Maximum number of blocks to scan (from the tip backwards) when resolving a
/// hash without a reverse index. `None` scans the whole available range.
const DEFAULT_HASH_SCAN_BLOCKS: u64 = 100_000;

#[derive(Debug, Deserialize)]
pub struct GetByHashParams {
    pub hash_hex: String,
    #[serde(default)]
    pub max_blocks: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct OperationByHashResponse {
    pub block_number: u64,
    pub op_index: usize,
    pub op_hash_hex: String,
    pub operation: OperationInfo,
}

// ── getpendings ──────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct PendingOpsResponse {
    pub size: usize,
    pub operations: Vec<PendingOpEntry>,
}

#[derive(Debug, Serialize)]
pub struct PendingOpEntry {
    pub op_type_name: String,
    pub op_hash_hex: String,
}

// ── findaccounts ─────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct FindAccountsParams {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub account_type: Option<u16>,
    #[serde(default)]
    pub min_balance: Option<u64>,
    #[serde(default)]
    pub max_balance: Option<u64>,
    #[serde(default)]
    pub start: u64,
    #[serde(default = "default_max")]
    pub max: u64,
}

#[derive(Debug, Serialize)]
pub struct FindAccountsResponse {
    pub accounts: Vec<AccountInfoResponse>,
    pub total: u64,
}

// ── nodestatus ───────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct NodeStatusResponse {
    pub current_height: u64,
    pub latest_block_hash_hex: String,
    /// Unique connected peers (deduplicated by peer id).
    pub peers_connected: u32,
    /// Peers in the gossipsub consensus mesh.
    pub peers_gossipsub_consensus: u32,
    /// Peers known via Kademlia DHT discovery (may not be connected).
    pub peers_kademlia_total: u32,
    pub syncing: bool,
    pub sync_target_height: u64,
    pub total_accounts: u64,
    pub total_blocks: u64,
    pub mempool_size: u64,
    pub current_round: u64,
    pub current_view: u64,
    pub validator_id: u64,
    pub chain_id: u64,
    pub uptime_seconds: u64,
    pub last_consensus_error: Option<String>,
}

// ── getvalidatorset ──────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct ValidatorSetResponse {
    pub active: Vec<ValidatorEntryResponse>,
    pub total: usize,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ValidatorEntryResponse {
    pub id: u64,
    pub status: String,
    pub ed25519_public_key_hex: String,
}

// ── validator admin ──────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ValidatorAddParams {
    pub ed25519_public_key_hex: String,
    pub activation_height: u64,
}

#[derive(Debug, Deserialize)]
pub struct ValidatorRemoveParams {
    pub validator_id: u64,
    pub activation_height: u64,
}

#[derive(Debug, Deserialize)]
pub struct ValidatorActivateParams {
    pub validator_id: u64,
    pub activation_height: u64,
}

#[derive(Debug, Deserialize)]
pub struct ValidatorDeactivateParams {
    pub validator_id: u64,
    pub activation_height: u64,
}

#[derive(Debug, Serialize)]
pub struct AdminResult {
    pub success: bool,
    pub error: Option<String>,
}

// ── Endpoint handlers ───────────────────────────────────────────────

pub fn handle_get_account(
    params: GetAccountParams,
    state: &AppState,
) -> Result<AccountInfoResponse, String> {
    let acc = match (params.account_number, params.address.as_deref()) {
        (Some(number), _) => state
            .storage
            .get_account(number)
            .map_err(|e| format!("storage error: {e}"))?
            .ok_or_else(|| format!("account {} not found", number))?,
        (None, Some(address)) => {
            let hash = augecoin_crypto::address::AddressHash::parse(address)
                .ok_or_else(|| "invalid AUGE address".to_string())?;
            let number = state
                .storage
                .resolve_address(&hash)
                .map_err(|e| format!("storage error: {e}"))?
                .ok_or_else(|| format!("account with address {address} not found"))?;
            state
                .storage
                .get_account(number)
                .map_err(|e| e.to_string())?
                .ok_or_else(|| "account disappeared".to_string())?
        }
        (None, None) => return Err("account_number or address is required".into()),
    };

    Ok(account_to_response(&acc))
}

pub fn handle_create_account(
    params: CreateAccountParams,
    state: &AppState,
) -> Result<CreateAccountResult, String> {
    let key: [u8; 32] = hex::decode(params.public_key_hex.trim())
        .map_err(|e| format!("invalid public key hex: {e}"))?
        .try_into()
        .map_err(|_| "public key must contain 32 bytes".to_string())?;
    let verifying = ed25519_dalek::VerifyingKey::from_bytes(&key)
        .map_err(|_| "invalid Ed25519 public key".to_string())?;
    let address = augecoin_crypto::address::derive_address(&verifying);

    let metadata: Vec<u8> = match params.metadata_hex {
        Some(ref hex_str) if !hex_str.is_empty() => {
            hex::decode(hex_str.trim()).map_err(|e| format!("invalid metadata hex: {e}"))?
        }
        _ => Vec::new(),
    };
    if metadata.len() > 32 {
        return Err("metadata must be at most 32 bytes".to_string());
    }

    if state
        .storage
        .pubkey_in_use(&key)
        .map_err(|e| e.to_string())?
    {
        let existing = state
            .storage
            .safebox()
            .map_err(|e| e.to_string())?
            .accounts
            .into_values()
            .find(|a| a.account_info.account_key.ed25519_public_key == key)
            .ok_or_else(|| "public key index inconsistency".to_string())?;
        return Ok(CreateAccountResult {
            accepted: true,
            status: "exists".into(),
            op_hash_hex: None,
            account_number: Some(existing.account_number),
            address,
            public_key: hex::encode(key),
            error: None,
        });
    }

    let admin_kp = state
        .admin_keypair
        .as_ref()
        .ok_or_else(|| "admin keypair not configured".to_string())?;

    // account_number == 0 → auto-select the first available Reserved AUGEID.
    // Skip accounts that accumulated rewards (validator accounts whose number
    // collides with the emission range) so we never hijack a validator's funds.
    let account_number = if params.account_number == 0 {
        state
            .storage
            .safebox()
            .map_err(|e| e.to_string())?
            .accounts
            .into_values()
            .find(|a| a.account_info.state == AccountState::Reserved && a.balance == 0)
            .map(|a| a.account_number)
            .ok_or_else(|| "no Reserved AUGEID available to activate".to_string())?
    } else {
        params.account_number
    };

    let op = Operation {
        op_type: augecoin_core::operation::OperationType::CreateAccount,
        payload: OperationPayload::CreateAccount {
            account_number,
            pubkey: key,
            initial_metadata: metadata,
        },
        chain_id: state
            .node_status
            .chain_id
            .load(std::sync::atomic::Ordering::SeqCst),
        signatures: vec![],
    };
    let signature = admin_kp.sign(&op.to_bytes_stripped());
    let op = Operation {
        signatures: vec![signature],
        ..op
    };
    let op_hash_hex = hex::encode(crate::hash_operation(&op));
    let op_bytes = op.to_bytes();

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let mut mempool = state
        .mempool
        .lock()
        .map_err(|e| format!("mempool lock poisoned: {e}"))?;
    match mempool.validate_and_admit(op, state.storage.as_ref(), now) {
        Ok(()) => {
            state.broadcast_op(op_bytes);
            println!("[rpc] createaccount submitted via consensus: op_hash={op_hash_hex} address={address}");
            Ok(CreateAccountResult {
                accepted: true,
                status: "pending".into(),
                op_hash_hex: Some(op_hash_hex),
                account_number: None,
                address,
                public_key: hex::encode(key),
                error: None,
            })
        }
        Err(e) => Ok(CreateAccountResult {
            accepted: false,
            status: "rejected".into(),
            op_hash_hex: Some(op_hash_hex),
            account_number: None,
            address,
            public_key: hex::encode(key),
            error: Some(e.to_string()),
        }),
    }
}

pub fn handle_faucet(
    params: FaucetParams,
    client_id: &str,
    state: &AppState,
) -> Result<FaucetResult, String> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_secs();

    // Resolve the destination AUGEID: by account number, or (legacy) by
    // address. New payments use the AUGEID exclusively.
    let recipient = if let Some(number) = params.account_number {
        state
            .storage
            .get_account(number)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("AUGEID {number} not found; register or buy one first"))?
    } else if let Some(address) = params.address.as_deref() {
        if !valid_address(address) {
            return Err("invalid AUGE address".into());
        }
        let hash = augecoin_crypto::address::AddressHash::parse(address)
            .ok_or_else(|| "invalid AUGE address".to_string())?;
        let number = state
            .storage
            .resolve_address(&hash)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "address has no account; first receive will activate it".to_string())?;
        state
            .storage
            .get_account(number)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "account disappeared".to_string())?
    } else {
        return Err("provide account_number (AUGEID) or address".into());
    };

    let claim_key = params
        .account_number
        .map(|n| format!("augeid:{n}"))
        .or_else(|| params.address.clone())
        .unwrap_or_default();
    {
        if state
            .storage
            .get_faucet_claim_timestamp(&claim_key)
            .map_err(|e| e.to_string())?
            .is_some_and(|last| now.saturating_sub(last) < 24 * 60 * 60)
        {
            return Err("faucet cooldown: AUGEID may claim once every 24 hours".into());
        }
        let claims = state
            .faucet_claims
            .lock()
            .map_err(|_| "faucet lock poisoned")?;
        if claims
            .get(&claim_key)
            .is_some_and(|last| now.saturating_sub(*last) < 24 * 60 * 60)
        {
            return Err("faucet cooldown: AUGEID may claim once every 24 hours".into());
        }
    }

    // Reserved AUGEIDs cannot receive AUGE.
    if recipient.account_info.state == AccountState::Reserved
        || recipient.account_info.state == AccountState::GiftPending
    {
        return Err("destination AUGEID is not yet active (Reserved/GiftPending)".into());
    }

    let keypair = state
        .faucet_keypair
        .as_ref()
        .ok_or_else(|| "faucet signing key is not configured".to_string())?;
    let source = state
        .storage
        .get_account(state.faucet_account)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "faucet account not found".to_string())?;
    if source.account_info.account_key.ed25519_public_key != keypair.verifying_key().to_bytes() {
        return Err("faucet key does not control faucet account".into());
    }
    let op = augecoin_core::operation::Operation {
        op_type: augecoin_core::operation::OperationType::Transaction,
        payload: augecoin_core::operation::OperationPayload::Transaction {
            senders: vec![augecoin_core::operation::SenderInfo {
                account: state.faucet_account,
                n_operation: source.n_operation,
                amount: state.faucet_amount,
                payload: vec![],
            }],
            receivers: vec![augecoin_core::operation::ReceiverInfo {
                account: recipient.account_number,
                amount: state.faucet_amount,
                payload: vec![],
            }],
            changers: vec![],
            fee: 0,
        },
        chain_id: state
            .node_status
            .chain_id
            .load(std::sync::atomic::Ordering::SeqCst),
        signatures: vec![],
    };
    let signature = keypair.sign(&op.to_bytes_stripped());
    let op = augecoin_core::operation::Operation {
        signatures: vec![signature],
        ..op
    };
    let tx_hash = hex::encode(crate::hash_operation(&op));
    let op_bytes = op.to_bytes();
    let mut mempool = state.mempool.lock().map_err(|_| "mempool lock poisoned")?;
    mempool
        .validate_and_admit(op, state.storage.as_ref(), now)
        .map_err(|e| e.to_string())?;
    state.broadcast_op(op_bytes);
    state
        .storage
        .put_faucet_claim(&claim_key, now, &tx_hash, "accepted")
        .map_err(|e| e.to_string())?;
    state
        .faucet_claims
        .lock()
        .map_err(|_| "faucet lock poisoned")?
        .insert(claim_key.clone(), now);
    println!(
        "[faucet] client={client_id} augeid={} tx_hash={tx_hash} status=accepted",
        recipient.account_number
    );
    Ok(FaucetResult {
        success: true,
        tx_hash: Some(tx_hash),
        error: None,
    })
}

pub fn handle_get_block(
    params: GetBlockParams,
    state: &AppState,
) -> Result<BlockHeaderInfo, String> {
    let block = state
        .storage
        .get_block(params.block_number)
        .map_err(|e| format!("storage error: {e}"))?
        .ok_or_else(|| format!("block {} not found", params.block_number))?;

    Ok(header_to_info(&block.header))
}

pub fn handle_get_block_operations(
    params: GetBlockParams,
    state: &AppState,
) -> Result<BlockOperationsResponse, String> {
    let block = state
        .storage
        .get_block(params.block_number)
        .map_err(|e| format!("storage error: {e}"))?
        .ok_or_else(|| format!("block {} not found", params.block_number))?;

    let operations: Vec<OperationInfo> = block.operations.iter().map(op_to_info).collect();

    Ok(BlockOperationsResponse {
        block: header_to_info(&block.header),
        operations,
    })
}

pub fn handle_get_operations(
    params: GetOperationsParams,
    state: &AppState,
) -> Result<Vec<OperationInfo>, String> {
    let block = state
        .storage
        .get_block(params.block_number)
        .map_err(|e| format!("storage error: {e}"))?
        .ok_or_else(|| format!("block {} not found", params.block_number))?;

    let start = params.start as usize;
    let ops = block
        .operations
        .iter()
        .skip(start)
        .map(op_to_info)
        .take(
            params
                .limit
                .map(|limit| limit.min(1000) as usize)
                .unwrap_or(usize::MAX),
        )
        .collect();
    Ok(ops)
}

/// Normalize a user-supplied hash for comparison (strip `0x`, lowercase).
fn normalize_hash_hex(hash_hex: &str) -> String {
    hash_hex.trim().trim_start_matches("0x").to_lowercase()
}

/// Scan window for hash lookups: `max_blocks` when provided, otherwise the
/// bounded default. Always clamped to the number of blocks actually stored.
fn hash_scan_window(state: &AppState, max_blocks: Option<u64>) -> Result<u64, String> {
    let height = state
        .storage
        .get_height()
        .map_err(|e| format!("storage error: {e}"))?;
    let limit = max_blocks
        .unwrap_or(DEFAULT_HASH_SCAN_BLOCKS)
        .max(1)
        .min(height + 1);
    Ok(limit)
}

/// Reverse-scan blocks from the tip looking for a block whose hash (header
/// hash, operations merkle root, safe-box hash or proof-of-work) matches.
pub fn handle_get_block_by_hash(
    params: GetByHashParams,
    state: &AppState,
) -> Result<BlockHeaderInfo, String> {
    let needle = normalize_hash_hex(&params.hash_hex);
    if needle.is_empty() {
        return Err("hash_hex is required".into());
    }

    let window = hash_scan_window(state, params.max_blocks)?;
    let height = state
        .storage
        .get_height()
        .map_err(|e| format!("storage error: {e}"))?;
    let from = height.saturating_sub(window - 1);

    for bn in (from..=height).rev() {
        let Some(block) = state
            .storage
            .get_block(bn)
            .map_err(|e| format!("storage error: {e}"))?
        else {
            continue;
        };
        let hdr = &block.header;
        if hex::encode(block.hash()) == needle
            || hex::encode(hdr.operations_hash) == needle
            || hex::encode(hdr.initial_safe_box_hash) == needle
            || hex::encode(hdr.proof_of_work) == needle
        {
            return Ok(header_to_info(hdr));
        }
    }

    Err("block not found for the given hash".into())
}

/// Reverse-scan blocks from the tip looking for an operation whose
/// blake3-512 hash matches, returning its location (`block·index`).
pub fn handle_get_operation_by_hash(
    params: GetByHashParams,
    state: &AppState,
) -> Result<OperationByHashResponse, String> {
    let needle = normalize_hash_hex(&params.hash_hex);
    if needle.is_empty() {
        return Err("hash_hex is required".into());
    }

    let window = hash_scan_window(state, params.max_blocks)?;
    let height = state
        .storage
        .get_height()
        .map_err(|e| format!("storage error: {e}"))?;
    let from = height.saturating_sub(window - 1);

    for bn in (from..=height).rev() {
        let Some(block) = state
            .storage
            .get_block(bn)
            .map_err(|e| format!("storage error: {e}"))?
        else {
            continue;
        };
        for (idx, op) in block.operations.iter().enumerate() {
            let op_hash = crate::hash_operation(op);
            if hex::encode(op_hash) == needle {
                return Ok(OperationByHashResponse {
                    block_number: bn,
                    op_index: idx,
                    op_hash_hex: hex::encode(op_hash),
                    operation: op_to_info(op),
                });
            }
        }
    }

    Err("operation not found for the given hash".into())
}

pub fn handle_send_operation(
    params: SendOperationParams,
    state: &AppState,
) -> Result<SendOperationResult, String> {
    let op_bytes = hex::decode(&params.hex).map_err(|e| format!("invalid hex: {e}"))?;

    let op = Operation::from_bytes(&op_bytes).map_err(|e| format!("invalid operation: {e}"))?;

    let op_hash = crate::hash_operation(&op);
    let op_hash_hex = hex::encode(op_hash);

    let chain_id = state
        .node_status
        .chain_id
        .load(std::sync::atomic::Ordering::SeqCst);

    if op.chain_id != chain_id {
        return Ok(SendOperationResult {
            accepted: false,
            op_hash_hex,
            error: Some(format!(
                "wrong chain_id: operation {}, expected {}",
                op.chain_id, chain_id
            )),
        });
    }

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    // Validate and admit *synchronously* into the official mempool.
    // A successful admission is the only thing that counts as acceptance.
    let mut mempool = state
        .mempool
        .lock()
        .map_err(|e| format!("mempool lock poisoned: {e}"))?;

    match mempool.validate_and_admit(op, state.storage.as_ref(), now) {
        Ok(()) => {
            state.broadcast_op(op_bytes);
            Ok(SendOperationResult {
                accepted: true,
                op_hash_hex,
                error: None,
            })
        }
        Err(e) => Ok(SendOperationResult {
            accepted: false,
            op_hash_hex,
            error: Some(e.to_string()),
        }),
    }
}

/// Admit an ordered batch from one sender while holding one mempool lock.
/// Accepted operations are broadcast individually after admission, matching
/// the single-operation endpoint's network semantics.
pub fn handle_send_operations(
    params: SendOperationsParams,
    state: &AppState,
) -> Result<SendOperationsResult, String> {
    if params.operations.is_empty() {
        return Ok(SendOperationsResult {
            accepted: 0,
            rejected: 0,
            errors: Vec::new(),
        });
    }

    let chain_id = state
        .node_status
        .chain_id
        .load(std::sync::atomic::Ordering::SeqCst);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let mut mempool = state
        .mempool
        .lock()
        .map_err(|e| format!("mempool lock poisoned: {e}"))?;
    let mut accepted = 0;
    let mut errors = Vec::new();
    let mut to_broadcast = Vec::new();

    for hex_op in params.operations {
        let op_bytes = match hex::decode(&hex_op) {
            Ok(bytes) => bytes,
            Err(e) => {
                errors.push(format!("invalid hex: {e}"));
                continue;
            }
        };
        let op = match Operation::from_bytes(&op_bytes) {
            Ok(op) => op,
            Err(e) => {
                errors.push(format!("invalid operation: {e}"));
                continue;
            }
        };
        if op.chain_id != chain_id {
            errors.push(format!(
                "wrong chain_id: operation {}, expected {}",
                op.chain_id, chain_id
            ));
            continue;
        }
        match mempool.validate_and_admit(op, state.storage.as_ref(), now) {
            Ok(()) => {
                accepted += 1;
                to_broadcast.push(op_bytes);
            }
            Err(e) => errors.push(e.to_string()),
        }
    }
    drop(mempool);

    if !to_broadcast.is_empty() {
        state.broadcast_op(encode_operation_batch(&to_broadcast));
    }

    Ok(SendOperationsResult {
        accepted,
        rejected: errors.len(),
        errors,
    })
}

pub fn handle_get_pendings(state: &AppState) -> Result<PendingOpsResponse, String> {
    let mempool = state
        .mempool
        .lock()
        .map_err(|e| format!("mempool lock poisoned: {e}"))?;

    let mut operations = Vec::with_capacity(mempool.len());
    for op in mempool.pending() {
        operations.push(PendingOpEntry {
            op_type_name: operation_type_name(op),
            op_hash_hex: hex::encode(crate::hash_operation(op)),
        });
    }

    Ok(PendingOpsResponse {
        size: operations.len(),
        operations,
    })
}

pub fn handle_find_accounts(
    params: FindAccountsParams,
    state: &AppState,
) -> Result<FindAccountsResponse, String> {
    let accounts = state
        .storage
        .iter_accounts()
        .map_err(|e| format!("storage error: {e}"))?;

    let filtered: Vec<AccountInfoResponse> = accounts
        .iter()
        .filter(|acc| {
            if let Some(ref name) = params.name {
                if let Some(ref acc_name) = acc.name {
                    if !acc_name.to_lowercase().contains(&name.to_lowercase()) {
                        return false;
                    }
                } else {
                    return false;
                }
            }
            if let Some(at) = params.account_type {
                if acc.account_type != at {
                    return false;
                }
            }
            if let Some(min_b) = params.min_balance {
                if acc.balance < min_b {
                    return false;
                }
            }
            if let Some(max_b) = params.max_balance {
                if acc.balance > max_b {
                    return false;
                }
            }
            true
        })
        .map(account_to_response)
        .collect();

    let total = filtered.len() as u64;
    let start = params.start as usize;
    let max = params.max.min(1000) as usize;
    let page: Vec<AccountInfoResponse> = filtered.into_iter().skip(start).take(max).collect();

    Ok(FindAccountsResponse {
        accounts: page,
        total,
    })
}

pub fn handle_get_account_count(state: &AppState) -> Result<u64, String> {
    state
        .storage
        .max_account_number()
        .map_err(|e| format!("storage error: {e}"))
}

pub fn handle_get_block_count(state: &AppState) -> Result<u64, String> {
    state
        .storage
        .get_height()
        .map_err(|e| format!("storage error: {e}"))
}

pub fn handle_get_node_status(state: &AppState) -> Result<NodeStatusResponse, String> {
    let status = &state.node_status;

    let current_height = status
        .block_height
        .load(std::sync::atomic::Ordering::SeqCst);
    let latest_block_hash = *status
        .latest_block_hash
        .lock()
        .map_err(|e| format!("lock error: {e}"))?;
    let peers_connected = status
        .peers_connected
        .load(std::sync::atomic::Ordering::SeqCst);
    let peers_gossipsub_consensus = status
        .peers_gossipsub_consensus
        .load(std::sync::atomic::Ordering::SeqCst);
    let peers_kademlia_total = status
        .peers_kademlia_total
        .load(std::sync::atomic::Ordering::SeqCst);
    let sync_target = status
        .sync_target_height
        .load(std::sync::atomic::Ordering::SeqCst);
    let mempool_size = status
        .mempool_size
        .load(std::sync::atomic::Ordering::SeqCst);
    let current_round = status
        .current_round
        .load(std::sync::atomic::Ordering::SeqCst);
    let current_view = status
        .current_view
        .load(std::sync::atomic::Ordering::SeqCst);
    let validator_id = status
        .validator_id
        .load(std::sync::atomic::Ordering::SeqCst);
    let chain_id = status.chain_id.load(std::sync::atomic::Ordering::SeqCst);
    let uptime_seconds = status
        .uptime_seconds
        .load(std::sync::atomic::Ordering::SeqCst);
    let last_consensus_error = status
        .last_consensus_error
        .lock()
        .map_err(|e| format!("lock error: {e}"))?
        .clone();

    let total_accounts = state.storage.max_account_number().unwrap_or(0);
    let total_blocks = state.storage.get_height().unwrap_or(0);

    Ok(NodeStatusResponse {
        current_height,
        latest_block_hash_hex: hex::encode(latest_block_hash),
        peers_connected,
        peers_gossipsub_consensus,
        peers_kademlia_total,
        syncing: sync_target > current_height,
        sync_target_height: sync_target,
        total_accounts,
        total_blocks,
        mempool_size,
        current_round,
        current_view,
        validator_id,
        chain_id,
        uptime_seconds,
        last_consensus_error,
    })
}

pub fn handle_get_validator_set(state: &AppState) -> Result<ValidatorSetResponse, String> {
    let raw = state
        .storage
        .get_validator_set_bytes()
        .map_err(|e| format!("storage error: {e}"))?;

    let active = match raw {
        Some(data) => {
            let vs = ValidatorSet::from_bytes(&data)
                .map_err(|e| format!("deserialization error: {e}"))?;
            vs.active_validators()
                .iter()
                .map(|v| ValidatorEntryResponse {
                    id: v.id,
                    status: validator_status_string(&v.status),
                    ed25519_public_key_hex: hex::encode(v.ed25519_public_key),
                })
                .collect()
        }
        None => vec![],
    };

    Ok(ValidatorSetResponse {
        total: active.len(),
        active,
    })
}

pub fn handle_validator_add(
    params: ValidatorAddParams,
    state: &AppState,
) -> Result<AdminResult, String> {
    let ed_hex = params.ed25519_public_key_hex;

    let ed_bytes: [u8; 32] = hex::decode(&ed_hex)
        .map_err(|e| format!("invalid hex: {e}"))?
        .try_into()
        .map_err(|_| "invalid ed25519 public key length".to_string())?;

    let admin_kp = state
        .admin_keypair
        .as_ref()
        .ok_or_else(|| "admin keypair not configured".to_string())?;

    let va_op = augecoin_core::operation::ValidatorAdminOp::Add {
        ed25519_public_key: ed_bytes,
        activation_height: params.activation_height,
    };
    let op = augecoin_core::operation::Operation {
        op_type: augecoin_core::operation::OperationType::ValidatorAdminOp,
        payload: augecoin_core::operation::OperationPayload::ValidatorAdmin(va_op),
        chain_id: state
            .node_status
            .chain_id
            .load(std::sync::atomic::Ordering::SeqCst),
        signatures: vec![],
    };
    let signature = admin_kp.sign(&op.to_bytes_stripped());
    let op = augecoin_core::operation::Operation {
        signatures: vec![signature],
        ..op
    };
    let op_hash = hex::encode(crate::hash_operation(&op));
    let op_bytes = op.to_bytes();

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let mut mempool = state
        .mempool
        .lock()
        .map_err(|e| format!("mempool lock poisoned: {e}"))?;
    match mempool.validate_and_admit(op, state.storage.as_ref(), now) {
        Ok(()) => {
            state.broadcast_op(op_bytes);
            println!("[admin] validator added via consensus: op_hash={op_hash}");
            Ok(AdminResult {
                success: true,
                error: None,
            })
        }
        Err(e) => Ok(AdminResult {
            success: false,
            error: Some(e.to_string()),
        }),
    }
}

pub fn handle_validator_remove(
    params: ValidatorRemoveParams,
    state: &AppState,
) -> Result<AdminResult, String> {
    let admin_kp = state
        .admin_keypair
        .as_ref()
        .ok_or_else(|| "admin keypair not configured".to_string())?;

    if let Err(e) = validator_must_exist(params.validator_id, state) {
        return Ok(AdminResult {
            success: false,
            error: Some(e),
        });
    }

    let va_op = augecoin_core::operation::ValidatorAdminOp::Remove {
        validator_id: params.validator_id,
        activation_height: params.activation_height,
    };
    let op = augecoin_core::operation::Operation {
        op_type: augecoin_core::operation::OperationType::ValidatorAdminOp,
        payload: augecoin_core::operation::OperationPayload::ValidatorAdmin(va_op),
        chain_id: state
            .node_status
            .chain_id
            .load(std::sync::atomic::Ordering::SeqCst),
        signatures: vec![],
    };
    let signature = admin_kp.sign(&op.to_bytes_stripped());
    let op = augecoin_core::operation::Operation {
        signatures: vec![signature],
        ..op
    };
    let op_hash = hex::encode(crate::hash_operation(&op));
    let op_bytes = op.to_bytes();

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let mut mempool = state
        .mempool
        .lock()
        .map_err(|e| format!("mempool lock poisoned: {e}"))?;
    match mempool.validate_and_admit(op, state.storage.as_ref(), now) {
        Ok(()) => {
            state.broadcast_op(op_bytes);
            println!(
                "[admin] validator {} removal submitted via consensus: op_hash={op_hash}",
                params.validator_id
            );
            Ok(AdminResult {
                success: true,
                error: None,
            })
        }
        Err(e) => Ok(AdminResult {
            success: false,
            error: Some(e.to_string()),
        }),
    }
}

pub fn handle_validator_activate(
    params: ValidatorActivateParams,
    state: &AppState,
) -> Result<AdminResult, String> {
    let admin_kp = state
        .admin_keypair
        .as_ref()
        .ok_or_else(|| "admin keypair not configured".to_string())?;

    if let Err(e) = validator_must_exist(params.validator_id, state) {
        return Ok(AdminResult {
            success: false,
            error: Some(e),
        });
    }

    let va_op = augecoin_core::operation::ValidatorAdminOp::Activate {
        validator_id: params.validator_id,
        activation_height: params.activation_height,
    };
    let op = augecoin_core::operation::Operation {
        op_type: augecoin_core::operation::OperationType::ValidatorAdminOp,
        payload: augecoin_core::operation::OperationPayload::ValidatorAdmin(va_op),
        chain_id: state
            .node_status
            .chain_id
            .load(std::sync::atomic::Ordering::SeqCst),
        signatures: vec![],
    };
    let signature = admin_kp.sign(&op.to_bytes_stripped());
    let op = augecoin_core::operation::Operation {
        signatures: vec![signature],
        ..op
    };
    let op_hash = hex::encode(crate::hash_operation(&op));
    let op_bytes = op.to_bytes();

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let mut mempool = state
        .mempool
        .lock()
        .map_err(|e| format!("mempool lock poisoned: {e}"))?;
    match mempool.validate_and_admit(op, state.storage.as_ref(), now) {
        Ok(()) => {
            state.broadcast_op(op_bytes);
            println!(
                "[admin] validator {} activation submitted via consensus: op_hash={op_hash}",
                params.validator_id
            );
            Ok(AdminResult {
                success: true,
                error: None,
            })
        }
        Err(e) => Ok(AdminResult {
            success: false,
            error: Some(e.to_string()),
        }),
    }
}

pub fn handle_validator_deactivate(
    params: ValidatorDeactivateParams,
    state: &AppState,
) -> Result<AdminResult, String> {
    let admin_kp = state
        .admin_keypair
        .as_ref()
        .ok_or_else(|| "admin keypair not configured".to_string())?;

    if let Err(e) = validator_must_exist(params.validator_id, state) {
        return Ok(AdminResult {
            success: false,
            error: Some(e),
        });
    }

    let va_op = augecoin_core::operation::ValidatorAdminOp::Deactivate {
        validator_id: params.validator_id,
        activation_height: params.activation_height,
    };
    let op = augecoin_core::operation::Operation {
        op_type: augecoin_core::operation::OperationType::ValidatorAdminOp,
        payload: augecoin_core::operation::OperationPayload::ValidatorAdmin(va_op),
        chain_id: state
            .node_status
            .chain_id
            .load(std::sync::atomic::Ordering::SeqCst),
        signatures: vec![],
    };
    let signature = admin_kp.sign(&op.to_bytes_stripped());
    let op = augecoin_core::operation::Operation {
        signatures: vec![signature],
        ..op
    };
    let op_hash = hex::encode(crate::hash_operation(&op));
    let op_bytes = op.to_bytes();

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let mut mempool = state
        .mempool
        .lock()
        .map_err(|e| format!("mempool lock poisoned: {e}"))?;
    match mempool.validate_and_admit(op, state.storage.as_ref(), now) {
        Ok(()) => {
            state.broadcast_op(op_bytes);
            println!(
                "[admin] validator {} deactivation submitted via consensus: op_hash={op_hash}",
                params.validator_id
            );
            Ok(AdminResult {
                success: true,
                error: None,
            })
        }
        Err(e) => Ok(AdminResult {
            success: false,
            error: Some(e.to_string()),
        }),
    }
}

// ── Helpers ─────────────────────────────────────────────────────────

fn validator_must_exist(validator_id: u64, state: &AppState) -> Result<(), String> {
    let raw = state
        .storage
        .get_validator_set_bytes()
        .map_err(|e| format!("storage error: {e}"))?;
    let exists = match raw {
        Some(data) => ValidatorSet::from_bytes(&data)
            .map_err(|e| format!("invalid validator set: {e}"))?
            .validators()
            .iter()
            .any(|v| v.id == validator_id),
        None => false,
    };
    if !exists {
        return Err(format!("validator {validator_id} not found"));
    }
    Ok(())
}

fn validator_status_string(status: &ValidatorStatus) -> String {
    match status {
        ValidatorStatus::Active => "active".into(),
        ValidatorStatus::Inactive => "inactive".into(),
        ValidatorStatus::PendingActivation { .. } => "pending".into(),
    }
}

fn account_to_response(acc: &Account) -> AccountInfoResponse {
    let address =
        ed25519_dalek::VerifyingKey::from_bytes(&acc.account_info.account_key.ed25519_public_key)
            .map(|vk| augecoin_crypto::address::derive_address(&vk))
            .unwrap_or_default();
    AccountInfoResponse {
        account_number: acc.account_number,
        balance: acc.balance,
        n_operation: acc.n_operation,
        name: acc.name.clone(),
        account_type: acc.account_type,
        account_data_hex: hex::encode(&acc.account_data),
        account_seal_hex: hex::encode(&acc.account_seal),
        state: format!("{:?}", acc.account_info.state),
        updated_on_block_passive_mode: acc.updated_on_block_passive_mode,
        updated_on_block_active_mode: acc.updated_on_block_active_mode,
        locked_until_block: acc.account_info.locked_until_block,
        price: acc.account_info.price,
        account_to_pay: acc.account_info.account_to_pay,
        account_key_ed_hex: hex::encode(acc.account_info.account_key.ed25519_public_key),
        address,
    }
}

fn header_to_info(hdr: &OperationBlockHeader) -> BlockHeaderInfo {
    BlockHeaderInfo {
        block_number: hdr.block_number,
        account_key_hex: hex::encode(hdr.account_key),
        reward: hdr.reward,
        fee: hdr.fee,
        protocol_version: hdr.protocol_version,
        protocol_available: hdr.protocol_available,
        timestamp: hdr.timestamp,
        initial_safe_box_hash_hex: hex::encode(hdr.initial_safe_box_hash),
        operations_hash_hex: hex::encode(hdr.operations_hash),
        block_hash_hex: hex::encode(hdr.hash()),
        block_payload_hex: hex::encode(&hdr.block_payload),
        proof_of_work_hex: hex::encode(hdr.proof_of_work),
        previous_proof_of_work_hex: hex::encode(hdr.previous_proof_of_work),
        leader_id: hdr.leader_id,
    }
}

fn op_to_info(op: &Operation) -> OperationInfo {
    let payload_json = operation_payload_to_json(&op.payload);
    OperationInfo {
        op_type: op.op_type as u8,
        op_type_name: operation_type_name(op),
        signatures_count: op.signatures.len(),
        op_hash_hex: hex::encode(crate::hash_operation(op)),
        payload: payload_json,
    }
}

fn operation_type_name(op: &Operation) -> String {
    match op.payload {
        OperationPayload::Transaction { .. } => "Transaction".into(),
        OperationPayload::AddressTransaction { .. } => "AddressTransaction".into(),
        OperationPayload::ChangeKey { .. } => "ChangeKey".into(),
        OperationPayload::RecoverFounds { .. } => "RecoverFounds".into(),
        OperationPayload::ListAccountForSale { .. } => "ListAccountForSale".into(),
        OperationPayload::DelistAccount { .. } => "DelistAccount".into(),
        OperationPayload::BuyAccount { .. } => "BuyAccount".into(),
        OperationPayload::ChangeKeySigned { .. } => "ChangeKeySigned".into(),
        OperationPayload::ChangeAccountInfo { .. } => "ChangeAccountInfo".into(),
        OperationPayload::MultiOperation { .. } => "MultiOperation".into(),
        OperationPayload::Data { .. } => "Data".into(),
        OperationPayload::ValidatorAdmin(_) => "ValidatorAdmin".into(),
        OperationPayload::CreateAccount { .. } => "CreateAccount".into(),
        OperationPayload::GiftAccount { .. } => "GiftAccount".into(),
        OperationPayload::AcceptGift { .. } => "AcceptGift".into(),
    }
}

fn operation_payload_to_json(payload: &OperationPayload) -> serde_json::Value {
    match payload {
        OperationPayload::Transaction {
            senders,
            receivers,
            changers,
            fee,
        } => {
            serde_json::json!({
                "senders": senders.iter().map(sender_to_json).collect::<Vec<_>>(),
                "receivers": receivers.iter().map(receiver_to_json).collect::<Vec<_>>(),
                "changers": changers.iter().map(changer_to_json).collect::<Vec<_>>(),
                "fee": fee,
            })
        }
        OperationPayload::AddressTransaction {
            senders,
            receivers,
            fee,
        } => serde_json::json!({
            "senders": senders.iter().map(sender_to_json).collect::<Vec<_>>(),
            "receivers": receivers.iter().map(|r| serde_json::json!({
                "address": r.address.to_address(), "amount": r.amount, "payload_hex": hex::encode(&r.payload)
            })).collect::<Vec<_>>(), "fee": fee
        }),
        OperationPayload::ChangeKey {
            account,
            n_operation,
            fee,
            new_ed25519_public_key,
            ..
        } => {
            serde_json::json!({
                "account": account,
                "n_operation": n_operation,
                "fee": fee,
                "new_ed25519_public_key_hex": hex::encode(new_ed25519_public_key),
            })
        }
        OperationPayload::RecoverFounds { account } => {
            serde_json::json!({ "account": account })
        }
        OperationPayload::ListAccountForSale {
            account,
            n_operation,
            sale_price,
            account_to_pay,
            new_ed25519_public_key,
            locked_until_block,
            fee,
            ..
        } => {
            serde_json::json!({
                "account": account,
                "n_operation": n_operation,
                "sale_price": sale_price,
                "account_to_pay": account_to_pay,
                "new_ed25519_public_key_hex": hex::encode(new_ed25519_public_key),
                "locked_until_block": locked_until_block,
                "fee": fee,
            })
        }
        OperationPayload::DelistAccount {
            account,
            n_operation,
            fee,
        } => {
            serde_json::json!({
                "account": account,
                "n_operation": n_operation,
                "fee": fee,
            })
        }
        OperationPayload::BuyAccount {
            buyer_account,
            n_operation,
            account_to_purchase,
            amount,
            fee,
            new_ed25519_public_key,
            seller_account,
            ..
        } => {
            serde_json::json!({
                "buyer_account": buyer_account,
                "n_operation": n_operation,
                "account_to_purchase": account_to_purchase,
                "amount": amount,
                "fee": fee,
                "new_ed25519_public_key_hex": hex::encode(new_ed25519_public_key),
                "seller_account": seller_account,
            })
        }
        OperationPayload::ChangeKeySigned {
            account,
            n_operation,
            fee,
            new_ed25519_public_key,
            ..
        } => {
            serde_json::json!({
                "account": account,
                "n_operation": n_operation,
                "fee": fee,
                "new_ed25519_public_key_hex": hex::encode(new_ed25519_public_key),
            })
        }
        OperationPayload::ChangeAccountInfo {
            account,
            n_operation,
            fee,
            new_ed25519_public_key,
            new_name,
            new_type,
            ..
        } => {
            serde_json::json!({
                "account": account,
                "n_operation": n_operation,
                "fee": fee,
                "new_ed25519_public_key_hex": hex::encode(new_ed25519_public_key),
                "new_name": new_name,
                "new_type": new_type,
            })
        }
        OperationPayload::MultiOperation {
            senders,
            receivers,
            changers,
            fee,
        } => {
            serde_json::json!({
                "senders": senders.iter().map(sender_to_json).collect::<Vec<_>>(),
                "receivers": receivers.iter().map(receiver_to_json).collect::<Vec<_>>(),
                "changers": changers.iter().map(changer_to_json).collect::<Vec<_>>(),
                "fee": fee,
            })
        }
        OperationPayload::Data {
            account,
            n_operation,
            fee,
            data,
            senders,
            receivers,
            changers,
        } => {
            serde_json::json!({
                "account": account,
                "n_operation": n_operation,
                "fee": fee,
                "data_hex": hex::encode(data),
                "senders": senders.iter().map(sender_to_json).collect::<Vec<_>>(),
                "receivers": receivers.iter().map(receiver_to_json).collect::<Vec<_>>(),
                "changers": changers.iter().map(changer_to_json).collect::<Vec<_>>(),
            })
        }
        OperationPayload::ValidatorAdmin(_) => {
            serde_json::json!({ "type": "ValidatorAdmin" })
        }
        OperationPayload::CreateAccount {
            account_number,
            pubkey,
            initial_metadata,
        } => {
            serde_json::json!({
                "account_number": account_number,
                "pubkey_hex": hex::encode(pubkey),
                "initial_metadata_hex": hex::encode(initial_metadata),
            })
        }
        OperationPayload::GiftAccount {
            account,
            n_operation,
            recipient_public_key,
            fee,
        } => {
            serde_json::json!({
                "account": account,
                "n_operation": n_operation,
                "recipient_public_key_hex": hex::encode(recipient_public_key),
                "fee": fee,
            })
        }
        OperationPayload::AcceptGift {
            account,
            n_operation,
            fee,
        } => {
            serde_json::json!({
                "account": account,
                "n_operation": n_operation,
                "fee": fee,
            })
        }
    }
}

fn sender_to_json(s: &SenderInfo) -> serde_json::Value {
    serde_json::json!({
        "account": s.account,
        "n_operation": s.n_operation,
        "amount": s.amount,
        "payload_hex": hex::encode(&s.payload),
    })
}

fn receiver_to_json(r: &ReceiverInfo) -> serde_json::Value {
    serde_json::json!({
        "account": r.account,
        "amount": r.amount,
        "payload_hex": hex::encode(&r.payload),
    })
}

fn changer_to_json(c: &augecoin_core::operation::ChangerInfo) -> serde_json::Value {
    serde_json::json!({
        "account": c.account,
        "n_operation": c.n_operation,
        "new_ed25519_public_key_hex": hex::encode(c.new_ed25519_public_key),
        "new_name": c.new_name,
        "new_type": c.new_type,
        "new_account_data_hex": hex::encode(&c.new_account_data),
        "new_account_seal_hex": hex::encode(&c.new_account_seal),
        "fee": c.fee,
    })
}

// ── Marketplace / AUGEID lifecycle ───────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ListInventoryParams {
    /// The validator's public key (hex) whose inventory to list.
    pub validator_public_key_hex: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct MarketplaceEntry {
    pub account_number: u64,
    pub price: u64,
    pub seller_public_key_hex: String,
    pub state: String,
    pub listed_at_block: u64,
}

#[derive(Debug, Serialize)]
pub struct MarketplaceResult {
    pub entries: Vec<MarketplaceEntry>,
}

#[derive(Debug, Serialize)]
pub struct InventoryResult {
    pub reserved: Vec<u64>,
    pub for_sale: Vec<u64>,
    pub owned: Vec<u64>,
}

#[derive(Debug, Deserialize)]
pub struct BuyAccountParams {
    pub buyer_account: u64,
    pub n_operation: u64,
    pub account_to_purchase: u64,
    pub amount: u64,
    #[serde(default)]
    pub fee: u64,
    pub new_public_key_hex: String,
    pub seller_account: u64,
    pub signature_hex: String,
}

#[derive(Debug, Deserialize)]
pub struct SellAccountParams {
    pub account: u64,
    pub n_operation: u64,
    pub sale_price: u64,
    #[serde(default)]
    pub account_to_pay: u64,
    #[serde(default)]
    pub locked_until_block: u64,
    #[serde(default)]
    pub fee: u64,
    /// The seller's own public key (hex) — must match the account key.
    pub new_public_key_hex: String,
    pub signature_hex: String,
}

#[derive(Debug, Deserialize)]
pub struct GiftAccountParams {
    pub account: u64,
    pub n_operation: u64,
    pub recipient_public_key_hex: String,
    #[serde(default)]
    pub fee: u64,
    pub signature_hex: String,
}

#[derive(Debug, Deserialize)]
pub struct AcceptGiftParams {
    pub account: u64,
    pub n_operation: u64,
    #[serde(default)]
    pub fee: u64,
    pub signature_hex: String,
}

#[derive(Debug, Deserialize)]
pub struct CancelSaleParams {
    pub account: u64,
    pub n_operation: u64,
    #[serde(default)]
    pub fee: u64,
    pub signature_hex: String,
}

#[derive(Debug, Serialize)]
pub struct LifecycleOpResult {
    pub accepted: bool,
    pub op_hash_hex: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ListPendingGiftsParams {
    pub recipient_public_key_hex: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct PendingGiftEntry {
    pub account_number: u64,
    pub recipient_public_key_hex: String,
    pub from_public_key_hex: String,
    pub gifted_at_block: u64,
    pub name: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct PendingGiftsResult {
    pub entries: Vec<PendingGiftEntry>,
}

fn parse_signature(hex_sig: &str) -> Result<augecoin_crypto::signature::HybridSignature, String> {
    let bytes = hex::decode(hex_sig.trim()).map_err(|e| format!("invalid signature hex: {e}"))?;
    let arr: [u8; 64] = bytes
        .try_into()
        .map_err(|_| "signature must be 64 bytes".to_string())?;
    Ok(augecoin_crypto::signature::HybridSignature { bytes: arr })
}

fn admit_signed_operation(state: &AppState, op: Operation) -> Result<String, String> {
    let op_hash_hex = hex::encode(crate::hash_operation(&op));
    let op_bytes = op.to_bytes();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let mut mempool = state
        .mempool
        .lock()
        .map_err(|_| "mempool lock poisoned".to_string())?;
    mempool
        .validate_and_admit(op, state.storage.as_ref(), now)
        .map_err(|e| e.to_string())?;
    drop(mempool);
    state.broadcast_op(op_bytes);
    Ok(op_hash_hex)
}

pub fn handle_list_accounts_for_sale(state: &AppState) -> Result<MarketplaceResult, String> {
    let listings = state
        .storage
        .list_for_sale()
        .map_err(|e| format!("storage error: {e}"))?;
    let entries = listings
        .into_iter()
        .map(|l| MarketplaceEntry {
            account_number: l.account_number,
            price: l.price,
            seller_public_key_hex: hex::encode(l.seller_public_key),
            state: "ForSale".to_string(),
            listed_at_block: l.listed_at_block,
        })
        .collect();
    Ok(MarketplaceResult { entries })
}

pub fn handle_list_validator_inventory(
    params: ListInventoryParams,
    state: &AppState,
) -> Result<InventoryResult, String> {
    let key: [u8; 32] = hex::decode(params.validator_public_key_hex.trim())
        .map_err(|e| format!("invalid validator public key hex: {e}"))?
        .try_into()
        .map_err(|_| "validator public key must be 32 bytes".to_string())?;

    let safebox = state
        .storage
        .safebox()
        .map_err(|e| format!("storage error: {e}"))?;

    let mut reserved = Vec::new();
    let mut for_sale = Vec::new();
    let mut owned = Vec::new();
    for acc in safebox.accounts.values() {
        if acc.account_info.account_key.ed25519_public_key != key {
            continue;
        }
        match acc.account_info.state {
            AccountState::Reserved => reserved.push(acc.account_number),
            AccountState::ForSale => for_sale.push(acc.account_number),
            AccountState::Owned => owned.push(acc.account_number),
            _ => {}
        }
    }
    reserved.sort();
    for_sale.sort();
    owned.sort();
    Ok(InventoryResult {
        reserved,
        for_sale,
        owned,
    })
}

/// List AUGEIDs currently in `GiftPending` state whose recipient is the given
/// public key. Read-only; backed by the resident `gift_index`.
pub fn handle_list_pending_gifts(
    params: ListPendingGiftsParams,
    state: &AppState,
) -> Result<PendingGiftsResult, String> {
    let recipient: [u8; 32] = hex::decode(params.recipient_public_key_hex.trim())
        .map_err(|e| format!("invalid recipient public key hex: {e}"))?
        .try_into()
        .map_err(|_| "recipient public key must be 32 bytes".to_string())?;

    let gifts = state
        .storage
        .list_pending_gifts()
        .map_err(|e| format!("storage error: {e}"))?;

    let mut entries = Vec::new();
    for g in gifts {
        if g.recipient_public_key != recipient {
            continue;
        }
        // The GiftPending account still holds the donor's key until acceptance,
        // so its current account_key identifies the sender (remetente).
        let from_public_key_hex = state
            .storage
            .get_account(g.account_number)
            .map_err(|e| format!("storage error: {e}"))?
            .map(|acc| hex::encode(acc.account_info.account_key.ed25519_public_key))
            .unwrap_or_default();
        let name = state
            .storage
            .get_account(g.account_number)
            .map_err(|e| format!("storage error: {e}"))?
            .and_then(|acc| acc.name);
        entries.push(PendingGiftEntry {
            account_number: g.account_number,
            recipient_public_key_hex: hex::encode(g.recipient_public_key),
            from_public_key_hex,
            gifted_at_block: g.gifted_at_block,
            name,
        });
    }
    entries.sort_by_key(|e| e.account_number);
    Ok(PendingGiftsResult { entries })
}

pub fn handle_buy_account(
    params: BuyAccountParams,
    state: &AppState,
) -> Result<LifecycleOpResult, String> {
    let new_pk: [u8; 32] = hex::decode(params.new_public_key_hex.trim())
        .map_err(|e| format!("invalid new public key hex: {e}"))?
        .try_into()
        .map_err(|_| "new public key must be 32 bytes".to_string())?;
    let sig = parse_signature(&params.signature_hex)?;

    let op = Operation {
        op_type: augecoin_core::operation::OperationType::BuyAccount,
        payload: OperationPayload::BuyAccount {
            buyer_account: params.buyer_account,
            n_operation: params.n_operation,
            account_to_purchase: params.account_to_purchase,
            amount: params.amount,
            fee: params.fee,
            new_ed25519_public_key: new_pk,
            seller_account: params.seller_account,
        },
        chain_id: state
            .node_status
            .chain_id
            .load(std::sync::atomic::Ordering::SeqCst),
        signatures: vec![sig],
    };
    match admit_signed_operation(state, op) {
        Ok(h) => Ok(LifecycleOpResult {
            accepted: true,
            op_hash_hex: Some(h),
            error: None,
        }),
        Err(e) => Ok(LifecycleOpResult {
            accepted: false,
            op_hash_hex: None,
            error: Some(e),
        }),
    }
}

pub fn handle_sell_account(
    params: SellAccountParams,
    state: &AppState,
) -> Result<LifecycleOpResult, String> {
    let new_pk: [u8; 32] = hex::decode(params.new_public_key_hex.trim())
        .map_err(|e| format!("invalid new public key hex: {e}"))?
        .try_into()
        .map_err(|_| "new public key must be 32 bytes".to_string())?;
    let sig = parse_signature(&params.signature_hex)?;

    let op = Operation {
        op_type: augecoin_core::operation::OperationType::ListAccountForSale,
        payload: OperationPayload::ListAccountForSale {
            account: params.account,
            n_operation: params.n_operation,
            sale_price: params.sale_price,
            account_to_pay: params.account_to_pay,
            new_ed25519_public_key: new_pk,
            locked_until_block: params.locked_until_block,
            fee: params.fee,
        },
        chain_id: state
            .node_status
            .chain_id
            .load(std::sync::atomic::Ordering::SeqCst),
        signatures: vec![sig],
    };
    match admit_signed_operation(state, op) {
        Ok(h) => Ok(LifecycleOpResult {
            accepted: true,
            op_hash_hex: Some(h),
            error: None,
        }),
        Err(e) => Ok(LifecycleOpResult {
            accepted: false,
            op_hash_hex: None,
            error: Some(e),
        }),
    }
}

pub fn handle_gift_account(
    params: GiftAccountParams,
    state: &AppState,
) -> Result<LifecycleOpResult, String> {
    let recipient: [u8; 32] = hex::decode(params.recipient_public_key_hex.trim())
        .map_err(|e| format!("invalid recipient public key hex: {e}"))?
        .try_into()
        .map_err(|_| "recipient public key must be 32 bytes".to_string())?;
    let sig = parse_signature(&params.signature_hex)?;

    let op = Operation {
        op_type: augecoin_core::operation::OperationType::GiftAccount,
        payload: OperationPayload::GiftAccount {
            account: params.account,
            n_operation: params.n_operation,
            recipient_public_key: recipient,
            fee: params.fee,
        },
        chain_id: state
            .node_status
            .chain_id
            .load(std::sync::atomic::Ordering::SeqCst),
        signatures: vec![sig],
    };
    match admit_signed_operation(state, op) {
        Ok(h) => Ok(LifecycleOpResult {
            accepted: true,
            op_hash_hex: Some(h),
            error: None,
        }),
        Err(e) => Ok(LifecycleOpResult {
            accepted: false,
            op_hash_hex: None,
            error: Some(e),
        }),
    }
}

pub fn handle_accept_gift(
    params: AcceptGiftParams,
    state: &AppState,
) -> Result<LifecycleOpResult, String> {
    let sig = parse_signature(&params.signature_hex)?;

    let op = Operation {
        op_type: augecoin_core::operation::OperationType::AcceptGift,
        payload: OperationPayload::AcceptGift {
            account: params.account,
            n_operation: params.n_operation,
            fee: params.fee,
        },
        chain_id: state
            .node_status
            .chain_id
            .load(std::sync::atomic::Ordering::SeqCst),
        signatures: vec![sig],
    };
    match admit_signed_operation(state, op) {
        Ok(h) => Ok(LifecycleOpResult {
            accepted: true,
            op_hash_hex: Some(h),
            error: None,
        }),
        Err(e) => Ok(LifecycleOpResult {
            accepted: false,
            op_hash_hex: None,
            error: Some(e),
        }),
    }
}

pub fn handle_cancel_sale(
    params: CancelSaleParams,
    state: &AppState,
) -> Result<LifecycleOpResult, String> {
    let sig = parse_signature(&params.signature_hex)?;

    let op = Operation {
        op_type: augecoin_core::operation::OperationType::DelistAccount,
        payload: OperationPayload::DelistAccount {
            account: params.account,
            n_operation: params.n_operation,
            fee: params.fee,
        },
        chain_id: state
            .node_status
            .chain_id
            .load(std::sync::atomic::Ordering::SeqCst),
        signatures: vec![sig],
    };
    match admit_signed_operation(state, op) {
        Ok(h) => Ok(LifecycleOpResult {
            accepted: true,
            op_hash_hex: Some(h),
            error: None,
        }),
        Err(e) => Ok(LifecycleOpResult {
            accepted: false,
            op_hash_hex: None,
            error: Some(e),
        }),
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod endpoint_tests {
    use super::*;
    use augecoin_core::account::{AccountInfo, AccountKey, AccountState};
    use augecoin_core::operation::OperationType;
    use augecoin_crypto::signature::Ed25519KeyPair;
    use std::sync::atomic::Ordering;

    fn create_keypair(seed: u64) -> Ed25519KeyPair {
        use augecoin_crypto::hdkeys::HdWallet;
        let seed_bytes = seed.to_be_bytes();
        let mut full_seed = [0u8; 64];
        full_seed[..8].copy_from_slice(&seed_bytes);
        HdWallet::from_seed(&full_seed).derive_keypair(0)
    }

    fn make_storage() -> (Storage, String) {
        use std::sync::atomic::AtomicU32;
        static C: AtomicU32 = AtomicU32::new(9000);
        let path = format!("/tmp/augecoin-rpc-ep-{}", C.fetch_add(1, Ordering::SeqCst));
        let storage = Storage::open(&path).unwrap();
        (storage, path)
    }

    fn make_state(storage: Storage) -> AppState {
        let node_status = Arc::new(NodeStatus::default());
        node_status
            .chain_id
            .store(1, std::sync::atomic::Ordering::SeqCst);
        let admin_kp = create_keypair(9999);
        let mut mempool = Mempool::new(augecoin_core::mempool::MempoolConfig {
            min_fee: augecoin_core::constants::MIN_FEE_AUGESAT,
            max_operations: 10_000,
            chain_id: 1,
            ttl_seconds: 3_600,
        });
        mempool.set_admin_pubkey(admin_kp.verifying_key().to_bytes());
        AppState {
            storage: Arc::new(storage),
            mempool: Arc::new(std::sync::Mutex::new(mempool)),
            node_status,
            api_keys: crate::auth::ApiKeyStore::empty(),
            rate_limiter: crate::auth::RateLimiter::new(1000, std::time::Duration::from_secs(60)),
            sensitive_rate_limiter: crate::auth::RateLimiter::new(
                50,
                std::time::Duration::from_secs(60),
            ),
            require_admin_auth: false,
            faucet_keypair: None,
            faucet_account: 1,
            faucet_amount: 100,
            faucet_claims: std::sync::Mutex::new(HashMap::new()),
            admin_keypair: Some(admin_kp),
            op_broadcaster: None,
        }
    }

    /// Simulate the consensus commit of pending admin/account operations:
    /// drain the mempool and apply each operation to storage exactly the way
    /// `execute_block` does (ValidatorAdmin -> ValidatorSet::apply_validator_admin,
    /// CreateAccount -> deterministic account creation).
    fn commit_pending_ops(state: &AppState) {
        let ops: Vec<Operation> = {
            let mut mp = state.mempool.lock().unwrap();
            let pending = mp.pending_cloned();
            mp.clear();
            pending
        };
        let height = state
            .node_status
            .block_height
            .load(std::sync::atomic::Ordering::SeqCst);
        for op in ops {
            match &op.payload {
                OperationPayload::ValidatorAdmin(_) => {
                    let raw = state.storage.get_validator_set_bytes().unwrap().unwrap();
                    let mut vs =
                        augecoin_consensus::validator::ValidatorSet::from_bytes(&raw).unwrap();
                    vs.apply_validator_admin(&op, height).unwrap();
                    vs.process_pending_activations(height);
                    state
                        .storage
                        .put_validator_set_bytes(&vs.to_bytes())
                        .unwrap();
                }
                OperationPayload::CreateAccount {
                    account_number,
                    pubkey,
                    initial_metadata,
                } => {
                    let mut acc = state
                        .storage
                        .get_account(*account_number)
                        .unwrap()
                        .unwrap_or_else(|| Account::new(*account_number, [0u8; 32], height));
                    acc.account_info.account_key = AccountKey {
                        ed25519_public_key: *pubkey,
                    };
                    acc.account_info.state = AccountState::Owned;
                    acc.n_operation = 0;
                    acc.account_data = initial_metadata.clone();
                    state.storage.put_account(&acc).unwrap();
                }
                _ => {}
            }
        }
    }

    fn make_test_account(number: u64, balance: u64) -> Account {
        let mut ed = [0u8; 32];
        ed[0..8].copy_from_slice(&number.to_be_bytes());
        Account {
            account_number: number,
            account_info: AccountInfo {
                state: AccountState::Normal,
                account_key: AccountKey {
                    ed25519_public_key: ed,
                },
                locked_until_block: 0,
                price: 0,
                account_to_pay: 0,
                new_public_key: None,
                hashed_secret: [0u8; 32],
            },
            balance,
            updated_on_block_passive_mode: 50,
            updated_on_block_active_mode: 50,
            n_operation: 3,
            name: None,
            account_type: 0,
            account_data: vec![],
            account_seal: vec![],
        }
    }

    // ── getaccount tests ─────────────────────────────────────────

    #[test]
    fn get_account_returns_existing() {
        let (storage, path) = make_storage();
        let state = make_state(storage);
        state
            .storage
            .put_account(&make_test_account(42, 5000))
            .unwrap();

        let result = handle_get_account(
            GetAccountParams {
                account_number: Some(42),
                address: None,
            },
            &state,
        );
        let acc = result.unwrap();
        assert_eq!(acc.account_number, 42);
        assert_eq!(acc.balance, 5000);

        std::fs::remove_dir_all(&path).ok();
    }

    #[test]
    fn get_account_not_found() {
        let (storage, path) = make_storage();
        let state = make_state(storage);

        let result = handle_get_account(
            GetAccountParams {
                account_number: Some(999),
                address: None,
            },
            &state,
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));

        std::fs::remove_dir_all(&path).ok();
    }

    // ── getblock tests ───────────────────────────────────────────

    #[test]
    fn get_block_not_found() {
        let (storage, path) = make_storage();
        let state = make_state(storage);

        let result = handle_get_block(GetBlockParams { block_number: 999 }, &state);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));

        std::fs::remove_dir_all(&path).ok();
    }

    #[test]
    fn get_block_operations_not_found() {
        let (storage, path) = make_storage();
        let state = make_state(storage);

        let result = handle_get_block_operations(GetBlockParams { block_number: 999 }, &state);
        assert!(result.is_err());

        std::fs::remove_dir_all(&path).ok();
    }

    // ── sendoperation tests ─────────────────────────────────────

    #[test]
    fn send_operation_invalid_hex() {
        let (storage, path) = make_storage();
        let state = make_state(storage);

        let result = handle_send_operation(
            SendOperationParams {
                hex: "not-hex-zzz".into(),
            },
            &state,
        );
        assert!(result.is_err());

        std::fs::remove_dir_all(&path).ok();
    }

    #[test]
    fn send_operation_rejected_without_signatures() {
        let (storage, path) = make_storage();
        let state = make_state(storage);

        let op = Operation {
            chain_id: 1,
            op_type: OperationType::Transaction,
            payload: OperationPayload::Transaction {
                senders: vec![SenderInfo {
                    account: 1,
                    n_operation: 0,
                    amount: 100,
                    payload: vec![],
                }],
                receivers: vec![ReceiverInfo {
                    account: 2,
                    amount: 90,
                    payload: vec![],
                }],
                changers: vec![],
                fee: 10,
            },
            signatures: vec![],
        };
        let op_hex = hex::encode(op.to_bytes());

        let result = handle_send_operation(SendOperationParams { hex: op_hex }, &state);
        let resp = result.unwrap();
        assert!(!resp.accepted);
        assert!(resp.error.unwrap().contains("invalid signature"));

        std::fs::remove_dir_all(&path).ok();
    }

    #[test]
    fn send_operation_accepted() {
        let (storage, path) = make_storage();
        let kp = create_keypair(1001);

        let ed = kp.verifying_key().to_bytes();
        let acc = Account {
            account_number: 1,
            account_info: AccountInfo {
                state: AccountState::Normal,
                account_key: AccountKey {
                    ed25519_public_key: ed,
                },
                locked_until_block: 0,
                price: 0,
                account_to_pay: 0,
                new_public_key: None,
                hashed_secret: [0u8; 32],
            },
            balance: 10000,
            updated_on_block_passive_mode: 50,
            updated_on_block_active_mode: 50,
            n_operation: 0,
            name: None,
            account_type: 0,
            account_data: vec![],
            account_seal: vec![],
        };
        let state = make_state(storage);
        state.storage.put_account(&acc).unwrap();

        let op = Operation {
            chain_id: 1,
            op_type: OperationType::Transaction,
            payload: OperationPayload::Transaction {
                senders: vec![SenderInfo {
                    account: 1,
                    n_operation: 0,
                    amount: 100,
                    payload: vec![],
                }],
                receivers: vec![ReceiverInfo {
                    account: 2,
                    amount: 90,
                    payload: vec![],
                }],
                changers: vec![],
                fee: augecoin_core::constants::MIN_FEE_AUGESAT,
            },
            signatures: vec![],
        };
        let msg = op.to_bytes_stripped();
        let sig = kp.sign(&msg);
        let op = Operation {
            signatures: vec![sig],
            ..op
        };

        let op_hex = hex::encode(op.to_bytes());

        let result = handle_send_operation(SendOperationParams { hex: op_hex }, &state);
        let resp = result.unwrap();
        assert!(resp.accepted);

        std::fs::remove_dir_all(&path).ok();
    }

    // ── getpendings tests ────────────────────────────────────────

    #[test]
    fn get_pendings_empty() {
        let (storage, path) = make_storage();
        let state = make_state(storage);

        let result = handle_get_pendings(&state);
        let info = result.unwrap();
        assert_eq!(info.size, 0);
        assert!(info.operations.is_empty());

        std::fs::remove_dir_all(&path).ok();
    }

    // ── nodestatus tests ─────────────────────────────────────────

    #[test]
    fn nodestatus_returns_current_state() {
        let (storage, path) = make_storage();
        let state = make_state(storage);
        state.node_status.block_height.store(100, Ordering::SeqCst);
        state.node_status.peers_connected.store(5, Ordering::SeqCst);

        let result = handle_get_node_status(&state);
        let status = result.unwrap();
        assert_eq!(status.current_height, 100);
        assert_eq!(status.peers_connected, 5);
        assert!(!status.syncing);

        std::fs::remove_dir_all(&path).ok();
    }

    #[test]
    fn nodestatus_shows_syncing() {
        let (storage, path) = make_storage();
        let state = make_state(storage);
        state.node_status.block_height.store(100, Ordering::SeqCst);
        state
            .node_status
            .sync_target_height
            .store(500, Ordering::SeqCst);

        let result = handle_get_node_status(&state);
        let status = result.unwrap();
        assert!(status.syncing);
        assert_eq!(status.sync_target_height, 500);

        std::fs::remove_dir_all(&path).ok();
    }

    // ── findaccounts tests ───────────────────────────────────────

    #[test]
    fn find_accounts_empty() {
        let (storage, path) = make_storage();
        let state = make_state(storage);

        let result = handle_find_accounts(
            FindAccountsParams {
                name: None,
                account_type: None,
                min_balance: None,
                max_balance: None,
                start: 0,
                max: 100,
            },
            &state,
        );
        let resp = result.unwrap();
        assert_eq!(resp.accounts.len(), 0);

        std::fs::remove_dir_all(&path).ok();
    }

    #[test]
    fn find_accounts_with_filters() {
        let (storage, path) = make_storage();
        let state = make_state(storage);
        state
            .storage
            .put_account(&make_test_account(1, 1000))
            .unwrap();
        state
            .storage
            .put_account(&make_test_account(2, 500))
            .unwrap();
        state
            .storage
            .put_account(&make_test_account(3, 2000))
            .unwrap();

        let result = handle_find_accounts(
            FindAccountsParams {
                name: None,
                account_type: None,
                min_balance: Some(1000),
                max_balance: None,
                start: 0,
                max: 100,
            },
            &state,
        );
        let resp = result.unwrap();
        assert_eq!(resp.accounts.len(), 2);

        std::fs::remove_dir_all(&path).ok();
    }

    // ── getaccountcount / getblockcount tests ────────────────────

    #[test]
    fn get_account_count() {
        let (storage, path) = make_storage();
        let state = make_state(storage);
        state
            .storage
            .put_account(&make_test_account(1, 100))
            .unwrap();
        state
            .storage
            .put_account(&make_test_account(2, 100))
            .unwrap();

        let count = handle_get_account_count(&state).unwrap();
        assert_eq!(count, 2);

        std::fs::remove_dir_all(&path).ok();
    }

    #[test]
    fn get_block_count() {
        let (storage, path) = make_storage();
        let state = make_state(storage);
        state.storage.put_height(42).unwrap();

        let count = handle_get_block_count(&state).unwrap();
        assert_eq!(count, 42);

        std::fs::remove_dir_all(&path).ok();
    }

    // ── validatorset tests ───────────────────────────────────────

    fn seed_validator_set(state: &AppState) {
        let kp = create_keypair(9999);
        let vk_bytes = kp.verifying_key().to_bytes();
        let mut vs = augecoin_consensus::validator::ValidatorSet::new(kp.verifying_key(), vec![]);
        let current_height = state
            .node_status
            .block_height
            .load(std::sync::atomic::Ordering::SeqCst);
        vs.add_validator_direct(vk_bytes, current_height, current_height);
        let data = vs.to_bytes();
        state.storage.put_validator_set_bytes(&data).unwrap();
    }

    #[test]
    fn get_validator_set_returns_data() {
        let (storage, path) = make_storage();
        let state = make_state(storage);
        seed_validator_set(&state);

        let result = handle_get_validator_set(&state);
        let vs = result.unwrap();
        assert_eq!(vs.total, 1);
        assert_eq!(vs.active.len(), 1);

        std::fs::remove_dir_all(&path).ok();
    }

    // ── validator admin tests ────────────────────────────────────
    //
    // These tests exercise the real flow: RPC handler signs and submits the
    // ValidatorAdmin operation to the mempool; a simulated consensus commit
    // then applies the pending operations to storage; only then is the
    // resulting state asserted.

    #[test]
    fn validator_add_and_remove() {
        let (storage, path) = make_storage();
        let state = make_state(storage);
        seed_validator_set(&state);
        state.node_status.block_height.store(1000, Ordering::SeqCst);

        let new_kp = create_keypair(8888);
        let new_pk_hex = hex::encode(new_kp.verifying_key().to_bytes());

        let add_result = handle_validator_add(
            ValidatorAddParams {
                ed25519_public_key_hex: new_pk_hex.clone(),
                activation_height: 100,
            },
            &state,
        );
        assert!(add_result.unwrap().success);

        // Before commit the validator set in storage is unchanged.
        let vs_before = handle_get_validator_set(&state).unwrap();
        assert_eq!(vs_before.total, 1);

        // Consensus commit applies the pending admin operation.
        commit_pending_ops(&state);

        let vs_after_add = handle_get_validator_set(&state).unwrap();
        assert_eq!(vs_after_add.total, 2);
        assert!(vs_after_add
            .active
            .iter()
            .any(|v| v.ed25519_public_key_hex == new_pk_hex));

        // remove the validator we just added (id=1, since 0 is the seed)
        let remove_result = handle_validator_remove(
            ValidatorRemoveParams {
                validator_id: 1,
                activation_height: 200,
            },
            &state,
        );
        assert!(remove_result.unwrap().success);

        commit_pending_ops(&state);

        let vs_after_remove = handle_get_validator_set(&state).unwrap();
        assert_eq!(vs_after_remove.total, 1);
        assert!(!vs_after_remove
            .active
            .iter()
            .any(|v| v.ed25519_public_key_hex == new_pk_hex));

        std::fs::remove_dir_all(&path).ok();
    }

    #[test]
    fn validator_activate_deactivate() {
        let (storage, path) = make_storage();
        let state = make_state(storage);
        seed_validator_set(&state);
        state.node_status.block_height.store(1000, Ordering::SeqCst);

        let new_kp = create_keypair(7777);
        let new_pk_hex = hex::encode(new_kp.verifying_key().to_bytes());

        handle_validator_add(
            ValidatorAddParams {
                ed25519_public_key_hex: new_pk_hex,
                activation_height: 100,
            },
            &state,
        )
        .unwrap();
        commit_pending_ops(&state);

        let deactivate_result = handle_validator_deactivate(
            ValidatorDeactivateParams {
                validator_id: 1,
                activation_height: 250,
            },
            &state,
        );
        assert!(deactivate_result.unwrap().success);
        commit_pending_ops(&state);

        let vs = handle_get_validator_set(&state).unwrap();
        assert_eq!(vs.total, 1, "deactivated validator must not be active");

        let activate_result = handle_validator_activate(
            ValidatorActivateParams {
                validator_id: 1,
                activation_height: 150,
            },
            &state,
        );
        assert!(activate_result.unwrap().success);
        commit_pending_ops(&state);

        let vs = handle_get_validator_set(&state).unwrap();
        assert_eq!(vs.total, 2, "reactivated validator must be active again");

        std::fs::remove_dir_all(&path).ok();
    }

    #[test]
    fn validator_remove_nonexistent() {
        let (storage, path) = make_storage();
        let state = make_state(storage);
        seed_validator_set(&state);

        let result = handle_validator_remove(
            ValidatorRemoveParams {
                validator_id: 99,
                activation_height: 100,
            },
            &state,
        );
        let resp = result.unwrap();
        assert!(!resp.success);
        assert!(resp.error.unwrap().contains("not found"));

        std::fs::remove_dir_all(&path).ok();
    }

    // ── createaccount tests ──────────────────────────────────────

    #[test]
    fn create_account_goes_through_mempool_and_commit() {
        let (storage, path) = make_storage();
        let state = make_state(storage);
        seed_validator_set(&state);
        state.node_status.block_height.store(10, Ordering::SeqCst);

        // The leader owns a Reserved AUGEID emitted by a previous block.
        let augeid = 100u64;
        let mut reserved = Account::new(augeid, [0u8; 32], 10);
        reserved.account_info.state = AccountState::Reserved;
        state.storage.put_account(&reserved).unwrap();

        let kp = create_keypair(4242);
        let pk_hex = hex::encode(kp.verifying_key().to_bytes());

        let result = handle_create_account(
            CreateAccountParams {
                account_number: augeid,
                public_key_hex: pk_hex.clone(),
                metadata_hex: None,
            },
            &state,
        )
        .unwrap();
        assert!(result.accepted);
        assert_eq!(result.status, "pending");
        assert!(result.op_hash_hex.is_some());

        // Not activated in storage before consensus commit.
        let before = state.storage.get_account(augeid).unwrap().unwrap();
        assert_eq!(before.account_info.state, AccountState::Reserved);

        commit_pending_ops(&state);

        let created = state.storage.get_account(augeid).unwrap().unwrap();
        assert_eq!(
            created.account_info.account_key.ed25519_public_key,
            kp.verifying_key().to_bytes()
        );
        assert_eq!(created.account_info.state, AccountState::Owned);
        assert_eq!(created.balance, 0);

        // Idempotent by key: reports the existing account.
        let again = handle_create_account(
            CreateAccountParams {
                account_number: augeid,
                public_key_hex: pk_hex,
                metadata_hex: None,
            },
            &state,
        )
        .unwrap();
        assert!(again.accepted);
        assert_eq!(again.status, "exists");
        assert_eq!(again.account_number, Some(augeid));

        // getaccount by address returns the same account.
        let by_addr = handle_get_account(
            GetAccountParams {
                account_number: None,
                address: Some(result.address.clone()),
            },
            &state,
        )
        .unwrap();
        assert_eq!(by_addr.account_number, augeid);

        std::fs::remove_dir_all(&path).ok();
    }

    // ── getoperations tests ──────────────────────────────────────

    #[test]
    fn get_operations_block_not_found() {
        let (storage, path) = make_storage();
        let state = make_state(storage);

        let result = handle_get_operations(
            GetOperationsParams {
                block_number: 999,
                start: 0,
                limit: None,
            },
            &state,
        );
        assert!(result.is_err());

        std::fs::remove_dir_all(&path).ok();
    }
}
