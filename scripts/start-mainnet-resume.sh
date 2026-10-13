#!/usr/bin/env bash
# AUGECOIN — start/restart mainnet validators WITHOUT wiping state.
#
# Unlike start-mainnet.sh, this never removes data directories: it resumes the
# existing chain from its last committed height. Validator keys are read from
# config/validator-keys/vN.env (never echoed). Pass AUGECOIN_STATE_ROOT_V6_ACTIVATION
# to schedule the protocol-v6 SMT activation at a future height.
set -eu

BIN="${AUGECOIN_BIN:-/opt/augecoin/target/release/augecoin-node}"
GENESIS="${AUGECOIN_GENESIS:-/opt/augecoin/config/genesis-mainnet.toml}"
ENVS="${AUGECOIN_KEYS_DIR:-/opt/augecoin/config/validator-keys}"
LOGS="${AUGECOIN_LOGS:-/opt/augecoin/logs}"
ACT="${AUGECOIN_STATE_ROOT_V6_ACTIVATION:-}"

declare -A P2P=( [1]=9200 [2]=9201 [3]=9202 [4]=9203 )
declare -A RPC=( [1]=9005 [2]=9006 [3]=9007 [4]=9008 )
declare -A WAL=( [1]=8081 [2]=8082 [3]=8083 [4]=8084 )
declare -A MET=( [1]=9100 [2]=9110 [3]=9120 [4]=9130 )
BOOT="/ip4/127.0.0.1/tcp/${P2P[1]}"

mkdir -p "$LOGS"

for I in 1 2 3 4; do
  VKEY=$(grep '^AUGECOIN_VALIDATOR_KEY_HEX=' "$ENVS/v$I.env" | cut -d= -f2)
  BOOT_ARG=()
  if [ "$I" -ne 1 ]; then
    BOOT_ARG=( "AUGECOIN_BOOTNODES=$BOOT" )
  fi
  ACTIVATION=()
  if [ -n "$ACT" ]; then
    ACTIVATION=( "AUGECOIN_STATE_ROOT_V6_ACTIVATION=$ACT" )
  fi
  env \
    AUGECOIN_VALIDATOR_ID=$I \
    AUGECOIN_VALIDATOR_KEY_HEX="$VKEY" \
    AUGECOIN_VALIDATOR_COUNT=4 \
    AUGECOIN_CHAIN_ID=1 \
    AUGECOIN_GENESIS_CONFIG="$GENESIS" \
    AUGECOIN_DATA_DIR=/opt/augecoin/data/node-$I \
    AUGECOIN_RPC_PORT=${RPC[$I]} \
    AUGECOIN_WALLET_PORT=${WAL[$I]} \
    AUGECOIN_METRICS_PORT=${MET[$I]} \
    AUGECOIN_P2P_PORT=${P2P[$I]} \
    AUGECOIN_BLOCK_TIME=5 \
    AUGECOIN_RPC_RATE_LIMIT=100000 \
    "${ACTIVATION[@]}" \
    "${BOOT_ARG[@]}" \
    setsid nohup "$BIN" > "$LOGS/validator-$I.log" 2>&1 &
  echo "validator $I -> PID $! (rpc :${RPC[$I]}, p2p :${P2P[$I]})"
  if [ "$I" -eq 1 ]; then sleep 5; else sleep 2; fi
done

if [ -n "$ACT" ]; then
  echo "SMT v6 activation scheduled at height $ACT"
else
  echo "SMT v6 disabled (legacy flat root)"
fi