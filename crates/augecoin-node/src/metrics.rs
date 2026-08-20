#![allow(dead_code)]
use prometheus::{
    self, Encoder, Histogram, HistogramOpts, IntCounter, IntGauge, Registry, TextEncoder,
};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

pub struct NodeMetrics {
    pub registry: Registry,
    pub block_height: IntGauge,
    pub block_time_seconds: IntGauge,
    pub peers_connected: IntGauge,
    pub peers_gossipsub: IntGauge,
    pub peers_kademlia: IntGauge,
    pub mempool_size: IntGauge,
    pub equivocation_events: IntCounter,
    pub block_propagation_latency_ms: IntGauge,
    pub current_round: IntGauge,
    pub sync_lag: IntGauge,
    pub blocks_total: IntCounter,
    pub transactions_total: IntCounter,
    pub transactions_per_second: IntGauge,
    pub consensus_latency: IntGauge,
    pub view_changes_total: IntCounter,
    pub catchup_blocks_total: IntCounter,
    pub safebox_bytes: IntGauge,
    pub dirty_accounts: IntGauge,
    pub snapshot_duration_seconds: Histogram,
    pub wal_flush_total: IntCounter,
    pub rocksdb_log_size: IntGauge,
    pub snapshot_total: IntCounter,
    pub block_serialized_bytes: IntGauge,
    pub block_utilization_pct: IntGauge,
    // Light-proposal transport metrics (FASE 2).
    pub proposal_bytes: IntGauge,
    pub proposal_hash_bytes: IntGauge,
    pub proposal_fill_ratio: IntGauge,
    pub tx_reuse_ratio: IntGauge,
    pub tx_fetch_count: IntCounter,
    pub tx_fetch_latency_ms: IntGauge,
    pub proposal_build_ms: IntGauge,
    pub proposal_reconstruction_ms: IntGauge,
}

impl NodeMetrics {
    pub fn new() -> Result<Self, prometheus::Error> {
        let registry = Registry::new();

        let block_height = IntGauge::new("augecoin_block_height", "Current block height")?;
        let block_time_seconds = IntGauge::new(
            "augecoin_block_time_seconds",
            "Average time in seconds between the last N blocks",
        )?;
        let peers_connected =
            IntGauge::new("augecoin_peers_connected", "Number of connected peers")?;
        let peers_gossipsub = IntGauge::new(
            "augecoin_peers_gossipsub_consensus",
            "Peers in the gossipsub consensus mesh",
        )?;
        let peers_kademlia = IntGauge::new(
            "augecoin_peers_kademlia_total",
            "Peers known via Kademlia DHT discovery (not necessarily connected)",
        )?;
        let mempool_size =
            IntGauge::new("augecoin_mempool_size", "Number of operations in mempool")?;
        let equivocation_events = IntCounter::new(
            "augecoin_equivocation_events_total",
            "Total number of equivocation events detected",
        )?;
        let block_propagation_latency_ms = IntGauge::new(
            "augecoin_block_propagation_latency_ms",
            "Last block propagation latency in milliseconds",
        )?;
        let current_round = IntGauge::new("augecoin_current_round", "Current consensus round")?;
        let sync_lag = IntGauge::new("augecoin_sync_lag", "Blocks behind the chain tip")?;
        let blocks_total = IntCounter::new("augecoin_blocks_total", "Total committed blocks")?;
        let transactions_total = IntCounter::new(
            "augecoin_transactions_total",
            "Total committed transactions",
        )?;
        let transactions_per_second = IntGauge::new(
            "augecoin_transactions_per_second",
            "Transactions committed per second in the last committed block interval",
        )?;
        let consensus_latency = IntGauge::new(
            "augecoin_consensus_latency",
            "Last consensus latency in milliseconds",
        )?;
        let view_changes_total = IntCounter::new(
            "augecoin_view_changes_total",
            "Total consensus view changes",
        )?;
        let catchup_blocks_total = IntCounter::new(
            "augecoin_catchup_blocks_total",
            "Total catch-up blocks applied",
        )?;

        let safebox_bytes = IntGauge::new(
            "augecoin_safebox_bytes",
            "Size in bytes of the SafeBox state",
        )?;
        let dirty_accounts = IntGauge::new(
            "augecoin_dirty_accounts",
            "Accounts modified since the last SafeBox snapshot",
        )?;
        let snapshot_duration_seconds = Histogram::with_opts(HistogramOpts::new(
            "augecoin_snapshot_duration_seconds",
            "Time to persist a full SafeBox snapshot",
        ))?;
        let wal_flush_total = IntCounter::new(
            "augecoin_wal_flush_total",
            "Total WAL flushes triggered by block commits",
        )?;
        let rocksdb_log_size = IntGauge::new(
            "augecoin_rocksdb_log_size",
            "Current RocksDB info LOG file size in bytes",
        )?;
        let snapshot_total = IntCounter::new(
            "augecoin_snapshot_total",
            "Total full SafeBox snapshots persisted",
        )?;
        let block_serialized_bytes = IntGauge::new(
            "augecoin_block_serialized_bytes",
            "Serialized size in bytes of the last committed block",
        )?;
        let block_utilization_pct = IntGauge::new(
            "augecoin_block_utilization_pct",
            "Utilization of the gossipsub transmit ceiling by the last committed block, in percent",
        )?;
        let proposal_bytes = IntGauge::new(
            "augecoin_proposal_bytes",
            "Wire size in bytes of the last light proposal",
        )?;
        let proposal_hash_bytes = IntGauge::new(
            "augecoin_proposal_hash_bytes",
            "Bytes contributed by tx hashes in the last light proposal",
        )?;
        let proposal_fill_ratio = IntGauge::new(
            "augecoin_proposal_fill_ratio",
            "Usage of the byte budget by the last proposed block, in percent",
        )?;
        let tx_reuse_ratio = IntGauge::new(
            "augecoin_tx_reuse_ratio",
            "Percent of proposal txs already present in the local mempool (target >90)",
        )?;
        let tx_fetch_count = IntCounter::new(
            "augecoin_tx_fetch_count",
            "Total transactions fetched on demand via GetTransactions",
        )?;
        let tx_fetch_latency_ms = IntGauge::new(
            "augecoin_tx_fetch_latency_ms",
            "Latency of the last on-demand transaction fetch, in milliseconds",
        )?;
        let proposal_build_ms = IntGauge::new(
            "augecoin_proposal_build_ms",
            "Time to assemble the last proposal, in milliseconds",
        )?;
        let proposal_reconstruction_ms = IntGauge::new(
            "augecoin_proposal_reconstruction_ms",
            "Time to reconstruct a block from the last light proposal, in milliseconds",
        )?;

        registry.register(Box::new(block_height.clone()))?;
        registry.register(Box::new(block_time_seconds.clone()))?;
        registry.register(Box::new(peers_connected.clone()))?;
        registry.register(Box::new(peers_gossipsub.clone()))?;
        registry.register(Box::new(peers_kademlia.clone()))?;
        registry.register(Box::new(mempool_size.clone()))?;
        registry.register(Box::new(equivocation_events.clone()))?;
        registry.register(Box::new(block_propagation_latency_ms.clone()))?;
        registry.register(Box::new(current_round.clone()))?;
        registry.register(Box::new(sync_lag.clone()))?;
        registry.register(Box::new(blocks_total.clone()))?;
        registry.register(Box::new(transactions_total.clone()))?;
        registry.register(Box::new(transactions_per_second.clone()))?;
        registry.register(Box::new(consensus_latency.clone()))?;
        registry.register(Box::new(view_changes_total.clone()))?;
        registry.register(Box::new(catchup_blocks_total.clone()))?;
        registry.register(Box::new(safebox_bytes.clone()))?;
        registry.register(Box::new(dirty_accounts.clone()))?;
        registry.register(Box::new(snapshot_duration_seconds.clone()))?;
        registry.register(Box::new(wal_flush_total.clone()))?;
        registry.register(Box::new(rocksdb_log_size.clone()))?;
        registry.register(Box::new(snapshot_total.clone()))?;
        registry.register(Box::new(block_serialized_bytes.clone()))?;
        registry.register(Box::new(block_utilization_pct.clone()))?;
        registry.register(Box::new(proposal_bytes.clone()))?;
        registry.register(Box::new(proposal_hash_bytes.clone()))?;
        registry.register(Box::new(proposal_fill_ratio.clone()))?;
        registry.register(Box::new(tx_reuse_ratio.clone()))?;
        registry.register(Box::new(tx_fetch_count.clone()))?;
        registry.register(Box::new(tx_fetch_latency_ms.clone()))?;
        registry.register(Box::new(proposal_build_ms.clone()))?;
        registry.register(Box::new(proposal_reconstruction_ms.clone()))?;

        Ok(NodeMetrics {
            registry,
            block_height,
            block_time_seconds,
            peers_connected,
            peers_gossipsub,
            peers_kademlia,
            mempool_size,
            equivocation_events,
            block_propagation_latency_ms,
            current_round,
            sync_lag,
            blocks_total,
            transactions_total,
            transactions_per_second,
            consensus_latency,
            view_changes_total,
            catchup_blocks_total,
            safebox_bytes,
            dirty_accounts,
            snapshot_duration_seconds,
            wal_flush_total,
            rocksdb_log_size,
            snapshot_total,
            block_serialized_bytes,
            block_utilization_pct,
            proposal_bytes,
            proposal_hash_bytes,
            proposal_fill_ratio,
            tx_reuse_ratio,
            tx_fetch_count,
            tx_fetch_latency_ms,
            proposal_build_ms,
            proposal_reconstruction_ms,
        })
    }
}

pub struct MetricsServer {
    running: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
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
                        handle_request(&stream, &registry);
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

fn handle_request(mut stream: &TcpStream, registry: &Registry) {
    let _ = stream.set_read_timeout(Some(std::time::Duration::from_millis(100)));
    let mut req_buf = [0u8; 1024];
    let _ = stream.read(&mut req_buf);

    let metric_families = registry.gather();
    let mut buffer = Vec::new();
    let encoder = TextEncoder::new();
    if encoder.encode(&metric_families, &mut buffer).is_err() {
        return;
    }

    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/plain; version=0.0.4\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        buffer.len()
    );

    let _ = stream.write_all(response.as_bytes());
    let _ = stream.write_all(&buffer);
    let _ = stream.flush();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metrics_registry_has_all_metrics() {
        let m = NodeMetrics::new().unwrap();
        let families = m.registry.gather();
        let names: Vec<&str> = families.iter().map(|f| f.get_name()).collect();

        assert!(names.contains(&"augecoin_block_height"));
        assert!(names.contains(&"augecoin_block_time_seconds"));
        assert!(names.contains(&"augecoin_peers_connected"));
        assert!(names.contains(&"augecoin_mempool_size"));
        assert!(names.contains(&"augecoin_equivocation_events_total"));
        assert!(names.contains(&"augecoin_block_propagation_latency_ms"));
    }

    #[test]
    fn metrics_update_and_read() {
        let m = NodeMetrics::new().unwrap();
        m.block_height.set(42);
        m.peers_connected.set(5);
        m.mempool_size.set(128);
        m.equivocation_events.inc_by(3);
        m.block_time_seconds.set(60);
        m.block_propagation_latency_ms.set(1500);

        let families = m.registry.gather();
        for family in &families {
            for metric in family.get_metric() {
                match family.get_name() {
                    "augecoin_block_height" => {
                        assert_eq!(metric.get_gauge().get_value(), 42.0);
                    }
                    "augecoin_block_time_seconds" => {
                        assert_eq!(metric.get_gauge().get_value(), 60.0);
                    }
                    "augecoin_peers_connected" => {
                        assert_eq!(metric.get_gauge().get_value(), 5.0);
                    }
                    "augecoin_mempool_size" => {
                        assert_eq!(metric.get_gauge().get_value(), 128.0);
                    }
                    "augecoin_equivocation_events_total" => {
                        assert_eq!(metric.get_counter().get_value(), 3.0);
                    }
                    "augecoin_block_propagation_latency_ms" => {
                        assert_eq!(metric.get_gauge().get_value(), 1500.0);
                    }
                    _ => {}
                }
            }
        }
    }

    #[test]
    fn metrics_server_starts_and_stops() {
        let m = NodeMetrics::new().unwrap();
        let registry = m.registry;

        let port: u16 = 19150;
        let mut server = MetricsServer::start(registry, port).unwrap();
        assert_eq!(server.port(), 19150);
        server.shutdown();
    }

    #[test]
    fn metrics_text_format_is_valid() {
        let m = NodeMetrics::new().unwrap();
        m.block_height.set(10);
        m.peers_connected.set(3);

        let families = m.registry.gather();
        let mut buffer = Vec::new();
        let encoder = TextEncoder::new();
        encoder.encode(&families, &mut buffer).unwrap();
        let text = String::from_utf8(buffer).unwrap();

        assert!(text.contains("augecoin_block_height 10"));
        assert!(text.contains("augecoin_peers_connected 3"));
        assert!(text.starts_with("# HELP") || text.starts_with("# TYPE"));
    }

    #[test]
    fn counter_increments() {
        let m = NodeMetrics::new().unwrap();
        m.equivocation_events.inc();
        m.equivocation_events.inc();
        m.equivocation_events.inc_by(5);

        let families = m.registry.gather();
        for family in &families {
            if family.get_name() == "augecoin_equivocation_events_total" {
                for metric in family.get_metric() {
                    assert_eq!(metric.get_counter().get_value(), 7.0);
                }
            }
        }
    }

    #[test]
    fn gauge_reset() {
        let m = NodeMetrics::new().unwrap();
        m.peers_connected.set(10);
        m.peers_connected.set(0);

        let families = m.registry.gather();
        for family in &families {
            if family.get_name() == "augecoin_peers_connected" {
                for metric in family.get_metric() {
                    assert_eq!(metric.get_gauge().get_value(), 0.0);
                }
            }
        }
    }
}
