# AUGECOIN Operational Documentation

## Node Setup

### Running a Validator Node

The `augecoin-node` binary accepts all configuration through environment variables — there is
no required config file for operation. A node.toml file is supported for static configuration
but environment variables take precedence.

```bash
# Build the binary
cargo build --release -p augecoin-node

# Run a single validator (dev mode)
AUGECOIN_DEV_MODE=1 \
AUGECOIN_VALIDATOR_ID=0 \
AUGECOIN_VALIDATOR_COUNT=1 \
  ./target/release/augecoin-node
```

For production, a validator node requires:

1. A validator keypair (see Key Management)
2. A genesis file (not required — nodes bootstrap from existing network state)
3. A data directory with write permissions

### Port Usage

| Port | Purpose |
|------|---------|
| 9000 (default) | Consensus/P2P port |
| 9443 | JSON-RPC over HTTP |
| 9100 | Prometheus metrics endpoint |
| 8081 (default wallet) | Wallet RPC + static fileserver |

### Process supervision

Run `augecoin-node` under systemd, supervisord, or similar. The process is not a daemon —
it runs in the foreground. Example systemd unit:

```ini
[Unit]
Description=AUGECOIN Validator Node
After=network-online.target

[Service]
Type=simple
ExecStart=/usr/local/bin/augecoin-node
Environment="AUGECOIN_DATA_DIR=/opt/augecoin/data"
Environment="AUGECOIN_VALIDATOR_KEY_FILE=/opt/augecoin/config/validator.key"
Environment="AUGECOIN_VALIDATOR_ID=1"
Environment="AUGECOIN_VALIDATOR_COUNT=4"
Environment="AUGECOIN_PEERS=validator-2:9000,validator-3:9000,validator-4:9000"
Restart=always
RestartSec=10

[Install]
WantedBy=multi-user.target
```

---

## Docker Setup

### 4-Node Testnet (docker compose)

A complete 4-validator + observer + Prometheus + Grafana stack is defined in
`infra/testnet-4node.yml`.

```bash
# 1. Generate genesis data (validator seeds, genesis.json, node configs)
bash scripts/genesis/generate.sh

# 2. Build and start the stack (4 validators + monitoring)
docker compose -f infra/testnet-4node.yml up --build -d

# 3. Optional: start the observer node
docker compose -f infra/testnet-4node.yml --profile observer up -d observer

# 4. Check status
curl -k https://localhost:9444/status
docker compose -f infra/testnet-4node.yml logs -f
```

### Service Overview

| Service | Container Name | Ports (host:container) |
|---------|---------------|------------------------|
| validator-1 | augecoin-validator-1 | 9001:9000, 9444:9443, 9101:9100 |
| validator-2 | augecoin-validator-2 | 9002:9000, 9445:9443, 9102:9100 |
| validator-3 | augecoin-validator-3 | 9003:9000, 9446:9443, 9103:9100 |
| validator-4 | augecoin-validator-4 | 9004:9000, 9447:9443, 9104:9100 |
| observer | augecoin-observer | 9443:9443, 9100:9100 |
| prometheus | augecoin-prometheus | 9090:9090 |
| grafana | augecoin-grafana | 3000:3000 |

Grafana login: `admin` / `augecoin` (configured in testnet-4node.yml environment).

### Custom Images

The included Dockerfile (`infra/Dockerfile`) is a multi-stage build:
1. Build stage: compiles `augecoin-node` from source inside `rust:1.83-slim-bookworm`
2. Run stage: `debian:bookworm-slim` with only the stripped binary and TLS certs

Exposed ports: 9100 (metrics), 9443 (RPC), 9000 (consensus), 9001 (wallet).

---

## Key Management

### Generating Keys

The `augecoin-keygen` binary creates a BIP39 mnemonic and derives an Ed25519 keypair at
HD path `m/0`:

```bash
cargo build --release -p augecoin-node
./target/release/augecoin-keygen
```

Output (to stdout/stdin):
```
mnemonic phrase (to stdout)
Ed25519 public key (hex) (to stderr)
Ed25519 private key (hex) (to stderr)
```

**The mnemonic and private key are printed to stdout/stderr once. Store them immediately
in a secure offline medium.** The tool uses:
- `rand::thread_rng()` (OS CSPRNG) for entropy
- BIP39 mnemonic → `HdWallet::from_mnemonic` → `derive_keypair(0)` → Ed25519 keypair

### Providing Keys to the Node

The node searches for a validator key in this order:

1. `AUGECOIN_VALIDATOR_KEY_HEX` — hex-encoded 32-byte seed (64 hex chars). Set directly in
   the environment.
2. `AUGECOIN_VALIDATOR_KEY_FILE` — path to a file containing the hex-encoded seed.
3. `AUGECOIN_DEV_MODE=1` — uses deterministic dev keys (not for production).

If none is set and stdout is not a terminal, the node exits with a fatal error. If running
interactively with none set, it prints a warning and exits.

**Example:**

```bash
# From environment variable
export AUGECOIN_VALIDATOR_KEY_HEX="a1b2c3d4e5f6..."

# From file (preferred for production — file can be access-controlled)
export AUGECOIN_VALIDATOR_KEY_FILE="/opt/augecoin/config/validator.key"
```

### Admin Key

Validator onboarding commands use a separate admin key. The admin mnemonic is provided via:
- `--mnemonic` CLI flag
- `--key-file` CLI flag (file containing the mnemonic)
- `AUGECOIN_ADMIN_MNEMONIC` environment variable

**Admin and validator consensus keys must be separate in production.** See the Mainnet
Readiness checklist.

### Key Storage Best Practices

- Never commit keys to version control
- Use filesystem permissions (`chmod 600`) on key files
- Consider hardware security modules (HSMs) or encrypted volumes
- Rotate validator keys periodically (requires admin add/remove via CLI)
- Backup mnemonics on paper in physically separate locations

---

## Validator Onboarding

All validator governance is performed through `augecoin-cli` with the admin key.
Changes take effect at a future `activation_height` — never immediately.

### Prerequisites

```bash
cargo build --release -p augecoin-cli
export ENDPOINT="https://localhost:9443"
export ADMIN_MNEMONIC="abandon abandon ..."
```

### Commands

**List validators:**
```bash
augecoin-cli --endpoint "$ENDPOINT" validator list
augecoin-cli --endpoint "$ENDPOINT" validator list --json
```

**Add a new validator:**
```bash
augecoin-cli --endpoint "$ENDPOINT" \
  --mnemonic "$ADMIN_MNEMONIC" \
  validator add \
    --ed25519-key "<64-char-hex-public-key>" \
    --activation-height 50000
```
The `ed25519_key` is the 32-byte Ed25519 public key of the new validator in hex (64 hex chars).
The validator is added with status `inactive` until `activation_height` is reached.

**Activate a validator:**
```bash
augecoin-cli --endpoint "$ENDPOINT" \
  --mnemonic "$ADMIN_MNEMONIC" \
  validator activate \
    --id 5 \
    --activation-height 55000
```

**Deactivate a validator:**
```bash
augecoin-cli --endpoint "$ENDPOINT" \
  --mnemonic "$ADMIN_MNEMONIC" \
  validator deactivate \
    --id 5 \
    --activation-height 60000
```

**Remove a validator:**
```bash
augecoin-cli --endpoint "$ENDPOINT" \
  --mnemonic "$ADMIN_MNEMONIC" \
  validator remove \
    --id 5 \
    --activation-height 65000
```

**Check validator earnings:**
```bash
augecoin-cli --endpoint "$ENDPOINT" validator earnings --all
augecoin-cli --endpoint "$ENDPOINT" validator earnings --id 1 --json
```

### Adoption Rules

- The validator set starts at genesis with 4 validators
- Quorum is 2/3+1 of active validators (e.g., 3 signatures out of 4)
- Adding a validator does not change quorum until the activation height is reached
- Equivocating validators can be removed by the admin
- Deactivated validators do not participate in consensus but remain on-chain

---

## Monitoring

### Prometheus Metrics

Each node exposes a Prometheus-compatible text format endpoint at `AUGECOIN_METRICS_PORT`
(default: `9100 + validator_id * 10`).

**Available metrics:**

| Metric | Type | Description |
|--------|------|-------------|
| `augecoin_block_height` | gauge | Current block height |
| `augecoin_block_time_seconds` | gauge | Configured block time in seconds |
| `augecoin_peers_connected` | gauge | Number of connected P2P peers |
| `augecoin_mempool_size` | gauge | Pending operations in mempool |
| `augecoin_equivocation_events_total` | counter | Total equivocation events detected |
| `augecoin_block_propagation_latency_ms` | gauge | Last block propagation latency |

Scrape endpoint:
```bash
curl http://localhost:9100/metrics
```

### Grafana Dashboard

Pre-configured in `infra/observability/grafana/dashboards/augecoin-node.json`.
The dashboard is automatically provisioned when using `testnet-4node.yml`.

**Produção (chain1):** a stack sobe com `infra/observability.yml` e coleta os
9 nós do host — 4 validadores gênesis (rede Docker `genesis1`,
172.31.100.11-14:9110) e 5 nós systemd (`validator-0` bootnode + `validator-1..4`,
métricas em 9100/9110/9120/9130/9140 via `host.docker.internal`).

```bash
cd infra && docker compose -f observability.yml up -d   # Prometheus + Grafana
```

Ambos ficam **restritos a localhost** (Prometheus `127.0.0.1:9091` — a porta
9090 da host já está ocupada; Grafana `127.0.0.1:3000`). Acesso remoto via SSH
tunnel ou proxy nginx com auth:

```bash
ssh -L 3000:127.0.0.1:3000 -L 9091:127.0.0.1:9091 <user>@chain1
```

Credenciais: `admin` / `$AUGECOIN_GRAFANA_ADMIN_PASSWORD` (default `augecoin`
— trocar antes de expor publicamente). Datasource auto-provisionado aponta para
`http://prometheus:9090`; todos os alvos são raspados a cada 15s. Verificar
saúde dos alvos:
```bash
curl -s http://127.0.0.1:9091/api/v1/targets | jq '.data.activeTargets[] | {instance:.labels.instance, health}'
```

### Alerting

The node's security subsystem exposes alert endpoints via JSON-RPC:

```bash
# Recent alerts
augecoin-cli --endpoint "$ENDPOINT" security alerts

# Live alert tail
augecoin-cli --endpoint "$ENDPOINT" security alerts --tail

# Equivocation proofs
augecoin-cli --endpoint "$ENDPOINT" security equivocations

# Banned peers
augecoin-cli --endpoint "$ENDPOINT" security banned-peers
```

Integrate these with Prometheus Alertmanager or external monitoring systems.

---

## Backup and Recovery

### Checkpoint Backup

The storage layer (`augecoin-storage`) supports account state checkpointing via
`AccountSnapshot`:

```rust
// In-code API (exposed via RPC in future versions):
let snapshot = storage.create_checkpoint(current_height);
let bytes = snapshot.to_bytes();
// Store `bytes` in cold storage or object storage
```

Checkpoint format:
- Version byte (`0x01`)
- Height (8 bytes, big-endian)
- Account count (4 bytes, big-endian)
- Per account: account number (8 bytes) + length-prefixed serialized Account

### Manual Backup

While the JSON-RPC API does not yet expose backup/restore commands directly, operators
should back up:

1. **RocksDB data directory** (`AUGECOIN_DATA_DIR`):
   ```bash
   # Stop the node first
   systemctl stop augecoin

   # Create a tarball backup
   tar -czf augecoin-backup-$(date +%Y%m%d-%H%M%S).tar.gz /opt/augecoin/data/
   ```

2. **Validator key file** — backup securely in a separate location.

3. **Genesis file** — retain the original `genesis.json` used to bootstrap.

### Disaster Recovery

1. Restore the RocksDB data directory from backup
2. Restore the validator key file
3. Verify checksum integrity:
   ```rust
   let checksum = storage.verify_checksum(); // BLAKE3 512-bit hash of all account state
   ```
4. Start the node — it will rejoin the network and catch up

**For state corruption:**
- The node stores state in 6 RocksDB column families: `accounts`, `blocks`, `safebox`,
  `op_index`, `validator_set`, `equivocation_proofs`
- Corruption in account state is detectable via `verify_checksum()`
- If corruption is detected, restore from the most recent clean checkpoint
- `restore_from_checkpoint()` atomically replaces account state while preserving
  other column families

**For total data loss:**
- Start a fresh node with the same validator key and genesis
- Bootstrap from peers (sync protocol)
- Validator key uniquely identifies the node; reusing it on a fresh data dir recovers
  the identity

---

## Incident Response

### Key Compromise

**Validator consensus key compromised:**

1. Immediately deactivate the compromised validator:
   ```bash
   augecoin-cli validator deactivate --id <ID> --activation-height <current+10>
   ```
2. Generate a new keypair with `augecoin-keygen`
3. Add the new key as a new validator:
   ```bash
   augecoin-cli validator add --ed25519-key <new-key> --activation-height <current+50>
   ```
4. Activate the new validator at the same activation height
5. Remove the old validator after the transition completes
6. Audit all blocks signed by the compromised key for malicious activity

**Admin key compromised:**

1. The admin is a single-key authority (ADR-004). Compromise of this key requires
   an emergency hard fork to replace the admin key.
2. Contact all validators immediately
3. Prepare a coordinated fork with the new admin key embedded
4. This scenario is a known risk; see `THREAT_MODEL.md`

### Consensus Failure

Symptoms: no blocks being produced, stalled height, no leader proposals.

1. Check validator status:
   ```bash
   augecoin-cli status
   ```
2. Verify quorum health — at least 2/3+1 validators must be online
3. Check individual validator logs:
   ```bash
   tail -f /opt/augecoin/logs/validator-0.log
   ```
4. Verify network connectivity between validators (consensus ports must be reachable)
5. Check NTP synchronization — block time drift beyond 2s tolerance can cause timeouts
6. If a single validator is absent: deactivate it, consensus continues with remaining
   validators if quorum is maintained
7. If multiple validators fail: resolve infrastructure issues; the chain will resume
   once quorum is restored

### State Divergence

Symptoms: nodes at the same height with different state roots.

1. Compare state root hashes across nodes:
   ```bash
   # Query block via RPC
   augecoin-cli get-block <height>
   ```
2. Identify the correct chain by verifying the state root against the canonical
   validator set's view
3. On nodes with divergent state:
   - Stop the node
   - Restore from the most recent checkpoint that matches the canonical chain
   - Restart and catch up via sync
4. Investigate root cause:
   - Non-deterministic execution
   - Corrupted storage
   - Malicious block proposal

---

## Upgrades

### Protocol Version Upgrades

AUGECOIN supports protocol upgrades via the `sig_scheme_version` field in transactions
(SPEC.md Section 2). Major consensus or transaction format changes require a coordinated
hard fork.

### Coordinated Rollout Procedure

1. **Proposal phase:**
   - Document the upgrade as an ADR in `DECISIONS.md`
   - Freeze protocol constants for the target version
   - Build and test the new version across all validators

2. **Activation height coordination:**
   - Agree on an `activation_height` with all validators — a future block number where
     the new rules take effect
   - The activation height must be > current height + sufficient lead time (minimum
     1,000 blocks / ~16 hours at 60s block time)

3. **Rollout:**
   - All validators upgrade their binary before `activation_height`
   - Node operates with both old and new logic until the transition height
   - At `activation_height`, the new rules take effect atomically

4. **Rollback plan:**
   - If a critical bug is discovered before activation, revert to the previous binary
   - If discovered after activation, a corrective hard fork is required
   - Always maintain the ability to replay from genesis

### Binary Upgrade

1. Build the new version: `cargo build --release -p augecoin-node`
2. Deploy the binary to all validators
3. Restart nodes one at a time (never all at once — preserve quorum)
4. Verify height is advancing and quorum is maintained after each restart

---

## Environment Variables Reference

All environment variables consumed by `augecoin-node` and `augecoin-cli`:

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `AUGECOIN_VALIDATOR_ID` | No | `0` | Unique numeric ID for this validator |
| `AUGECOIN_VALIDATOR_COUNT` | No | `4` | Total validators in the genesis set |
| `AUGECOIN_VALIDATOR_KEY_HEX` | Yes* | — | 32-byte validator seed as 64-char hex string |
| `AUGECOIN_VALIDATOR_KEY_FILE` | Yes* | — | Path to file containing the hex-encoded seed |
| `AUGECOIN_DEV_MODE` | No | `""` | Set to `"1"` for deterministic dev keys (not for production) |
| `AUGECOIN_DATA_DIR` | No | `/opt/augecoin/data/node-{AUGECOIN_VALIDATOR_ID}` | RocksDB data directory |
| `AUGECOIN_METRICS_PORT` | No | `9100 + validator_id * 10` | Prometheus metrics HTTP port |
| `AUGECOIN_RPC_PORT` | No | `9005 + validator_id` | JSON-RPC HTTP port |
| `AUGECOIN_WALLET_PORT` | No | `8081 + validator_id` | Wallet RPC + web UI port |
| `AUGECOIN_P2P_PORT` | No | `9101 + validator_id` | LibP2P listen port |
| `AUGECOIN_EXTERNAL_ADDRESS` | No | (auto-detected) | Explicit multiaddr to advertise to peers (see below) |
| `AUGECOIN_BOOTNODES` | No | `""` | Comma-separated bootnode multiaddrs, e.g. `/ip4/1.2.3.4/tcp/9200` |
| `AUGECOIN_BLOCK_TIME` | No | `15` | Block time in seconds |
| `AUGECOIN_WALLET_DIR` | No | `apps/wallet-web/dist` | Path to web wallet static files |
| `AUGECOIN_ADMIN_MNEMONIC` | No† | — | Admin BIP39 mnemonic (CLI only; used by `augecoin-cli`) |

\* One of `AUGECOIN_VALIDATOR_KEY_HEX`, `AUGECOIN_VALIDATOR_KEY_FILE`, or
`AUGECOIN_DEV_MODE=1` must be set.

† Not consumed by `augecoin-node`; used only by `augecoin-cli` as a fallback when
`--mnemonic` or `--key-file` flags are not provided.

### External address

In environments with NAT, VPS or multiple interfaces, the address observed by
remote peers (`identify.observed_addr`) may not be the one you want to
advertise (e.g. a public IP instead of an internal one, or vice-versa). Set
`AUGECOIN_EXTERNAL_ADDRESS` to force a stable, dialable address:

```bash
AUGECOIN_EXTERNAL_ADDRESS=/ip4/203.0.113.10/tcp/9200
```

Resolution priority when announcing an address to peers:

1. `AUGECOIN_EXTERNAL_ADDRESS` (explicit, validated at startup);
2. `identify.observed_addr` (learned from a remote peer);
3. the local listen address.

`0.0.0.0` / `::` are rejected as invalid external addresses. The node logs the
decision at startup:

```
[network] external address configured: /ip4/203.0.113.10/tcp/9200
```

### Gossipsub hardening

The gossipsub transport is configured with an explicit `max_transmit_size` of
`1_048_576` bytes (1 MiB) plus pinned `message_id_fn`, `duplicate_cache_time`,
`heartbeat_interval`, `history_length`, `history_gossip` and mesh parameters
(`mesh_n`/`mesh_n_low`/`mesh_n_high`). Any published message larger than the
limit is rejected before transmission:

```
[network] gossipsub max_transmit_size=1048576
[network] oversized message rejected: 1048577 bytes (max 1048576)
```
