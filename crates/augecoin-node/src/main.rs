mod execution;
mod genesis;
mod metrics;

use augecoin_consensus::validator::ValidatorSet;
use augecoin_core::block::OperationBlock;
use augecoin_core::constants::{CT_CHAIN_ID_TESTNET, MIN_FEE_AUGESAT};
use augecoin_core::mempool::{Mempool, MempoolConfig};
use augecoin_core::operation::Operation;
use augecoin_core::proposal::{op_hash, GetTransactions, ProposalMessage, TransactionsResponse};
use augecoin_network::transport::start_consensus_network;
use augecoin_network::{identity_from_ed25519_seed, parse_external_address, Bootnodes};
use augecoin_node::alerts::AlertManager;
use augecoin_node::consensus::{reconstruct_from_proposal, ConsensusEngine, ConsensusEvent};
use augecoin_node::metrics::{MetricsServer, NodeMetrics};
use augecoin_storage::Storage;
use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn start_rpc_server(
    storage: Arc<Storage>,
    mempool: Arc<Mutex<Mempool>>,
    node_status: Arc<augecoin_rpc::endpoints::NodeStatus>,
    port: u16,
    admin_keypair: Option<augecoin_crypto::signature::HybridKeyPair>,
    faucet_enabled: bool,
    op_broadcast_tx: crossbeam_channel::Sender<Vec<u8>>,
) {
    let admin_api_keys: Vec<String> = std::env::var("AUGECOIN_ADMIN_API_KEYS")
        .unwrap_or_default()
        .split(',')
        .filter(|s| !s.is_empty())
        .map(|s| s.trim().to_string())
        .collect();
    let keys_configured = !admin_api_keys.is_empty();
    let rate_limit: u32 = std::env::var("AUGECOIN_RPC_RATE_LIMIT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(300);
    let faucet_keypair = if faucet_enabled {
        // Fail-fast already happened in main(); the variable is guaranteed
        // to be present and well-formed here.
        let raw = std::env::var("AUGECOIN_FAUCET_KEY_HEX").expect("validated at startup");
        let bytes = hex::decode(raw.trim()).expect("validated at startup");
        let seed: [u8; 32] = bytes[..32].try_into().expect("validated at startup");
        Some(augecoin_crypto::signature::HybridKeyPair::from_seed(seed))
    } else {
        None
    };
    let faucet_account = std::env::var("AUGECOIN_FAUCET_ACCOUNT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let faucet_amount = std::env::var("AUGECOIN_FAUCET_AMOUNT_AUGE")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(100)
        .saturating_mul(augecoin_core::constants::ONE_AUGE);
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("failed to create tokio runtime for RPC");
        rt.block_on(async move {
            augecoin_rpc::install_crypto_provider();

            let (cert_pem, key_pem) = match augecoin_rpc::load_tls_pem() {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("[rpc] FATAL: TLS configuration error: {e}");
                    return;
                }
            };
            let tls_config =
                match augecoin_rpc::RpcServer::rustls_config_from_pem(&cert_pem, &key_pem) {
                    Ok(c) => c,
                    Err(e) => {
                        eprintln!("[rpc] FATAL: TLS setup failed: {e}");
                        return;
                    }
                };

            let state = Arc::new(augecoin_rpc::endpoints::AppState {
                storage,
                mempool,
                node_status,
                api_keys: augecoin_rpc::auth::ApiKeyStore::new(admin_api_keys),
                rate_limiter: augecoin_rpc::auth::RateLimiter::new(rate_limit, Duration::from_secs(60)),
                sensitive_rate_limiter: augecoin_rpc::auth::RateLimiter::new(10, Duration::from_secs(60)),
                require_admin_auth: true,
                faucet_keypair,
                faucet_account,
                faucet_amount,
                faucet_claims: std::sync::Mutex::new(std::collections::HashMap::new()),
                admin_keypair,
                op_broadcaster: Some(op_broadcast_tx),
            });

            let settings = augecoin_rpc::RpcSettings {
                jsonrpc_addr: format!("0.0.0.0:{port}").parse().expect("invalid RPC addr"),
            };
            let server = augecoin_rpc::RpcServer::new(settings);
            let faucet_active = state.faucet_keypair.is_some();
            let faucet_account = state.faucet_account;
            let faucet_amount = state.faucet_amount;
            let faucet_address = state.faucet_keypair.as_ref().map(|kp| {
                augecoin_crypto::address::derive_address(&kp.verifying_key())
            });
            match server.start_jsonrpc(tls_config, state).await {
                Ok(addr) => {
                    println!("[rpc] JSON-RPC over TLS listening on {addr}");
                    if !keys_configured {
                        eprintln!(
                            "[rpc] WARNING: no AUGECOIN_ADMIN_API_KEYS set; admin methods \
                             (validatorAdd/Remove/Activate/Deactivate) are blocked"
                        );
                    } else {
                        println!("[rpc] admin API key auth enabled");
                    }
                    if faucet_active {
                        println!(
                            "[faucet] ACTIVE account={} amount={} AUGE address={} (endpoint: POST /faucet)",
                            faucet_account,
                            faucet_amount / augecoin_core::constants::ONE_AUGE,
                            faucet_address.as_deref().unwrap_or("unknown"),
                        );
                        println!(
                            "[faucet] fund the faucet address above and set AUGECOIN_FAUCET_ACCOUNT to its account number"
                        );
                    } else {
                        println!("[faucet] disabled (start with --enable-faucet to enable)");
                    }
                }
                Err(e) => eprintln!("[rpc] FATAL: failed to start RPC server: {e}"),
            }
            std::future::pending::<()>().await;
        });
    });
}

fn start_wallet_server(port: u16) {
    std::thread::spawn(move || {
        let listener = match TcpListener::bind(format!("0.0.0.0:{port}")) {
            Ok(l) => l,
            Err(e) => {
                eprintln!("[wallet] failed to bind 0.0.0.0:{port}: {e}");
                return;
            }
        };
        println!("[wallet] static file server listening on 0.0.0.0:{port}");
        for mut stream in listener.incoming().flatten() {
            stream.set_read_timeout(Some(Duration::from_secs(2))).ok();
            let mut buf = vec![0u8; 4096];
            let n = match stream.read(&mut buf) {
                Ok(n) if n > 0 => n,
                _ => continue,
            };
            let req = String::from_utf8_lossy(&buf[..n]);
            if req.starts_with("GET /") {
                serve_static(&mut stream, &req);
            } else {
                let _ = stream
                    .write_all(b"HTTP/1.1 405 Method Not Allowed\r\nConnection: close\r\n\r\n");
            }
        }
    });
}

fn serve_static(stream: &mut std::net::TcpStream, req: &str) {
    let path = if let Some(line) = req.lines().next() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 {
            parts[1].trim_start_matches('/')
        } else {
            "index.html"
        }
    } else {
        "index.html"
    };
    let path = if path.is_empty() { "index.html" } else { path };

    let wallet_dir =
        std::env::var("AUGECOIN_WALLET_DIR").unwrap_or_else(|_| "apps/wallet-web/dist".to_string());
    let file_path = format!("{wallet_dir}/{path}");

    if let Ok(data) = std::fs::read(&file_path) {
        let mime = if path.ends_with(".html") {
            "text/html"
        } else if path.ends_with(".js") {
            "application/javascript"
        } else if path.ends_with(".css") {
            "text/css"
        } else if path.ends_with(".wasm") {
            "application/wasm"
        } else {
            "application/octet-stream"
        };
        let resp = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: {mime}\r\nAccess-Control-Allow-Origin: *\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            data.len()
        );
        let _ = stream.write_all(resp.as_bytes());
        let _ = stream.write_all(&data);
    } else {
        let err = "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
        let _ = stream.write_all(err.as_bytes());
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NodePhase {
    Idle,
    AwaitingSignatures,
    Executed,
}

#[allow(clippy::too_many_arguments)]
fn update_node_status(
    status: &augecoin_rpc::endpoints::NodeStatus,
    block_height: u64,
    latest_block_hash: [u8; 64],
    mempool_size: usize,
    current_round: u64,
    current_view: u64,
    peers_connected: u32,
    peers_gossipsub: u32,
    peers_kademlia: u32,
    syncing: bool,
    sync_target_height: u64,
    uptime_seconds: u64,
    last_error: Option<String>,
) {
    status.block_height.store(block_height, Ordering::SeqCst);
    if let Ok(mut guard) = status.latest_block_hash.lock() {
        *guard = latest_block_hash;
    }
    status
        .mempool_size
        .store(mempool_size as u64, Ordering::SeqCst);
    status.current_round.store(current_round, Ordering::SeqCst);
    status.current_view.store(current_view, Ordering::SeqCst);
    status
        .peers_connected
        .store(peers_connected, Ordering::SeqCst);
    status
        .peers_gossipsub_consensus
        .store(peers_gossipsub, Ordering::SeqCst);
    status
        .peers_kademlia_total
        .store(peers_kademlia, Ordering::SeqCst);
    status.sync_target_height.store(
        if syncing {
            sync_target_height
        } else {
            block_height
        },
        Ordering::SeqCst,
    );
    status
        .syncing
        .store(if syncing { 1 } else { 0 }, Ordering::SeqCst);
    status
        .uptime_seconds
        .store(uptime_seconds, Ordering::SeqCst);
    if let Ok(mut guard) = status.last_consensus_error.lock() {
        *guard = last_error;
    }
}

fn load_safe_box_hash(storage: &Storage) -> [u8; 64] {
    storage.safebox_hash().unwrap_or([0u8; 64])
}

/// Update SafeBox / RocksDB persistence metrics after a block commit.
fn update_safebox_metrics(storage: &Storage, node_metrics: &NodeMetrics, force: bool) {
    let before = std::time::Instant::now();
    if force {
        let _ = storage.flush_safebox_snapshot();
    }
    let duration = before.elapsed().as_secs_f64();

    node_metrics
        .dirty_accounts
        .set(storage.dirty_accounts() as i64);
    node_metrics
        .rocksdb_log_size
        .set(storage.rocksdb_log_size() as i64);
    node_metrics
        .safebox_bytes
        .set(storage.last_snapshot_bytes() as i64);
    node_metrics.wal_flush_total.inc();
    if force {
        node_metrics.snapshot_total.inc();
        node_metrics.snapshot_duration_seconds.observe(duration);
    }
}

fn load_latest_block_hash(storage: &Storage, height: u64) -> [u8; 64] {
    if height == 0 {
        return [0u8; 64];
    }
    storage
        .get_block(height)
        .ok()
        .flatten()
        .map(|b| b.hash())
        .unwrap_or([0u8; 64])
}

fn sync_validator_set_from_storage(engine: &mut ConsensusEngine, storage: &Storage) {
    if let Ok(Some(bytes)) = storage.get_validator_set_bytes() {
        if let Ok(new_set) = ValidatorSet::from_bytes(&bytes) {
            let old_active = engine.validator_set.active_validators().len();
            let new_active = new_set.active_validators().len();
            if old_active != new_active {
                if new_active > old_active {
                    println!(
                        "[validator] validator added: ValidatorSet updated {} -> {} active validators",
                        old_active, new_active
                    );
                } else {
                    println!(
                        "[validator] validator removed: ValidatorSet updated {} -> {} active validators",
                        old_active, new_active
                    );
                }
                let new_quorum = augecoin_consensus::quorum::quorum_threshold(new_active as u64);
                println!(
                    "[validator] quorum recalculated: {}/{}",
                    new_quorum, new_active
                );
            }
            engine.update_validator_set(new_set);
        }
    }
}

fn parse_cli_args() -> (bool, Option<u64>) {
    let args: Vec<String> = std::env::args().collect();
    let mut enable_faucet = false;
    let mut node_id: Option<u64> = None;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--enable-faucet" => enable_faucet = true,
            "--node-id" => {
                i += 1;
                let raw = args
                    .get(i)
                    .map(|s| s.trim_start_matches('v').trim().to_string())
                    .unwrap_or_default();
                node_id = raw.parse().ok();
            }
            _ => {}
        }
        i += 1;
    }
    (enable_faucet, node_id)
}

fn validate_faucet_config(faucet_enabled: bool) {
    if !faucet_enabled {
        return;
    }
    match std::env::var("AUGECOIN_FAUCET_KEY_HEX") {
        Ok(raw) => {
            let raw = raw.trim();
            match hex::decode(raw) {
                Ok(bytes) if bytes.len() >= 32 => {}
                Ok(bytes) => {
                    eprintln!(
                        "FATAL: faucet is enabled but AUGECOIN_FAUCET_KEY_HEX contains only {} \
                         byte(s); it must be at least 32 bytes (64 hex chars).\n\
                         Generate a new faucet key with: ./scripts/setup-faucet.sh",
                        bytes.len()
                    );
                    std::process::exit(1);
                }
                Err(e) => {
                    eprintln!(
                        "FATAL: faucet is enabled but AUGECOIN_FAUCET_KEY_HEX is not valid hex: {e}.\n\
                         Generate a new faucet key with: ./scripts/setup-faucet.sh"
                    );
                    std::process::exit(1);
                }
            }
        }
        Err(_) => {
            eprintln!(
                "FATAL: faucet is enabled (--enable-faucet or AUGECOIN_ENABLE_FAUCET=1) but \
                 AUGECOIN_FAUCET_KEY_HEX is not set.\n\
                 Refusing to start half-functional. Options:\n\
                   1. Generate a faucet key:  ./scripts/setup-faucet.sh\n\
                      then export it:         export AUGECOIN_FAUCET_KEY_HEX=<hex>\n\
                   2. Or start without the faucet (remove --enable-faucet)."
            );
            std::process::exit(1);
        }
    }
}

fn main() {
    let startup_time = now_secs();
    let (faucet_flag, cli_node_id) = parse_cli_args();
    let faucet_enabled =
        faucet_flag || std::env::var("AUGECOIN_ENABLE_FAUCET").unwrap_or_default() == "1";
    validate_faucet_config(faucet_enabled);

    let validator_id: u64 = cli_node_id.unwrap_or_else(|| {
        std::env::var("AUGECOIN_VALIDATOR_ID")
            .unwrap_or_else(|_| "0".to_string())
            .parse()
            .unwrap_or(0)
    });
    let validator_count: u64 = std::env::var("AUGECOIN_VALIDATOR_COUNT")
        .unwrap_or_else(|_| "4".to_string())
        .parse()
        .unwrap_or(4);
    let chain_id: u64 = std::env::var("AUGECOIN_CHAIN_ID")
        .unwrap_or_else(|_| CT_CHAIN_ID_TESTNET.to_string())
        .parse()
        .unwrap_or(CT_CHAIN_ID_TESTNET);

    let data_dir = std::env::var("AUGECOIN_DATA_DIR")
        .unwrap_or_else(|_| format!("/opt/augecoin/data/node-{validator_id}"));
    let metrics_port: u16 = std::env::var("AUGECOIN_METRICS_PORT")
        .unwrap_or_else(|_| (9100 + validator_id * 10).to_string())
        .parse()
        .unwrap_or(9100);
    let rpc_port: u16 = std::env::var("AUGECOIN_RPC_PORT")
        .unwrap_or_else(|_| (9005 + validator_id).to_string())
        .parse()
        .unwrap_or(9005);
    let wallet_port: u16 = std::env::var("AUGECOIN_WALLET_PORT")
        .unwrap_or_else(|_| (8081 + validator_id).to_string())
        .parse()
        .unwrap_or(8081);
    let p2p_port: u16 = std::env::var("AUGECOIN_P2P_PORT")
        .unwrap_or_else(|_| (9101 + validator_id).to_string())
        .parse()
        .unwrap_or(9101);
    let external_address: Option<augecoin_network::Multiaddr> =
        match std::env::var("AUGECOIN_EXTERNAL_ADDRESS") {
            Ok(raw) if !raw.trim().is_empty() => match parse_external_address(&raw) {
                Ok(addr) => Some(addr),
                Err(e) => {
                    eprintln!("[network] FATAL: invalid AUGECOIN_EXTERNAL_ADDRESS: {e}");
                    std::process::exit(1);
                }
            },
            _ => None,
        };
    let block_time: u64 = std::env::var("AUGECOIN_BLOCK_TIME")
        .unwrap_or_else(|_| augecoin_core::constants::CT_BLOCK_TIME_SECONDS.to_string())
        .parse()
        .unwrap_or(augecoin_core::constants::CT_BLOCK_TIME_SECONDS);

    // Light-proposal transport (FASE 2). Default ON. Set AUGECOIN_HEAVY_PROPOSAL=1
    // to force the legacy full-block proposal (fallback for mixed networks).
    let heavy_proposal = std::env::var("AUGECOIN_HEAVY_PROPOSAL").unwrap_or_default() == "1";
    // Optional zstd compression for TransactionsResponse (negotiated per peer).
    let tx_compression = std::env::var("AUGECOIN_TX_COMPRESSION").unwrap_or_default() != "0";
    // Fetch timeout (seconds) and max retries for missing transactions.
    let fetch_timeout_secs: u64 = std::env::var("AUGECOIN_TX_FETCH_TIMEOUT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(3);
    let fetch_max_retries: u32 = std::env::var("AUGECOIN_TX_FETCH_RETRIES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(3);

    let bootnodes_str = std::env::var("AUGECOIN_BOOTNODES").unwrap_or_else(|_| "".to_string());
    let bootnode_addrs: Vec<augecoin_network::Multiaddr> = bootnodes_str
        .split(',')
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.trim().parse().ok())
        .collect();

    println!("══════════════════════════════════════════");
    println!("       AUGECOIN NODE - Validator #{validator_id:<6}");
    println!("══════════════════════════════════════════");
    println!(" Data dir:    {data_dir:<28}");
    println!(" Chain ID:    {chain_id:<28}");
    println!(" RPC:         0.0.0.0:{rpc_port:<23}");
    println!(" Wallet:      0.0.0.0:{wallet_port:<23}");
    println!(" Metrics:     0.0.0.0:{metrics_port:<23}");
    println!(" P2P LibP2P:  0.0.0.0:{p2p_port:<23}");
    println!(" Block time:  {block_time}s{:<25}", "");
    println!("══════════════════════════════════════════");
    println!();

    println!("[init] opening RocksDB at {data_dir}");
    std::fs::create_dir_all(&data_dir).expect("failed to create data directory");
    let storage = Arc::new(Storage::open(&data_dir).expect("failed to open storage"));

    let is_dev_mode = std::env::var("AUGECOIN_DEV_MODE").unwrap_or_default() == "1";

    // SafeBox snapshot interval: only full persistence is throttled, never the
    // in-memory (consensus) state. Dev writes a snapshot every 100 blocks,
    // mainnet every 1000. Overridable via AUGECOIN_SAFEBOX_SNAPSHOT_INTERVAL.
    let safebox_snapshot_interval = std::env::var("AUGECOIN_SAFEBOX_SNAPSHOT_INTERVAL")
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .unwrap_or(if is_dev_mode {
            augecoin_core::constants::SAFEBOX_SNAPSHOT_INTERVAL_DEV
        } else {
            augecoin_core::constants::SAFEBOX_SNAPSHOT_INTERVAL_MAINNET
        });
    storage.set_safebox_snapshot_interval(safebox_snapshot_interval);
    println!("[init] SafeBox snapshot interval: {safebox_snapshot_interval} blocks");

    let genesis_path = std::env::var("AUGECOIN_GENESIS_CONFIG").ok();
    genesis::initialize(&storage, genesis_path.as_deref().map(std::path::Path::new))
        .unwrap_or_else(|e| {
            eprintln!("[genesis] FATAL: {e}");
            std::process::exit(1);
        });

    let (initial_validator_set, my_keypair, _admin_kp) = if is_dev_mode {
        let (vs, all_keys, admin) =
            augecoin_node::consensus::create_dev_validator_set_and_keys(validator_count);
        let my_kp = if (validator_id as usize) < all_keys.len() {
            all_keys[validator_id as usize].1.clone()
        } else {
            // Validator id outside the genesis set (e.g. a 5th validator
            // joining a 4-node dev network): derive its key deterministically.
            augecoin_node::consensus::create_dev_validator_key(validator_id)
        };
        (vs, my_kp, admin)
    } else {
        let key_hex = if let Ok(hex) = std::env::var("AUGECOIN_VALIDATOR_KEY_HEX") {
            hex
        } else if let Ok(file_path) = std::env::var("AUGECOIN_VALIDATOR_KEY_FILE") {
            std::fs::read_to_string(&file_path).unwrap_or_else(|e| {
                eprintln!("ERROR: cannot read AUGECOIN_VALIDATOR_KEY_FILE '{file_path}': {e}");
                std::process::exit(1);
            })
        } else {
            let is_terminal = unsafe { libc::isatty(libc::STDIN_FILENO) != 0 };
            if !is_terminal {
                eprintln!(
                    "FATAL: No validator key configured. Set AUGECOIN_VALIDATOR_KEY_HEX, \
                     AUGECOIN_VALIDATOR_KEY_FILE, or AUGECOIN_DEV_MODE=1."
                );
                std::process::exit(1);
            }
            eprintln!(
                "WARNING: No validator key configured (interactive terminal detected).\n\
                 Set AUGECOIN_VALIDATOR_KEY_HEX, AUGECOIN_VALIDATOR_KEY_FILE, or AUGECOIN_DEV_MODE=1.\n\
                 Refusing to start without a key."
            );
            std::process::exit(1);
        };

        let key_hex = key_hex.trim();
        let key_bytes = hex::decode(key_hex).unwrap_or_else(|e| {
            eprintln!("ERROR: invalid hex in validator key: {e}");
            std::process::exit(1);
        });
        if key_bytes.len() < 32 {
            eprintln!("ERROR: validator key must be at least 32 bytes (64 hex chars)");
            std::process::exit(1);
        }

        let seed: [u8; 32] = key_bytes[..32].try_into().unwrap();
        let my_kp = augecoin_crypto::signature::HybridKeyPair::from_seed(seed);
        let verifying = my_kp.verifying_key();

        let vk_bytes = verifying.to_bytes();
        let vi = augecoin_consensus::validator::ValidatorInfo::new_active(validator_id, vk_bytes);
        let vs = augecoin_consensus::validator::ValidatorSet::new(verifying, vec![vi]);

        let admin_kp = augecoin_crypto::signature::HybridKeyPair::from_seed(seed);

        (vs, my_kp, admin_kp)
    };

    let validator_set = match storage.get_validator_set_bytes() {
        Ok(Some(bytes)) => {
            println!("[init] loading existing validator set from storage");
            ValidatorSet::from_bytes(&bytes).unwrap_or_else(|e| {
                eprintln!("[init] failed to deserialize stored validator set: {e}, using genesis");
                initial_validator_set.clone()
            })
        }
        Ok(None) => {
            println!("[init] storing genesis validator set");
            let data = initial_validator_set.to_bytes();
            storage
                .put_validator_set_bytes(&data)
                .expect("failed to store validator set");
            initial_validator_set.clone()
        }
        Err(e) => {
            eprintln!("[init] storage error: {e}, using genesis");
            initial_validator_set.clone()
        }
    };

    let height = storage.get_height().unwrap_or(0);
    println!("[init] current block height: {height}");
    println!(
        "[init] validators: {}",
        validator_set.active_validators().len()
    );
    for v in validator_set.active_validators() {
        println!(
            "[init]   validator #{}: ed25519={}...",
            v.id,
            hex::encode(&v.ed25519_public_key[..8])
        );
    }

    // `max_operations` is now a memory/DoS safety ceiling only — the block
    // builder decides how many operations actually go into a block by
    // serialized byte size (see augecoin_core::limits). It is deliberately
    // generous; transmission safety is guaranteed by the byte-aware builder.
    let mempool_max_operations: usize = std::env::var("AUGECOIN_MEMPOOL_MAX_OPERATIONS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(20_000);
    let mempool_ttl_seconds: u64 = std::env::var("AUGECOIN_MEMPOOL_TTL_SECONDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(3_600);

    let mempool = Arc::new(Mutex::new(Mempool::new(MempoolConfig {
        min_fee: MIN_FEE_AUGESAT,
        max_operations: mempool_max_operations,
        chain_id,
        ttl_seconds: mempool_ttl_seconds,
    })));
    {
        let admin_pk = validator_set.admin_public_key().to_bytes();
        mempool
            .lock()
            .expect("mempool lock")
            .set_admin_pubkey(admin_pk);
    }

    let node_status = Arc::new(augecoin_rpc::endpoints::NodeStatus {
        block_height: AtomicU64::new(height),
        latest_block_hash: std::sync::Mutex::new(load_latest_block_hash(&storage, height)),
        peers_connected: AtomicU32::new(0),
        peers_gossipsub_consensus: AtomicU32::new(0),
        peers_kademlia_total: AtomicU32::new(0),
        syncing: AtomicU64::new(0),
        sync_target_height: AtomicU64::new(0),
        mempool_size: AtomicU64::new(0),
        current_round: AtomicU64::new(0),
        current_view: AtomicU64::new(0),
        validator_id: AtomicU64::new(validator_id),
        chain_id: AtomicU64::new(chain_id),
        uptime_seconds: AtomicU64::new(0),
        last_consensus_error: std::sync::Mutex::new(None),
    });

    // Gossip channel for operations admitted via RPC. The sender is handed to
    // the RPC layer (so sendOperation/faucet/admin ops reach every validator)
    // and the receiver is drained by the network thread.
    let (op_broadcast_tx, op_broadcast_rx) = crossbeam_channel::unbounded::<Vec<u8>>();

    start_rpc_server(
        storage.clone(),
        mempool.clone(),
        node_status.clone(),
        rpc_port,
        Some(_admin_kp.clone()),
        faucet_enabled,
        op_broadcast_tx,
    );
    start_wallet_server(wallet_port);

    let node_metrics = NodeMetrics::new().expect("failed to create metrics");
    let metrics_registry = node_metrics.registry.clone();
    node_metrics.block_time_seconds.set(block_time as i64);
    node_metrics.block_height.set(height as i64);

    let _metrics_server = MetricsServer::start(metrics_registry, metrics_port)
        .expect("failed to start metric server");
    println!("[metrics] listening on 0.0.0.0:{metrics_port}");

    let alert_manager = Arc::new(AlertManager::new(1000));

    let validator_seed: [u8; 32] = my_keypair.signing_key_bytes();
    let libp2p_identity = identity_from_ed25519_seed(&validator_seed);
    let swarm = augecoin_network::build_validator_swarm(libp2p_identity, p2p_port)
        .expect("failed to build validator swarm");
    let bootnodes = Bootnodes::new(bootnode_addrs);
    let network_handle = start_consensus_network(
        swarm,
        p2p_port,
        bootnodes,
        op_broadcast_rx,
        external_address,
    );

    println!("[network] LibP2P consensus transport initialized on port {p2p_port}");

    let mut engine = ConsensusEngine::new(
        validator_id,
        validator_count,
        my_keypair.clone(),
        validator_set.clone(),
        storage.clone(),
        mempool.clone(),
        block_time,
    );
    engine.attach_network(Arc::new(network_handle));

    std::thread::sleep(Duration::from_secs(2));

    println!("[consensus] waiting for peer discovery...");
    let mut peers_up = false;
    for attempt in 0..30 {
        let connected = engine.peer_count();
        if connected > 0 {
            peers_up = true;
            println!(
                "[network] {} peers discovered after {}s",
                connected, attempt
            );
            break;
        }
        std::thread::sleep(Duration::from_secs(1));
    }
    if !peers_up {
        println!("[network] no peers discovered yet, continuing...");
    }

    println!();
    println!("[consensus] starting multi-validator consensus ({validator_count} validators)");
    println!();

    let mut height = height;
    let mut prev_safe_box_hash = load_safe_box_hash(&storage);
    let mut latest_block_hash = load_latest_block_hash(&storage, height);
    let mut pending_proposal: Option<OperationBlock> = None;
    let mut collected_sigs: Vec<augecoin_crypto::signature::HybridSignature> = Vec::new();
    let mut prior_proposals: Vec<OperationBlock> = Vec::new();
    // Light-proposal reconstruction state: height -> (proposal, resolved ops).
    // `None` slots are transactions still missing from the local mempool.
    let mut pending_light: HashMap<u64, (ProposalMessage, Vec<Option<Operation>>)> = HashMap::new();
    let mut light_fetch_deadline: HashMap<u64, u64> = HashMap::new();
    let mut light_fetch_attempts: HashMap<u64, u32> = HashMap::new();
    let mut light_fetch_start: HashMap<u64, std::time::Instant> = HashMap::new();
    let mut phase = NodePhase::Idle;
    let mut round_number: u64 = 0;
    let mut round_start_time = now_secs();
    let mut last_block_time = round_start_time;
    let total_round_timeout = block_time + 5;

    let mut round_change_votes: HashMap<u64, HashSet<u64>> = HashMap::new();
    let mut my_round_changes: HashSet<u64> = HashSet::new();

    let mut last_propose_time = round_start_time;
    let propose_retry = 1u64;

    let mut catching_up = false;
    let mut sync_target = height;
    let mut last_sync_request = 0u64;
    let mut last_status_request = 0u64;
    let mut peers_heights: HashMap<u64, u64> = HashMap::new();
    let mut gossiped_seen: HashSet<[u8; 64]> = HashSet::new();

    loop {
        let now = now_secs();

        let connected_peers = engine.peer_count();
        let gossipsub_peers = engine.gossipsub_peer_count();
        let kademlia_peers = engine.kademlia_peer_count();
        node_metrics.peers_connected.set(connected_peers as i64);
        node_metrics.peers_gossipsub.set(gossipsub_peers as i64);
        node_metrics.peers_kademlia.set(kademlia_peers as i64);

        // Admit gossiped operations (received from peers) into the local
        // mempool. Dedup via hash so a node that already admitted the op
        // locally (through its own RPC) never re-admits it.
        while let Some(op_bytes) = engine.try_recv_operation() {
            let operations = if op_bytes.first() == Some(&0xff) {
                match decode_operation_batch(&op_bytes) {
                    Ok(operations) => operations,
                    Err(e) => {
                        eprintln!("[mempool] ignoring invalid operation batch: {e}");
                        continue;
                    }
                }
            } else {
                vec![op_bytes]
            };
            let mut mp = match mempool.lock() {
                Ok(g) => g,
                Err(_) => continue,
            };
            for op_bytes in operations {
                match Operation::from_bytes(&op_bytes) {
                    Ok(op) => {
                        let op_hash = augecoin_core::hash::blake3_512(&op_bytes);
                        if !gossiped_seen.insert(op_hash) {
                            continue;
                        }
                        if mp.validate_and_admit(op, storage.as_ref(), now).is_err() {
                            // duplicate / already committed / invalid: ignore.
                            gossiped_seen.remove(&op_hash);
                        }
                    }
                    Err(e) => {
                        eprintln!("[mempool] ignoring invalid gossiped operation: {e}");
                    }
                }
            }
        }

        if now >= last_status_request + 5 {
            engine.broadcast_to_others(&ConsensusEvent::StatusRequest {
                requester_id: validator_id,
            });
            last_status_request = now;
        }

        while let Ok(event) = engine.event_rx.try_recv() {
            match event {
                ConsensusEvent::BlockRequest {
                    height: req_h,
                    requester_id,
                } => {
                    if let Ok(Some(block)) = storage.get_block(req_h) {
                        let resp = ConsensusEvent::BlockResponse {
                            height: req_h,
                            block_bytes: block.to_bytes(),
                            quorum_sigs: block.quorum_signatures.clone(),
                            from_id: validator_id,
                        };
                        engine.send_to(requester_id, &resp);
                    }
                }
                ConsensusEvent::BlockResponse {
                    height: resp_h,
                    block_bytes,
                    quorum_sigs,
                    ..
                } => {
                    if catching_up && resp_h == height + 1 {
                        match apply_catchup_block(
                            &block_bytes,
                            &quorum_sigs,
                            &engine.validator_set,
                            &storage,
                        ) {
                            Ok(new_sb_hash) => {
                                height = resp_h;
                                prev_safe_box_hash = new_sb_hash;
                                latest_block_hash = OperationBlock::from_bytes(&block_bytes)
                                    .map(|b| b.hash())
                                    .unwrap_or([0u8; 64]);
                                node_metrics.block_height.set(height as i64);
                                node_metrics.catchup_blocks_total.inc();
                                node_metrics
                                    .sync_lag
                                    .set(sync_target.saturating_sub(height) as i64);
                                let mpsize = mempool.lock().map(|m| m.len()).unwrap_or(0);
                                update_node_status(
                                    &node_status,
                                    height,
                                    latest_block_hash,
                                    mpsize,
                                    0,
                                    0,
                                    connected_peers,
                                    gossipsub_peers,
                                    kademlia_peers,
                                    true,
                                    sync_target,
                                    now.saturating_sub(startup_time),
                                    None,
                                );
                                println!(
                                    "[sync] applied block {resp_h} ({} behind)",
                                    sync_target.saturating_sub(height)
                                );
                                sync_validator_set_from_storage(&mut engine, &storage);
                            }
                            Err(e) => {
                                eprintln!("[sync] REJECTED block {resp_h}: {e}");
                                catching_up = false;
                            }
                        }
                    }
                }
                ConsensusEvent::RoundChange {
                    height: rc_height,
                    new_round,
                    from_id,
                    signature,
                } => {
                    if rc_height != height + 1 {
                        continue;
                    }
                    if !engine.verify_round_change(rc_height, new_round, from_id, &signature) {
                        eprintln!(
                            "[consensus] invalid round-change signature from v{from_id}, ignoring"
                        );
                        continue;
                    }
                    round_change_votes
                        .entry(new_round)
                        .or_default()
                        .insert(from_id);
                    let quorum = engine.quorum_threshold() as usize;
                    if round_change_votes
                        .get(&new_round)
                        .map(|s| s.len())
                        .unwrap_or(0)
                        >= quorum
                        && new_round > round_number
                    {
                        println!(
                            "[consensus] VIEW-CHANGE to round {new_round} (quorum {}/{}) voters={:?}",
                            round_change_votes.get(&new_round).unwrap().len(),
                            quorum,
                            round_change_votes.get(&new_round)
                        );
                        round_number = new_round;
                        round_start_time = now;
                        phase = NodePhase::Idle;
                        pending_proposal = None;
                        collected_sigs.clear();
                        prior_proposals.clear();
                    }
                }
                ConsensusEvent::StatusRequest { requester_id } => {
                    let resp = ConsensusEvent::StatusResponse {
                        height,
                        from_id: validator_id,
                    };
                    engine.send_to(requester_id, &resp);
                }
                ConsensusEvent::StatusResponse {
                    height: peer_h,
                    from_id,
                } => {
                    peers_heights.insert(from_id, peer_h);
                    if peer_h > height {
                        catching_up = true;
                        sync_target = sync_target.max(peer_h);
                    }
                }
                ConsensusEvent::BlockProposal {
                    height: ev_height,
                    block_bytes,
                    from_id,
                } => {
                    if ev_height > height + 1 {
                        catching_up = true;
                        sync_target = sync_target.max(ev_height);
                        continue;
                    }
                    if ev_height != height + 1 {
                        continue;
                    }
                    if phase == NodePhase::Idle {
                        match OperationBlock::from_bytes(&block_bytes) {
                            Ok(block) => accept_proposal(
                                &engine,
                                block,
                                ev_height,
                                from_id,
                                validator_id,
                                now,
                                &mut phase,
                                &mut round_number,
                                &mut round_start_time,
                                &mut pending_proposal,
                                &mut collected_sigs,
                                &mut prior_proposals,
                                &alert_manager,
                                &storage,
                            ),
                            Err(e) => {
                                eprintln!("[consensus] failed to deserialize block proposal: {e}");
                            }
                        }
                    }
                }
                ConsensusEvent::LightProposal {
                    height: ev_height,
                    proposal,
                    from_id,
                } => {
                    if ev_height > height + 1 {
                        catching_up = true;
                        sync_target = sync_target.max(ev_height);
                        continue;
                    }
                    if ev_height != height + 1 {
                        continue;
                    }
                    if phase != NodePhase::Idle {
                        continue;
                    }
                    // PART 13: reject proposals with an invalid leader signature.
                    if !engine.verify_proposal_signature(&proposal) {
                        eprintln!(
                            "[consensus] invalid light-proposal signature from v{from_id} at height {ev_height}"
                        );
                        continue;
                    }
                    // Resolve as many transactions as possible from the local
                    // mempool via O(1) hash lookup. Missing ones are fetched
                    // on demand from the proposer.
                    let (resolved, missing): (Vec<Option<Operation>>, Vec<usize>) = {
                        let mp = match mempool.lock() {
                            Ok(g) => g,
                            Err(_) => continue,
                        };
                        let resolved: Vec<Option<Operation>> = proposal
                            .tx_hashes
                            .iter()
                            .map(|h| mp.get_by_hash(h).map(|a| (*a).clone()))
                            .collect();
                        let missing = resolved
                            .iter()
                            .enumerate()
                            .filter(|(_, o)| o.is_none())
                            .map(|(i, _)| i)
                            .collect();
                        (resolved, missing)
                    };
                    // tx_reuse_ratio: how much of the proposal we already had.
                    let reuse_ratio = if proposal.tx_hashes.is_empty() {
                        100i64
                    } else {
                        ((proposal.tx_hashes.len() - missing.len()) as f64
                            / proposal.tx_hashes.len() as f64
                            * 100.0)
                            .round() as i64
                    };
                    node_metrics.tx_reuse_ratio.set(reuse_ratio);

                    if missing.is_empty() {
                        let ops: Vec<Operation> =
                            resolved.into_iter().map(Option::unwrap).collect();
                        let recon_start = std::time::Instant::now();
                        let recon = reconstruct_from_proposal(&proposal, ops);
                        node_metrics
                            .proposal_reconstruction_ms
                            .set(recon_start.elapsed().as_millis() as i64);
                        match recon {
                            Ok(block) => accept_proposal(
                                &engine,
                                block,
                                ev_height,
                                from_id,
                                validator_id,
                                now,
                                &mut phase,
                                &mut round_number,
                                &mut round_start_time,
                                &mut pending_proposal,
                                &mut collected_sigs,
                                &mut prior_proposals,
                                &alert_manager,
                                &storage,
                            ),
                            Err(e) => {
                                eprintln!(
                                    "[consensus] failed to reconstruct light proposal from v{from_id} at height {ev_height}: {e}"
                                );
                            }
                        }
                    } else {
                        let hashes: Vec<[u8; 64]> =
                            missing.iter().map(|&i| proposal.tx_hashes[i]).collect();
                        let request = GetTransactions {
                            height: ev_height,
                            hashes,
                            supports_compression: tx_compression,
                        };
                        node_metrics.tx_fetch_count.inc_by(missing.len() as u64);
                        let req = ConsensusEvent::GetTransactions {
                            height: ev_height,
                            request: Box::new(request),
                            requester_id: validator_id,
                        };
                        engine.send_to(from_id, &req);
                        light_fetch_start.insert(ev_height, std::time::Instant::now());
                        pending_light.insert(ev_height, (*proposal, resolved));
                        light_fetch_deadline.insert(ev_height, now + fetch_timeout_secs);
                        light_fetch_attempts.entry(ev_height).or_insert(0);
                        println!(
                            "[consensus] light proposal height={ev_height}: {} txs missing, fetching from v{from_id}",
                            missing.len()
                        );
                    }
                }
                ConsensusEvent::GetTransactions {
                    height: g_height,
                    request,
                    requester_id,
                } => {
                    // Respond with only the transactions we actually hold (O(1)
                    // hash lookups), compressed iff the requester advertises
                    // support. Missing/unknown hashes are simply omitted.
                    let transactions: Vec<Operation> = {
                        let mp = match mempool.lock() {
                            Ok(g) => g,
                            Err(_) => continue,
                        };
                        request
                            .hashes
                            .iter()
                            .filter_map(|h| mp.get_by_hash(h).map(|a| (*a).clone()))
                            .collect()
                    };
                    let response = TransactionsResponse {
                        height: g_height,
                        transactions,
                        compressed: request.supports_compression && tx_compression,
                    };
                    let resp = ConsensusEvent::TransactionsResponse {
                        height: g_height,
                        response: Box::new(response),
                        from_id: validator_id,
                    };
                    engine.send_to(requester_id, &resp);
                }
                ConsensusEvent::TransactionsResponse {
                    height: r_height,
                    response,
                    from_id: _,
                } => {
                    let outcome: Option<(OperationBlock, u64)> = (|| {
                        let (proposal, resolved) = pending_light.get_mut(&r_height)?;
                        // Merge only requested transactions, keyed by hash, into
                        // their exact proposal slot (defends against duplicate /
                        // out-of-order / unsolicited responses).
                        for op in &response.transactions {
                            let h = op_hash(op);
                            if let Some(i) = proposal.tx_hashes.iter().position(|&x| x == h) {
                                if resolved[i].is_none() {
                                    resolved[i] = Some(op.clone());
                                }
                            }
                        }
                        if !resolved.iter().all(|o| o.is_some()) {
                            return None;
                        }
                        let ops: Vec<Operation> =
                            resolved.iter().map(|o| o.clone().unwrap()).collect();
                        match reconstruct_from_proposal(proposal, ops) {
                            Ok(block) => Some((block, proposal.header.leader_id)),
                            Err(e) => {
                                eprintln!(
                                    "[consensus] reconstruction failed after fetch at height {r_height}: {e}"
                                );
                                None
                            }
                        }
                    })();
                    if let Some((block, leader_id)) = outcome {
                        if let Some(t0) = light_fetch_start.remove(&r_height) {
                            node_metrics
                                .tx_fetch_latency_ms
                                .set(t0.elapsed().as_millis() as i64);
                        }
                        pending_light.remove(&r_height);
                        light_fetch_deadline.remove(&r_height);
                        light_fetch_attempts.remove(&r_height);
                        accept_proposal(
                            &engine,
                            block,
                            r_height,
                            leader_id,
                            validator_id,
                            now,
                            &mut phase,
                            &mut round_number,
                            &mut round_start_time,
                            &mut pending_proposal,
                            &mut collected_sigs,
                            &mut prior_proposals,
                            &alert_manager,
                            &storage,
                        );
                    }
                }
                ConsensusEvent::SignVote {
                    height: ev_height,
                    block_hash,
                    signature,
                    voter_id,
                } => {
                    if ev_height != height + 1 {
                        if ev_height > height + 1 {
                            catching_up = true;
                            sync_target = sync_target.max(ev_height);
                        }
                        continue;
                    }
                    if phase == NodePhase::AwaitingSignatures {
                        if let Some(ref block) = pending_proposal {
                            if block.hash() != block_hash {
                                continue;
                            }
                            if engine.verify_block_sig(block, voter_id, &signature) {
                                let already_has =
                                    collected_sigs.iter().any(|s| s.bytes == signature.bytes);
                                if !already_has {
                                    collected_sigs.push(signature.clone());
                                    println!(
                                        "[consensus] collected signature from v{voter_id} ({}/{})",
                                        collected_sigs.len(),
                                        engine.quorum_threshold()
                                    );
                                }
                            } else {
                                eprintln!("[consensus] invalid signature from v{voter_id}");
                            }
                        }
                    }
                }
                ConsensusEvent::CommitNotification {
                    height: ev_height,
                    block_bytes,
                    quorum_sigs,
                    from_id: _,
                } => {
                    if ev_height > height + 1 {
                        println!(
                            "[sync] detected commit at height {ev_height}, local {height}; catching up"
                        );
                        catching_up = true;
                        sync_target = sync_target.max(ev_height);
                        continue;
                    }
                    if ev_height != height + 1 {
                        continue;
                    }
                    if phase == NodePhase::Executed {
                        continue;
                    }
                    match OperationBlock::from_bytes(&block_bytes) {
                        Ok(block) => {
                            if block.header.block_number != ev_height {
                                continue;
                            }
                            let mut full_block = block.clone();
                            full_block.quorum_signatures = quorum_sigs.clone();

                            if let Err(e) =
                                execution::verify_block_quorum(&full_block, &engine.validator_set)
                            {
                                eprintln!(
                                    "[block {ev_height}] REJECTED: quorum verification failed: {e}"
                                );
                                phase = NodePhase::Idle;
                                continue;
                            }

                            match execution::execute_block(&full_block, &storage) {
                                Ok(new_safe_box_hash) => {
                                    prev_safe_box_hash = new_safe_box_hash;
                                    height = ev_height;
                                    node_metrics.block_height.set(height as i64);
                                    node_metrics.blocks_total.inc();
                                    node_metrics
                                        .transactions_total
                                        .inc_by(full_block.operations.len() as u64);
                                    node_metrics.transactions_per_second.set(
                                        (full_block.operations.len() as u64
                                            / now.saturating_sub(last_block_time).max(1))
                                            as i64,
                                    );
                                    update_safebox_metrics(&storage, &node_metrics, false);

                                    {
                                        let mut mp = match mempool.lock() {
                                            Ok(g) => g,
                                            Err(_) => continue,
                                        };
                                        mp.remove_committed(&full_block.operations);
                                    }

                                    sync_validator_set_from_storage(&mut engine, &storage);

                                    latest_block_hash = full_block.hash();
                                    let mpsize = mempool.lock().map(|m| m.len()).unwrap_or(0);
                                    node_metrics.mempool_size.set(mpsize as i64);
                                    update_node_status(
                                        &node_status,
                                        height,
                                        latest_block_hash,
                                        mpsize,
                                        0,
                                        0,
                                        connected_peers,
                                        gossipsub_peers,
                                        kademlia_peers,
                                        false,
                                        height,
                                        now.saturating_sub(startup_time),
                                        None,
                                    );

                                    println!(
                                        "[block {ev_height}] EXECUTED  reward={:.8} AUGE  ops={}  quorum={}/{}",
                                        block.header.reward as f64 / 100_000_000.0,
                                        block.operations.len(),
                                        quorum_sigs.len(),
                                        engine.validator_set.active_validators().len()
                                    );

                                    phase = NodePhase::Executed;
                                    last_block_time = now;
                                    round_start_time = now;
                                    pending_proposal = None;
                                    collected_sigs.clear();
                                    prior_proposals.clear();
                                    round_number = 0;
                                    round_change_votes.clear();
                                    my_round_changes.clear();
                                }
                                Err(e) => {
                                    eprintln!("[block {ev_height}] EXECUTION FAILED: {e}");
                                    if let Ok(mut guard) = node_status.last_consensus_error.lock() {
                                        *guard = Some(format!("execution failed: {e}"));
                                    }
                                    phase = NodePhase::Idle;
                                    pending_proposal = None;
                                    collected_sigs.clear();
                                    prior_proposals.clear();
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("[consensus] failed to deserialize committed block: {e}");
                        }
                    }
                }
            }
        }

        // PART 13: on-demand fetch timeout + retry for light-proposal
        // reconstruction. A validator that never receives the missing txs
        // retries the fetch up to `fetch_max_retries`, then drops the proposal
        // (the round-change path recovers liveness without breaking determinism).
        if !pending_light.is_empty() {
            let heights: Vec<u64> = pending_light.keys().copied().collect();
            for h in heights {
                let deadline = *light_fetch_deadline.get(&h).unwrap_or(&u64::MAX);
                if now < deadline {
                    continue;
                }
                let attempts = light_fetch_attempts.get(&h).copied().unwrap_or(0);
                if attempts >= fetch_max_retries {
                    eprintln!(
                        "[consensus] fetch timeout for height {h} after {attempts} retries; dropping proposal"
                    );
                    pending_light.remove(&h);
                    light_fetch_deadline.remove(&h);
                    light_fetch_attempts.remove(&h);
                    light_fetch_start.remove(&h);
                    continue;
                }
                if let Some((proposal, resolved)) = pending_light.get(&h) {
                    let missing: Vec<usize> = resolved
                        .iter()
                        .enumerate()
                        .filter(|(_, o)| o.is_none())
                        .map(|(i, _)| i)
                        .collect();
                    let hashes: Vec<[u8; 64]> =
                        missing.iter().map(|&i| proposal.tx_hashes[i]).collect();
                    let request = GetTransactions {
                        height: h,
                        hashes,
                        supports_compression: tx_compression,
                    };
                    let req = ConsensusEvent::GetTransactions {
                        height: h,
                        request: Box::new(request),
                        requester_id: validator_id,
                    };
                    engine.send_to(proposal.header.leader_id, &req);
                    node_metrics.tx_fetch_count.inc_by(missing.len() as u64);
                    light_fetch_deadline.insert(h, now + fetch_timeout_secs);
                    light_fetch_attempts.insert(h, attempts + 1);
                    light_fetch_start.insert(h, std::time::Instant::now());
                    eprintln!(
                        "[consensus] fetch retry {} for height {h} ({} txs missing)",
                        attempts + 1,
                        missing.len()
                    );
                }
            }
        }

        if catching_up {
            if let Some(&tip) = peers_heights.values().max() {
                sync_target = sync_target.max(tip);
            }
            let leader_now = leader_for(height, round_number, &engine.validator_set);
            // Exit catch-up when fully caught up, or when we are exactly one
            // block behind *and* we are the leader for the next block (in which
            // case normal consensus will propose it instead of requesting a
            // block that does not exist yet).
            if height >= sync_target {
                catching_up = false;
                println!("[sync] catch-up complete at height {height}");
                sync_validator_set_from_storage(&mut engine, &storage);
            } else if height + 1 == sync_target && leader_now == validator_id {
                catching_up = false;
                println!("[sync] catch-up complete at height {height} (next leader is us)");
                sync_validator_set_from_storage(&mut engine, &storage);
            } else if now >= last_sync_request + 2 {
                let req = ConsensusEvent::BlockRequest {
                    height: height + 1,
                    requester_id: validator_id,
                };
                engine.broadcast_to_others(&req);
                last_sync_request = now;
                println!(
                    "[sync] requesting block {} (target {})",
                    height + 1,
                    sync_target
                );
            }

            let mpsize = mempool.lock().map(|m| m.len()).unwrap_or(0);
            node_metrics.mempool_size.set(mpsize as i64);
            update_node_status(
                &node_status,
                height,
                latest_block_hash,
                mpsize,
                0,
                0,
                connected_peers,
                gossipsub_peers,
                kademlia_peers,
                true,
                sync_target,
                now.saturating_sub(startup_time),
                None,
            );

            std::thread::sleep(Duration::from_millis(200));
            continue;
        }

        let uptime = now.saturating_sub(startup_time);
        let mpsize = mempool.lock().map(|m| m.len()).unwrap_or(0);
        node_metrics.mempool_size.set(mpsize as i64);
        update_node_status(
            &node_status,
            height,
            latest_block_hash,
            mpsize,
            round_number,
            round_number,
            connected_peers,
            gossipsub_peers,
            kademlia_peers,
            false,
            height,
            uptime,
            None,
        );

        let elapsed_since_last = now.saturating_sub(last_block_time);
        if elapsed_since_last < block_time && phase == NodePhase::Executed {
            let remaining = block_time - elapsed_since_last;
            std::thread::sleep(Duration::from_secs(remaining.min(1)));
            continue;
        }

        if phase == NodePhase::Executed {
            phase = NodePhase::Idle;
        }

        // A validator may have received a commit from another leader while
        // its previous proposal was still pending. Drop those stale nonces
        // before building or voting on the next block. Also expire operations
        // that have lingered past the mempool TTL (second layer of defense:
        // nothing may accumulate indefinitely).
        if let Ok(mut mp) = mempool.lock() {
            mp.remove_stale(storage.as_ref());
            let expired = mp.evict_expired(now);
            if expired > 0 {
                eprintln!("[mempool] evicted {expired} expired operations");
            }
        }

        let leader_id = leader_for(height, round_number, &engine.validator_set);

        let voted_away = my_round_changes.iter().any(|r| *r > round_number);

        if phase == NodePhase::Idle && leader_id == validator_id && !voted_away {
            let prev_hash = if height > 0 {
                storage
                    .get_block(height)
                    .ok()
                    .flatten()
                    .map(|b| b.hash())
                    .unwrap_or([0u8; 64])
            } else {
                [0u8; 64]
            };

            match engine.build_block(height + 1, prev_hash, prev_safe_box_hash) {
                Ok(block) => {
                    println!(
                        "[consensus] proposing block height={} (leader=v{validator_id}, round {round_number})",
                        height + 1
                    );

                    let build_start = std::time::Instant::now();
                    let sig = engine.sign_block(&block);
                    collected_sigs = vec![sig];

                    let proposal_event = if heavy_proposal {
                        let block_bytes = block.to_bytes();
                        ConsensusEvent::BlockProposal {
                            height: height + 1,
                            block_bytes,
                            from_id: validator_id,
                        }
                    } else {
                        let light = engine.build_light_proposal(&block);
                        node_metrics
                            .proposal_bytes
                            .set(light.serialized_size() as i64);
                        node_metrics
                            .proposal_hash_bytes
                            .set(light.hash_bytes() as i64);
                        ConsensusEvent::LightProposal {
                            height: height + 1,
                            proposal: Box::new(light),
                            from_id: validator_id,
                        }
                    };
                    node_metrics
                        .proposal_build_ms
                        .set(build_start.elapsed().as_millis() as i64);
                    let fill_ratio = ((block.serialized_size() as f64
                        / augecoin_core::limits::max_block_serialized_size() as f64)
                        * 100.0)
                        .round() as i64;
                    node_metrics.proposal_fill_ratio.set(fill_ratio);
                    engine.broadcast_to_others(&proposal_event);

                    pending_proposal = Some(block);
                    phase = NodePhase::AwaitingSignatures;
                    round_start_time = now;
                    last_propose_time = now;
                }
                Err(e) => {
                    eprintln!("[consensus] failed to build block: {e}");
                }
            }
        }

        if phase == NodePhase::AwaitingSignatures {
            let threshold = engine.quorum_threshold();

            if (collected_sigs.len() as u64) < threshold
                && leader_id == validator_id
                && !voted_away
                && now > last_propose_time + propose_retry
            {
                if let Some(ref block) = pending_proposal {
                    let proposal_event = if heavy_proposal {
                        ConsensusEvent::BlockProposal {
                            height: height + 1,
                            block_bytes: block.to_bytes(),
                            from_id: validator_id,
                        }
                    } else {
                        let light = engine.build_light_proposal(block);
                        ConsensusEvent::LightProposal {
                            height: height + 1,
                            proposal: Box::new(light),
                            from_id: validator_id,
                        }
                    };
                    engine.broadcast_to_others(&proposal_event);
                    last_propose_time = now;
                }
            }
            if collected_sigs.len() as u64 >= threshold {
                if let Some(ref block) = pending_proposal {
                    let mut committed_block = block.clone();
                    committed_block.quorum_signatures = collected_sigs.clone();

                    if let Err(e) =
                        execution::verify_block_quorum(&committed_block, &engine.validator_set)
                    {
                        eprintln!(
                            "[block {}] REJECTED: quorum verification failed: {e}",
                            height + 1
                        );
                        phase = NodePhase::Idle;
                        pending_proposal = None;
                        collected_sigs.clear();
                    } else {
                        match execution::execute_block(&committed_block, &storage) {
                            Ok(new_safe_box_hash) => {
                                prev_safe_box_hash = new_safe_box_hash;
                                height += 1;

                                if std::env::var("AUGECOIN_ARCHIVE_MODE").unwrap_or_default() != "1"
                                    && height > augecoin_core::constants::PRUNE_KEEP_BLOCKS
                                {
                                    let cutoff =
                                        height - augecoin_core::constants::PRUNE_KEEP_BLOCKS;
                                    if let Ok(count) = storage.prune_blocks(cutoff) {
                                        if count > 0 {
                                            println!("[block {height}] pruned {count} old blocks");
                                        }
                                    }
                                }

                                node_metrics.block_height.set(height as i64);
                                node_metrics.blocks_total.inc();
                                node_metrics
                                    .transactions_total
                                    .inc_by(committed_block.operations.len() as u64);
                                node_metrics.transactions_per_second.set(
                                    (committed_block.operations.len() as u64
                                        / now.saturating_sub(last_block_time).max(1))
                                        as i64,
                                );
                                update_safebox_metrics(&storage, &node_metrics, false);

                                {
                                    let mut mp = match mempool.lock() {
                                        Ok(g) => g,
                                        Err(_) => continue,
                                    };
                                    mp.remove_committed(&committed_block.operations);
                                }

                                sync_validator_set_from_storage(&mut engine, &storage);

                                latest_block_hash = committed_block.hash();
                                let mpsize = mempool.lock().map(|m| m.len()).unwrap_or(0);
                                node_metrics.mempool_size.set(mpsize as i64);
                                update_node_status(
                                    &node_status,
                                    height,
                                    latest_block_hash,
                                    mpsize,
                                    0,
                                    0,
                                    connected_peers,
                                    gossipsub_peers,
                                    kademlia_peers,
                                    false,
                                    height,
                                    now.saturating_sub(startup_time),
                                    None,
                                );

                                let commit = ConsensusEvent::CommitNotification {
                                    height,
                                    block_bytes: committed_block.to_bytes(),
                                    quorum_sigs: collected_sigs.clone(),
                                    from_id: validator_id,
                                };
                                engine.broadcast_to_others(&commit);

                                println!(
                                    "[block {height}] COMMITTED  reward={:.8} AUGE  ops={}  quorum={}/{}",
                                    block.header.reward as f64 / 100_000_000.0,
                                    block.operations.len(),
                                    collected_sigs.len(),
                                    engine.validator_set.active_validators().len()
                                );

                                // Block saturation telemetry: how close the
                                // committed block is to the gossip transmit
                                // ceiling. Structured so it can be aggregated
                                // and alerted on later.
                                let serialized_bytes = committed_block.serialized_size();
                                let max_transmit = augecoin_core::limits::max_transmit_size();
                                let utilization_pct =
                                    ((serialized_bytes as f64 / max_transmit as f64) * 100.0)
                                        .round() as i64;
                                let residual_mempool = mempool.lock().map(|m| m.len()).unwrap_or(0);
                                node_metrics
                                    .block_serialized_bytes
                                    .set(serialized_bytes as i64);
                                node_metrics.block_utilization_pct.set(utilization_pct);
                                let telemetry = serde_json::json!({
                                    "event": "block_committed",
                                    "height": height,
                                    "ops": block.operations.len(),
                                    "serialized_bytes": serialized_bytes,
                                    "max_transmit_size": max_transmit,
                                    "block_utilization_pct": utilization_pct,
                                    "residual_mempool": residual_mempool,
                                });
                                println!("[telemetry] {telemetry}");

                                phase = NodePhase::Executed;
                                last_block_time = now;
                                round_start_time = now;
                                pending_proposal = None;
                                collected_sigs.clear();
                                prior_proposals.clear();
                                round_number = 0;
                                round_change_votes.clear();
                                my_round_changes.clear();
                            }
                            Err(e) => {
                                eprintln!("[block {}] COMMIT FAILED: {e}", height + 1);
                                if let Ok(mut guard) = node_status.last_consensus_error.lock() {
                                    *guard = Some(format!("execution failed: {e}"));
                                }
                                phase = NodePhase::Idle;
                                pending_proposal = None;
                                collected_sigs.clear();
                                prior_proposals.clear();
                            }
                        }
                    }
                }
            }
        }

        if now > round_start_time + total_round_timeout && phase != NodePhase::Executed {
            let target_round = round_number + 1;
            if !my_round_changes.contains(&target_round) {
                let sig = engine.sign_round_change(height + 1, target_round);
                let rc = ConsensusEvent::RoundChange {
                    height: height + 1,
                    new_round: target_round,
                    from_id: validator_id,
                    signature: sig,
                };
                engine.broadcast_to_others(&rc);
                round_change_votes
                    .entry(target_round)
                    .or_default()
                    .insert(validator_id);
                my_round_changes.insert(target_round);
                eprintln!(
                    "[consensus] round timeout height={} round={round_number}; broadcasting round-change to round {target_round}",
                    height + 1
                );
            }
            round_start_time = now;
            pending_proposal = None;
            collected_sigs.clear();
            phase = NodePhase::Idle;
        }

        std::thread::sleep(Duration::from_millis(200));
    }
}

fn leader_for(height: u64, round_number: u64, validator_set: &ValidatorSet) -> u64 {
    let active = validator_set.active_validators();
    if active.is_empty() {
        return 0;
    }
    let index = ((height + 1 + round_number) as usize) % active.len();
    active[index].id
}

fn decode_operation_batch(data: &[u8]) -> Result<Vec<Vec<u8>>, String> {
    if data.len() < 5 || data[0] != 0xff {
        return Err("invalid operation batch marker".into());
    }
    let mut pos = 1;
    let read_u32 = |data: &[u8], pos: &mut usize| -> Result<u32, String> {
        if *pos + 4 > data.len() {
            return Err("operation batch is truncated".into());
        }
        let value = u32::from_be_bytes(data[*pos..*pos + 4].try_into().unwrap());
        *pos += 4;
        Ok(value)
    };
    let count = read_u32(data, &mut pos)? as usize;
    let mut operations = Vec::with_capacity(count);
    for _ in 0..count {
        let len = read_u32(data, &mut pos)? as usize;
        if pos + len > data.len() {
            return Err("operation batch is truncated".into());
        }
        operations.push(data[pos..pos + len].to_vec());
        pos += len;
    }
    if pos != data.len() {
        return Err("operation batch has trailing bytes".into());
    }
    Ok(operations)
}

/// Accept a fully-reconstructed proposal (from either the light or the legacy
/// heavy path): round-jump, equivocation detection, self-vote and phase
/// transition. Shared by both transports so they can never diverge.
#[allow(clippy::too_many_arguments)]
fn accept_proposal(
    engine: &ConsensusEngine,
    block: OperationBlock,
    ev_height: u64,
    from_id: u64,
    validator_id: u64,
    now: u64,
    phase: &mut NodePhase,
    round_number: &mut u64,
    round_start_time: &mut u64,
    pending_proposal: &mut Option<OperationBlock>,
    collected_sigs: &mut Vec<augecoin_crypto::signature::HybridSignature>,
    prior_proposals: &mut Vec<OperationBlock>,
    alert_manager: &AlertManager,
    storage: &Storage,
) {
    let height = ev_height.saturating_sub(1);
    let cur_leader = leader_for(height, *round_number, &engine.validator_set);
    if block.header.block_number == ev_height && block.header.leader_id != cur_leader {
        let mut jumped = false;
        let mut probe = *round_number + 1;
        while probe <= *round_number + 16 {
            if leader_for(height, probe, &engine.validator_set) == block.header.leader_id {
                println!(
                    "[consensus] round-jump to round {probe} (proposal from v{})",
                    block.header.leader_id
                );
                *round_number = probe;
                *round_start_time = now;
                *pending_proposal = None;
                collected_sigs.clear();
                jumped = true;
                break;
            }
            probe += 1;
        }
        if !jumped {
            eprintln!(
                "[consensus] invalid proposal: leader_id={} expected={}, height={} expected={}",
                block.header.leader_id, cur_leader, block.header.block_number, ev_height
            );
            return;
        }
    }
    let leader_id = leader_for(height, *round_number, &engine.validator_set);
    if block.header.leader_id == leader_id && block.header.block_number == ev_height {
        for prior in prior_proposals.iter() {
            if prior.header.leader_id == block.header.leader_id
                && prior.header.block_number == block.header.block_number
                && prior.hash() != block.hash()
            {
                let proofs = augecoin_consensus::equivocation::detect_equivocation(&[
                    prior.clone(),
                    block.clone(),
                ]);
                if !proofs.is_empty() {
                    eprintln!(
                        "[security] EQUIVOCATION DETECTED: validator #{from_id} at height {ev_height}"
                    );
                    alert_manager.fire_equivocation(
                        from_id,
                        ev_height,
                        "double proposal at same height",
                    );
                    let proof_bytes = serde_json::to_vec(&proofs).unwrap_or_default();
                    storage
                        .put_equivocation_proof(ev_height, from_id, &proof_bytes)
                        .ok();
                }
            }
        }
        prior_proposals.push(block.clone());

        println!(
            "[consensus] received block proposal height={ev_height} from validator #{from_id} (round {round_number})",
            round_number = *round_number
        );
        *pending_proposal = Some(block.clone());

        let sig = engine.sign_block(&block);
        let block_hash = block.hash();
        *collected_sigs = vec![sig.clone()];

        let vote = ConsensusEvent::SignVote {
            height: ev_height,
            block_hash,
            signature: sig,
            voter_id: validator_id,
        };
        engine.broadcast_to_others(&vote);

        *phase = NodePhase::AwaitingSignatures;
        *round_start_time = now;
    } else {
        eprintln!(
            "[consensus] invalid proposal: leader_id={} expected={}, height={} expected={}",
            block.header.leader_id, leader_id, block.header.block_number, ev_height
        );
    }
}

fn apply_catchup_block(
    block_bytes: &[u8],
    quorum_sigs: &[augecoin_crypto::signature::HybridSignature],
    validator_set: &ValidatorSet,
    storage: &Storage,
) -> Result<[u8; 64], String> {
    let block = OperationBlock::from_bytes(block_bytes).map_err(|e| e.to_string())?;

    let mut full_block = block;
    full_block.quorum_signatures = quorum_sigs.to_vec();

    execution::verify_block_quorum(&full_block, validator_set).map_err(|e| e.to_string())?;

    let new_sb_hash = execution::execute_block(&full_block, storage).map_err(|e| e.to_string())?;

    Ok(new_sb_hash)
}
