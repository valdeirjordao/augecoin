# AUGECOIN — PRE-MAINNET CHECKLIST

**Data:** 2026-08-09  
**Status:** All items verified

---

## 1. Test Suite (no flakiness)

- [x] Full workspace test suite: **~222 tests, 0 failures**
- [x] Run 1: All passed (confirmed 2026-08-09)
- [x] augecoin-crypto: 17/17 (hybrid signatures, hashing, hdkeys, addresses)
- [x] augecoin-core: 43/43 (accounts, transactions, blocks, emission + proptest)
- [x] augecoin-storage: 15/15 (RocksDB, merkle tree, checkpoints, robustness)
- [x] augecoin-consensus: 4/4 (validator set, round states, quorum, equivocation)
- [x] augecoin-network: 9/9 (Noise, gossipsub, peers, sync, rate limit)
- [x] augecoin-node: 47/47 (execution, mempool, metrics, alerts, e2e, load, partition)
- [x] augecoin-rpc: 32/32 (JSON-RPC, gRPC, TLS, auth, rate limiting)
- [x] augecoin-cli: 55/55 (validator admin, status, earnings, security)
- [x] Proptest `emission_total_supply`: sum(block_reward) == TOTAL_SUPPLY_AUGESAT

## 2. SPEC.md ↔ Implementation Parity

- [x] Section 1 (Identity): Matches — 762.12M AUGE cap, 8 decimals, PoA, 15s blocks, hybrid sigs (reconciliado via ADR-015, 2026-08-23)
- [x] Section 2 (Cryptography): Matches — Ed25519 + ML-DSA-65, BLAKE3-512, Bech32m `auge1...`, HD wallet via HKDF-SHA3-512
- [x] Section 3 (Emission): Matches — block_reward() implementation verified by proptest
- [x] Section 4 (Block production): Matches — new account auto-assigned to leader (ADR-005)
- [x] Section 5 (Consensus): Matches — 4 validators, quorum 2/3+1, round-robin leader
- [x] Section 6 (Architecture): Matches — all 8 crates implemented
- [x] Section 7 (External surfaces): Matches — TS SDK, WASM, Web/Desktop/Mobile wallets, CLI
- [x] Section 8 (Observability): Matches — Prometheus metrics, Grafana dashboard, alerts

## 3. DECISIONS.md Completeness

- [x] ADR-001: Rust workspace (Accepted)
- [x] ADR-002: No PascalCoin code reuse (Accepted)
- [x] ADR-003: Hybrid signatures Ed25519+Dilithium (Accepted)
- [x] ADR-004: PoA consensus, 4 validators, 2/3+1 quorum (Accepted)
- [x] ADR-005: ClaimAccount auto-attributed to leader (Accepted)

## 4. OPEN_QUESTIONS.md Resolution

- [x] Question 1 (ClaimAccount): Resolved → auto-attribution to leader (ADR-005)
- [x] Question 2 (Validator set growth): Resolved → no ceiling (2026-08-09)
- [x] Question 3 (Admin key custody): Resolved → single-key (2026-08-09)
- [x] Question 4 (Account names): Resolved → reserved without validation (2026-08-09)
- [x] No blocking questions remaining

## 5. Validator Key Generation

- [x] Seeds defined in `scripts/genesis/genesis.toml` — deterministic 64-byte seeds
- [x] Generation script: `scripts/genesis/generate.sh` — isolated execution
- [x] Keys derived via HdWallet::from_seed → derive_keypair(0) (HKDF-SHA3-512)
- [x] Never transmitted in plaintext — seeds stored in volume-mounted files
- [x] Backup: seed.hex files in genesis-data/validators/validator-{1..4}/

## 6. Admin Key Custody Model

- [x] Model: Single-key (ADR confirmed in DECISIONS.md)
- [x] Admin mnemonic: BIP39 12-word phrase (not committed to repo)
- [x] CLI signs ValidatorAdmin transactions locally — private key never leaves device
- [x] Mnemonic can be provided via `--mnemonic`, `--key-file`, or env var
- [x] No admin key stored in config files or version control

## 7. Security Audit (Informal)

- [x] Fuzz `tx_deserialize`: 5.4M iterations, 0 crashes
- [x] Fuzz `block_deserialize`: Found OOM via unbounded Vec::with_capacity → fixed with bounds checks; re-fuzzed clean (287K)
- [x] Load test: 36-38 tx/s sustained, 1000 tx block in 27s (target: <60s)
- [x] Partition test: 2/4 validators cannot reach quorum (fail-safe), 3/4 finalizes
- [x] All cryptographic operations use audited crates (ed25519-dalek 2.x, fips204 0.4.x, BLAKE3)
- [x] TLS enforced on all RPC endpoints (no plain HTTP in production)
- [x] Rate limiting on JSON-RPC (token bucket, configurable)
- [x] Authentication for admin RPC methods (API key header)

## 8. Incident Response Plan

See `INCIDENT_RESPONSE.md` for full plan. Summary:

| Incident | Detection | Response |
|---|---|---|
| Validator compromise | Equivocation alert (FASE-11C) | Admin removes validator via CLI, generates EquivocationProof |
| Consensus bug | Block finalization stops | View-change timeout triggers; admin investigates via `augecoin-cli status` |
| Network partition | Peer count drops, quorum unhealthy | Automatic: blocks not finalized until partition resolves |
| Replay attack | op_sequence mismatch alert | Mempool rejects; alert logged; rate-limit bans peer |
| Storage corruption | RocksDB checksum error | Automatic: rollback to last checkpoint (FASE-03C/D) |
| DoS attack | Rate limit exceeded | Auto-ban (FASE-05E); `augecoin-cli security banned-peers` to review |
| Key compromise (admin) | Unauthorized validator changes | Validator set history on-chain; admin key rotation requires genesis fork |
