#!/usr/bin/env bash
# setup-faucet.sh — generate a new signing key for the AUGECOIN testnet faucet.
#
# stdout: exactly 64 hex chars (32-byte seed). Nothing else is written to
#         stdout, so the key can be captured directly:
#             export AUGECOIN_FAUCET_KEY_HEX=$(./scripts/setup-faucet.sh)
# stderr: setup instructions (they never contain the key itself, so the key
#         does not end up in redirected logs, systemd journals or shell
#         history files).
set -euo pipefail

if command -v openssl >/dev/null 2>&1; then
    KEY_HEX="$(openssl rand -hex 32)"
else
    KEY_HEX="$(head -c 32 /dev/urandom | od -An -tx1 | tr -d ' \n')"
fi

if [ "${#KEY_HEX}" -ne 64 ]; then
    echo "ERROR: failed to generate 32 random bytes" >&2
    exit 1
fi

# The key goes to stdout ONLY (never echoed again anywhere else).
echo "$KEY_HEX"

cat >&2 <<'EOF'

[setup-faucet] New faucet key generated (printed on stdout, not repeated here).

How to configure the node:

  1. Export it in the current shell:
       export AUGECOIN_FAUCET_KEY_HEX=$(./scripts/setup-faucet.sh)
     (re-run as shown, or export the value you already captured)

  2. Or persist it in a .env file with restrictive permissions:
       umask 077
       echo "AUGECOIN_FAUCET_KEY_HEX=<the-hex-you-captured>" >> .env
       # then load it before starting the node:  set -a; . ./.env; set +a

  3. Start the node with the faucet enabled:
       ./target/release/augecoin-node --enable-faucet
     The node logs the faucet ADDRESS (public) at startup; fund that
     account and set AUGECOIN_FAUCET_ACCOUNT to its account number.

Security notes:
  - The key is never written to any file by this script.
  - Do not paste the key into chat tickets, issues or commit history.
  - With systemd, use an EnvironmentFile with mode 0600, not inline ExecStart args.
EOF
