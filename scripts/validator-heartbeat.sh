#!/usr/bin/env bash
# Heartbeat de validador AUGECOIN: RPC local -> ops backend.
# Uso: validator-heartbeat.sh <augeid 1-4>
set -euo pipefail

AUGEID="${1:?uso: validator-heartbeat.sh <augeid>}"
case "$AUGEID" in 1|2|3|4) ;; *) echo "augeid invalido: $AUGEID" >&2; exit 2;; esac

RPC_PORT=$((9005 + AUGEID))
OPS_URL="http://127.0.0.1:8790/validator/heartbeat"
NODE_URL="https://127.0.0.1:${RPC_PORT}/"

STATUS=$(curl -sfk --max-time 5 -X POST "$NODE_URL" \
  -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","method":"nodestatus","params":{},"id":1}')

UPTIME=$(jq -r '.result.uptime_seconds // 0' <<< "$STATUS")
BLOCK=$(jq -r '.result.current_height // empty' <<< "$STATUS")

PAYLOAD=$(jq -nc --arg augeid "$AUGEID" --argjson uptime "$UPTIME" --argjson block "${BLOCK:-null}" \
  '{augeid:$augeid, uptime:$uptime, block:$block}')

RESP=$(curl -sf --max-time 5 -X POST "$OPS_URL" \
  -H 'Content-Type: application/json' -d "$PAYLOAD")

AUTHORIZED=$(jq -r '.authorized // false' <<< "$RESP")
ONLINE=$(jq -r '.online // false' <<< "$RESP")
echo "augeid=$AUGEID uptime=${UPTIME}s block=${BLOCK:-?} authorized=$AUTHORIZED online=$ONLINE"

[ "$AUTHORIZED" = "true" ] || { echo "heartbeat rejeitado (authorized=false)" >&2; exit 1; }
