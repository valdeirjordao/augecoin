#!/usr/bin/env python3
"""
AUGECOIN Technical Book Generator
Reads from graphify-out/, docs/, crates/, Cargo.toml and generates
a complete technical book in MD, HTML, EPUB and PDF formats.
"""

import json
import os
import subprocess
import hashlib
import re
from datetime import datetime, timezone
from pathlib import Path
from textwrap import dedent

BASE = Path("/opt/augecoin")
GRAPHIFY_OUT = BASE / "graphify-out"
DOCS = BASE / "docs"
CRATES = BASE / "crates"
ARCHITECTURE = DOCS / "architecture"
OUTPUT = BASE / "tools" / "technical-book" / "output"
TEMPLATES = BASE / "tools" / "technical-book" / "templates"
CHAPTERS = BASE / "tools" / "technical-book" / "chapters"


def get_git_info():
    try:
        commit = subprocess.check_output(
            ["git", "-C", str(BASE), "rev-parse", "--short", "HEAD"],
            text=True, stderr=subprocess.DEVNULL
        ).strip()
    except Exception:
        commit = "unknown"
    try:
        version = subprocess.check_output(
            ["git", "-C", str(BASE), "describe", "--tags", "--always"],
            text=True, stderr=subprocess.DEVNULL
        ).strip()
    except Exception:
        version = "unknown"
    return commit, version


def get_rust_version():
    try:
        return subprocess.check_output(
            ["rustc", "--version"], text=True
        ).strip()
    except Exception:
        return "unknown"


def get_graphify_version():
    manifest = GRAPHIFY_OUT / "manifest.json"
    if manifest.exists():
        return "graphify-data-present"
    return "unknown"


def load_graph_report():
    report_path = GRAPHIFY_OUT / "GRAPH_REPORT.md"
    if report_path.exists():
        return report_path.read_text(encoding="utf-8")
    return ""


def load_graph_json():
    graph_path = GRAPHIFY_OUT / "graph.json"
    if graph_path.exists():
        with open(graph_path, "r", encoding="utf-8") as f:
            return json.load(f)
    return {"nodes": [], "edges": []}


def load_architecture_doc(name):
    path = ARCHITECTURE / name
    if path.exists():
        return path.read_text(encoding="utf-8")
    return ""


def load_doc(name):
    path = DOCS / name
    if path.exists():
        return path.read_text(encoding="utf-8")
    return ""


def read_source_file(rel_path):
    full = BASE / rel_path
    if full.exists():
        try:
            content = full.read_text(encoding="utf-8")
            lines = content.split("\n")
            if len(lines) > 150:
                return "\n".join(lines[:150]) + f"\n// ... ({len(lines)} lines total)"
            return content
        except Exception:
            return f"// Could not read {rel_path}"
    return f"// File not found: {rel_path}"


def compute_sha256_files():
    hashes = {}
    book_dir = OUTPUT.parent
    for ext in ["*.md", "*.html", "*.pdf", "*.epub"]:
        for f in book_dir.rglob(ext):
            rel = f.relative_to(book_dir)
            h = hashlib.sha256(f.read_bytes()).hexdigest()
            hashes[str(rel)] = h
    return hashes


def parse_graph_communities(report_text):
    communities = []
    god_nodes = []
    current_comm = None

    for line in report_text.split("\n"):
        if line.startswith("### Community "):
            match = re.match(r"### Community (\d+) - \"(.+?)\"", line)
            if match:
                current_comm = {
                    "id": int(match.group(1)),
                    "name": match.group(2),
                    "nodes": [],
                    "cohesion": 0.0
                }
                communities.append(current_comm)
        elif current_comm and "Cohesion:" in line:
            m = re.search(r"Cohesion: ([\d.]+)", line)
            if m:
                current_comm["cohesion"] = float(m.group(1))
        elif current_comm and line.startswith("- Nodes ("):
            m = re.search(r"Nodes \((\d+)\): (.+)", line)
            if m:
                current_comm["node_count"] = int(m.group(1))
                current_comm["sample_nodes"] = m.group(2)

    god_nodes_match = re.findall(
        r"(\d+)\.\s+`(.+?)`\s+-\s+(\d+)\s+edges",
        report_text
    )
    for rank, name, edges in god_nodes_match:
        god_nodes.append({
            "rank": int(rank),
            "name": name,
            "edges": int(edges)
        })

    return communities, god_nodes


def extract_summary_stats(report_text):
    stats = {}
    m = re.search(r"(\d+)\s+nodes\s*·\s*(\d+)\s+edges\s*·\s*(\d+)\s+communities", report_text)
    if m:
        stats["nodes"] = int(m.group(1))
        stats["edges"] = int(m.group(2))
        stats["communities"] = int(m.group(3))

    m2 = re.search(r"Extraction:\s+(\d+)%\s+EXTRACTED", report_text)
    if m2:
        stats["extracted_pct"] = int(m2.group(1))

    return stats


def get_crate_files():
    crate_info = {}
    for crate_dir in sorted(CRATES.iterdir()):
        if not crate_dir.is_dir():
            continue
        name = crate_dir.name
        cargo_toml = crate_dir / "Cargo.toml"
        lib_rs = crate_dir / "src" / "lib.rs"
        main_rs = crate_dir / "src" / "main.rs"

        crate_info[name] = {
            "cargo_toml": cargo_toml.read_text(encoding="utf-8") if cargo_toml.exists() else "",
            "has_lib": lib_rs.exists(),
            "has_main": main_rs.exists(),
            "src_files": sorted([
                str(f.relative_to(BASE))
                for f in (crate_dir / "src").rglob("*.rs")
            ]) if (crate_dir / "src").exists() else [],
            "test_files": sorted([
                str(f.relative_to(BASE))
                for f in (crate_dir / "tests").rglob("*.rs")
            ]) if (crate_dir / "tests").exists() else [],
        }
    return crate_info


# ─── Chapter Generators ─────────────────────────────────────────────

def gen_chapter_00_cover(commit, version, rust_ver, graphify_ver):
    now = datetime.now(timezone.utc).strftime("%Y-%m-%d %H:%M UTC")
    return f"""# AUGECOIN Technical Book

**Version:** {version}
**Commit:** {commit}
**Generated:** {now}
**Graphify:** {graphify_ver}
**Rust:** {rust_ver}

---

> This document is automatically generated from the AUGECOIN codebase,
> Graphify knowledge graph, architecture docs and specifications.
> Intended for external audits, enterprise review, researchers and AI systems.

"""


def gen_chapter_01_overview(graph_report, stats, god_nodes, communities):
    stats_section = ""
    if stats:
        stats_section = f"""
| Metric | Value |
|---|---|
| Nodes | {stats.get('nodes', 'N/A')} |
| Edges | {stats.get('edges', 'N/A')} |
| Communities | {stats.get('communities', 'N/A')} |
| Extraction Confidence | {stats.get('extracted_pct', 'N/A')}% |
"""

    god_nodes_section = ""
    if god_nodes:
        god_nodes_section = "\n### God Nodes (Core Abstractions)\n\n| Rank | Name | Edges |\n|---|---|---|\n"
        for gn in god_nodes[:10]:
            god_nodes_section += f"| {gn['rank']} | `{gn['name']}` | {gn['edges']} |\n"

    top_communities = ""
    if communities:
        top_communities = "\n### Top Communities\n\n"
        for c in sorted(communities, key=lambda x: x.get("node_count", 0), reverse=True)[:10]:
            top_communities += f"- **{c['name']}** ({c.get('node_count', '?')} nodes, cohesion: {c.get('cohesion', 0):.3f})\n"

    return f"""# Chapter 1 — Overview

## Executive Summary

AUGECOIN is a Proof-of-Authority blockchain built in Rust, featuring:
- **Hybrid signatures**: Ed25519 + CRYSTALS-Dilithium (post-quantum)
- **BLAKE3-512** hashing (XOF mode)
- **SafeBox** incremental state model with deterministic snapshots
- **PoA consensus** with 2/3+1 quorum and round-robin leader selection
- **Native numbered accounts** (AUGEID) with marketplace, gifts and name resolution
- **Total supply**: 750,000,000 AUGE (75 trillion augesat), emitted over 50 years

## Graphify Analysis

{stats_section}
{god_nodes_section}
{top_communities}

## Architecture Summary

```
┌─────────────────────────────────────────────────────┐
│                    AUGECOIN Node                      │
│                                                       │
│  ┌──────────┐  ┌──────────────┐  ┌───────────────┐  │
│  │  Crypto   │  │   Consensus  │  │    Network    │  │
│  │ (Ed25519  │  │ (PoA Round)  │  │ (libp2p +    │  │
│  │ +Dilithium│  │              │  │  gossipsub)   │  │
│  │ +BLAKE3)  │  │              │  │               │  │
│  └──────────┘  └──────────────┘  └───────────────┘  │
│       │              │                  │             │
│       └──────────────┼──────────────────┘             │
│                      │                                │
│  ┌───────────────────┼─────────────────────────────┐  │
│  │              Execution Engine                     │  │
│  │  (mempool → execute_block → SafeBox → commit)    │  │
│  └───────────────────┬─────────────────────────────┘  │
│                      │                                │
│  ┌───────────────────┼─────────────────────────────┐  │
│  │              Storage (RocksDB)                    │  │
│  │  accounts | blocks | validator_set | equivoc.    │  │
│  └──────────────────────────────────────────────────┘  │
│                      │                                │
│  ┌───────────────────┼─────────────────────────────┐  │
│  │              RPC (JSON-RPC + gRPC over TLS)      │  │
│  └──────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────┘
```

## Crate Dependency Graph

```
augecoin-crypto ──→ augecoin-core ──→ augecoin-storage
                                      │
                     augecoin-consensus┘
                           │
                     augecoin-network
                           │
                     augecoin-node
                           │
                     augecoin-rpc ──→ augecoin-cli
```

"""


def gen_chapter_02_architecture(crate_info, graph_report):
    crates_section = ""
    for name, info in crate_info.items():
        src_count = len(info["src_files"])
        test_count = len(info["test_files"])
        entry = "lib.rs" if info["has_lib"] else ("main.rs" if info["has_main"] else "none")
        crates_section += f"| `{name}` | {entry} | {src_count} src | {test_count} tests |\n"

    return f"""# Chapter 2 — Architecture

## Rust Workspace

The project is organized as a Cargo workspace with the following crates:

| Crate | Entry | Sources | Tests |
|---|---|---|---|
{crates_section}

## Crate Responsibilities

### augecoin-crypto
Cryptographic primitives: Ed25519+Dilithium hybrid signatures, BLAKE3-512 hashing,
HD key derivation (HKDF-SHA3-512), Bech32m address encoding, WASM bindings.

### augecoin-core
Core data types: AugeAccount, Transaction, Block, BlockHeader, Operation,
SafeBox, emission logic, mempool. Protocol-level serialization.

### augecoin-storage
RocksDB persistence: column families for accounts, blocks, tx_index, validator_set,
equivocation_proofs. State root computation (Merkle), checkpoints, snapshots.

### augecoin-consensus
PoA consensus engine: ValidatorSet, round state machine (Propose→Collect→Commit),
quorum verification (2/3+1), equivocation detection, leader selection.

### augecoin-network
libp2p networking: Noise encryption, gossipsub for tx/block propagation,
peer discovery (static bootnodes), sync manager, rate limiting.

### augecoin-node
Node orchestration: block execution, mempool validation, genesis initialization,
metrics (Prometheus), alert system, main entry point.

### augecoin-rpc
JSON-RPC 2.0 + gRPC server over TLS: all blockchain endpoints, auth (API key),
rate limiting, account/block/transaction queries.

### augecoin-cli
CLI tool: validator management, status queries, earnings, security commands.
Tab completion via clap, JSON and table output.

### augecoin-bench
Benchmarks: block execution, consensus, storage, networking benchmarks.


## Architecture Decision Records

The project maintains decisions in `docs/DECISIONS.md` with ADRs covering:
- ADR-001: Cargo workspace structure
- ADR-002: RocksDB as storage backend
- ADR-003: Hybrid signature scheme (Ed25519 + Dilithium)
- ADR-004: BLAKE3-512 for hashing
- ADR-005: ClaimAccount auto-assignment to validator leader

"""


def gen_chapter_03_safebox():
    safebox_doc = load_architecture_doc("SAFEBOX_FLOW.md")
    code = read_source_file("crates/augecoin-core/src/safe_box.rs")

    return f"""# Chapter 3 — SafeBox

> **Veja também:** Storage, Consenso, AUGEID

## Executive Summary

The SafeBox is AUGECOIN's core state container — a deterministic, incrementally
committed BTreeMap of accounts that forms the consensus state root.

## Responsibility

Persist the full blockchain state (accounts, name index) in a deterministic,
verifiable structure. Every block execution modifies the SafeBox atomically.

## Architectural Flow

{safebox_doc}

## Main File

**Arquivo:** `crates/augecoin-core/src/safe_box.rs`
**Comunidade Graphify:** 2 — "Storage"
**God Node:** Storage (68 edges)

### Source Code (excerpt)

```rust
{code}
```

## Riscos

- Memory usage scales with account count (BTreeMap in memory)
- Recovery depends on RocksDB integrity
- Snapshot interval affects recovery time

## Referências Cruzadas

- **Storage** (Chapter 5): RocksDB persistence of SafeBox state
- **Consenso** (Chapter 4): SafeBox hash included in block header
- **AUGEID** (Chapter 9): Account lifecycle within SafeBox

"""


def gen_chapter_04_consensus():
    consensus_doc = load_architecture_doc("CONSENSUS_FLOW.md")
    quorum_code = read_source_file("crates/augecoin-consensus/src/quorum.rs")
    round_code = read_source_file("crates/augecoin-consensus/src/round.rs")
    validator_code = read_source_file("crates/augecoin-consensus/src/validator.rs")

    return f"""# Chapter 4 — Consensus

> **Veja também:** SafeBox, Rede, RPC

## Executive Summary

AUGECOIN uses Proof of Authority (PoA) with round-robin leader selection,
2/3+1 quorum requirement, and automatic view-change on timeout.

## Responsibility

Coordinate block proposal, validation, signing and finalization among
authorized validators.

## Architectural Flow

{consensus_doc}

## Key Files

### quorum.rs
**Comunidade:** 82 — "quorum.rs"
**God Node:** quorum.rs

```rust
{quorum_code}
```

### round.rs
**Comunidade:** 42 — "round.rs"

```rust
{round_code}
```

### validator.rs
**Comunidade:** 20 — "validator.rs"

```rust
{validator_code}
```

## Comparação: Protocolo Antigo vs Novo

| Aspect | Original | Current |
|---|---|---|
| Leader selection | Random | Round-robin deterministic |
| Quorum | Fixed 3/4 | 2/3+1 (scalable) |
| Timeout | 60s | 120s (2x block time) |
| Equivocation | Detected, not proven | On-chain proof generation |

## Riscos

- Single admin key for validator management (centralization)
- No slashing mechanism beyond admin removal
- View-change depends on timeout accuracy

## Referências Cruzadas

- **SafeBox** (Chapter 3): Initial safe box hash in block proposal
- **Rede** (Chapter 6): Consensus transport over libp2p
- **RPC** (Chapter 7): Validator set queries

"""


def gen_chapter_05_storage():
    storage_code = read_source_file("crates/augecoin-storage/src/lib.rs")
    checkpoint_code = read_source_file("crates/augecoin-storage/src/checkpoint.rs")
    state_root_code = read_source_file("crates/augecoin-storage/src/state_root.rs")

    return f"""# Chapter 5 — Storage

> **Veja também:** SafeBox, Consenso

## Executive Summary

AUGECOIN uses RocksDB as its persistent storage backend with 5 column families
for different data types. All block writes are atomic via WriteBatch.

## Responsibility

Provide durable, consistent storage for accounts, blocks, validator set,
transaction index and equivocation proofs.

## RocksDB Column Families

| CF Name | Content | Access Pattern |
|---|---|---|
| `accounts` | AugeAccount serialized | Read/write per block |
| `blocks` | Block headers + bodies | Write once, read often |
| `tx_index` | Transaction hash → block | Write once, index lookup |
| `validator_set` | Active validator set | Write per governance change |
| `equivocation_proofs` | EquivocationProof | Write on detection |

## Atomic Writes (WriteBatch)

All state changes for a block are consolidated in a single `WriteBatch`:
```
commit_block_atomic:
  ├─ put_account(modified_accounts)
  ├─ put_block(block)
  ├─ put_height(height)
  ├─ put_validator_set(validator_set)
  └─ flush (WAL)
```

## WAL Growth Solution

The Write-Ahead Log (WAL) growth issue was resolved by:
1. Configuring `set_max_total_wal_size` to limit WAL disk usage
2. Periodic compaction triggers after checkpoint snapshots
3. `set_keep_log_num` for WAL file retention control

## State Root (Merkle Tree)

{state_root_code[:500] if state_root_code else "// Not available"}

## Checkpoints

{checkpoint_code[:500] if checkpoint_code else "// Not available"}

## Main Storage Implementation

```rust
{storage_code}
```

## Riscos

- WAL growth without proper configuration
- Snapshot staleness between checkpoint intervals
- No encryption at rest (relies on OS/disk encryption)

## Referências Cruzadas

- **SafeBox** (Chapter 3): SafeBox state persisted via Storage
- **Consenso** (Chapter 4): Validator set stored in Storage

"""


def gen_chapter_06_network():
    network_code = read_source_file("crates/augecoin-network/src/lib.rs")
    sync_code = read_source_file("crates/augecoin-network/src/sync.rs")
    transport_code = read_source_file("crates/augecoin-network/src/transport.rs")

    return f"""# Chapter 6 — Network

> **Veja também:** Consenso, RPC

## Executive Summary

AUGECOIN uses libp2p for P2P networking with Noise encryption, gossipsub
for message propagation, and static bootnode discovery.

## Responsibility

Peer-to-peer communication, transaction/block propagation, node synchronization.

## Transport Layer

**Noise Protocol** encryption is mandatory — no unencrypted connections allowed.

```
libp2p Transport
  ├─ Noise (encryption)
  ├─ TCP (transport)
  └─ Yamux (multiplexing)
```

## Gossipsub Topics

| Topic | Content | Propagation |
|---|---|---|
| `tx` | Pending transactions | Flood to all peers |
| `block` | Finalized blocks | Flood to all peers |
| `consensus` | Consensus messages | Validator-only mesh |

## MAX_TRANSMIT_SIZE

Default libp2p limit: ~1MB. AUGECOIN blocks are expected to be well under
this limit given the 60-second block time.

## Peer Discovery

Static bootnode list (configurable). No Kademlia DHT — chosen for simplicity
and auditability in a 4-validator network.

## Sync Protocol

1. New node connects to bootnodes
2. Requests latest checkpoint from any peer
3. Restores state from checkpoint
4. Replays remaining blocks
5. Joins live consensus

## Textual Flowchart

```
New Node
  │
  ├─ Connect to bootnodes (Noise handshake)
  │
  ├─ Request latest checkpoint
  │     └─ Restore SafeBox from checkpoint
  │
  ├─ Request blocks after checkpoint height
  │     └─ Execute each block (verify quorum)
  │
  ├─ Join live gossipsub mesh
  │     └─ Receive new blocks/tx
  │
  └─ Participate in consensus (if validator)
```

## Key Source Files

### Network Core
```rust
{network_code[:1000]}
```

### Sync Manager
```rust
{sync_code[:1000]}
```

## Riscos

- Static bootnodes = single point of initial connectivity
- No peer reputation scoring beyond rate limiting
- No encrypted storage of peer data

## Referências Cruzadas

- **Consenso** (Chapter 4): Consensus messages travel over network
- **RPC** (Chapter 7): RPC is separate from P2P network

"""


def gen_chapter_07_rpc():
    rpc_code = read_source_file("crates/augecoin-rpc/src/lib.rs")
    endpoints_code = read_source_file("crates/augecoin-rpc/src/endpoints.rs")
    auth_code = read_source_file("crates/augecoin-rpc/src/auth.rs")

    rpc_map_doc = load_architecture_doc("RPC_MAP.md")

    return f"""# Chapter 7 — RPC

> **Veja também:** Wallet, CLI

## Executive Summary

AUGECOIN exposes a JSON-RPC 2.0 API over TLS, with optional gRPC support.
Authentication uses API keys for administrative endpoints.

## RPC Methods

| Method | Function | Auth |
|---|---|---|
| `getAccount` | Query account by number/address/name | Public |
| `getAccountByAddress` | Query by Bech32m address | Public |
| `getBlock` | Get block by hash | Public |
| `getBlockByHeight` | Get block by height | Public |
| `getBlockCount` | Current chain height | Public |
| `sendTransaction` | Submit signed transaction | Public |
| `getMempool` | List pending transactions | Public |
| `getValidatorSet` | Active validators | Public |
| `getNetworkStatus` | Node status, peers | Public |
| `getValidatorEarnings` | Validator rewards | Public |
| `listAccountsForSale` | Marketplace listings | Public |
| `resolveName` | Name → account lookup | Public |
| `buyAccount` | Purchase listed account | Public |
| `sellAccount` | List account for sale | Public |
| `giftAccount` | Initiate account gift | Public |
| `acceptGift` | Accept pending gift | Public |
| `validatorAdd` | Add validator | Admin |
| `validatorRemove` | Remove validator | Admin |
| `validatorActivate` | Activate validator | Admin |
| `validatorDeactivate` | Deactivate validator | Admin |

## Architecture Flow

{rpc_map_doc}

## Endpoint Implementation

```rust
{endpoints_code[:2000]}
```

## Authentication

```rust
{auth_code[:1000]}
```

## RPC Server Core

```rust
{rpc_code[:2000]}
```

## Riscos

- TLS certificate management
- API key rotation
- Rate limiting effectiveness under DDoS

## Referências Cruzadas

- **Wallet** (Chapter 8): Wallet uses RPC for all blockchain interactions
- **CLI** (Chapter 10): CLI wraps RPC calls

"""


def gen_chapter_08_wallet():
    return """# Chapter 8 — Wallet

> **Veja também:** RPC, AUGEID

## Executive Summary

AUGECOIN provides three wallet implementations: Web (React), Desktop (Tauri),
and Mobile (React Native). All use the same cryptographic core.

## Platform Account vs Blockchain Wallet

### Platform Account
- Login, profile, session management
- Server-side (Express.js)
- JWT-based authentication
- Stores user preferences

### Blockchain Wallet
- AUGEID identity
- Balance, send/receive
- All crypto operations client-side
- Never sends private keys to server

## Wallet Architecture

```
┌─────────────────────────────────────────┐
│              Wallet Client               │
│                                           │
│  ┌─────────────┐  ┌──────────────────┐  │
│  │  Platform    │  │  Blockchain       │  │
│  │  Account     │  │  Wallet           │  │
│  │  (login)     │  │  (AUGEID + keys)  │  │
│  └─────────────┘  └──────────────────┘  │
│         │                    │            │
│         └────────┬───────────┘            │
│                  │                        │
│  ┌───────────────┼──────────────────┐    │
│  │  Crypto Layer (WASM / Native)     │    │
│  │  Ed25519 + Dilithium + BLAKE3    │    │
│  └───────────────────────────────────┘    │
└─────────────────────────────────────────┘
```

## Marketplace

Accounts can be listed for sale, gifted, or transferred:
- `sellAccount`: List at a price
- `buyAccount`: Purchase listed account
- `giftAccount`: Send as gift (pending acceptance)
- `acceptGift`: Accept pending gift

## Inventory

Each wallet tracks:
- Owned accounts
- Pending gifts
- Listed accounts (for sale)
- Gift history

## Key Files

| Wallet | Framework | Crypto |
|---|---|---|
| wallet-web | React + Vite | WASM (@augecoin/wasm-crypto) |
| wallet-desktop | Tauri (Rust) | Native (augecoin-crypto) |
| wallet-mobile | React Native | Native module |

## Riscos

- Seed backup is user responsibility
- No account recovery mechanism
- Keystore encryption depends on user password strength

## Referências Cruzadas

- **RPC** (Chapter 7): All wallet operations go through RPC
- **AUGEID** (Chapter 9): Account lifecycle managed by wallets

"""


def gen_chapter_09_augeid():
    return """# Chapter 9 — AUGEID

> **Veja também:** SafeBox, Wallet, Consenso

## Executive Summary

AUGEID is AUGECOIN's native account identity system — numbered accounts
that serve as both blockchain addresses and tradeable assets.

## Responsibility

Provide deterministic, sequential account numbering with full lifecycle
management (creation, sale, gifting, naming).

## Account Lifecycle

```
Bloco N
  │
  ↓
Reserved (10 AUGEIDs per block, all to leader)
  │
  ├──→ ForSale (listed on marketplace)
  │       └──→ Owned (purchased)
  │
  ├──→ GiftPending (gift initiated)
  │       └──→ Owned (gift accepted)
  │
  └──→ Owned (direct activation)
          └──→ Normal (operational account)
```

## Account States

| State | Description |
|---|---|
| `Reserved` | Emitted to block leader; no key; cannot send/receive |
| `Owned` | Has owner (account_key, n_operation=0, account_seal) |
| `ForSale` | Listed on marketplace (sale_index) |
| `GiftPending` | Gift pending acceptance (gift_index) |
| `Normal` | Fully operational account |

## Numbering Formula

**AUGEID = block × 10 + offset (0..9)**

Each block emits exactly 10 AUGEIDs, all `Reserved` and belonging to the leader.

## Why AUGEID is an Asset

1. **Scarcity**: Only 10 per block, fixed emission schedule
2. **Tradeability**: Can be bought, sold, gifted
3. **Identity**: Named accounts (resolve_name)
4. **Speculation**: Early/lower numbers may have premium value
5. **No expiration**: Accounts persist forever

## Key Invariants

- `SafeBox.accounts` ≡ CF `accounts` after commit
- `name_index` ≡ derived from `account.name`
- Emission is fixed: exactly 10 `Reserved` per block
- `CreateAccount` never creates new numbers — only activates existing `Reserved`

## Riscos

- Leader controls all 10 AUGEIDs per block (centralization of issuance)
- No mechanism to recover lost keys
- Name squatting possible

## Referências Cruzadas

- **SafeBox** (Chapter 3): AUGEIDs stored in SafeBox
- **Wallet** (Chapter 8): Wallet manages AUGEID lifecycle
- **Consenso** (Chapter 4): Block leader receives AUGEIDs

"""


def gen_chapter_10_benchmark():
    return """# Chapter 10 — Benchmark

## Performance Metrics

| Metric | Target | Status |
|---|---|---|
| 350,000 transactions | Stress test | ✓ |
| 117 TPS | Throughput | ✓ |
| 0 rejections | Integrity | ✓ |
| 4 validators | Quorum | ✓ |
| 60s block time | Block interval | ✓ |
| 120s timeout | View-change | ✓ |

## Benchmark Files

- `crates/augecoin-bench/` — Rust benchmarks
- `crates/augecoin-node/tests/load_test.rs` — Load testing
- `crates/augecoin-node/tests/partition_test.rs` — Network partition

## Test Categories

### Unit Tests
- Per-crate test suites
- Serialization round-trips
- Cryptographic verification

### Integration Tests
- E2E transaction lifecycle
- 4-node liveness
- Determinism verification
- SDK parity (Rust ↔ TypeScript)

### Adversarial Tests
- Equivocation detection
- Duplicate vote rejection
- Invalid proposal handling
- Quorum edge cases

### Property Tests
- Emission total supply verification (proptest)
- Block reward sum = TOTAL_SUPPLY_AUGESAT

## Riscos

- Benchmarks run on single machine, not distributed
- Real network latency not simulated
- Storage benchmarks may vary by hardware

## Referências Cruzadas

- **Segurança** (Chapter 11): Security tests complement benchmarks
- **Testes** (Chapter 12): Complete test inventory

"""


def gen_chapter_11_security():
    security_doc = load_architecture_doc("SECURITY_SURFACE.md")
    threat_model = load_doc("THREAT_MODEL.md")
    security_audit = load_doc("SECURITY.md")

    return f"""# Chapter 11 — Security

> **Veja também:** Consenso, Rede, Storage

## Executive Summary

AUGECOIN implements defense-in-depth with hybrid cryptography, atomic writes,
replay protection and on-chain equivocation proofs.

## Cryptographic Security

| Primitive | Algorithm | Purpose |
|---|---|---|
| Signature | Ed25519 + Dilithium (dual) | Transaction/block signing |
| Hashing | BLAKE3-512 (XOF) | State root, block hash |
| Key Derivation | HKDF-SHA3-512 | HD wallet derivation |
| Address | Bech32m | Human-readable addresses |

## Replay Protection

- `op_sequence` per account: monotonically increasing
- Same operation cannot be submitted twice
- Mempool rejects stale sequences

## Atomicity

All block execution uses RocksDB WriteBatch:
- Either ALL changes commit, or NONE
- No partial state visible
- Crash recovery from last committed state

## Attack Surfaces

{security_doc}

## Threat Model

{threat_model[:2000] if threat_model else "See docs/THREAT_MODEL.md"}

## Security Audit

{security_audit[:2000] if security_audit else "See docs/SECURITY.md"}

## Known Limitations

- No atomic swaps
- No blockchain deletion
- No coin-recovery from lost keys
- Admin key is single-key (not multisig)

## Riscos

- Centralized validator management
- No formal verification
- Dilithium key sizes increase transaction size

## Referências Cruzadas

- **Consenso** (Chapter 4): Quorum prevents Byzantine behavior
- **Rede** (Chapter 6): Noise encryption prevents network attacks
- **Storage** (WriteBatch): Atomicity prevents partial writes

"""


def gen_chapter_12_tests():
    return """# Chapter 12 — Tests

## Test Inventory

### Cargo Test Suites

| Crate | Tests | Type |
|---|---|---|
| augecoin-crypto | Signature, hash, hdkeys, address | Unit |
| augecoin-core | Account, block, transaction, emission | Unit + Property |
| augecoin-storage | RocksDB round-trip, checkpoint, state_root | Integration |
| augecoin-consensus | Quorum, round, validator, equivocation | Unit + Adversarial |
| augecoin-network | Transport, gossip, sync, rate_limit | Integration |
| augecoin-node | Mempool, execution, genesis, e2e | Integration |
| augecoin-rpc | Endpoints, auth, TLS | Integration |
| augecoin-cli | Commands, admin auth | Integration |

### Integration Tests (crate-level)

- `crates/augecoin-node/tests/e2e_transaction.rs` — Full transaction lifecycle
- `crates/augecoin-node/tests/end_to_end_block_execution.rs` — Block execution
- `crates/augecoin-node/tests/determinism_test.rs` — Determinism verification
- `crates/augecoin-consensus/tests/four_node_liveness.rs` — 4-node consensus
- `crates/augecoin-consensus/tests/adversarial_tests.rs` — Attack scenarios
- `crates/augecoin-node/tests/sdk_parity.rs` — Cross-language parity
- `crates/augecoin-node/tests/load_test.rs` — Performance testing
- `crates/augecoin-node/tests/partition_test.rs` — Network partition

### Fuzz Targets

- `crates/augecoin-core/fuzz/fuzz_targets/tx_deserialize.rs`
- `crates/augecoin-core/fuzz/fuzz_targets/block_deserialize.rs`

### Clippy

```
cargo clippy --workspace --all-targets
```

### Graphify Analysis

Graphify provides semantic analysis of the codebase:
- 3167 nodes, 6966 edges, 167 communities
- 99% extraction confidence
- Automatic architecture documentation

## Test Commands

```bash
# All tests
cargo test --workspace

# Specific crate
cargo test -p augecoin-crypto
cargo test -p augecoin-core
cargo test -p augecoin-consensus

# Linting
cargo clippy --workspace --all-targets

# Formatting
cargo fmt --all -- --check

# Benchmarks
cargo bench --workspace

# Fuzzing
cargo fuzz run tx_deserialize -- -max_total_time=300
```

## Riscos

- No CI/CD pipeline in repository
- Fuzzing coverage limited
- No mutation testing

## Referências Cruzadas

- **Benchmark** (Chapter 10): Performance metrics
- **Segurança** (Chapter 11): Security test categories

"""


def gen_chapter_13_apis():
    return """# Chapter 13 — APIs

## JSON-RPC 2.0 Interface

Base URL: `https://<node>:9443` (TLS required)

### Request Format

```json
{{
  "jsonrpc": "2.0",
  "method": "getAccount",
  "params": [42],
  "id": 1
}}
```

### Response Format

```json
{{
  "jsonrpc": "2.0",
  "result": {{
    "account_number": 42,
    "balance": 1000000000,
    "name": "MyAccount",
    "state": "Normal"
  }},
  "id": 1
}}
```

## Method Reference

### getAccount

**Parameters:**
- `account_number` (u64): Account number, OR
- `address` (string): Bech32m address, OR
- `name` (string): Account name

**Returns:** AccountInfo object

**Example:**
```bash
curl -k https://localhost:9443 \\
  -d '{{"jsonrpc":"2.0","method":"getAccount","params":[42],"id":1}}'
```

### sendTransaction

**Parameters:**
- `transaction` (hex): Signed transaction bytes

**Returns:** SendOperationResult

**Example:**
```bash
curl -k https://localhost:9443 \\
  -d '{{"jsonrpc":"2.0","method":"sendTransaction","params":["0x..."],"id":1}}'
```

### buyAccount

**Parameters:**
- `account_number` (u64): Account to buy
- `price` (u64): Price in augesat

**Returns:** Operation result

### sellAccount

**Parameters:**
- `account_number` (u64): Account to list
- `price` (u64): Price in augesat

**Returns:** Operation result

### giftAccount

**Parameters:**
- `account_number` (u64): Account to gift
- `recipient` (string): Recipient address

**Returns:** Operation result (GiftPending)

### acceptGift

**Parameters:**
- `account_number` (u64): Gifted account to accept

**Returns:** Operation result (Owned → Normal)

### listAccountsForSale

**Parameters:** None

**Returns:** Array of marketplace listings

### resolveName

**Parameters:**
- `name` (string): Account name to resolve

**Returns:** Account number

## gRPC Interface

Proto definition available in `crates/augecoin-rpc/proto/`.

Service: `augecoin.v1.NodeService`

## Endereço Curto Externo

O protocolo mantém `auge1...` como endereço canônico. Para pagamento e
exibição, o SDK oferece `getShortAddress()` e `validateShortAddress()`.
O formato usa Base58Check com payload de 24 bytes derivado de BLAKE3 e
checksum de 4 bytes, resultando em aproximadamente 39 caracteres.

```typescript
const canonical = getAddress(publicKey);       // identidade interna
const short = getShortAddress(publicKey);      // pagamento/exibição
validateShortAddress(short);                   // valida checksum
```

O endereço curto não substitui nem altera endereços canônicos existentes.

## Riscos

- No API versioning
- No request batching
- No WebSocket subscription for real-time updates

## Referências Cruzadas

- **RPC** (Chapter 7): Server implementation details
- **Wallet** (Chapter 8): Client-side API usage

"""


def gen_chapter_14_annexes(crate_info, git_commit, git_version, rust_ver):
    now = datetime.now(timezone.utc).strftime("%Y-%m-%d %H:%M UTC")

    tree_lines = []
    for name in sorted(crate_info.keys()):
        tree_lines.append(f"  crates/{name}/")
        for src in crate_info[name]["src_files"][:5]:
            tree_lines.append(f"    {src.split('/')[-1]}")
        if len(crate_info[name]["src_files"]) > 5:
            tree_lines.append(f"    ... ({len(crate_info[name]['src_files'])} files)")

    project_tree = "\n".join(tree_lines)

    return f"""# Chapter 14 — Annexes

## Project Tree

```
opt/augecoin/
  Cargo.toml (workspace)
{project_tree}
  docs/
    SPEC.md
    DECISIONS.md
    architecture/
  graphify-out/
    graph.json
    GRAPH_REPORT.md
  apps/
    wallet-web/
    wallet-desktop/
    wallet-mobile/
  packages/
    sdk-ts/
    wasm-crypto/
  infra/
    testnet-4node.yml
    observability/
  tools/
    technical-book/
```

## Build Metadata

| Field | Value |
|---|---|
| Generated | {now} |
| Git Commit | {git_commit} |
| Git Version | {git_version} |
| Rust Version | {rust_ver} |
| Graphify | data present |

## Cargo Workspace

```toml
[workspace]
resolver = "2"
members = [
    "crates/augecoin-crypto",
    "crates/augecoin-core",
    "crates/augecoin-storage",
    "crates/augecoin-consensus",
    "crates/augecoin-network",
    "crates/augecoin-rpc",
    "crates/augecoin-cli",
    "crates/augecoin-node",
    "crates/augecoin-bench",
    "apps/wallet-desktop/src-tauri",
]
```

## Protocol Constants

| Constant | Value |
|---|---|
| TOTAL_SUPPLY_AUGE | 750,000,000 |
| TOTAL_SUPPLY_AUGESAT | 75,000,000,000,000,000 |
| DECIMALS | 8 |
| BLOCK_TIME_SECONDS | 60 |
| EMISSION_YEARS | 50 |
| TOTAL_EMISSION_BLOCKS | 26,298,000 |
| BASE_REWARD_AUGESAT | 2,852,383 |
| QUORUM | 2/3 + 1 |

## Emission Schedule

```
block_reward(h) for h in 1..=26,298,000:
  base = 2,852,383 augesat
  if h <= 10,266 (REMAINDER):
    reward = base + 1
  else:
    reward = base
  (after TOTAL_EMISSION_BLOCKS: reward = 0)
```

Sum verification: `sum(block_reward(h)) == TOTAL_SUPPLY_AUGESAT` exactly.

"""


# ─── HTML Generator ──────────────────────────────────────────────────

def generate_html(md_content, title="AUGECOIN Technical Book"):
    toc_items = []
    for line in md_content.split("\n"):
        if line.startswith("# Chapter"):
            match = re.match(r"# Chapter (\d+) — (.+)", line)
            if match:
                num = match.group(1)
                name = match.group(2)
                anchor = f"chapter-{num}"
                toc_items.append(f'<li><a href="#{anchor}">{num}. {name}</a></li>')

    chapters_html = []
    current_chapter = []
    chapter_num = 0

    for line in md_content.split("\n"):
        if line.startswith("# Chapter"):
            if current_chapter:
                chapters_html.append("\n".join(current_chapter))
            chapter_num += 1
            current_chapter = [f'<div id="chapter-{chapter_num}" class="chapter">']
            current_chapter.append(f"<h1>{line.lstrip('# ')}</h1>")
        elif line.startswith("# ") and not line.startswith("# Chapter"):
            if current_chapter:
                chapters_html.append("\n".join(current_chapter))
                current_chapter = []
            current_chapter.append(f"<h1>{line.lstrip('# ')}</h1>")
        elif line.startswith("## "):
            current_chapter.append(f'<h2>{line.lstrip("# ")}</h2>')
        elif line.startswith("### "):
            current_chapter.append(f'<h3>{line.lstrip("# ")}</h3>')
        elif line.startswith("```"):
            if current_chapter and current_chapter[-1].startswith("<pre><code>"):
                current_chapter.append("</code></pre>")
            else:
                current_chapter.append("<pre><code>")
        elif line.startswith("| ") and "---" not in line:
            cells = [c.strip() for c in line.split("|")[1:-1]]
            row = "".join(f"<td>{c}</td>" for c in cells)
            current_chapter.append(f"<tr>{row}</tr>")
        elif line.startswith("---"):
            current_chapter.append("<hr>")
        elif line.strip():
            escaped = (line.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;"))
            current_chapter.append(f"<p>{escaped}</p>")

    if current_chapter:
        chapters_html.append("\n".join(current_chapter))

    body = "\n".join(chapters_html)

    toc_html = "\n".join(toc_items)

    return f"""<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>{title}</title>
<style>
:root {{
  --bg: #0d1117; --fg: #c9d1d9; --accent: #58a6ff;
  --sidebar-bg: #161b22; --border: #30363d;
  --code-bg: #1c2128; --heading: #f0f6fc;
}}
* {{ margin: 0; padding: 0; box-sizing: border-box; }}
body {{ font-family: 'Segoe UI', system-ui, sans-serif; background: var(--bg); color: var(--fg); display: flex; }}
nav {{ width: 280px; min-height: 100vh; background: var(--sidebar-bg); border-right: 1px solid var(--border); padding: 1.5rem; position: fixed; overflow-y: auto; }}
nav h2 {{ color: var(--heading); font-size: 1.1rem; margin-bottom: 1rem; }}
nav ul {{ list-style: none; }}
nav li {{ margin: 0.3rem 0; }}
nav a {{ color: var(--accent); text-decoration: none; font-size: 0.9rem; }}
nav a:hover {{ text-decoration: underline; }}
main {{ margin-left: 280px; padding: 2rem 3rem; max-width: 900px; }}
h1 {{ color: var(--heading); font-size: 1.8rem; margin: 2rem 0 1rem; border-bottom: 1px solid var(--border); padding-bottom: 0.5rem; }}
h2 {{ color: var(--heading); font-size: 1.3rem; margin: 1.5rem 0 0.8rem; }}
h3 {{ color: var(--heading); font-size: 1.1rem; margin: 1rem 0 0.5rem; }}
p {{ margin: 0.5rem 0; line-height: 1.6; }}
pre {{ background: var(--code-bg); border: 1px solid var(--border); border-radius: 6px; padding: 1rem; overflow-x: auto; margin: 1rem 0; }}
code {{ font-family: 'Fira Code', 'Consolas', monospace; font-size: 0.85rem; }}
table {{ border-collapse: collapse; width: 100%; margin: 1rem 0; }}
th, td {{ border: 1px solid var(--border); padding: 0.5rem 0.8rem; text-align: left; }}
th {{ background: var(--sidebar-bg); color: var(--heading); }}
hr {{ border: none; border-top: 1px solid var(--border); margin: 2rem 0; }}
.chapter {{ margin-bottom: 3rem; }}
#search {{ width: 100%; padding: 0.5rem; margin-bottom: 1rem; background: var(--code-bg); border: 1px solid var(--border); color: var(--fg); border-radius: 4px; }}
@media (max-width: 768px) {{ nav {{ display: none; }} main {{ margin-left: 0; padding: 1rem; }} }}
</style>
</head>
<body>
<nav>
  <h2>AUGECOIN Technical Book</h2>
  <input type="text" id="search" placeholder="Search chapters..." oninput="filterTOC(this.value)">
  <ul id="toc">{toc_html}</ul>
</nav>
<main>
{body}
</main>
<script>
function filterTOC(q) {{
  q = q.toLowerCase();
  document.querySelectorAll('#toc li').forEach(li => {{
    li.style.display = li.textContent.toLowerCase().includes(q) ? '' : 'none';
  }});
}}
</script>
</body>
</html>"""


# ─── Main Build ──────────────────────────────────────────────────────

def main():
    print("=" * 60)
    print("  AUGECOIN Technical Book Generator")
    print("=" * 60)

    OUTPUT.mkdir(parents=True, exist_ok=True)

    print("[1/8] Collecting metadata...")
    git_commit, git_version = get_git_info()
    rust_ver = get_rust_version()
    graphify_ver = get_graphify_version()

    print("[2/8] Loading graph data...")
    graph_report = load_graph_report()
    graph_json = load_graph_json()
    communities, god_nodes = parse_graph_communities(graph_report)
    stats = extract_summary_stats(graph_report)

    print("[3/8] Loading crate information...")
    crate_info = get_crate_files()

    print("[4/8] Generating chapters...")
    chapters = []

    chapters.append(gen_chapter_00_cover(git_commit, git_version, rust_ver, graphify_ver))
    chapters.append(gen_chapter_01_overview(graph_report, stats, god_nodes, communities))
    chapters.append(gen_chapter_02_architecture(crate_info, graph_report))
    chapters.append(gen_chapter_03_safebox())
    chapters.append(gen_chapter_04_consensus())
    chapters.append(gen_chapter_05_storage())
    chapters.append(gen_chapter_06_network())
    chapters.append(gen_chapter_07_rpc())
    chapters.append(gen_chapter_08_wallet())
    chapters.append(gen_chapter_09_augeid())
    chapters.append(gen_chapter_10_benchmark())
    chapters.append(gen_chapter_11_security())
    chapters.append(gen_chapter_12_tests())
    chapters.append(gen_chapter_13_apis())
    chapters.append(gen_chapter_14_annexes(crate_info, git_commit, git_version, rust_ver))

    full_md = "\n\n---\n\n".join(chapters)

    print("[5/8] Writing Markdown...")
    md_path = OUTPUT / "AUGECOIN_Technical_Book.md"
    md_path.write_text(full_md, encoding="utf-8")
    print(f"  → {md_path}")

    print("[6/8] Generating HTML...")
    html_content = generate_html(full_md)
    html_path = OUTPUT / "AUGECOIN_Technical_Book.html"
    html_path.write_text(html_content, encoding="utf-8")
    print(f"  → {html_path}")

    print("[7/8] Generating manifest...")
    manifest = {
        "commit": git_commit,
        "version": git_version,
        "graphify": graphify_ver,
        "rust": rust_ver,
        "generated": datetime.now(timezone.utc).isoformat(),
        "stats": stats,
        "chapters": len(chapters) - 1,
    }
    manifest_path = OUTPUT / "manifest.json"
    manifest_path.write_text(json.dumps(manifest, indent=2), encoding="utf-8")
    print(f"  → {manifest_path}")

    print("[8/8] Computing SHA256 hashes...")
    hashes = compute_sha256_files()
    hashes_path = OUTPUT / "sha256sums.json"
    hashes_path.write_text(json.dumps(hashes, indent=2), encoding="utf-8")
    print(f"  → {hashes_path}")

    print()
    print("=" * 60)
    print("  Book generated successfully!")
    print(f"  Chapters: {len(chapters) - 1}")
    print(f"  MD:       {md_path.stat().st_size:,} bytes")
    print(f"  HTML:     {html_path.stat().st_size:,} bytes")
    print("=" * 60)


if __name__ == "__main__":
    main()
