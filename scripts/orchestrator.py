#!/usr/bin/env python3
"""
AUGECOIN testnet — transaction orchestrator (real on-chain operations).

Orchestrates real AUGE transfers between the 4 dev-validator accounts and
valdeir's account, plus a one-time AUGEID donation to valdeir.

What it does:
  1. Derives the 4 deterministic dev-validator Ed25519 keys (same scheme as
     augecoin-node `create_dev_validator_set_and_keys`: HKDF-SHA3-512).
  2. Reads valdeir's public key (from the wallet-web SQLite DB, or --valdeir-key).
  3. Donates one Reserved AUGEID owned by validator #0 to valdeir
     (GiftAccount -> GiftPending; valdeir accepts in the wallet).
  4. Loops: every block, submits N real AUGE transfers (default 10) between
     the validators (and to valdeir once the gift is accepted), using correct
     per-account n_operation chaining.

Consensus is never touched: every op is signed client-side and admitted by the
node. No private key leaves this process; valdeir's key is never needed here
(we only send *to* valdeir's account and gift *to* valdeir's public key).

Usage:
  python3 orchestrator.py --donate-only            # just donate an AUGEID
  python3 orchestrator.py --rounds 5               # donate + 5 blocks of tx
  python3 orchestrator.py --continuous             # donate + loop forever
"""

import argparse
import hashlib
import hmac
import json
import os
import sqlite3
import sys
import time
import urllib.request

from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey

CHAIN_ID = 2
DECIMALS = 8
AUGESAT_PER_AUGE = 10 ** DECIMALS
MIN_FEE_AUGESAT = 1_000
VALIDATOR_COUNT = 4
VALDEIR_DEFAULT_KEY = "6a6ee5e38727f8890ec6a470d21564afe4308effbe03b76d24e7daa6d7d21503"
PLATFORM_DB = "/opt/augecoin/apps/wallet-web/server/data/wallet-web.sqlite"

RPC_URL = "https://www.augeco.in/rpc"


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

    def u8(self, v): self.buf.append(v & 0xFF)
    def u16(self, v): self.buf += int(v).to_bytes(2, "big")
    def u32(self, v): self.buf += int(v).to_bytes(4, "big")
    def u64(self, v): self.buf += int(v).to_bytes(8, "big")
    def fixed(self, b): self.buf += bytes(b)
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
    w.u64(sender); w.u64(n_operation); w.u64(amount); w.bytes(b"")
    w.u32(1)
    w.u64(receiver); w.u64(amount); w.bytes(b"")
    w.u32(0)
    w.u64(fee)
    w.u64(CHAIN_ID)
    return w.finish()


def serialize_gift_stripped(account, n_operation, recipient_pubkey, fee=MIN_FEE_AUGESAT):
    w = Writer()
    w.u8(0x0D)
    w.u64(account); w.u64(n_operation)
    w.fixed(bytes.fromhex(recipient_pubkey))
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


# ── JSON-RPC client ────────────────────────────────────────────────────
class RpcError(Exception):
    pass


def rpc(method, params=None, retries=3):
    payload = {"jsonrpc": "2.0", "id": 1, "method": method, "params": params or {}}
    data = json.dumps(payload).encode()
    last = None
    for _ in range(retries):
        try:
            req = urllib.request.Request(
                RPC_URL, data=data, headers={"Content-Type": "application/json"}, method="POST"
            )
            with urllib.request.urlopen(req, timeout=20) as resp:
                body = json.loads(resp.read().decode())
            if "error" in body:
                raise RpcError(body["error"].get("message", str(body["error"])))
            return body.get("result", body)
        except RpcError:
            raise
        except Exception as e:  # noqa: BLE001
            last = e
            time.sleep(0.5)
    raise RpcError(f"rpc {method} failed: {last}")


def get_account(acct):
    return rpc("getaccount", {"account_number": acct})


def get_height():
    return int(rpc("nodestatus", {}).get("current_height", 0))


def wait_for_confirmations(expected_nonces, timeout=120):
    """Wait until every accepted operation is reflected on-chain."""
    deadline = time.time() + timeout
    while time.time() < deadline:
        current = {i: get_account(i)["n_operation"] for i in expected_nonces}
        if all(current[i] >= expected for i, expected in expected_nonces.items()):
            return current
        time.sleep(0.5)
    raise RpcError(
        "confirmation timeout: expected nonces "
        f"{expected_nonces}, current={current}"
    )


def list_inventory(pubkey_hex):
    return rpc("listvalidatorinventory", {"validator_public_key_hex": pubkey_hex})


# ── Operations ─────────────────────────────────────────────────────────
def submit_transfer(key, sender, n_operation, receiver, amount):
    stripped = serialize_transfer_stripped(sender, n_operation, receiver, amount, MIN_FEE_AUGESAT)
    return rpc("sendoperation", {"hex": full_signed(stripped, sign(key, stripped))})


def submit_gift(key, account, n_operation, recipient_pubkey):
    stripped = serialize_gift_stripped(account, n_operation, recipient_pubkey)
    return rpc("giftaccount", {
        "account": account,
        "n_operation": n_operation,
        "recipient_public_key_hex": recipient_pubkey,
        "fee": MIN_FEE_AUGESAT,
        "signature_hex": sign(key, stripped).hex(),
    })


# ── valdeir's public key ───────────────────────────────────────────────
def load_valdeir_key(override=None):
    if override:
        return override.strip()
    if os.path.exists(PLATFORM_DB):
        try:
            con = sqlite3.connect(f"file:{PLATFORM_DB}?mode=ro", uri=True)
            row = con.execute(
                "SELECT public_key_hex FROM platform_users WHERE email = ?",
                ("valdeir1986@gmail.com",),
            ).fetchone()
            con.close()
            if row and row[0]:
                return row[0]
        except Exception as e:  # noqa: BLE001
            print(f"[orchestrator] could not read platform DB: {e}", file=sys.stderr)
    return VALDEIR_DEFAULT_KEY


def fmt_auge(augesat):
    return f"{augesat / AUGESAT_PER_AUGE:.8f}".rstrip("0").rstrip(".")


# ── Donation / activation ──────────────────────────────────────────────
def donate_augeid(keys, valdeir_pubkey):
    v0 = keys[0]
    inv = list_inventory(v0.public_key().public_bytes_raw().hex())
    reserved = inv.get("reserved", [])
    if not reserved:
        print("[orchestrator] validator #0 has no Reserved AUGEID to donate yet.")
        return None
    target = min(reserved)
    acc = get_account(target)
    res = submit_gift(v0, target, acc["n_operation"], valdeir_pubkey)
    if res.get("accepted"):
        print(f"[orchestrator] AUGEID #{target} donated to valdeir "
              f"({valdeir_pubkey[:12]}…) -> GiftPending (accept in wallet).")
        return target
    print(f"[orchestrator] donation failed: {res.get('error')}", file=sys.stderr)
    return None


def activate_augeid(keys, valdeir_pubkey):
    """Activate a Reserved AUGEID directly for `pubkey` via createaccount
    (admin-signed server-side). No gift/accept dance — the account becomes
    Owned by valdeir immediately."""
    v0 = keys[0]
    inv = list_inventory(v0.public_key().public_bytes_raw().hex())
    reserved = inv.get("reserved", [])
    if not reserved:
        print("[orchestrator] validator #0 has no Reserved AUGEID to activate yet.")
        return None
    target = min(reserved)
    res = rpc("createaccount", {"account_number": target, "public_key_hex": valdeir_pubkey})
    if res.get("accepted"):
        print(f"[orchestrator] AUGEID #{target} activated for valdeir "
              f"({valdeir_pubkey[:12]}…) -> Owned (no accept needed).")
        return target
    print(f"[orchestrator] activation failed: {res.get('error')}", file=sys.stderr)
    return None


# ── Main ───────────────────────────────────────────────────────────────
def main():
    global RPC_URL
    ap = argparse.ArgumentParser(description="AUGECOIN testnet transaction orchestrator")
    ap.add_argument("--rpc", default=RPC_URL)
    ap.add_argument("--transactions-per-block", type=int, default=10)
    ap.add_argument("--amount", type=float, default=0.01, help="AUGE per transfer")
    ap.add_argument("--valdeir-key", default=None)
    ap.add_argument("--valdeir-account", type=int, default=None,
                    help="use an existing AUGEID as valdeir's account (skip donation)")
    ap.add_argument("--donate-only", action="store_true")
    ap.add_argument("--activate", action="store_true", help="activate an AUGEID directly for valdeir (no accept needed)")
    ap.add_argument("--continuous", action="store_true")
    ap.add_argument("--rounds", type=int, default=1, help="blocks to run (--continuous overrides)")
    args = ap.parse_args()

    RPC_URL = args.rpc

    amount_augesat = int(round(args.amount * AUGESAT_PER_AUGE))
    keys = [derive_ed25519(validator_seed(i), 0) for i in range(VALIDATOR_COUNT)]
    valdeir_pubkey = load_valdeir_key(args.valdeir_key)

    print("[orchestrator] validator accounts:")
    for i, k in enumerate(keys):
        print(f"  #{i}  {k.public_key().public_bytes_raw().hex()}")
    print(f"[orchestrator] valdeir pubkey: {valdeir_pubkey}")
    print(f"[orchestrator] amount/transfer: {fmt_auge(amount_augesat)} AUGE")

    donated = None
    if args.activate:
        valdeir_account = activate_augeid(keys, valdeir_pubkey)
        if valdeir_account is not None:
            print(f"[orchestrator] activated AUGEID #{valdeir_account} for valdeir; link it in the wallet registry.")
        return
    if args.valdeir_account is not None:
        valdeir_account = int(args.valdeir_account)
        print(f"[orchestrator] valdeir account: #{valdeir_account} (using existing AUGEID)")
    else:
        donated = donate_augeid(keys, valdeir_pubkey)
        if args.donate_only:
            return
        valdeir_account = None
        if donated is not None:
            try:
                if get_account(donated)["state"] in ("Owned", "Normal"):
                    valdeir_account = donated
            except RpcError:
                pass

    total_rounds = 0 if args.continuous else args.rounds
    rounds_done = 0

    try:
        height = get_height()
        while True:
            # Synchronize each round to a committed block (block time ~10s).
            while True:
                h = get_height()
                if h > height:
                    height = h
                    break
                time.sleep(0.4)

            if valdeir_account is None and donated is not None:
                try:
                    if get_account(donated)["state"] in ("Owned", "Normal"):
                        valdeir_account = donated
                        print(f"[orchestrator] valdeir accepted AUGEID #{donated}; including it in transfers.")
                except RpcError:
                    pass

            nonces = {i: get_account(i)["n_operation"] for i in range(VALIDATOR_COUNT)}
            expected_nonces = dict(nonces)
            ok = 0
            for i in range(args.transactions_per_block):
                sender_id = i % VALIDATOR_COUNT
                if valdeir_account is not None and i % 5 == 4:
                    receiver = valdeir_account
                else:
                    receiver = (sender_id + 1 + (i // VALIDATOR_COUNT)) % VALIDATOR_COUNT

                try:
                    res = submit_transfer(keys[sender_id], sender_id, nonces[sender_id], receiver, amount_augesat)
                except RpcError:
                    nonces = {i: get_account(i)["n_operation"] for i in range(VALIDATOR_COUNT)}
                    try:
                        res = submit_transfer(keys[sender_id], sender_id, nonces[sender_id], receiver, amount_augesat)
                    except RpcError as e2:
                        print(f"[orchestrator] tx failed #{sender_id}->{receiver}: {e2}")
                        continue

                if res.get("accepted"):
                    nonces[sender_id] += 1
                    expected_nonces[sender_id] = nonces[sender_id]
                    ok += 1
                else:
                    print(f"[orchestrator] tx rejected #{sender_id}->{receiver}: {res.get('error')}")
                    nonces = {i: get_account(i)["n_operation"] for i in range(VALIDATOR_COUNT)}

            print(f"[orchestrator] block #{height}: {ok}/{args.transactions_per_block} transfers admitted.")
            if ok:
                wait_for_confirmations(expected_nonces)
                print(f"[orchestrator] block #{height}: all {ok} transfers confirmed.")
            rounds_done += 1

            if total_rounds and rounds_done >= total_rounds:
                break
    except KeyboardInterrupt:
        print("\n[orchestrator] stopped.")


if __name__ == "__main__":
    main()
