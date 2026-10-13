#!/bin/bash
# Isolated 4-validator testnet for the protocol-v6 SMT state root.
#
# Starts four local validators in dev mode, all activating the SMT state root at
# the same height, under an isolated data/log directory. It never touches the
# production data dir. Use it to validate consensus convergence before any
# production migration.
#
# Usage:
#   ACT=3 BLOCK_TIME=2 scripts/testnet-smt-4node.sh start
#   scripts/testnet-smt-4node.sh stop
#   scripts/testnet-smt-4node.sh audit      # independent rebuild of all roots
#
set -u

BIN="${AUGECOIN_BIN:-/opt/augecoin/target/release/augecoin-node}"
ROOT="${TESTNET_DIR:-/tmp/opencode/augecoin-smt-testnet}"
DATA="$ROOT/data"
LOGS="$ROOT/logs"
ACT="${ACT:-3}"
BLOCK_TIME="${BLOCK_TIME:-5}"
BASE_P2P="${BASE_P2P:-19200}"
BASE_RPC="${BASE_RPC:-19005}"
BOOT="/ip4/127.0.0.1/tcp/${BASE_P2P}"

start_node() {
  local id=$1
  local boot=$2
  env AUGECOIN_VALIDATOR_ID="$id" \
    AUGECOIN_VALIDATOR_COUNT=4 \
    AUGECOIN_DEV_MODE=1 \
    AUGECOIN_DATA_DIR="$DATA/node-$id" \
    AUGECOIN_RPC_PORT=$((BASE_RPC + id)) \
    AUGECOIN_WALLET_PORT=$((18081 + id)) \
    AUGECOIN_METRICS_PORT=$((19100 + id * 10)) \
    AUGECOIN_P2P_PORT=$((BASE_P2P + id)) \
    AUGECOIN_BLOCK_TIME="$BLOCK_TIME" \
    AUGECOIN_STATE_ROOT_V6_ACTIVATION="$ACT" \
    AUGECOIN_BOOTNODES="$boot" \
    setsid nohup "$BIN" > "$LOGS/node-$id.log" 2>&1 &
  echo $! > "$LOGS/node-$id.pid"
}

case "${1:-start}" in
  start)
    mkdir -p "$DATA" "$LOGS"
    start_node 0 ""
    sleep 4
    for id in 1 2 3; do
      start_node "$id" "$BOOT"
      sleep 1
    done
    echo "started 4 validators under $DATA (SMT activation height=$ACT)"
    echo "logs: $LOGS/node-{0,1,2,3}.log"
    disown -a 2>/dev/null || true
    ;;
  stop)
    for id in 0 1 2 3; do
      [ -f "$LOGS/node-$id.pid" ] && kill "$(cat "$LOGS/node-$id.pid")" 2>/dev/null
    done
    sleep 2
    pkill -f "augecoin-node" 2>/dev/null || true
    echo "stopped"
    ;;
  audit)
    AUGECOIN_AUDIT_DIRS="$DATA/node-0,$DATA/node-1,$DATA/node-2,$DATA/node-3" \
    AUGECOIN_AUDIT_ACTIVATION="$ACT" \
    cargo test --release -p augecoin-node --test testnet_root_audit \
      -- --ignored --nocapture
    ;;
  *)
    echo "usage: $0 {start|stop|audit}" >&2
    exit 1
    ;;
esac