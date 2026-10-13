#!/usr/bin/env bash
# AUGECOIN — start mainnet with 4 genesis validators (v1..v4).
set -u
BIN=/opt/augecoin/target/release/augecoin-node
GENESIS=/opt/augecoin/config/genesis-mainnet.toml
ENVS=/opt/augecoin/config/validator-keys
# NOTE: never touch this script's destructive section on a live chain — it wipes
# data dirs. Use scripts/start-mainnet-resume.sh for restarts. Admin key must be
# supplied via the environment; no default secret is embedded.
ADMIN_API_KEY="${ADMIN_API_KEY:-}"
ADMIN_KEY_HEX=$(grep '^AUGECOIN_ADMIN_KEY_HEX=' "$ENVS/admin.env" | cut -d= -f2)

if [ ! -x "$BIN" ]; then
  echo "ERRO: binario nao compilado: $BIN" >&2
  exit 1
fi

# Fresh mainnet: wipe previous node state (genesis re-initializes).
echo "Limpando data dirs antigos..."
rm -rf /opt/augecoin/data/node-1 /opt/augecoin/data/node-2 \
       /opt/augecoin/data/node-3 /opt/augecoin/data/node-4 \
       /opt/augecoin/data/node-0 /opt/augecoin/data/tmp-node-* 2>/dev/null
mkdir -p /opt/augecoin/data /opt/augecoin/logs

declare -A P2P=( [1]=9200 [2]=9201 [3]=9202 [4]=9203 )
declare -A RPC=( [1]=9005 [2]=9006 [3]=9007 [4]=9008 )
declare -A WAL=( [1]=8081 [2]=8082 [3]=8083 [4]=8084 )
declare -A MET=( [1]=9100 [2]=9110 [3]=9120 [4]=9130 )

BOOT="/ip4/127.0.0.1/tcp/${P2P[1]}"

for I in 1 2 3 4; do
  VKEY=$(grep '^AUGECOIN_VALIDATOR_KEY_HEX=' "$ENVS/v$I.env" | cut -d= -f2)
  EXTRA=()
  BOOT_ARG=()
  if [ "$I" -eq 1 ]; then
    EXTRA=( "AUGECOIN_ADMIN_KEY_HEX=$ADMIN_KEY_HEX" )
  else
    BOOT_ARG=( "AUGECOIN_BOOTNODES=$BOOT" )
  fi
  env \
    AUGECOIN_VALIDATOR_ID=$I \
    AUGECOIN_VALIDATOR_KEY_HEX=$VKEY \
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
    AUGECOIN_ADMIN_API_KEYS=$ADMIN_API_KEY \
    "${EXTRA[@]}" \
    "${BOOT_ARG[@]}" \
    nohup "$BIN" > "/opt/augecoin/logs/validator-$I.log" 2>&1 &
  echo "validator $I -> PID $! (rpc :${RPC[$I]}, p2p :${P2P[$I]})"
  if [ "$I" -eq 1 ]; then sleep 5; else sleep 2; fi
done

echo "Mainnet iniciando. Logs em /opt/augecoin/logs/validator-*.log"
