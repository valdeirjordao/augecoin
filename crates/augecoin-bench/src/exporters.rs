//! Exporters: `benchmark.json`, `benchmark.csv`, and `benchmark.html` with
//! inline charts (no external network needed).

use crate::metrics::MetricsSnapshot;
use crate::modes::BurstReport;
use crate::report::BenchmarkResult;
use anyhow::{Context, Result};
use std::path::Path;

/// Write the full result document to `benchmark.json`.
pub fn export_json(result: &BenchmarkResult, dir: &Path) -> Result<()> {
    let path = dir.join("benchmark.json");
    let json = serde_json::to_string_pretty(result).context("serialize benchmark.json")?;
    std::fs::write(&path, json).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

/// Write per-burst rows to `benchmark.csv`.
pub fn export_csv(result: &BenchmarkResult, dir: &Path) -> Result<()> {
    let path = dir.join("benchmark.csv");
    let mut wtr = csv::Writer::from_path(&path).context("create benchmark.csv")?;
    wtr.write_record([
        "mode",
        "requested",
        "submitted",
        "confirmed",
        "submitted_tps",
        "confirmed_tps",
        "elapsed_secs",
        "p50_ms",
        "p95_ms",
        "p99_ms",
    ])
    .context("write csv header")?;
    for b in &result.bursts {
        wtr.write_record([
            &b.mode,
            &b.requested.to_string(),
            &b.submitted.to_string(),
            &b.confirmed.to_string(),
            &format!("{:.2}", b.submitted_tps),
            &format!("{:.2}", b.confirmed_tps),
            &format!("{:.3}", b.elapsed_secs),
            &b.p50_ms.to_string(),
            &b.p95_ms.to_string(),
            &b.p99_ms.to_string(),
        ])
        .context("write csv row")?;
    }
    wtr.flush().context("flush benchmark.csv")?;
    Ok(())
}

/// Write a self-contained HTML report with inline SVG charts to
/// `benchmark.html`.
pub fn export_html(result: &BenchmarkResult, dir: &Path) -> Result<()> {
    let path = dir.join("benchmark.html");
    let html = build_html(result);
    std::fs::write(&path, html).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

fn build_html(result: &BenchmarkResult) -> String {
    let mut rows = String::new();
    for b in &result.bursts {
        rows.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{:.2}</td><td>{:.2}</td>\
             <td>{}</td><td>{}</td><td>{}</td></tr>",
            b.mode,
            b.requested,
            b.submitted,
            b.confirmed,
            b.submitted_tps,
            b.confirmed_tps,
            b.p50_ms,
            b.p95_ms,
            b.p99_ms
        ));
    }

    let mut snap_rows = String::new();
    for s in &result.snapshots {
        snap_rows.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td>{:.1}</td><td>{}</td><td>{}</td><td>{}</td>\
             <td>{:.2}</td><td>{:.2}</td></tr>",
            s.timestamp_unix,
            s.block_height,
            s.block_time_seconds,
            s.mempool_size,
            s.transactions_total,
            s.rocksdb_log_size,
            s.cpu_percent,
            s.rss_mb
        ));
    }

    // Build simple SVG bar chart of per-burst confirmed TPS.
    let chart = bar_chart_svg(&result.bursts);

    format!(
        r#"<!DOCTYPE html>
<html lang="pt-BR">
<head>
<meta charset="utf-8"/>
<title>AUGECOIN Benchmark Report</title>
<style>
 body {{ font-family: sans-serif; margin: 2em; color:#222; }}
 h1,h2 {{ color:#1a1a2e; }}
 table {{ border-collapse: collapse; width:100%; margin:1em 0; }}
 th,td {{ border:1px solid #ccc; padding:6px 10px; text-align:left; }}
 th {{ background:#eef2f7; }}
 .card {{ border:1px solid #ddd; border-radius:8px; padding:1em 1.2em; margin:1em 0; }}
 .metric {{ font-size:1.6em; font-weight:bold; color:#0b6e4f; }}
</style>
</head>
<body>
<h1>AUGECOIN Benchmark</h1>
<div class="card">
 <p><b>Modo:</b> {mode} &nbsp; <b>Versão:</b> {version}</p>
 <p><b>Duração:</b> {dur:.2}s &nbsp; <b>Endpoints:</b> {eps}</p>
 <p class="metric">TPS enviado: {stps:.2} &nbsp;|&nbsp; TPS confirmado: {ctps:.2}</p>
 <p>Operações: {sub} enviadas / {conf} confirmadas</p>
 <p>Latência P50: {p50}ms &nbsp; P95: {p95}ms &nbsp; P99: {p99}ms</p>
</div>

<h2>Throughput por burst</h2>
{chart}

<h2>Tabela de bursts</h2>
<table>
<tr><th>Modo</th><th>Pedidas</th><th>Enviadas</th><th>Confirmadas</th>
<th>TPS env</th><th>TPS conf</th><th>P50</th><th>P95</th><th>P99</th></tr>
{rows}
</table>

<h2>Snapshots de métricas</h2>
<table>
<tr><th>ts</th><th>height</th><th>block_time</th><th>mempool</th>
<th>tx_total</th><th>rocksdb_log</th><th>cpu%</th><th>rss_mb</th></tr>
{snap_rows}
</table>
</body>
</html>"#,
        mode = result.mode,
        version = result.meta.bench_version,
        dur = result.meta.duration_secs,
        eps = result.config.endpoints.join(", "),
        stps = result.throughput.submitted_tps,
        ctps = result.throughput.confirmed_tps,
        sub = result.throughput.submitted,
        conf = result.throughput.confirmed,
        p50 = result.latency.p50_ms,
        p95 = result.latency.p95_ms,
        p99 = result.latency.p99_ms,
        chart = chart,
        rows = rows,
        snap_rows = snap_rows,
    )
}

/// Render a horizontal bar chart (SVG) of confirmed TPS per burst.
fn bar_chart_svg(bursts: &[BurstReport]) -> String {
    let max = bursts
        .iter()
        .map(|b| b.confirmed_tps)
        .fold(0.0f64, f64::max)
        .max(1.0);
    let bar_w = 600usize;
    let row_h = 24usize;
    let pad = 8usize;
    let height = bursts.len() * row_h + pad * 2;
    let mut svg =
        format!(r#"<svg width="700" height="{height}" xmlns="http://www.w3.org/2000/svg">"#);
    for (i, b) in bursts.iter().enumerate() {
        let w = ((b.confirmed_tps / max) * bar_w as f64) as usize;
        let y = pad + i * row_h;
        svg.push_str(&format!(
            r#"<text x="2" y="{y}" font-size="12">{mode}</text>
            <rect x="90" y="{y}" width="{w}" height="16" fill="{color}" rx="2"/>
            <text x="{lbl}" y="{y}" font-size="11">{val:.1}</text>"#,
            mode = b.mode,
            y = y,
            w = w,
            color = "#0b6e4f",
            lbl = 96 + w,
            val = b.confirmed_tps,
        ));
    }
    svg.push_str("</svg>");
    svg
}

/// Write all three artifacts into `dir`.
pub fn export_all(result: &BenchmarkResult, dir: &Path) -> Result<()> {
    std::fs::create_dir_all(dir).context("create output dir")?;
    export_json(result, dir)?;
    export_csv(result, dir)?;
    export_html(result, dir)?;
    Ok(())
}

/// Helper used by tests.
pub fn export_to_snapshot_csv(snaps: &[MetricsSnapshot], dir: &Path) -> Result<()> {
    let path = dir.join("snapshots.csv");
    let mut wtr = csv::Writer::from_path(&path).context("create snapshots.csv")?;
    wtr.write_record([
        "ts",
        "height",
        "block_time",
        "mempool",
        "tx_total",
        "blocks",
        "rocksdb_log",
        "cpu",
        "rss_mb",
    ])
    .context("write snap csv header")?;
    for s in snaps {
        wtr.write_record([
            &s.timestamp_unix.to_string(),
            &s.block_height.to_string(),
            &format!("{:.1}", s.block_time_seconds),
            &s.mempool_size.to_string(),
            &s.transactions_total.to_string(),
            &s.blocks_total.to_string(),
            &s.rocksdb_log_size.to_string(),
            &format!("{:.2}", s.cpu_percent),
            &format!("{:.2}", s.rss_mb),
        ])
        .context("write snap csv row")?;
    }
    wtr.flush().context("flush snapshots.csv")?;
    Ok(())
}
