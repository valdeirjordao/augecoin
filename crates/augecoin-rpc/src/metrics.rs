#![allow(dead_code)]
use prometheus::{
    self, Encoder, Histogram, HistogramOpts, IntCounter, IntGauge, Registry, TextEncoder,
};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

/// Metrics for the augecoin-rpc JSON-RPC layer.
/// Exposed on `/metrics` for Prometheus scraping.
pub struct RpcMetrics {
    pub registry: Registry,
    // ── Counters ──────────────────────────────────────────────────────
    pub requests_total: IntCounter,
    pub auth_failures_total: IntCounter,
    pub rate_limited_total: IntCounter,
    pub errors_total: IntCounter,
    // ── Histograms ───────────────────────────────────────────────────
    pub request_duration_seconds: Histogram,
    // ── Gauges ───────────────────────────────────────────────────────
    pub active_connections: IntGauge,
}

impl RpcMetrics {
    pub fn new() -> Result<Self, prometheus::Error> {
        let registry = Registry::new();

        let requests_total = IntCounter::new(
            "augecoin_rpc_requests_total",
            "Total JSON-RPC requests received",
        )?;
        let auth_failures_total = IntCounter::new(
            "augecoin_rpc_auth_failures_total",
            "Total requests rejected due to missing/invalid API key",
        )?;
        let rate_limited_total = IntCounter::new(
            "augecoin_rpc_rate_limited_total",
            "Total requests rejected by rate limiter",
        )?;
        let errors_total = IntCounter::new(
            "augecoin_rpc_errors_total",
            "Total requests that returned an error result",
        )?;
        let request_duration_seconds = Histogram::with_opts(HistogramOpts::new(
            "augecoin_rpc_request_duration_seconds",
            "JSON-RPC request processing latency",
        ))?;
        let active_connections = IntGauge::new(
            "augecoin_rpc_active_connections",
            "Current number of active TCP connections",
        )?;

        registry.register(Box::new(requests_total.clone()))?;
        registry.register(Box::new(auth_failures_total.clone()))?;
        registry.register(Box::new(rate_limited_total.clone()))?;
        registry.register(Box::new(errors_total.clone()))?;
        registry.register(Box::new(request_duration_seconds.clone()))?;
        registry.register(Box::new(active_connections.clone()))?;

        Ok(RpcMetrics {
            registry,
            requests_total,
            auth_failures_total,
            rate_limited_total,
            errors_total,
            request_duration_seconds,
            active_connections,
        })
    }
}

pub struct MetricsServer {
    running: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
    port: u16,
}

impl MetricsServer {
    pub fn start(registry: Registry, port: u16) -> Result<Self, std::io::Error> {
        let running = Arc::new(AtomicBool::new(true));
        let running_clone = running.clone();
        let addr = format!("0.0.0.0:{port}");

        let listener = TcpListener::bind(&addr)?;

        let handle = thread::spawn(move || {
            for stream in listener.incoming() {
                if !running_clone.load(Ordering::SeqCst) {
                    break;
                }
                match stream {
                    Ok(stream) => {
                        let _ = handle_request(stream, &registry);
                    }
                    Err(_) => break,
                }
            }
        });

        Ok(MetricsServer {
            running,
            handle: Some(handle),
            port,
        })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn shutdown(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        let _ = TcpStream::connect(format!("127.0.0.1:{}", self.port));
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for MetricsServer {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn handle_request(mut stream: TcpStream, registry: &Registry) -> Result<(), std::io::Error> {
    let _ = stream.set_read_timeout(Some(Duration::from_millis(100)));
    let mut req_buf = [0u8; 1024];
    let _ = stream.read(&mut req_buf);

    let metric_families = registry.gather();
    let mut buffer = Vec::new();
    let encoder = TextEncoder::new();
    if encoder.encode(&metric_families, &mut buffer).is_err() {
        return Ok(());
    }

    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/plain; version=0.0.4\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        buffer.len()
    );

    stream.write_all(response.as_bytes())?;
    stream.write_all(&buffer)?;
    stream.flush()?;
    Ok(())
}
