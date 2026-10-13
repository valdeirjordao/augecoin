# Credential rotation runbook

## Status of the exposure

Real secrets existed (values never printed) in:

* `config/validator-keys/{admin,v1,v2,v3,v4}.env` — validator signing seeds and
  admin material (untracked at HEAD, **also present in Git history**).
* `infra/genesis1/{v1-key-key.txt,v2..v4.env}` — validator key material.
* `scripts/start-mainnet.sh` / `scripts/start-validators.sh` — hardcoded admin
  API key default.

## Done (no consensus impact)

* `.gitignore` now excludes `config/validator-keys/`, `*.env`, `*.key`,
  `*.pem`, `*-key-key.txt`, `**/secrets/`.
* `scripts/start-validators.sh` no longer ships a default admin key.
* File permissions tightened to `0700`/`0600` for all key files and dirs.
* The admin **API** key line in `config/validator-keys/admin.env` was rotated
  (old value backed up to `admin.env.bak-<ts>`); this rotates the value that was
  previously committed. It affects only the local admin panel, not consensus.
* Fresh key material provisioned under `config/validator-keys-new/` (mode 0600,
  untracked): four new ed25519 signing seeds and a new admin API key, for the
  on-chain ceremony below.

## Still required

### 1. Purge Git history (must be done before any public push)

The old seeds remain recoverable from history. Use `git filter-repo` (or BFG)
to remove `config/validator-keys/`, `infra/genesis1/*.env`,
`infra/genesis1/*-key-key.txt` and any `regenesis-backup-*/envs/*` from **all**
commits and tags, then force-push and have every clone re-clone. Rotating the
secrets (below) also makes the old history worthless.

### 2. Validator consensus key rotation (coordinated ceremony)

The validator signing keys are the critical exposure: a leaked seed allows
equivocation. The set is changed on-chain with the activation-height–gated admin
operations (`scripts/validator-admin.sh`, `crates/augecoin-ops`). Rotate **one
validator at a time** so quorum (3/4) is never lost:

For each validator `i` in 1..4:
1. Bring up a new node instance for `i` using the new seed from
   `config/validator-keys-new/v$i.env` (isolated data dir and ports).
2. `validator-admin.sh <admin-mnemonic> <rpc-port> add <new_ed25519_pubkey> <A_i>`
   to add the new key with activation height `A_i` ≈ current + 200.
3. Wait until the set activation height `A_i` is reached and the new instance is
   signing/producing (`validator list` shows it active).
4. `validator-admin.sh ... deactivate <old_id> <A_i + k>` then
   `remove <old_id> <A_i + k>` to retire the old key.
5. Confirm the network stays at quorum and roots converge, then move to `i+1`.

Prerequisites: the node admin RPC must have `AUGECOIN_ADMIN_API_KEYS` set
(currently the running validators have none, so admin methods are blocked), and
the offline admin mnemonic/key must be available (from the secure store, not the
repo). The on-chain **admin key** itself cannot be changed live and requires a
governance/genesis action; treat it as separately managed.

Rehearse the whole ceremony on the isolated testnet
(`scripts/testnet-smt-4node.sh`) before touching mainnet.

### 3. Ops / gateway / node shared secrets

* `GATEWAY_JWT_SECRET`, `GATEWAY_ADMIN_BOOTSTRAP_KEY` (gateway systemd unit +
  Postgres `api_keys`/`developers` rows).
* `AUGECOIN_OPS_ADMIN_KEY`, `AUGECOIN_OPS_NODE_ADMIN_KEY` (ops unit).
* Node `AUGECOIN_ADMIN_API_KEYS` (must match `AUGECOIN_OPS_NODE_ADMIN_KEY`).

Rotate by generating new values, updating the systemd drop-ins
(`systemctl edit <unit>`), `daemon-reload`, restarting the unit, and updating the
matching row/hash in Postgres. These do not affect consensus.

### 4. Post-rotation verification

* All validators on the new keys, quorum 4/4, identical `initial_safe_box_hash`
  across nodes (via `getblock`).
* Ops/gateway authenticate with the new secrets; old secrets rejected.
* Git history scan clean for key material.