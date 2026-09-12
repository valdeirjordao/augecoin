#!/usr/bin/env bash
# Creates the four external Docker secrets consumed by the genesis1 deployment.
set -euo pipefail
umask 077

readonly SECRET_DIR="${AUGECOIN_VALIDATOR_SECRET_DIR:-/etc/augecoin/validator-seeds}"

fail() { echo "ERROR: $*" >&2; exit 1; }

[[ $(id -u) -eq 0 ]] || fail "run as root so validator seed files stay protected"
command -v docker >/dev/null || fail "Docker is required on the deployment host"
docker info --format '{{.Swarm.LocalNodeState}}' 2>/dev/null | grep -qx active || \
  fail "Docker Swarm must be active before creating external Docker secrets"
[[ -d "$SECRET_DIR" ]] || fail "secret directory does not exist: $SECRET_DIR"
[[ ! -L "$SECRET_DIR" ]] || fail "secret directory must not be a symbolic link"
[[ $(stat -c '%u:%g' "$SECRET_DIR") == 0:0 ]] || fail "secret directory must be owned by root:root"
[[ $(stat -c '%a' "$SECRET_DIR") == 700 ]] || fail "secret directory must have mode 0700"

# Preflight all inputs and destinations before creating any secret.
for id in 1 2 3 4; do
  name="augecoin_validator_${id}_key"
  file="$SECRET_DIR/v${id}.key"
  [[ -f "$file" ]] || fail "missing validator seed file: $file"
  [[ ! -L "$file" ]] || fail "$file must not be a symbolic link"
  [[ $(stat -c '%F' "$file") == "regular file" ]] || fail "$file must be a regular file"
  [[ $(stat -c '%u:%g' "$file") == 0:0 ]] || fail "$file must be owned by root:root"
  [[ $(stat -c '%a' "$file") == 600 ]] || fail "$file must have mode 0600"
  [[ -s "$file" ]] || fail "$file is empty"
  LC_ALL=C grep -zEq '^[[:space:]]*[[:xdigit:]]{64}[[:space:]]*$' "$file" || \
    fail "$file must contain exactly one 32-byte seed encoded as 64 hex characters"
  if docker secret inspect "$name" >/dev/null 2>&1; then
    fail "secret already exists: $name; rotate with a separately reviewed procedure"
  fi
done

for id in 1 2 3 4; do
  name="augecoin_validator_${id}_key"
  docker secret create "$name" "$SECRET_DIR/v${id}.key" >/dev/null
  echo "created $name"
done

echo "All validator secrets created. Deploy only the reviewed validator configuration."
