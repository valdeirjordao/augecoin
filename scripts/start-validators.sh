#!/bin/bash
set -e

BINARY="/opt/augecoin/target/release/augecoin-node"
WALLET_DIR="/opt/augecoin/apps/wallet-web/dist"
DATA_DIR="/opt/augecoin/data"
LOG_DIR="/opt/augecoin/logs"

pkill -f "augecoin-node" 2>/dev/null || true
sleep 1

rm -rf "$DATA_DIR"/node-0 "$DATA_DIR"/node-1 "$DATA_DIR"/node-2 "$DATA_DIR"/node-3

mkdir -p "$DATA_DIR" "$LOG_DIR"

# Match the protocol default; override with BLOCK_TIME=... when needed.
BLOCK_TIME="${BLOCK_TIME:-15}"
MAX_TRANSMIT_SIZE="${MAX_TRANSMIT_SIZE:-2097152}"
MEMPOOL_MAX_OPERATIONS="${MEMPOOL_MAX_OPERATIONS:-40000}"

# Admin API key for the operational panel (validatoradd/remove/activate/deactivate).
# Override with ADMIN_API_KEY=... before running.
ADMIN_API_KEY="${ADMIN_API_KEY:-7f36cf566c429ffc1c0c8a931a1836cc2c03ec3a01964ace}"

echo "Starting 4 Augecoin validators with LibP2P..."
echo "  Admin API key: $ADMIN_API_KEY"
echo ""

echo "  Starting bootstrap validator #0..."
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
AUGECOIN_WALLET_DIR=$WALLET_DIR \
AUGECOIN_ADMIN_API_KEYS=$ADMIN_API_KEY \
$BINARY > "$LOG_DIR/validator-0.log" 2>&1 &
PIDS[0]=$!

sleep 3

BOOTNODE_ADDR=$(grep -oP 'listening on \K/ip4/.*' "$LOG_DIR/validator-0.log" | head -1)
if [ -z "$BOOTNODE_ADDR" ]; then
    BOOTNODE_ADDR="/ip4/127.0.0.1/tcp/9200"
fi
echo "  Bootstrap address: $BOOTNODE_ADDR"

for ID in 1 2 3; do
    LOG="$LOG_DIR/validator-${ID}.log"
    echo "  Starting validator #${ID}..."
    AUGECOIN_VALIDATOR_ID=$ID \
    AUGECOIN_VALIDATOR_COUNT=4 \
    AUGECOIN_DEV_MODE=1 \
    AUGECOIN_BOOTNODES="$BOOTNODE_ADDR" \
    AUGECOIN_DATA_DIR="$DATA_DIR/node-${ID}" \
    AUGECOIN_RPC_PORT=$((9005 + ID)) \
    AUGECOIN_RPC_RATE_LIMIT=100000 \
    AUGECOIN_WALLET_PORT=$((8081 + ID)) \
    AUGECOIN_METRICS_PORT=$((9100 + ID * 10)) \
    AUGECOIN_P2P_PORT=$((9200 + ID)) \
    AUGECOIN_BLOCK_TIME=$BLOCK_TIME \
    AUGECOIN_MAX_TRANSMIT_SIZE=$MAX_TRANSMIT_SIZE \
    AUGECOIN_MEMPOOL_MAX_OPERATIONS=$MEMPOOL_MAX_OPERATIONS \
    AUGECOIN_WALLET_DIR=$WALLET_DIR \
    AUGECOIN_ADMIN_API_KEYS=$ADMIN_API_KEY \
    $BINARY > "$LOG" 2>&1 &
    PIDS[$ID]=$!
done

echo ""
echo "All 4 validators started. Waiting for peer discovery..."
sleep 5

echo ""
echo "Validator status:"
for ID in 0 1 2 3; do
    LOG="$LOG_DIR/validator-${ID}.log"
    if kill -0 "${PIDS[$ID]}" 2>/dev/null; then
        PEERS=$(grep -oP 'unique peers: \K[0-9]+' "$LOG" | tail -1)
        HEIGHT=$(grep -oP '\[block \K[0-9]+(?=\] COMMITTED)' "$LOG" | tail -1)
        echo "  Validator #${ID}: RUNNING (peers: ${PEERS:-0}, height: ${HEIGHT:-0})"
    else
        echo "  Validator #${ID}: FAILED"
    fi
done

echo ""
echo "Logs: tail -f $LOG_DIR/validator-{0,1,2,3}.log"
