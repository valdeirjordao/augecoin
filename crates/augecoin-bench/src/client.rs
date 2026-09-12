//! JSON-RPC client for the official AUGECOIN RPC surface.
//!
//! Uses Tokio + reqwest. High-concurrency fan-out is done with
//! [`futures::stream::FuturesUnordered`] so thousands of in-flight requests
//! can be driven concurrently against one or more PoA validators.

use crate::{DEFAULT_RPC_PATH, DEFAULT_RPC_TIMEOUT};
use anyhow::{anyhow, Context, Result};
use futures::stream::{FuturesUnordered, StreamExt};
use reqwest::Client;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;

/// A JSON-RPC 2.0 request envelope.
#[derive(Debug, Clone, Serialize)]
pub struct JsonRpcRequest<T: Serialize> {
    pub jsonrpc: String,
    pub id: u64,
    pub method: String,
    pub params: T,
}

impl<T: Serialize> JsonRpcRequest<T> {
    pub fn new(id: u64, method: &str, params: T) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id,
            method: method.into(),
            params,
        }
    }
}

/// Generic JSON-RPC response (success or error).
#[derive(Debug, Clone, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: Option<String>,
    pub id: Option<Value>,
    pub result: Option<Value>,
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct JsonRpcError {
    pub code: Option<i64>,
    pub message: Option<String>,
    pub data: Option<Value>,
}

/// Configuration for a bench client targeting a set of validators.
#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// Base URLs (e.g. `https://127.0.0.1:9005`) of the validators to hit.
    /// For load generation the client round-robins across these.
    pub endpoints: Vec<String>,
    /// JSON-RPC path appended to each endpoint (usually empty).
    pub path: String,
    /// Per-request timeout.
    pub timeout: Duration,
    /// Optional API key sent as `x-api-key`.
    pub api_key: Option<String>,
    /// Optional metrics path (e.g. `/metrics`) on each metrics endpoint.
    pub metrics_path: String,
    /// Base URLs of the Prometheus endpoints. These live on a *different*
    /// port than the JSON-RPC endpoint in the AUGECOIN node (RPC is 9005+
    /// while Prometheus is 9100+), so they are configured separately. When
    /// empty, the RPC endpoints are used as a fallback.
    pub metrics_endpoints: Vec<String>,
}

impl ClientConfig {
    pub fn new(endpoints: Vec<String>) -> Self {
        Self {
            endpoints,
            path: DEFAULT_RPC_PATH.into(),
            timeout: DEFAULT_RPC_TIMEOUT,
            api_key: None,
            metrics_path: crate::DEFAULT_METRICS_PATH.into(),
            metrics_endpoints: Vec::new(),
        }
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn with_api_key(mut self, key: Option<String>) -> Self {
        self.api_key = key;
        self
    }

    pub fn with_metrics_endpoints(mut self, endpoints: Vec<String>) -> Self {
        self.metrics_endpoints = endpoints;
        self
    }

    /// The base URLs to scrape Prometheus metrics from.
    pub fn metrics_targets(&self) -> &[String] {
        if self.metrics_endpoints.is_empty() {
            &self.endpoints
        } else {
            &self.metrics_endpoints
        }
    }
}

/// Lightweight concurrent JSON-RPC client over one or more validators.
#[derive(Debug, Clone)]
pub struct BenchClient {
    pub config: ClientConfig,
    http: Client,
    /// Round-robin counter for endpoint selection.
    next: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

impl BenchClient {
    /// Build a client with the given configuration and a shared reqwest pool.
    pub fn new(config: ClientConfig) -> Result<Self> {
        let http = Client::builder()
            .timeout(config.timeout)
            .pool_max_idle_per_host(64)
            .danger_accept_invalid_certs(true) // AUGECOIN testnet uses self-signed TLS
            .build()
            .context("failed to build HTTP client")?;
        Ok(Self {
            config,
            http,
            next: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        })
    }

    /// Pick the next validator endpoint (round-robin).
    pub fn next_endpoint(&self) -> Result<&str> {
        if self.config.endpoints.is_empty() {
            return Err(anyhow!("no validator endpoints configured"));
        }
        let idx = self.next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let i = idx % self.config.endpoints.len();
        Ok(&self.config.endpoints[i])
    }

    fn rpc_url(&self, endpoint: &str) -> String {
        if endpoint.ends_with('/') {
            format!("{}{}", endpoint, self.config.path)
        } else if self.config.path.is_empty() {
            endpoint.to_string()
        } else {
            format!("{}/{}", endpoint.trim_end_matches('/'), self.config.path)
        }
    }

    fn metrics_url(&self, endpoint: &str) -> String {
        format!(
            "{}{}",
            endpoint.trim_end_matches('/'),
            self.config.metrics_path
        )
    }

    /// Perform a single synchronous JSON-RPC call (returns the `result` Value).
    pub async fn call(&self, method: &str, params: Value) -> Result<Value> {
        let endpoint = self.next_endpoint()?.to_string();
        let url = self.rpc_url(&endpoint);
        let id = 1;
        let req = JsonRpcRequest::new(id, method, params);
        let mut builder = self.http.post(&url).json(&req);
        if let Some(key) = &self.config.api_key {
            builder = builder.header("x-api-key", key);
        }
        let resp: JsonRpcResponse = builder
            .send()
            .await
            .with_context(|| format!("RPC send failed for {method} @ {endpoint}"))?
            .json()
            .await
            .with_context(|| format!("RPC parse failed for {method} @ {endpoint}"))?;
        if let Some(err) = resp.error {
            return Err(anyhow!("RPC error for {method}: {:?}", err.message));
        }
        resp.result
            .ok_or_else(|| anyhow!("RPC {method} returned no result"))
    }

    /// Fire a JSON-RPC call to a specific endpoint (used by distributed mode).
    pub async fn call_on(&self, endpoint: &str, method: &str, params: Value) -> Result<Value> {
        let url = self.rpc_url(endpoint);
        let req = JsonRpcRequest::new(1, method, params);
        let mut builder = self.http.post(&url).json(&req);
        if let Some(key) = &self.config.api_key {
            builder = builder.header("x-api-key", key);
        }
        let resp: JsonRpcResponse = builder
            .send()
            .await
            .with_context(|| format!("RPC send failed for {method} @ {endpoint}"))?
            .json()
            .await
            .with_context(|| format!("RPC parse failed for {method} @ {endpoint}"))?;
        if let Some(err) = resp.error {
            return Err(anyhow!("RPC error for {method}: {:?}", err.message));
        }
        resp.result
            .ok_or_else(|| anyhow!("RPC {method} returned no result"))
    }

    /// Scrape the Prometheus text exposition from a validator's `/metrics`.
    pub async fn scrape_metrics(&self, endpoint: &str) -> Result<String> {
        let url = self.metrics_url(endpoint);
        self.http
            .get(&url)
            .send()
            .await
            .with_context(|| format!("metrics scrape failed @ {url}"))?
            .text()
            .await
            .with_context(|| format!("metrics body read failed @ {url}"))
    }

    /// Scrape metrics from all configured validators concurrently.
    pub async fn scrape_all_metrics(&self) -> Result<Vec<(String, String)>> {
        let mut futs = FuturesUnordered::new();
        for ep in self.config.metrics_targets() {
            let ep = ep.clone();
            let this = self.clone();
            futs.push(async move {
                let body = this.scrape_metrics(&ep).await?;
                Ok::<_, anyhow::Error>((ep, body))
            });
        }
        let mut out = Vec::new();
        while let Some(res) = futs.next().await {
            out.push(res?);
        }
        Ok(out)
    }

    /// Run many JSON-RPC calls concurrently and collect the results.
    ///
    /// `n` requests are issued concurrently using [`FuturesUnordered`]; the
    /// caller supplies a closure that builds `(method, params)` from an index.
    pub async fn fan_out<T, F>(&self, n: usize, build: F) -> Vec<Result<Value>>
    where
        F: Fn(usize) -> (String, Value),
    {
        let mut futs = FuturesUnordered::new();
        for i in 0..n {
            let (method, params) = build(i);
            let this = self.clone();
            futs.push(async move { this.call(&method, params).await });
        }
        let mut results = Vec::with_capacity(n);
        while let Some(r) = futs.next().await {
            results.push(r);
        }
        results
    }
}

/// Deserialize a `Value` into a concrete type.
pub fn from_value<T: DeserializeOwned>(v: Value) -> Result<T> {
    serde_json::from_value(v).context("failed to deserialize RPC result")
}
