#!/usr/bin/env python3
"""
AUGECOIN block-synchronized stress test.

Drives a fixed number of real on-chain transfers per block between the 4
dev-validator accounts (senders) and a target account (receiver), starting at
a chosen block height and running for a chosen number of blocks.

Sender accounts (0..3) are the deterministic dev-validator Ed25519 keys
(same HKDF-SHA3-512 scheme as the node). The receiver (default 40) is an
already-owned account; we only send *to* it, so no private key is needed.

Concurrency model: the node enforces strict per-account n_operation ordering,
so a single account's transfers must be admitted in nonce order. We therefore
pin each sender account to one validator RPC endpoint and submit that
account's transfers sequentially, while the 4 accounts run in parallel
(4 worker threads). This mirrors `crates/augecoin-bench/src/modes.rs`.

Usage:
  python3 stress_test.py \
      --senders 0,1,2,3 --receiver 40 \
       --tx-per-block 12000 --blocks 500 \
      --start-block 4190 --amount 0.001
"""

import argparse
import hashlib
import hmac
import json
import os
import sys
import threading
import time
import urllib3

from concurrent.futures import ThreadPoolExecutor, as_completed
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey

import requests

urllib3.disable_warnings(urllib3.exceptions.InsecureRequestWarning)

CHAIN_ID = 2
DECIMALS = 8
AUGESAT_PER_AUGE = 10 ** DECIMALS
MIN_FEE_AUGESAT = 1_000

DEFAULT_ENDPOINTS = [
    "https://127.0.0.1:9005",
    "https://127.0.0.1:9006",
    "https://127.0.0.1:9007",
    "https://127.0.0.1:9008",
]


# ── HKDF-SHA3-512 (mirrors crates/augecoin-crypto/src/hdkeys.rs) ───────
def hkdf_extract(salt: bytes, ikm: bytes) -> bytes:
    return hmac.new(salt, ikm, hashlib.sha3_512).digest()


def hkdf_expand(prk: bytes, info: bytes, length: int) -> bytes:
    out, t, i = b"", b"", 1
    while len(out) < length:
        t = hmac.new(prk, t + info + bytes([i]), hashlib.sha3_512).digest()
        out += t
        i += 1
    return out[:length]


def derive_ed25519(seed: bytes, index: int) -> Ed25519PrivateKey:
    prk = hkdf_extract(b"", seed)
    info = b"augecoin-hdkey-" + index.to_bytes(8, "big")
    okm = hkdf_expand(prk, info, 64)
    return Ed25519PrivateKey.from_private_bytes(okm[:32])


def validator_seed(vid: int) -> bytes:
    seed = bytearray(64)
    seed[0] = 0xAB
    seed[1:9] = vid.to_bytes(8, "big")
    return bytes(seed)


# ── Operation serialization (mirrors crates/augecoin-core/src/operation.rs) ──
class Writer:
    def __init__(self):
        self.buf = bytearray()

    def u8(self, v):
        self.buf.append(v & 0xFF)

    def u16(self, v):
        self.buf += int(v).to_bytes(2, "big")

    def u32(self, v):
        self.buf += int(v).to_bytes(4, "big")

    def u64(self, v):
        self.buf += int(v).to_bytes(8, "big")

    def fixed(self, b):
        self.buf += bytes(b)

    def bytes(self, b):
        b = bytes(b)
        self.u16(len(b))
        self.buf += b

    def finish(self):
        return bytes(self.buf)


def serialize_transfer_stripped(sender, n_operation, receiver, amount, fee=MIN_FEE_AUGESAT):
    w = Writer()
    w.u8(0x01)
    w.u32(1)
    w.u64(sender)
    w.u64(n_operation)
    w.u64(amount)
    w.bytes(b"")
    w.u32(1)
    w.u64(receiver)
    w.u64(amount)
    w.bytes(b"")
    w.u32(0)
    w.u64(fee)
    w.u64(CHAIN_ID)
    return w.finish()


def sign(privkey, stripped):
    return privkey.sign(stripped)


def full_signed(stripped, signature):
    w = Writer()
    w.buf += stripped
    w.u32(1)
    w.fixed(signature)
    return w.finish().hex()


# ── JSON-RPC client (connection-pooled, self-signed TLS) ───────────────
class RpcClient:
    def __init__(self, endpoint):
        self.endpoint = endpoint
        self.session = requests.Session()
        self.session.verify = False
        self.session.headers.update({"Content-Type": "application/json"})
        self._lock = threading.Lock()

    def call(self, method, params=None, timeout=30):
        payload = {"jsonrpc": "2.0", "id": 1, "method": method, "params": params or {}}
        with self._lock:
            resp = self.session.post(self.endpoint, data=json.dumps(payload), timeout=timeout)
        body = resp.json()
        if "error" in body:
            raise RuntimeError(body["error"].get("message", str(body["error"])))
        return body.get("result", body)

    def send_operation(self, key, sender, n_operation, receiver, amount):
        stripped = serialize_transfer_stripped(sender, n_operation, receiver, amount)
        hex_op = full_signed(stripped, sign(key, stripped))
        return self.call("sendoperation", {"hex": hex_op})

    def send_operations(self, operations):
        return self.call("sendoperations", {"operations": operations}, timeout=120)

    def get_account(self, account):
        return self.call("getaccount", {"account_number": account})

    def get_height(self):
        return int(self.call("nodestatus", {}).get("current_height", 0))


def log(msg):
    line = f"[stress-test {time.strftime('%H:%M:%S')}] {msg}"
    print(line, flush=True)
    try:
        with open(LOG_PATH, "a") as f:
            f.write(line + "\n")
    except OSError:
        pass


LOG_PATH = os.environ.get("AUGECOIN_STRESS_LOG", "/opt/augecoin/logs/stress-test.log")


def fmt_auge(augesat):
    return f"{augesat / AUGESAT_PER_AUGE:.8f}".rstrip("0").rstrip(".")


def main():
    ap = argparse.ArgumentParser(description="AUGECOIN block-synchronized stress test")
    ap.add_argument("--senders", default="0,1,2,3", help="comma-separated sender accounts")
    ap.add_argument("--receiver", type=int, default=40)
    ap.add_argument("--tx-per-block", type=int, default=12000)
    ap.add_argument("--blocks", type=int, default=500)
    ap.add_argument("--start-block", type=int, default=4190)
    ap.add_argument("--amount", type=float, default=0.001, help="AUGE per transfer")
    ap.add_argument("--endpoints", default=",".join(DEFAULT_ENDPOINTS),
                    help="comma-separated validator RPC endpoints")
    ap.add_argument("--confirm-timeout", type=int, default=180,
                    help="seconds to wait for a block's ops to confirm")
    args = ap.parse_args()

    # Keep each run self-contained so the dashboard never mixes old runs.
    try:
        with open(LOG_PATH, "w", encoding="utf-8"):
            pass
    except OSError as exc:
        sys.exit(f"cannot initialize stress log {LOG_PATH}: {exc}")

    senders = [int(x) for x in args.senders.split(",") if x.strip()]
    if len(senders) < 1:
        sys.exit("at least one sender is required")
    endpoints = [e for e in args.endpoints.split(",") if e.strip()]
    amount_augesat = int(round(args.amount * AUGESAT_PER_AUGE))
    receiver = args.receiver
    per_sender = args.tx_per_block // len(senders)
    remainder = args.tx_per_block % len(senders)

    if amount_augesat < 1:
        sys.exit("amount too small")
    if args.tx_per_block > 40_000:
        log(f"WARNING: tx-per-block {args.tx_per_block} exceeds mempool max_operations 40000")

    keys = {sid: derive_ed25519(validator_seed(sid), 0) for sid in senders}
    clients = {sid: RpcClient(endpoints[i % len(endpoints)]) for i, sid in enumerate(senders)}

    log(f"senders={senders} receiver={receiver} amount={fmt_auge(amount_augesat)} AUGE")
    log(f"tx-per-block={args.tx_per_block} blocks={args.blocks} start-block={args.start_block}")
    log(f"per-sender={per_sender} (+{remainder} spread) endpoints={endpoints}")

    # ── Wait for `start_block - 1` to commit ──────────────────────────
    # The first batch must land in `start_block` itself. Wait until block
    # `start_block - 1` is committed, then fire immediately so the ops enter
    # the mempool during `start_block`'s window and are sealed into block
    # `start_block` at the next commit.
    height = clients[senders[0]].get_height()
    log(f"current height={height}; waiting for block {args.start_block - 1}...")
    while height < args.start_block - 1:
        time.sleep(1)
        height = clients[senders[0]].get_height()
    log(f"reached block {height}; first batch targets block {args.start_block}.")

    def worker_block(sid, count):
        """Submit one ordered batch for a sender."""
        client = clients[sid]
        key = keys[sid]
        nonce = client.get_account(sid)["n_operation"]
        operations = []
        for offset in range(count):
            stripped = serialize_transfer_stripped(
                sid, nonce + offset, receiver, amount_augesat
            )
            operations.append(full_signed(stripped, sign(key, stripped)))
        try:
            result = client.send_operations(operations)
            accepted = int(result.get("accepted", 0))
            rejected = int(result.get("rejected", count - accepted))
        except Exception:
            accepted = 0
            rejected = count
        return sid, accepted, rejected

    total_accepted = 0
    total_rejected = 0
    expected = {sid: clients[sid].get_account(sid)["n_operation"] for sid in senders}

    for blk in range(args.blocks):
        target = height + 1

        # Assign each sender its share; the first `remainder` senders take +1.
        counts = {sid: per_sender + (1 if i < remainder else 0) for i, sid in enumerate(senders)}

        t0 = time.time()
        with ThreadPoolExecutor(max_workers=len(senders)) as ex:
            futures = [ex.submit(worker_block, sid, counts[sid]) for sid in senders]
            for fut in as_completed(futures):
                sid, acc, rej = fut.result()
                expected[sid] += acc
                total_accepted += acc
                total_rejected += rej

        submitted = sum(counts.values())
        elapsed = time.time() - t0
        log(f"block target={target}: submitted {submitted} in {elapsed:.1f}s "
            f"(admitted {total_accepted}, rejected {total_rejected})")

        # ── Wait for confirmation: all senders' nonces reach expected ──
        deadline = time.time() + args.confirm_timeout
        while time.time() < deadline:
            confirmed = all(
                clients[sid].get_account(sid)["n_operation"] >= expected[sid]
                for sid in senders
            )
            if confirmed:
                height += 1
                log(f"block target={target}: all ops confirmed (block {target} committed).")
                break
            time.sleep(1)
        else:
            log(f"WARNING: confirmation timeout for block target={target} "
                f"(expected nonces {expected}).")
            height = clients[senders[0]].get_height()

    log(f"DONE. total admitted={total_accepted} rejected={total_rejected} "
        f"across {args.blocks} blocks.")


if __name__ == "__main__":
    main()
