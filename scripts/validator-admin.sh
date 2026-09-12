#!/usr/bin/env bash
# Signs ValidatorAdmin operations locally and submits only the signed transaction over TLS.
set -euo pipefail
umask 077

usage() {
  cat >&2 <<'EOF'
Usage:
  validator-admin.sh <mnemonic-file> <local-rpc-port> list
  validator-admin.sh <mnemonic-file> <local-rpc-port> add <ed25519-public-key-hex> <activation-height>
  validator-admin.sh <mnemonic-file> <local-rpc-port> remove|activate|deactivate <validator-id> <activation-height>
EOF
  exit 2
}

[[ $# -ge 3 ]] || usage
key_file=$1
port=$2
action=$3
shift 3
[[ $port =~ ^[0-9]{2,5}$ ]] || usage
[[ -f "$key_file" && -r "$key_file" ]] || { echo "ERROR: admin mnemonic file is unreadable" >&2; exit 1; }
[[ ! -L "$key_file" ]] || { echo "ERROR: admin mnemonic file must not be a symbolic link" >&2; exit 1; }
mode=$(stat -c '%a' "$key_file")
[[ "$mode" == 600 ]] || { echo "ERROR: admin mnemonic file must be mode 0600" >&2; exit 1; }
cli="${AUGECOIN_CLI:-augecoin-cli}"
command -v "$cli" >/dev/null || { echo "ERROR: augecoin-cli is required locally" >&2; exit 1; }
endpoint="${AUGECOIN_ADMIN_ENDPOINT:-https://localhost:${port}}"

case "$action" in
  list)
    [[ $# -eq 0 ]] || usage
    exec "$cli" --endpoint "$endpoint" validator list
    ;;
  add)
    [[ $# -eq 2 ]] || usage
    exec "$cli" --endpoint "$endpoint" --key-file "$key_file" validator add \
      --ed25519-key "$1" --activation-height "$2"
    ;;
  remove|activate|deactivate)
    [[ $# -eq 2 ]] || usage
    exec "$cli" --endpoint "$endpoint" --key-file "$key_file" validator "$action" \
      --id "$1" --activation-height "$2"
    ;;
  *) usage ;;
esac
