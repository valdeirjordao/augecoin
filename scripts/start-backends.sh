#!/usr/bin/env bash
# AUGECOIN — start backend services (ops + wallet-web) for operacional dashboard.
set -u
OPS_BIN=/opt/augecoin/target/release/augecoin-ops
WALLET_DIR=/opt/augecoin/apps/wallet-web/server

# --- augecoin-ops ---
export AUGECOIN_OPS_DATABASE_URL=postgres://augecoin:augecoin_ops_pw_2026@127.0.0.1:5432/augecoin_ops
export AUGECOIN_OPS_ADMIN_KEY=c2d61323dcb5151d5ce952501e64a7b1b38bace4dce7a70105d6fed997980dbe
export AUGECOIN_OPS_LISTEN_ADDR=127.0.0.1:8790
export AUGECOIN_OPS_NODE_RPC_URL=https://127.0.0.1:9005
export AUGECOIN_OPS_NODE_ADMIN_KEY=7f36cf566c429ffc1c0c8a931a1836cc2c03ec3a01964ace

if [ ! -x "$OPS_BIN" ]; then echo "ERRO: $OPS_BIN ausente"; exit 1; fi
nohup "$OPS_BIN" > /opt/augecoin/logs/augecoin-ops.log 2>&1 &
echo "augecoin-ops -> PID $!"

# --- wallet-web (Node) ---
cd "$WALLET_DIR"
set -a; . ./.env; set +a
nohup node src/index.js > /opt/augecoin/logs/wallet-web.log 2>&1 &
echo "wallet-web -> PID $!"
