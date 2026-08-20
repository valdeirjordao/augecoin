#!/usr/bin/env python3
"""
AUGECOIN real-time monitoring API.

Aggregates, on a background loop, the state needed by the /saude.html
dashboard: chain status (JSON-RPC) and Prometheus metrics from all 4
validators, the running stress test's log, and a live `MessageTooLarge`
counter (the liveness signal this whole exercise is guarding).

Served as JSON over HTTP on 127.0.0.1:8900. The nginx `augeco.in` site
proxies `/saude-api/` to this port, so the browser page stays same-origin.

Endpoints:
  GET /status  -> the cached aggregate snapshot (JSON)
  GET /        -> tiny health/liveness page
"""

import json
import re
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

import requests
import urllib3

urllib3.disable_warnings(urllib3.exceptions.InsecureRequestWarning)

# ── Configuration ─────────────────────────────────────────────────────────
RPC_PORTS = [9005, 9006, 9007, 9008]
METRICS_PORTS = [9100, 9110, 9120, 9130]
STRESS_LOG = "/opt/augecoin/logs/stress-test-12000.log"
VALIDATOR_LOGS = [
    "/opt/augecoin/logs/validator-0.log",
    "/opt/augecoin/logs/validator-1.log",
    "/opt/augecoin/logs/validator-2.log",
    "/opt/augecoin/logs/validator-3.log",
]

# `MessageTooLarge` lines already present in the validator logs *before* the
# current 10.150-tx/block stress run started (historical bug). The dashboard
# reports the delta above this baseline as "new" rejections.
MESSAGE_TOO_LARGE_BASELINE = [68, 68, 55, 67]

LISTEN = ("0.0.0.0", 8900)
REFRESH_INTERVAL = 3.0

# ── Aggregation helpers ───────────────────────────────────────────────────

_snapshot = {"ts": 0, "initialized": False}


def fetch_rpc_status(port):
    try:
        r = requests.post(
            f"https://127.0.0.1:{port}",
            json={"jsonrpc": "2.0", "id": 1, "method": "nodestatus", "params": {}},
            verify=False,
            timeout=3,
        )
        return r.json().get("result", {})
    except Exception:
        return {}


def fetch_metrics(port):
    out = {}
    try:
        r = requests.get(f"http://127.0.0.1:{port}/metrics", timeout=3)
        for line in r.text.splitlines():
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            parts = line.split()
            if len(parts) >= 2 and parts[0].startswith("augecoin_"):
                try:
                    out[parts[0]] = float(parts[1])
                except ValueError:
                    pass
    except Exception:
        pass
    return out


def read_tail(path, n=40):
    try:
        with open(path, "r", encoding="utf-8", errors="replace") as f:
            lines = f.read().splitlines()
        return lines[-n:]
    except OSError:
        return []


def count_message_too_large():
    counts = []
    for path in VALIDATOR_LOGS:
        try:
            with open(path, "r", encoding="utf-8", errors="replace") as f:
                data = f.read()
            counts.append(data.count("MessageTooLarge"))
        except OSError:
            counts.append(0)
    return counts


def parse_stress_log(lines):
    cfg = {}
    m = None
    for line in lines:
        m = re.search(
            r"tx-per-block=(\d+) blocks=(\d+) start-block=(\d+)", line
        )
        if m:
            cfg = {
                "tx_per_block": int(m.group(1)),
                "blocks": int(m.group(2)),
                "start_block": int(m.group(3)),
            }
            break

    total_admitted = 0
    total_rejected = 0
    current_target = None
    blocks_done = 0
    last_phase = "idle"

    for line in lines:
        m = re.search(r"block target=(\d+): submitted \d+ in [\d.]+s \(admitted (\d+), rejected (\d+)\)", line)
        if m:
            current_target = int(m.group(1))
            total_admitted = int(m.group(2))
            total_rejected = int(m.group(3))
            last_phase = "submitting"
            continue
        if "all ops confirmed" in line:
            blocks_done += 1
            last_phase = "confirming"
            continue
        if "confirmation timeout" in line:
            last_phase = "timeout"
            continue
        if "DONE." in line:
            last_phase = "done"
            continue
        if "waiting for block" in line:
            last_phase = "waiting"
        elif "aligning to a fresh commit" in line:
            last_phase = "aligning"

    return {
        "config": cfg,
        "current_target": current_target,
        "total_admitted": total_admitted,
        "total_rejected": total_rejected,
        "blocks_done": blocks_done,
        "phase": last_phase,
    }


def refresh():
    global _snapshot
    # Parse progress from the FULL log (config + all confirmations), so the
    # block counter reflects the whole run, not just the most recent lines.
    # Only the tail is kept for the on-screen log panel.
    all_lines = read_tail(STRESS_LOG, 100_000)
    stress = parse_stress_log(all_lines)
    stress["lines"] = all_lines[-25:]

    validators = []
    chain_height = 0
    chain_tps = 0
    for i in range(4):
        rpc = fetch_rpc_status(RPC_PORTS[i])
        metrics = fetch_metrics(METRICS_PORTS[i])
        height = int(rpc.get("current_height", 0) or 0)
        chain_height = max(chain_height, height)
        chain_tps = max(chain_tps, int(metrics.get("augecoin_transactions_per_second", 0)))
        validators.append(
            {
                "id": i,
                "rpc_ok": bool(rpc),
                "metrics_ok": bool(metrics),
                "height": height,
                "peers": rpc.get("peers_connected", 0),
                "gossipsub": rpc.get("peers_gossipsub_consensus", 0),
                "kademlia": rpc.get("peers_kademlia_total", 0),
                "mempool_size": rpc.get("mempool_size", 0),
                "syncing": rpc.get("syncing", False),
                "uptime_seconds": rpc.get("uptime_seconds", 0),
                "current_round": rpc.get("current_round", 0),
                "last_error": rpc.get("last_consensus_error"),
                "utilization_pct": int(metrics.get("augecoin_block_utilization_pct", 0)),
                "serialized_bytes": int(metrics.get("augecoin_block_serialized_bytes", 0)),
                "transactions_total": int(metrics.get("augecoin_transactions_total", 0)),
                "transactions_per_second": int(
                    metrics.get("augecoin_transactions_per_second", 0)
                ),
                "blocks_total": int(metrics.get("augecoin_blocks_total", 0)),
            }
        )

    mtl_current = count_message_too_large()
    mtl_new = sum(max(c - b, 0) for c, b in zip(mtl_current, MESSAGE_TOO_LARGE_BASELINE))

    _snapshot = {
        "ts": int(time.time()),
        "initialized": True,
        "chain_height": chain_height,
        "transactions_per_second": chain_tps,
        "stress": {
            "running": stress["phase"] not in ("done", "idle"),
            "phase": stress["phase"],
            "config": stress["config"],
            "current_target": stress["current_target"],
            "total_admitted": stress["total_admitted"],
            "total_rejected": stress["total_rejected"],
            "blocks_done": stress["blocks_done"],
            "blocks_total": stress["config"].get("blocks"),
            "lines": stress["lines"],
        },
        "validators": validators,
        "message_too_large": {
            "baseline": MESSAGE_TOO_LARGE_BASELINE,
            "current": mtl_current,
            "new": mtl_new,
        },
    }


def refresher():
    while True:
        try:
            refresh()
        except Exception as e:  # noqa: BLE001
            print(f"[monitor] refresh error: {e}", flush=True)
        time.sleep(REFRESH_INTERVAL)


class Handler(BaseHTTPRequestHandler):
    def _send_json(self, obj, status=200):
        body = json.dumps(obj).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Access-Control-Allow-Origin", "*")
        self.send_header("Cache-Control", "no-store")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        if self.path in ("/", "/status", "/status/"):
            self._send_json(_snapshot)
        else:
            self._send_json({"error": "not found"}, status=404)

    def log_message(self, *args):
        pass


def main():
    threading.Thread(target=refresher, daemon=True).start()
    print(f"[monitor] AUGECOIN monitor API listening on {LISTEN[0]}:{LISTEN[1]}", flush=True)
    ThreadingHTTPServer(LISTEN, Handler).serve_forever()


if __name__ == "__main__":
    main()
