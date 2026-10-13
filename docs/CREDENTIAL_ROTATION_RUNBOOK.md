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

### 2. Validator consensus key rotation — **DONE (2026-10-13)**

Executed live on chain_id=1 with the activation-height `validatoradd` /
`validatordeactivate` + `validatorremove` mechanism, one validator at a time,
each new node synchronising from a copy of an existing DB:

| step | added id (new key) | retired old id |
|---|---|---|
| 1 | 5 | 1 |
| 2 | 6 | 2 |
| 3 | 7 | 3 |
| 4 | 8 | 4 |

The on-chain active set is now **{5,6,7,8}**; the original {1,2,3,4} keys are
deactivated and removed. All four new validators converge on the same v6 root
and the chain kept quorum throughout. New signing seeds are in
`config/validator-keys-new/` (mode 0600, untracked); the new public keys are in
the corresponding `*.pub` files. `scripts/start-mainnet-current.sh` resumes this
rotated set (ids 5..8, data dirs `node-5..8`).

For a future rotation, repeat the same procedure with new ids/keys; the
per-step runbook remains:

For each validator `i`:
1. Bring up a new node instance for `i` using a fresh seed (data dir copied from
   a running peer so it starts at the current height).
2. `validatoradd <new_pubkey> <A_i>` with `A_i` ≈ current + 40.
3. Wait until `A_i`; confirm the new id signs blocks.
4. `validatordeactivate <old_id> <current>` then `validatorremove <old_id> <current>`.
5. Stop the old instance; confirm quorum and identical roots before the next `i`.

Prerequisites: a node with `AUGECOIN_ADMIN_KEY_HEX` (offline admin key) and
`AUGECOIN_ADMIN_API_KEYS` (admin panel key) configured, called with the
`x-api-key` header.

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