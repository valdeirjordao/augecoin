#!/usr/bin/env bash
# AUGECOIN — start/restart the CURRENT (post-rotation) mainnet validator set.
#
# After the 2026-10-13 consensus-key rotation the on-chain validator set is
# ids {5,6,7,8} (the original {1,2,3,4} were deactivated and removed). Each id
# uses one of the freshly provisioned seeds. This script resumes without wiping
# state and keeps the protocol-v6 SMT activation height.
#
# Usage:
#   scripts/start-mainnet-current.sh start
#   scripts/start-mainnet-current.sh stop
#   scripts/start-mainnet-current.sh restart <id 5..8>
set -eu

BIN="${AUGECOIN_BIN:-/opt/augecoin/target/release/augecoin-node}"
GENESIS="${AUGECOIN_GENESIS:-/opt/augecoin/config/genesis-mainnet.toml}"
KEYS="${AUGECOIN_ROTATED_KEYS:-/opt/augecoin/config/validator-keys-new}"
LOGS="${AUGECOIN_LOGS:-/opt/augecoin/logs}"
ACT="${AUGECOIN_STATE_ROOT_V6_ACTIVATION:-356000}"

# id -> (data dir, key file, p2p, rpc, wallet, metrics)
declare -A KEYFILE=( [5]=v1.env [6]=v2.env [7]=v3.env [8]=v4.env )
declare -A P2P=( [5]=9205 [6]=9206 [7]=9207 [8]=9208 )
declare -A RPC=( [5]=9009 [6]=9010 [7]=9011 [8]=9012 )
declare -A WAL=( [5]=8085 [6]=8086 [7]=8087 [8]=8088 )
declare -A MET=( [5]=9140 [6]=9150 [7]=9160 [8]=9170 )
BOOT="/ip4/127.0.0.1/tcp/9205"

start_one() {
  local id=$1
  local key
  key=$(grep '^AUGECOIN_VALIDATOR_KEY_HEX=' "$KEYS/${KEYFILE[$id]}" | cut -d= -f2)
  local boot=(); [ "$id" -ne 5 ] && boot=( "AUGECOIN_BOOTNODES=$BOOT" )
  env AUGECOIN_VALIDATOR_ID="$id" \
    AUGECOIN_VALIDATOR_KEY_HEX="$key" \
    AUGECOIN_VALIDATOR_COUNT=4 \
    AUGECOIN_CHAIN_ID=1 \
    AUGECOIN_GENESIS_CONFIG="$GENESIS" \
    AUGECOIN_DATA_DIR="/opt/augecoin/data/node-$id" \
    AUGECOIN_RPC_PORT=${RPC[$id]} \
    AUGECOIN_WALLET_PORT=${WAL[$id]} \
    AUGECOIN_METRICS_PORT=${MET[$id]} \
    AUGECOIN_P2P_PORT=${P2P[$id]} \
    AUGECOIN_BLOCK_TIME=5 \
    AUGECOIN_RPC_RATE_LIMIT=100000 \
    AUGECOIN_STATE_ROOT_V6_ACTIVATION="$ACT" \
    "${boot[@]}" \
    setsid nohup "$BIN" > "$LOGS/node-$id.log" 2>&1 &
  echo $! > "$LOGS/node-$id.pid"
  echo "started node-$id pid=$!"
}

stop_one() {
  local id=$1
  local dir="/opt/augecoin/data/node-$id"
  for pid in $(pgrep -f "release/augecoin-node" || true); do
    local d
    d=$(tr '\0' '\n' < "/proc/$pid/environ" 2>/dev/null | grep '^AUGECOIN_DATA_DIR=' | cut -d= -f2 || true)
    if [ "$d" = "$dir" ]; then kill "$pid" 2>/dev/null || true; fi
  done
  rm -f "$LOGS/node-$id.pid"
}

case "${1:-start}" in
  start)
    mkdir -p "$LOGS"
    start_one 5; sleep 4
    for id in 6 7 8; do start_one "$id"; sleep 1; done
    ;;
  stop)
    for id in 5 6 7 8; do stop_one "$id"; done
    ;;
  restart)
    id="${2:?id 5..8}"; stop_one "$id"; sleep 4; start_one "$id"
    ;;
  *) echo "usage: $0 {start|stop|restart <id>}" >&2; exit 1;;
esac