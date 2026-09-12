#!/usr/bin/env bash
# Opens one local TLS tunnel to a validator RPC and optionally its metrics port.
set -euo pipefail

usage() {
  echo "Usage: $0 <ssh-user@validator-host> <validator-id 1-4> [local-rpc-port]" >&2
  exit 2
}

[[ $# -ge 2 && $# -le 3 ]] || usage
host=$1
id=$2
[[ $id =~ ^[1-4]$ ]] || usage
local_rpc=${3:-$((19400 + id))}
remote_rpc=$((19400 + id))
remote_metrics=$((19300 + id))

command -v ssh >/dev/null || { echo "ERROR: ssh is required" >&2; exit 1; }
exec ssh -N \
  -o ExitOnForwardFailure=yes \
  -o ServerAliveInterval=30 \
  -o ServerAliveCountMax=3 \
  -L "127.0.0.1:${local_rpc}:127.0.0.1:${remote_rpc}" \
  -L "127.0.0.1:$((local_rpc - 100)):127.0.0.1:${remote_metrics}" \
  "$host"
