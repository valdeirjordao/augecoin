//! Load-generation modes.
//!
//! Each mode drives the official AUGECOIN RPCs concurrently with
//! [`futures::stream::FuturesUnordered`]. A mode receives a set of funded
//! accounts (keypairs), a `BenchClient`, and the number of operations to
//! submit, returning a batch report (submitted/confirmed counts and latency
//! samples).

use crate::client::BenchClient;
use crate::ops::{getaccount_params, giftaccount_params, sellaccount_params, sendoperation_params};
use crate::stats::Throughput;
use crate::{ops, LatencySample};
use anyhow::{anyhow, Context, Result};
use augecoin_crypto::signature::HybridKeyPair;
use futures::stream::{FuturesUnordered, StreamExt};
use serde_json::Value;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// The load profile for a bench run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BenchMode {
    Transfer,
    Marketplace,
    Gift,
    Rename,
    Mixed,
}

impl BenchMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Transfer => "transfer",
            Self::Marketplace => "marketplace",
            Self::Gift => "gift",
            Self::Rename => "rename",
            Self::Mixed => "mixed",
        }
    }
}

impl std::str::FromStr for BenchMode {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_ascii_lowercase().as_str() {
            "transfer" => Ok(Self::Transfer),
            "marketplace" => Ok(Self::Marketplace),
            "gift" => Ok(Self::Gift),
            "rename" => Ok(Self::Rename),
            "mixed" => Ok(Self::Mixed),
            other => Err(anyhow!(
                "unknown mode '{other}'; expected transfer|marketplace|gift|rename|mixed"
            )),
        }
    }
}

/// A funded account used by the bench.
#[derive(Clone)]
pub struct BenchAccount {
    pub key: HybridKeyPair,
    pub account: u64,
}

/// Result of a single load burst.
#[derive(Debug, Clone, serde::Serialize)]
pub struct BurstReport {
    pub mode: String,
    pub requested: usize,
    pub submitted: usize,
    pub confirmed: usize,
    pub submitted_tps: f64,
    pub confirmed_tps: f64,
    pub elapsed_secs: f64,
    pub p50_ms: u64,
    pub p95_ms: u64,
    pub p99_ms: u64,
}

/// Run a transfer-only burst.
///
/// Each iteration sends from a rotating sender account to another account.
/// The `n_operation` nonces are tracked per sender and updated as operations
/// are admitted (mirroring the orchestrator's client-side nonce chaining).
pub async fn run_transfer(
    client: &BenchClient,
    accounts: &[BenchAccount],
    n_ops: usize,
    confirmed_timeout: Duration,
) -> Result<BurstReport> {
    let start = Instant::now();

    // Track nonces per account.
    let mut nonces: Vec<u64> = Vec::with_capacity(accounts.len());
    for acc in accounts {
        let v = client
            .call("getaccount", getaccount_params(acc.account))
            .await
            .ok()
            .and_then(|r| ops::parse_n_operation(&r).ok())
            .unwrap_or(0);
        nonces.push(v);
    }

    // Fan out with FuturesUnordered, one stream per account.
    //
    // The AUGECOIN mempool enforces strict per-account n_operation ordering,
    // so operations of the *same* account must be admitted in nonce order.
    // We therefore serialize each account's submissions (guaranteeing all are
    // accepted) while running the accounts' streams concurrently to keep the
    // HTTP layer parallel. Returns (account_idx, submitted_at_ms, accepted).
    if accounts.len() < 2 {
        return Err(anyhow!("transfer mode requires at least 2 funded accounts"));
    }
    let mut futs = FuturesUnordered::new();
    for acc_idx in 0..accounts.len() {
        let acc = &accounts[acc_idx];
        let client = client.clone();
        let base_nonce = nonces[acc_idx];
        let acc_num = acc.account;
        let recv_num = accounts[(acc_idx + 1) % accounts.len()].account;
        let key = acc.key.clone();
        // Pin this account's operations to a single validator. The mempool
        // validates n_operation ordering *per node*, so scattering one
        // account's nonces across validators (round-robin) would leave gaps
        // and get most operations rejected as InvalidNOperation.
        let endpoint = client
            .config
            .endpoints
            .get(acc_idx % client.config.endpoints.len().max(1))
            .cloned()
            .unwrap_or_default();
        // This account handles ops at indices acc_idx, acc_idx+n, acc_idx+2n, ...
        let mut nonce = base_nonce;
        let mut k = acc_idx;
        futs.push(async move {
            let mut accepted = 0u64;
            while k < n_ops {
                let op = ops::make_transfer(&key, acc_num, nonce, recv_num, 1);
                let params = sendoperation_params(&op);
                let ok = client
                    .call_on(&endpoint, "sendoperation", params.clone())
                    .await
                    .map(|v| ops::parse_accepted(&v).unwrap_or(false))
                    .unwrap_or(false);
                if ok {
                    accepted += 1;
                    nonce += 1;
                } else {
                    // A transient rejection (e.g. balance/order) — retry same nonce
                    // once after a short pause to avoid permanent stalls.
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    let retry = client
                        .call_on(&endpoint, "sendoperation", params)
                        .await
                        .map(|v| ops::parse_accepted(&v).unwrap_or(false))
                        .unwrap_or(false);
                    if retry {
                        accepted += 1;
                        nonce += 1;
                    }
                }
                k += accounts.len();
            }
            (acc_idx, base_nonce, accepted)
        });
    }

    // Collect per-account results.
    let mut submitted = 0usize;
    let mut accepted_target = nonces.clone();
    let mut pending: Vec<LatencySample> = Vec::new();
    while let Some((acc_idx, base_nonce, accepted)) = futs.next().await {
        submitted += accepted as usize;
        accepted_target[acc_idx] = base_nonce + accepted;
        for _ in 0..accepted {
            pending.push(LatencySample {
                submitted_at_ms: now_ms(),
                confirmed_at_ms: 0,
                op_index: pending.len(),
                mode: "transfer",
            });
        }
    }

    // Confirmation pass: poll until all submitted nonces advance on-chain.
    let confirmed = wait_confirmations(
        client,
        accounts,
        &accepted_target,
        &mut pending,
        confirmed_timeout,
    )
    .await?;

    let elapsed = start.elapsed();
    let summary = crate::stats::LatencySummary::from_samples(&pending);
    let tp = Throughput::compute(submitted as u64, confirmed as u64, elapsed);
    Ok(BurstReport {
        mode: "transfer".into(),
        requested: n_ops,
        submitted,
        confirmed,
        submitted_tps: tp.submitted_tps,
        confirmed_tps: tp.confirmed_tps,
        elapsed_secs: tp.elapsed_secs,
        p50_ms: summary.p50_ms,
        p95_ms: summary.p95_ms,
        p99_ms: summary.p99_ms,
    })
}

/// Run a marketplace burst (alternating sell/buy using the official RPCs).
pub async fn run_marketplace(
    client: &BenchClient,
    accounts: &[BenchAccount],
    n_ops: usize,
    confirmed_timeout: Duration,
) -> Result<BurstReport> {
    if accounts.len() < 2 {
        return Err(anyhow!(
            "marketplace mode requires at least 2 funded accounts"
        ));
    }
    let start = Instant::now();
    let mut nonces: Vec<u64> = Vec::with_capacity(accounts.len());
    for acc in accounts {
        let v = client
            .call("getaccount", getaccount_params(acc.account))
            .await
            .ok()
            .and_then(|r| ops::parse_n_operation(&r).ok())
            .unwrap_or(0);
        nonces.push(v);
    }

    // One operation per account: each account lists itself for sale exactly
    // once (a buy depends on a committed sell, so a sell-only burst keeps
    // every nonce independent and runs fully concurrently).
    let limit = n_ops.min(accounts.len());
    let sale_price = 10u64;
    let mut submitted = 0usize;
    let mut pending: Vec<LatencySample> = Vec::new();
    let mut accepted_target = nonces.clone();
    let mut futs = FuturesUnordered::new();
    for acc_idx in 0..limit {
        let acc = &accounts[acc_idx];
        let client = client.clone();
        let nonce = nonces[acc_idx];
        let next_acc = accounts[(acc_idx + 1) % accounts.len()].account;
        let params = sellaccount_params(&acc.key, acc.account, nonce, sale_price, next_acc);
        futs.push(async move {
            let res = client.call("sellaccount", params).await;
            (res, acc_idx)
        });
    }

    while let Some((res, acc_idx)) = futs.next().await {
        if let Ok(value) = res {
            if ops::parse_accepted(&value).unwrap_or(false) {
                accepted_target[acc_idx] += 1;
                submitted += 1;
                pending.push(LatencySample {
                    submitted_at_ms: now_ms(),
                    confirmed_at_ms: 0,
                    op_index: pending.len(),
                    mode: "marketplace",
                });
            }
        }
    }

    let confirmed = wait_confirmations(
        client,
        accounts,
        &accepted_target,
        &mut pending,
        confirmed_timeout,
    )
    .await?;
    let elapsed = start.elapsed();
    let summary = crate::stats::LatencySummary::from_samples(&pending);
    let tp = Throughput::compute(submitted as u64, confirmed as u64, elapsed);
    Ok(BurstReport {
        mode: "marketplace".into(),
        requested: n_ops,
        submitted,
        confirmed,
        submitted_tps: tp.submitted_tps,
        confirmed_tps: tp.confirmed_tps,
        elapsed_secs: tp.elapsed_secs,
        p50_ms: summary.p50_ms,
        p95_ms: summary.p95_ms,
        p99_ms: summary.p99_ms,
    })
}

/// Run a gift burst (gift accounts among the funded set).
pub async fn run_gift(
    client: &BenchClient,
    accounts: &[BenchAccount],
    n_ops: usize,
    confirmed_timeout: Duration,
) -> Result<BurstReport> {
    if accounts.is_empty() {
        return Err(anyhow!("gift mode requires at least 1 funded account"));
    }
    let start = Instant::now();
    let mut nonces: Vec<u64> = Vec::with_capacity(accounts.len());
    for acc in accounts {
        let v = client
            .call("getaccount", getaccount_params(acc.account))
            .await
            .ok()
            .and_then(|r| ops::parse_n_operation(&r).ok())
            .unwrap_or(0);
        nonces.push(v);
    }

    // One operation per account: each account gifts its AUGEID exactly once
    // (gifting locks the account), so every nonce is independent and the
    // burst runs fully concurrently.
    let limit = n_ops.min(accounts.len());
    let mut submitted = 0usize;
    let mut pending: Vec<LatencySample> = Vec::new();
    let mut accepted_target = nonces.clone();
    let mut futs = FuturesUnordered::new();
    for acc_idx in 0..limit {
        let acc = &accounts[acc_idx];
        let client = client.clone();
        let nonce = nonces[acc_idx];
        let recv_pk = accounts[(acc_idx + 1) % accounts.len()]
            .key
            .verifying_key()
            .to_bytes();
        let params = giftaccount_params(&acc.key, acc.account, nonce, recv_pk);
        futs.push(async move {
            let res = client.call("giftaccount", params).await;
            (res, acc_idx)
        });
    }
    while let Some((res, acc_idx)) = futs.next().await {
        if let Ok(value) = res {
            if ops::parse_accepted(&value).unwrap_or(false) {
                accepted_target[acc_idx] += 1;
                submitted += 1;
                pending.push(LatencySample {
                    submitted_at_ms: now_ms(),
                    confirmed_at_ms: 0,
                    op_index: pending.len(),
                    mode: "gift",
                });
            }
        }
    }
    let confirmed = wait_confirmations(
        client,
        accounts,
        &accepted_target,
        &mut pending,
        confirmed_timeout,
    )
    .await?;
    let elapsed = start.elapsed();
    let summary = crate::stats::LatencySummary::from_samples(&pending);
    let tp = Throughput::compute(submitted as u64, confirmed as u64, elapsed);
    Ok(BurstReport {
        mode: "gift".into(),
        requested: n_ops,
        submitted,
        confirmed,
        submitted_tps: tp.submitted_tps,
        confirmed_tps: tp.confirmed_tps,
        elapsed_secs: tp.elapsed_secs,
        p50_ms: summary.p50_ms,
        p95_ms: summary.p95_ms,
        p99_ms: summary.p99_ms,
    })
}

/// Run a rename burst (ChangeAccountInfo via sendoperation).
pub async fn run_rename(
    client: &BenchClient,
    accounts: &[BenchAccount],
    n_ops: usize,
    confirmed_timeout: Duration,
) -> Result<BurstReport> {
    if accounts.is_empty() {
        return Err(anyhow!("rename mode requires at least 1 funded account"));
    }
    let start = Instant::now();
    let mut nonces: Vec<u64> = Vec::with_capacity(accounts.len());
    for acc in accounts {
        let v = client
            .call("getaccount", getaccount_params(acc.account))
            .await
            .ok()
            .and_then(|r| ops::parse_n_operation(&r).ok())
            .unwrap_or(0);
        nonces.push(v);
    }

    // One operation per account: each account renames itself exactly once,
    // which keeps every nonce independent and lets us run fully concurrently
    // (no per-account nonce ordering to preserve).
    let limit = n_ops.min(accounts.len());
    let ts = now_ms();
    let mut submitted = 0usize;
    let mut pending: Vec<LatencySample> = Vec::new();
    let mut accepted_target = nonces.clone();
    let mut futs = FuturesUnordered::new();
    for acc_idx in 0..limit {
        let acc = &accounts[acc_idx];
        let client = client.clone();
        let nonce = nonces[acc_idx];
        let op = ops::make_rename(
            &acc.key,
            acc.account,
            nonce,
            format!("bench-{}-{ts}", acc.account),
        );
        let params = sendoperation_params(&op);
        futs.push(async move {
            let res = client.call("sendoperation", params).await;
            (res, acc_idx)
        });
    }
    while let Some((res, acc_idx)) = futs.next().await {
        if let Ok(value) = res {
            if ops::parse_accepted(&value).unwrap_or(false) {
                accepted_target[acc_idx] += 1;
                submitted += 1;
                pending.push(LatencySample {
                    submitted_at_ms: now_ms(),
                    confirmed_at_ms: 0,
                    op_index: pending.len(),
                    mode: "rename",
                });
            }
        }
    }
    let confirmed = wait_confirmations(
        client,
        accounts,
        &accepted_target,
        &mut pending,
        confirmed_timeout,
    )
    .await?;
    let elapsed = start.elapsed();
    let summary = crate::stats::LatencySummary::from_samples(&pending);
    let tp = Throughput::compute(submitted as u64, confirmed as u64, elapsed);
    Ok(BurstReport {
        mode: "rename".into(),
        requested: n_ops,
        submitted,
        confirmed,
        submitted_tps: tp.submitted_tps,
        confirmed_tps: tp.confirmed_tps,
        elapsed_secs: tp.elapsed_secs,
        p50_ms: summary.p50_ms,
        p95_ms: summary.p95_ms,
        p99_ms: summary.p99_ms,
    })
}

/// Run a mixed burst that splits the requested ops across the other modes.
pub async fn run_mixed(
    client: &BenchClient,
    accounts: &[BenchAccount],
    n_ops: usize,
    confirmed_timeout: Duration,
) -> Result<BurstReport> {
    let start = Instant::now();
    let mut reports = Vec::new();
    let quarters = [n_ops / 4, n_ops / 4, n_ops / 4, n_ops - 3 * (n_ops / 4)];
    for (i, n) in quarters.iter().enumerate() {
        if *n == 0 {
            continue;
        }
        let rep = match i {
            0 => run_transfer(client, accounts, *n, confirmed_timeout).await?,
            1 => run_marketplace(client, accounts, *n, confirmed_timeout).await?,
            2 => run_gift(client, accounts, *n, confirmed_timeout).await?,
            _ => run_rename(client, accounts, *n, confirmed_timeout).await?,
        };
        reports.push(rep);
    }
    let submitted: usize = reports.iter().map(|r| r.submitted).sum();
    let confirmed: usize = reports.iter().map(|r| r.confirmed).sum();
    let elapsed = start.elapsed();
    let tp = Throughput::compute(submitted as u64, confirmed as u64, elapsed);
    Ok(BurstReport {
        mode: "mixed".into(),
        requested: n_ops,
        submitted,
        confirmed,
        submitted_tps: tp.submitted_tps,
        confirmed_tps: tp.confirmed_tps,
        elapsed_secs: tp.elapsed_secs,
        p50_ms: reports.iter().map(|r| r.p50_ms).max().unwrap_or(0),
        p95_ms: reports.iter().map(|r| r.p95_ms).max().unwrap_or(0),
        p99_ms: reports.iter().map(|r| r.p99_ms).max().unwrap_or(0),
    })
}

/// Dispatch a burst to the correct mode implementation.
pub async fn run_burst(
    mode: BenchMode,
    client: &BenchClient,
    accounts: &[BenchAccount],
    n_ops: usize,
    confirmed_timeout: Duration,
) -> Result<BurstReport> {
    match mode {
        BenchMode::Transfer => run_transfer(client, accounts, n_ops, confirmed_timeout).await,
        BenchMode::Marketplace => run_marketplace(client, accounts, n_ops, confirmed_timeout).await,
        BenchMode::Gift => run_gift(client, accounts, n_ops, confirmed_timeout).await,
        BenchMode::Rename => run_rename(client, accounts, n_ops, confirmed_timeout).await,
        BenchMode::Mixed => run_mixed(client, accounts, n_ops, confirmed_timeout).await,
    }
}

/// Poll on-chain nonces until all submitted operations are confirmed.
async fn wait_confirmations(
    client: &BenchClient,
    accounts: &[BenchAccount],
    target_nonces: &[u64],
    pending: &mut [LatencySample],
    timeout: Duration,
) -> Result<usize> {
    let deadline = Instant::now() + timeout;
    let mut confirmed = 0usize;
    while Instant::now() < deadline {
        let mut all = true;
        for (idx, acc) in accounts.iter().enumerate() {
            let expected = target_nonces[idx];
            let current = client
                .call("getaccount", getaccount_params(acc.account))
                .await
                .ok()
                .and_then(|r| ops::parse_n_operation(&r).ok())
                .unwrap_or(0);
            if current < expected {
                all = false;
            }
        }
        if all {
            confirmed = pending.len();
            break;
        }
        tokio::time::sleep(Duration::from_millis(400)).await;
    }
    // Stamp confirmation times (approximate final poll time).
    let now = now_ms();
    for s in pending.iter_mut() {
        if s.confirmed_at_ms == 0 {
            s.confirmed_at_ms = now;
        }
    }
    Ok(confirmed)
}

/// Current unix time in milliseconds.
pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Validate that the given accounts exist on-chain.
pub async fn ensure_accounts(client: &BenchClient, accounts: &[BenchAccount]) -> Result<()> {
    for acc in accounts {
        let result = client
            .call("getaccount", getaccount_params(acc.account))
            .await
            .with_context(|| format!("getaccount failed for #{}", acc.account))?;
        if result.get("balance").and_then(Value::as_u64).unwrap_or(0) == 0 {
            eprintln!(
                "[bench] WARNING: account #{} has zero balance; ops may be rejected",
                acc.account
            );
        }
    }
    Ok(())
}
