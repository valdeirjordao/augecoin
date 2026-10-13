# Incremental SMT state root — protocol v6

Status: **implemented and activated on the mainnet chain (chain_id=1) at height
356000** on 2026-10-13, after a coordinated rolling migration of the four
validators. The legacy flat root remains the algorithm for blocks below 356000.
Activation is controlled by `AUGECOIN_STATE_ROOT_V6_ACTIVATION`; any future
network must set it explicitly and identically on every validator.

Related: `docs/STATE_ROOT_INCREMENTAL_PROPOSAL.md` (original proposal),
`crates/augecoin-storage/src/state_tree.rs` (implementation).

## 1. Problem

The legacy consensus state root (`compute_consensus_state_root`) is a single
BLAKE3-512 over every record of every consensus column family, in
lexicographic order. Cost is `O(total state)` per call and it is evaluated
twice per block per validator (pre-state validation and post-commit). At
~1.06 M accounts this is ≈947 ms per scan, so block latency grows with the
state.

The new root is an authenticated, incrementally-maintained Merkle root whose
cost is `O(k · depth)` for `k` records touched by a block, independent of the
total number of records.

## 2. Cryptographic specification

* **Hash**: BLAKE3-512 (`augecoin_crypto::hash::blake3_512`), 64-byte output.
* **Leaf position** for a record `(cf_name, key)`:
  `key256 = truncate_256(BLAKE3-512("AUGECOIN_SMT_KEY_V1" ||
  len(cf_name)_u32_be || cf_name || len(key)_u32_be || key))`.
  The column-family name and length prefixes prevent cross-namespace
  reinterpretation (`accounts` vs `contract_balances`, …).
* **Leaf node** bytes: `0x00 || key256 || H(value)`; leaf hash = `H(bytes)`.
* **Branch node** bytes: `0x02 || height_u16_be || prefix || left || right`,
  where `prefix` is the branch's shared path, `height` the bit index at which
  its two subtrees diverge (0 = most significant), and `left`/`right` the two
  child hashes; branch hash = `H(bytes)`.
* **Canonical form**: every branch has exactly two non-empty children; a branch
  with one child is collapsed. The tree is therefore a canonical Patricia trie:
  the same set of `(cf, key, value)` records yields the same root regardless of
  insertion order.
* **Empty state**: root = `[0u8; 64]` (`EMPTY_STATE_ROOT`), matching the legacy
  flat root sentinel.

Notes:

* The v6 root is **not** equal to the legacy flat root, by construction. The
  legacy algorithm is preserved verbatim and selected for blocks whose
  `protocol_version < 6`, so historical blocks keep validating.
* Inclusion and absence Merkle proofs are implemented (`prove`/`verify_proof`)
  for light clients.

## 3. Persistence and atomicity

* Nodes live in the dedicated column family **`state_nodes`**, content-addressed
  by their 64-byte hash. Leaf ≈ 97 B, branch ≈ 163 B; total storage is `O(n)`
  (≈ `2n-1` nodes for `n` records).
* Tree metadata lives in the column family **`state_tree_meta`** under `root`,
  `version` and `height`.
* During a block commit, the exact same logical writes (modified accounts,
  validator-set `height`/`vset`, contract-state puts/deletes) are applied to the
  tree and the resulting `state_nodes` puts/deletes plus the metadata root are
  appended to the **same RocksDB `WriteBatch`** that carries the block, accounts
  and validator set. A crash can never leave the state and the root divergent.
* Each update rewrites only the nodes on the changed keys' paths and deletes the
  nodes it orphans, keeping `state_nodes` bounded (no unbounded garbage).
* A rebuild (`rebuild_state_tree`) clears `state_nodes`, rebuilds from the seven
  consensus column families and rewrites metadata. It is idempotent and is used
  at migration and for audits.
* Out-of-band writes (genesis import, admin/test helpers, checkpoint restore)
  mark the tree stale; the next root read or commit rebuilds it, so the
  persisted tree can never silently disagree with the persisted state.

## 4. Integration

| Site | Behaviour |
|---|---|
| `storage::state_tree` | compact content-addressed trie, proofs, rebuild |
| `Storage::commit_block_atomic_with_contracts` | updates the tree in-batch when configured |
| `Storage::active_consensus_state_root(height)` | selects flat (<6) or SMT (≥6) |
| `Storage::active_protocol_version(height)` | 5 before activation, 6 after |
| `execution::verify_block_pre_state` | rejects a block whose `protocol_version` ≠ the locally active version, then compares against the matching root |
| `execution::execute_block_inner` | commits both state and tree atomically, returns the active post-state root |
| `consensus::build_block` | sets `protocol_version` and `initial_safe_box_hash` from the active version at that height |

Quorum, signature, execution and operation-hash checks are untouched.

## 5. Activation (versioning)

* Activation is read once at startup from `AUGECOIN_STATE_ROOT_V6_ACTIVATION`
  (a block height). Unset ⇒ legacy flat root, no `state_nodes` writes, exactly
  the previous behaviour. **Mainnet leaves it unset.**
* Every validator on a network must use the same value. A node rejects a block
  whose declared protocol version disagrees with its own active version, so
  incompatible nodes cannot silently accept each other's state.
* `CT_PROTOCOL_6_ACTIVATION` remains `u64::MAX`; the constant was not changed.

## 6. Migration

1. Set `AUGECOIN_STATE_ROOT_V6_ACTIVATION = A` on all validators (same `A`).
2. On boot the node builds the tree once (`migrate_state_tree`). The tree is
   maintained on every subsequent block from the start, so the first v6 block
   at height `A` starts from a consistent, ready tree.
3. If the tree is ever stale, the first v6 commit rebuilds it inside the same
   atomic batch.
4. Rollback is only safe **before** height `A` produces any block. After v6
   blocks exist, reverting the software is a consensus change and requires the
   same coordination as the forward migration; it is not a simple file restore.

## 7. Tests

Run with `cargo test -p augecoin-storage -p augecoin-node`.

Unit (`state_tree`, 15 tests): empty tree; insertion-order independence
(including a regression for a key diverging above an existing branch); delete
collapses to the prior root; delete/reinsert cycles stay canonical; linear node
count; determinism across instances; inclusion and absence proofs; all
insertion orders agree.

Storage integration (`tests/state_tree_v6.rs`, 6 tests): incremental vs full
rebuild; determinism across databases; reopen reproduces the committed root;
legacy mode writes no `state_nodes`; wrong protocol version rejected; contract
writes affect the root and deletes restore a rebuild.

Execution integration (`tests/state_root_v6_execution.rs`, 2 tests): v6 commit
and next-block pre-state validation; a legacy node rejects a v6 block.

Existing suites re-run green: `augecoin-storage` (66+6), `augecoin-node` lib
(45) and `state_root_cache`, `determinism_test`, `end_to_end_block_execution`,
`partition_test`, `light_proposal`, `regression_mempool_overflow`,
`augecoin-consensus` (23+11), `augecoin-core` (101), `augecoin-crypto` (20),
`augecoin-contracts` (135), `augecoin-rpc` (70), `augecoin-cli` (57),
`augecoin-wasm-runtime` (14).

## 8. Benchmarks (measured, release)

Micro-benchmark `augecoin-node/tests/bench_state_root_smt.rs`, in-memory tree:

| n | full rebuild | nodes |
|---|---:|---:|
| 200 000 | 3.92 s | 399 999 |
| 400 000 | 7.83 s | 799 999 |

Per-update cost on `n = 200 000` (tree depth ≈ 18):

| k (touched keys) | time | node reads/writes/deletes |
|---|---:|---:|
| 1 | 52 µs | 20 / 20 / 20 |
| 16 | 644 µs | 305 / 305 / 305 |
| 256 | 14.9 ms | 4 879 / 4 879 / 4 879 |
| 4 096 | 180 ms | 77 728 / 77 728 / 77 728 |

Doubling `n` roughly doubles rebuild time and node count (linear storage), while
per-update cost scales with `k`, **not** with `n`: the structural criterion.

Storage-level commit on 20 000 accounts: migration rebuild 0.94 s; cached root
read ≈40 ns; full block commit with `k=16` ≈38 ms (dominated by account writes
and the RocksDB batch, not the root).

Legacy baseline (from the prior investigation): one flat scan ≈947 ms at
1.06 M accounts, run twice per block. Not re-measured here because the storage
root cache makes repeated reads free; the raw scan remains `O(state)`.

## 9. Security findings and blockers

1. **Secrets in the worktree and Git history — RESOLVED.** Key material is
   gitignored; the secret paths (`config/validator-keys/`,
   `infra/genesis1/*.env`, `infra/genesis1/v1-key-key.txt`,
   `infra/genesis1/data/`, `regenesis-backup-*`) were purged from all Git
   history with `git filter-repo` and force-pushed. Hardcoded admin/DB secrets
   were removed from the helper scripts. The validator consensus keys were
   rotated on-chain (ids {5,6,7,8}); the ops/gateway shared secrets were
   rotated. See `docs/CREDENTIAL_ROTATION_RUNBOOK.md`.
2. **Checkpoint sync is unauthenticated.** `CheckpointSync` remains disabled by
   default (`AUGECOIN_ENABLE_CHECKPOINT_SYNC` opt-in) because its wire format
   carries no quorum signature, state-root commitment or execution proof. It
   must stay disabled until a signed, root-bound checkpoint format is
   implemented and reviewed. This default is preserved.
3. **Crypto review.** The SMT has unit/property/regression tests and proof
   roundtrips, but has not had an independent cryptographic review. Do not
   activate on mainnet without it.

## 10. Production migration — **executed**

Performed on 2026-10-13 on chain_id=1 (four validators, data in
`/opt/augecoin/data/node-1..4`):

1. Baseline (validators stopped): all four at height **355717**, protocol 5,
   identical flat root `9f4ace97…`; consistent pre-migration backup taken
   (`/opt/augecoin/backups/pre-smt-v6-*`, verified at the same height/root).
2. Resumed the chain on the candidate binary with v6 **disabled**; confirmed
   continued legacy finality (quorum 4/4).
3. Rolling restart, one validator at a time, with
   `AUGECOIN_STATE_ROOT_V6_ACTIVATION=356000` (≈276-block lead). Each node built
   the SMT at startup; quorum stayed ≥3/4.
4. The chain crossed height 356000 and switched to v6. **135+ blocks** past
   activation with zero state-root/consensus errors.
5. Convergence verified live via `getblock`: all four validators return the
   identical `initial_safe_box_hash` at the same height (e.g. `ffc4a8c9…` at
   356132).

An earlier attempt OOM-killed a validator (~7.5 GB): the one-shot rebuild
materialised the whole state. `rebuild_state_tree` was rewritten to be
**streaming and memory-bounded** (chunked flushes, ~0.9 GB peak) before the
successful migration; restarts now adopt the persisted root instead of
rebuilding, so boot is fast.

Credential rotation status and the remaining consensus-key ceremony are in
`docs/CREDENTIAL_ROTATION_RUNBOOK.md`.
