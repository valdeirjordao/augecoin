#!/usr/bin/env bash
# AUGECOIN — start backend services (ops + wallet-web) for operacional dashboard.
set -u
OPS_BIN=/opt/augecoin/target/release/augecoin-ops
WALLET_DIR=/opt/augecoin/apps/wallet-web/server

# --- augecoin-ops ---
# Secrets must be supplied by the environment (systemd EnvironmentFile or a
# shell that has sourced a root-only file). No defaults are embedded so a shared
# secret can never be committed to the repository.
: "${AUGECOIN_OPS_DATABASE_URL:?set AUGECOIN_OPS_DATABASE_URL}"
: "${AUGECOIN_OPS_ADMIN_KEY:?set AUGECOIN_OPS_ADMIN_KEY}"
: "${AUGECOIN_OPS_NODE_ADMIN_KEY:?set AUGECOIN_OPS_NODE_ADMIN_KEY}"
export AUGECOIN_OPS_DATABASE_URL
export AUGECOIN_OPS_ADMIN_KEY
export AUGECOIN_OPS_NODE_ADMIN_KEY
export AUGECOIN_OPS_LISTEN_ADDR="${AUGECOIN_OPS_LISTEN_ADDR:-127.0.0.1:8790}"
export AUGECOIN_OPS_NODE_RPC_URL="${AUGECOIN_OPS_NODE_RPC_URL:-https://127.0.0.1:9005}"

if [ ! -x "$OPS_BIN" ]; then echo "ERRO: $OPS_BIN ausente"; exit 1; fi
nohup "$OPS_BIN" > /opt/augecoin/logs/augecoin-ops.log 2>&1 &
echo "augecoin-ops -> PID $!"

# --- wallet-web (Node) ---
cd "$WALLET_DIR"
set -a; . ./.env; set +a
nohup node src/index.js > /opt/augecoin/logs/wallet-web.log 2>&1 &
echo "wallet-web -> PID $!"
