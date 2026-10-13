//! AUGECOIN stress / load harness (Fase 18).
//!
//! Drives a running node over JSON-RPC with N concurrent clients for a fixed
//! duration, across three scenarios:
//!   - transfers: native AUGE transfers (signed with derived keys)
//!   - token:    AUGE20 mint/burn (contract_execute)
//!   - heavy:    deploy + repeated contract_execute with a large gas limit
//!
//! Prints a report with throughput, p50/p95/p99 latency and rejection-by-reason.
//! Requires a live node (endpoint + funded accounts / admin API key).

#![allow(clippy::too_many_arguments)]

mod metrics;

use std::sync::Arc;
use std::time::Instant;

use anyhow::{anyhow, Context};
use clap::{Parser, ValueEnum};
use serde_json::{json, Value};
use tokio::sync::Mutex;

use augecoin_contracts::TokenCall;
use augecoin_core::constants::{CT_CHAIN_ID_MAINNET, MIN_FEE_AUGESAT};
use augecoin_core::operation::{
    Operation, OperationPayload, OperationType, ReceiverInfo, SenderInfo,
};
use augecoin_crypto::hdkeys::HdWallet;
use augecoin_crypto::signature::HybridKeyPair;

use metrics::Metrics;

#[derive(Parser)]
#[command(
    name = "augecoin-stress",
    about = "AUGECOIN stress / load test harness (Fase 18)"
)]
struct Args {
    /// Node JSON-RPC endpoint.
    #[arg(long, default_value = "https://localhost:9443")]
    endpoint: String,

    /// Node admin API key (required for contract scenarios).
    #[arg(long)]
    api_key: Option<String>,

    /// Mnemonic whose derived keys own the funded accounts (transfers scenario).
    #[arg(long)]
    mnemonic: Option<String>,

    /// Stress scenario.
    #[arg(long, value_enum, default_value_t = Scenario::Transfers)]
    scenario: Scenario,

    /// Number of concurrent clients.
    #[arg(long, default_value_t = 8)]
    clients: u64,

    /// Duration in seconds.
    #[arg(long, default_value_t = 10)]
    duration_secs: u64,

    /// Transfer / token amount (whole units).
    #[arg(long, default_value_t = 1)]
    amount: u64,

    /// Fee in augesat (transfers).
    #[arg(long)]
    fee: Option<u64>,

    /// Token contract id hex (token/heavy scenarios).
    #[arg(long)]
    contract_id: Option<String>,

    /// WASM hex to deploy first (heavy scenario).
    #[arg(long)]
    wasm_hex: Option<String>,

    /// Gas limit (whole units) for heavy executes.
    #[arg(long, default_value_t = 1_000_000)]
    gas_limit: u64,

    /// Accept self-signed TLS certificates.
    #[arg(long)]
    insecure: bool,

    /// Write the report as CSV to this path.
    #[arg(long)]
    csv: Option<String>,
}

#[derive(Copy, Clone, Debug, ValueEnum)]
enum Scenario {
    Transfers,
    Token,
    Heavy,
}

#[derive(Clone)]
struct RpcClient {
    endpoint: String,
    client: reqwest::Client,
}

impl RpcClient {
    fn new(endpoint: String, insecure: bool) -> Self {
        let client = reqwest::Client::builder()
            .danger_accept_invalid_certs(insecure)
            .build()
            .expect("reqwest client");
        RpcClient { endpoint, client }
    }

    async fn call(&self, method: &str, params: Value) -> anyhow::Result<Value> {
        let resp = self
            .client
            .post(&self.endpoint)
            .json(&json!({ "jsonrpc": "2.0", "method": method, "params": params, "id": 1 }))
            .send()
            .await
            .context("rpc request failed")?;
        let body: Value = resp.json().await.context("rpc json decode")?;
        if let Some(err) = body.get("error") {
            return Err(anyhow!("rpc error: {err}"));
        }
        Ok(body.get("result").cloned().unwrap_or(Value::Null))
    }

    async fn send_operation(&self, hex: &str) -> anyhow::Result<(bool, Option<String>)> {
        let v = self.call("sendoperation", json!({ "hex": hex })).await?;
        let accepted = v.get("accepted").and_then(|x| x.as_bool()).unwrap_or(false);
        let error = v
            .get("error")
            .and_then(|x| x.as_str())
            .map(|s| s.to_string());
        Ok((accepted, error))
    }

    async fn contract_execute(
        &self,
        contract_id_hex: &str,
        call_hex: &str,
        api_key: &str,
        gas_limit_hex: Option<String>,
    ) -> anyhow::Result<(bool, Option<String>)> {
        let v = self
            .call(
                "contract_execute",
                json!({
                    "contract_id_hex": contract_id_hex,
                    "call_hex": call_hex,
                    "sender_hex": api_key,
                    "gas_limit_hex": gas_limit_hex,
                }),
            )
            .await?;
        let ok = v.get("success").and_then(|x| x.as_bool()).unwrap_or(false);
        let error = v
            .get("error")
            .and_then(|x| x.as_str())
            .map(|s| s.to_string());
        Ok((ok, error))
    }

    async fn contract_store_code(
        &self,
        wasm_hex: &str,
        api_key: &str,
    ) -> anyhow::Result<(bool, Option<String>, Option<String>)> {
        let v = self
            .call(
                "contract_store_code",
                json!({ "wasm_hex": wasm_hex, "sender_hex": api_key }),
            )
            .await?;
        let accepted = v.get("accepted").and_then(|x| x.as_bool()).unwrap_or(false);
        let code_id = v
            .get("code_id_hex")
            .and_then(|x| x.as_str())
            .map(|s| s.to_string());
        let error = v
            .get("error")
            .and_then(|x| x.as_str())
            .map(|s| s.to_string());
        Ok((accepted, code_id, error))
    }

    async fn get_n_operation(&self, account: u64) -> anyhow::Result<u64> {
        let v = self
            .call("getaccount", json!({ "account_number": account }))
            .await?;
        v.get("n_operation")
            .and_then(|x| x.as_u64())
            .ok_or_else(|| anyhow!("account {account} has no n_operation"))
    }
}

/// Build a signed native Transaction (account -> account), mirroring the CLI.
fn build_transfer(
    from: u64,
    to: u64,
    amount: u64,
    fee: u64,
    n_operation: u64,
    keypair: &HybridKeyPair,
) -> Operation {
    let sender = SenderInfo {
        account: from,
        n_operation,
        amount,
        payload: Vec::new(),
    };
    let mut op = Operation {
        op_type: OperationType::Transaction,
        payload: OperationPayload::Transaction {
            senders: vec![sender],
            receivers: vec![ReceiverInfo {
                account: to,
                amount,
                payload: Vec::new(),
            }],
            changers: Vec::new(),
            fee,
        },
        chain_id: CT_CHAIN_ID_MAINNET,
        signatures: Vec::new(),
    };
    let message = op.to_bytes_stripped();
    let sig = keypair.sign(&message);
    op.signatures = vec![sig];
    op
}

async fn run_transfers(
    rpc: &RpcClient,
    mnemonic: &str,
    clients: u64,
    deadline: Instant,
    fee: u64,
    amount: u64,
    metrics: Arc<Mutex<Metrics>>,
) -> anyhow::Result<()> {
    let wallet = HdWallet::from_mnemonic(mnemonic).map_err(|e| anyhow!(e))?;
    let mut handles = Vec::new();
    for i in 0..clients {
        let rpc = rpc.clone();
        let m = metrics.clone();
        let kp = wallet.derive_keypair(i);
        let from = i;
        let to = (i + 1) % clients;
        handles.push(tokio::spawn(async move {
            let n_op = rpc.get_n_operation(from).await.unwrap_or(0);
            let mut n = n_op;
            loop {
                if Instant::now() >= deadline {
                    break;
                }
                let op = build_transfer(from, to, amount, fee, n, &kp);
                let hex = hex::encode(op.to_bytes());
                let start = Instant::now();
                let (ok, reason) = rpc
                    .send_operation(&hex)
                    .await
                    .unwrap_or((false, Some("rpc error".into())));
                let lat = start.elapsed().as_micros() as u64;
                m.lock().await.record(lat, ok, reason);
                n += 1;
            }
        }));
    }
    for h in handles {
        let _ = h.await;
    }
    Ok(())
}

async fn run_token(
    rpc: &RpcClient,
    api_key: &str,
    contract_id: &str,
    clients: u64,
    deadline: Instant,
    amount: u64,
    metrics: Arc<Mutex<Metrics>>,
) -> anyhow::Result<()> {
    let mut handles = Vec::new();
    for i in 0..clients {
        let rpc = rpc.clone();
        let m = metrics.clone();
        let api = api_key.to_string();
        let cid = contract_id.to_string();
        handles.push(tokio::spawn(async move {
            loop {
                if Instant::now() >= deadline {
                    break;
                }
                let call = if i % 2 == 0 {
                    TokenCall::Mint {
                        to: (i + 1) % clients,
                        amount,
                    }
                } else {
                    TokenCall::Burn { amount }
                };
                let call_hex = hex::encode(call.to_bytes());
                let start = Instant::now();
                let (ok, reason) = rpc
                    .contract_execute(&cid, &call_hex, &api, None)
                    .await
                    .unwrap_or((false, Some("rpc error".into())));
                let lat = start.elapsed().as_micros() as u64;
                m.lock().await.record(lat, ok, reason);
            }
        }));
    }
    for h in handles {
        let _ = h.await;
    }
    Ok(())
}

async fn run_heavy(
    rpc: &RpcClient,
    api_key: &str,
    contract_id: Option<String>,
    wasm_hex: Option<String>,
    clients: u64,
    deadline: Instant,
    gas_limit: u64,
    amount: u64,
    metrics: Arc<Mutex<Metrics>>,
) -> anyhow::Result<()> {
    let gas_limit_hex = Some(hex::encode(gas_limit.to_be_bytes()));
    let mut cid = contract_id;
    if let Some(wasm) = wasm_hex {
        let (accepted, code_id, error) = rpc.contract_store_code(&wasm, api_key).await?;
        if !accepted {
            return Err(anyhow!("deploy failed: {:?}", error));
        }
        let deployed = code_id.ok_or_else(|| anyhow!("deploy returned no contract id"))?;
        println!("deployed contract {deployed}");
        cid = Some(deployed);
    }
    let cid = cid.ok_or_else(|| anyhow!("heavy scenario requires --contract-id or --wasm-hex"))?;
    let mut handles = Vec::new();
    for i in 0..clients {
        let rpc = rpc.clone();
        let m = metrics.clone();
        let api = api_key.to_string();
        let cid = cid.clone();
        let gas_limit_hex = gas_limit_hex.clone();
        handles.push(tokio::spawn(async move {
            loop {
                if Instant::now() >= deadline {
                    break;
                }
                // Heavy repeated execute; for AUGE20 a no-op Transfer to self
                // still exercises host gas accounting when gas_limit is large.
                let call = TokenCall::Transfer { to: i, amount };
                let call_hex = hex::encode(call.to_bytes());
                let start = Instant::now();
                let (ok, reason) = rpc
                    .contract_execute(&cid, &call_hex, &api, gas_limit_hex.clone())
                    .await
                    .unwrap_or((false, Some("rpc error".into())));
                let lat = start.elapsed().as_micros() as u64;
                m.lock().await.record(lat, ok, reason);
            }
        }));
    }
    for h in handles {
        let _ = h.await;
    }
    Ok(())
}

fn report(metrics: &Metrics, elapsed_secs: f64, csv: &Option<String>) -> anyhow::Result<()> {
    let total = metrics.total();
    let throughput = total as f64 / elapsed_secs;
    let p50 = metrics.percentile_us(0.5);
    let p95 = metrics.percentile_us(0.95);
    let p99 = metrics.percentile_us(0.99);

    println!("=== AUGECOIN Stress Report (Fase 18) ===");
    println!("duration_s        : {elapsed_secs:.2}");
    println!("total_ops         : {total}");
    println!("throughput_ops/s   : {throughput:.2}");
    println!("accepted          : {}", metrics.accepted());
    println!("rejected          : {}", metrics.rejected());
    println!("latency_us p50/p95/p99: {p50}/{p95}/{p99}");
    println!("rejection by reason:");
    for (k, v) in metrics.by_reason() {
        println!("  {k:<12}: {v}");
    }

    if let Some(path) = csv {
        let mut lines = vec![
            "metric,value".to_string(),
            format!("duration_s,{elapsed_secs:.2}"),
            format!("total_ops,{total}"),
            format!("throughput_ops_s,{throughput:.2}"),
            format!("accepted,{}", metrics.accepted()),
            format!("rejected,{}", metrics.rejected()),
            format!("latency_us_p50,{p50}"),
            format!("latency_us_p95,{p95}"),
            format!("latency_us_p99,{p99}"),
        ];
        for (k, v) in metrics.by_reason() {
            lines.push(format!("reason_{k},{v}"));
        }
        std::fs::write(path, lines.join("\n")).context("write csv")?;
        println!("csv written to {path}");
    }
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let clients = args.clients.max(1);
    let start = Instant::now();
    let deadline = start + std::time::Duration::from_secs(args.duration_secs);
    let metrics = Arc::new(Mutex::new(Metrics::new()));
    let rpc = RpcClient::new(args.endpoint.clone(), args.insecure);

    match args.scenario {
        Scenario::Transfers => {
            let mnemonic = args
                .mnemonic
                .clone()
                .ok_or_else(|| anyhow!("transfers scenario requires --mnemonic"))?;
            run_transfers(
                &rpc,
                &mnemonic,
                clients,
                deadline,
                args.fee.unwrap_or(MIN_FEE_AUGESAT),
                args.amount,
                metrics.clone(),
            )
            .await?;
        }
        Scenario::Token => {
            let api_key = args
                .api_key
                .clone()
                .ok_or_else(|| anyhow!("token scenario requires --api-key"))?;
            let cid = args
                .contract_id
                .clone()
                .ok_or_else(|| anyhow!("token scenario requires --contract-id"))?;
            run_token(
                &rpc,
                &api_key,
                &cid,
                clients,
                deadline,
                args.amount,
                metrics.clone(),
            )
            .await?;
        }
        Scenario::Heavy => {
            let api_key = args
                .api_key
                .clone()
                .ok_or_else(|| anyhow!("heavy scenario requires --api-key"))?;
            run_heavy(
                &rpc,
                &api_key,
                args.contract_id.clone(),
                args.wasm_hex.clone(),
                clients,
                deadline,
                args.gas_limit,
                args.amount,
                metrics.clone(),
            )
            .await?;
        }
    }

    let elapsed = start.elapsed().as_secs_f64().max(0.001);
    let metrics_guard = metrics.lock().await;
    report(&metrics_guard, elapsed, &args.csv)?;
    Ok(())
}
