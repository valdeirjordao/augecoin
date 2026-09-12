#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
GENESIS_DIR="${SCRIPT_DIR}/genesis-data"
CONFIG_FILE="${SCRIPT_DIR}/genesis.toml"

echo "=== AUGECOIN Genesis Generation ==="
echo "Chain ID: augecoin-testnet-1"
echo "Validators: 4 (round-robin, quorum 2/3+1)"
echo ""

# Create output directories
mkdir -p "${GENESIS_DIR}/validators"
for i in 1 2 3 4; do
    mkdir -p "${GENESIS_DIR}/validators/validator-${i}"
done
mkdir -p "${GENESIS_DIR}/observer"

# ── Validator key generation ────────────────────────────────────────
# Each validator seed produces a deterministic Ed25519 + ML-DSA-65 keypair
# via HKDF-SHA3-512 (SPEC.md section 2).

generate_validator_keys() {
    local id=$1
    local seed=$2
    local name=$3
    local dir="${GENESIS_DIR}/validators/${name}"

    # Derive keypair from seed using augecoin-cli or embedded logic
    # For genesis: the seed is a 64-byte hex value
    # HD wallet: HdWallet::from_seed -> derive_keypair(0) -> HybridKeyPair
    # This script stores the seed; the node derives keys at startup.

    echo "${seed}" > "${dir}/seed.hex"

    # Store validator metadata
    cat > "${dir}/validator.toml" << EOFTOML
[validator]
id = ${id}
name = "${name}"
seed_file = "seed.hex"
status = "active"

[validator.keys]
# Ed25519 + ML-DSA-65 derived from seed at index 0
derivation_path = "m/0"
EOFTOML

    echo "  Validator ${id} (${name}): seed generated -> ${dir}/seed.hex"
}

echo "Generating validator identities..."
echo ""

declare -a VALIDATOR_SEEDS=(
    "genesis-validator-1-key-seed-00000000000000000000000000000000000000000000000000000000000000"
    "genesis-validator-2-key-seed-00000000000000000000000000000000000000000000000000000000000000"
    "genesis-validator-3-key-seed-00000000000000000000000000000000000000000000000000000000000000"
    "genesis-validator-4-key-seed-00000000000000000000000000000000000000000000000000000000000000"
)

for i in 1 2 3 4; do
    idx=$((i - 1))
    generate_validator_keys "$i" "${VALIDATOR_SEEDS[$idx]}" "validator-${i}"
done

echo ""

# ── Genesis block (block 0) ─────────────────────────────────────────
echo "Generating genesis block (block 0)..."

cat > "${GENESIS_DIR}/genesis.json" << EOFJSON
{
  "chain_id": "augecoin-testnet-1",
  "genesis_time": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "consensus": {
    "type": "poa",
    "quorum_threshold": 3,
    "block_time_seconds": 15,
    "timeout_seconds": 120
  },
  "emission": {
    "total_supply_auge": 750000000,
    "total_supply_augesat": 75000000000000000,
    "emission_years": 50,
    "total_emission_blocks": 26298000,
    "base_reward_augesat": 2852383,
    "remainder_augesat": 22446192
  },
  "initial_validators": [
    { "id": 1, "name": "validator-1", "bootnode": true },
    { "id": 2, "name": "validator-2", "bootnode": true },
    { "id": 3, "name": "validator-3", "bootnode": true },
    { "id": 4, "name": "validator-4", "bootnode": true }
  ],
  "admin": {
    "mnemonic": "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about",
    "key_index": 0
  },
  "genesis_block": {
    "height": 0,
    "prev_hash": "00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
    "state_root": "00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
    "tx_merkle_root": "00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
    "timestamp": "$(date +%s)",
    "leader_id": 0,
    "round_reward": 0,
    "new_account_number": 0,
    "transactions": []
  }
}
EOFJSON

echo "  Genesis block written to ${GENESIS_DIR}/genesis.json"
echo ""

# ── Bootnode configuration ──────────────────────────────────────────
echo "Generating bootnode configuration..."
# All 4 validators act as bootnodes
cat > "${GENESIS_DIR}/bootnodes.txt" << EOFLIST
/dns4/validator-1/tcp/9000
/dns4/validator-2/tcp/9000
/dns4/validator-3/tcp/9000
/dns4/validator-4/tcp/9000
EOFLIST

echo "  Bootnodes: 4 peers"
echo ""

# ── Observer configuration ──────────────────────────────────────────
cat > "${GENESIS_DIR}/observer/node.toml" << EOFTOML
[node]
role = "observer"
listen_addr = "0.0.0.0:9000"

[network]
bootnodes = [
    "/dns4/validator-1/tcp/9000",
    "/dns4/validator-2/tcp/9000",
    "/dns4/validator-3/tcp/9000",
    "/dns4/validator-4/tcp/9000"
]

[rpc]
jsonrpc_addr = "0.0.0.0:9443"
grpc_addr = "0.0.0.0:9444"

[metrics]
port = 9100

[storage]
data_dir = "/data"
EOFTOML

# ── Per-validator node configuration ────────────────────────────────
for i in 1 2 3 4; do
    # Build bootnode list excluding self
    local bootnodes=""
    for j in 1 2 3 4; do
        if [ "$j" != "$i" ]; then
            bootnodes="${bootnodes}/dns4/validator-${j}/tcp/9000,"
        fi
    done
    bootnodes="${bootnodes%,}"

    cat > "${GENESIS_DIR}/validators/validator-${i}/node.toml" << EOFTOML
[node]
role = "validator"
validator_id = ${i}
listen_addr = "0.0.0.0:9000"

[network]
bootnodes = [${bootnodes}]

[rpc]
jsonrpc_addr = "0.0.0.0:9443"
grpc_addr = "0.0.0.0:9444"

[metrics]
port = 9100

[storage]
data_dir = "/data"

[consensus]
propose_timeout_seconds = 120
EOFTOML

    echo "  Config written: validator-${i}/node.toml"
done

echo ""
echo "=== Genesis generation complete ==="
echo "Output directory: ${GENESIS_DIR}"
echo ""
echo "Next steps:"
echo "  docker compose -f infra/testnet-4node.yml up --build -d"
echo "  augecoin-cli status --endpoint https://localhost:9443"
echo "  augecoin-cli validator list --endpoint https://localhost:19443"
