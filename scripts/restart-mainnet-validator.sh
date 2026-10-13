#!/usr/bin/env bash
# AUGECOIN — restart a single mainnet validator in place (no wipe), optionally
# scheduling the protocol-v6 SMT activation. Used for a coordinated rolling
# upgrade that preserves quorum.
#
# Usage: restart-mainnet-validator.sh <id 1..4> [activation_height]
set -eu

ID="${1:?validator id 1..4}"
ACT="${2:-}"
BIN="${AUGECOIN_BIN:-/opt/augecoin/target/release/augecoin-node}"
GENESIS="${AUGECOIN_GENESIS:-/opt/augecoin/config/genesis-mainnet.toml}"
ENVS="${AUGECOIN_KEYS_DIR:-/opt/augecoin/config/validator-keys}"
LOGS="${AUGECOIN_LOGS:-/opt/augecoin/logs}"

declare -A P2P=( [1]=9200 [2]=9201 [3]=9202 [4]=9203 )
declare -A RPC=( [1]=9005 [2]=9006 [3]=9007 [4]=9008 )
declare -A WAL=( [1]=8081 [2]=8082 [3]=8083 [4]=8084 )
declare -A MET=( [1]=9100 [2]=9110 [3]=9120 [4]=9130 )
BOOT="/ip4/127.0.0.1/tcp/${P2P[1]}"
DIR="/opt/augecoin/data/node-$ID"

# Stop the process that owns this data dir, if any.
for pid in $(pgrep -f "release/augecoin-node" || true); do
  d=$(tr '\0' '\n' < "/proc/$pid/environ" 2>/dev/null | grep '^AUGECOIN_DATA_DIR=' | cut -d= -f2 || true)
  if [ "$d" = "$DIR" ]; then
    echo "stopping pid $pid for node-$ID"
    kill "$pid" 2>/dev/null || true
  fi
done
sleep 4

VKEY=$(grep '^AUGECOIN_VALIDATOR_KEY_HEX=' "$ENVS/v$ID.env" | cut -d= -f2)
BOOT_ARG=(); [ "$ID" -ne 1 ] && BOOT_ARG=( "AUGECOIN_BOOTNODES=$BOOT" )
ACTIVATION=(); [ -n "$ACT" ] && ACTIVATION=( "AUGECOIN_STATE_ROOT_V6_ACTIVATION=$ACT" )
ADMIN=()
[ -n "${AUGECOIN_ADMIN_KEY_HEX:-}" ] && ADMIN+=( "AUGECOIN_ADMIN_KEY_HEX=$AUGECOIN_ADMIN_KEY_HEX" )
[ -n "${AUGECOIN_ADMIN_API_KEYS:-}" ] && ADMIN+=( "AUGECOIN_ADMIN_API_KEYS=$AUGECOIN_ADMIN_API_KEYS" )

env \
  AUGECOIN_VALIDATOR_ID=$ID \
  AUGECOIN_VALIDATOR_KEY_HEX="$VKEY" \
  AUGECOIN_VALIDATOR_COUNT=4 \
  AUGECOIN_CHAIN_ID=1 \
  AUGECOIN_GENESIS_CONFIG="$GENESIS" \
  AUGECOIN_DATA_DIR="$DIR" \
  AUGECOIN_RPC_PORT=${RPC[$ID]} \
  AUGECOIN_WALLET_PORT=${WAL[$ID]} \
  AUGECOIN_METRICS_PORT=${MET[$ID]} \
  AUGECOIN_P2P_PORT=${P2P[$ID]} \
  AUGECOIN_BLOCK_TIME=5 \
  AUGECOIN_RPC_RATE_LIMIT=100000 \
  "${ADMIN[@]}" \
  "${ACTIVATION[@]}" \
  "${BOOT_ARG[@]}" \
  setsid nohup "$BIN" > "$LOGS/validator-$ID.log" 2>&1 &
echo "started node-$ID pid=$! activation=${ACT:-none} admin=$([ ${#ADMIN[@]} -gt 0 ] && echo yes || echo no)"