#!/bin/bash
# Non-destructive validator restart (keeps RocksDB data; clears in-memory mempool).
set -e

BINARY="/opt/augecoin/target/release/augecoin-node"
DATA_DIR="/opt/augecoin/data"
LOG_DIR="/opt/augecoin/logs"
# Match the protocol defaults; override either value in the environment.
BLOCK_TIME="${BLOCK_TIME:-15}"
MAX_TRANSMIT_SIZE="${MAX_TRANSMIT_SIZE:-2097152}"
MEMPOOL_MAX_OPERATIONS=40000

mkdir -p "$LOG_DIR"

echo "=== RESTART $(date) ==="

pkill -f "augecoin-node" 2>/dev/null || true
sleep 1

# Start bootstrap validator #0.
AUGECOIN_VALIDATOR_ID=0 \
AUGECOIN_VALIDATOR_COUNT=4 \
AUGECOIN_DEV_MODE=1 \
AUGECOIN_DATA_DIR="$DATA_DIR/node-0" \
AUGECOIN_RPC_PORT=9005 \
AUGECOIN_RPC_RATE_LIMIT=100000 \
AUGECOIN_WALLET_PORT=8081 \
AUGECOIN_METRICS_PORT=9100 \
AUGECOIN_P2P_PORT=9200 \
AUGECOIN_BLOCK_TIME=$BLOCK_TIME \
AUGECOIN_MAX_TRANSMIT_SIZE=$MAX_TRANSMIT_SIZE \
AUGECOIN_MEMPOOL_MAX_OPERATIONS=$MEMPOOL_MAX_OPERATIONS \
setsid "$BINARY" </dev/null >>"$LOG_DIR/validator-0.log" 2>&1 &

sleep 4

BOOTNODE_ADDR=$(grep -oP 'listening on \K/ip4/.*' "$LOG_DIR/validator-0.log" | tail -1)
if [ -z "$BOOTNODE_ADDR" ]; then
    BOOTNODE_ADDR="/ip4/127.0.0.1/tcp/9200"
fi
echo "bootnode: $BOOTNODE_ADDR"

for ID in 1 2 3; do
    AUGECOIN_VALIDATOR_ID=$ID \
    AUGECOIN_VALIDATOR_COUNT=4 \
    AUGECOIN_DEV_MODE=1 \
    AUGECOIN_BOOTNODES="$BOOTNODE_ADDR" \
    AUGECOIN_DATA_DIR="$DATA_DIR/node-$ID" \
    AUGECOIN_RPC_PORT=$((9005 + ID)) \
    AUGECOIN_RPC_RATE_LIMIT=100000 \
    AUGECOIN_WALLET_PORT=$((8081 + ID)) \
    AUGECOIN_METRICS_PORT=$((9100 + ID * 10)) \
    AUGECOIN_P2P_PORT=$((9200 + ID)) \
    AUGECOIN_BLOCK_TIME=$BLOCK_TIME \
    AUGECOIN_MAX_TRANSMIT_SIZE=$MAX_TRANSMIT_SIZE \
    AUGECOIN_MEMPOOL_MAX_OPERATIONS=$MEMPOOL_MAX_OPERATIONS \
    setsid "$BINARY" </dev/null >>"$LOG_DIR/validator-$ID.log" 2>&1 &
done

echo "started validators 0-3"
