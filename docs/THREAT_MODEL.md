# AUGECOIN — Threat Model

This document catalogs known threats to the AUGECOIN network and the defenses
in place (or planned) for each.

---

### Threat: Byzantine Validator (Equivocation)

- **Attack:** A validator signs and broadcasts two conflicting blocks at the
  same height, attempting to fork the chain.
- **Impact:** Network fork; clients may see inconsistent state; double-spend
  window opens.
- **Existing Defense:** Built-in equivocation detection — duplicate votes at
  the same height generate an equivocation proof that is gossiped and logged.
  Alerts fire via Prometheus and CLI.
- **Missing Defense:** Automatic slashing. The protocol detects equivocation
  but relies on the admin to manually deactivate the offender.
- **Mitigation:** Monitor equivocation alerts; run `augecoin-cli security
  equivocations` on every alert; have a pre-signed deactivation transaction
  ready.

### Threat: Malicious Peer (Spam / Gossip Abuse)

- **Attack:** A peer floods the gossip mesh with invalid messages (empty
  blocks, malformed transactions, duplicate votes) to waste bandwidth and CPU.
- **Impact:** Degraded performance for honest validators; increased mempool
  pressure; potential for eclipse attacks on specific nodes.
- **Existing Defense:** libp2p Noise authentication ensures only peers with
  valid Ed25519 identities can join the mesh. Invalid messages are dropped
  before gossip relay.
- **Missing Defense:** No peer reputation scoring. A peer that sends 10 000
  invalid messages is treated the same as one that sends none.
- **Mitigation:** Operators should configure firewall rules to rate-limit
  incoming P2P connections per IP. Consider a future peer-scoring subsystem
  that banishes repeat offenders.

### Threat: Stolen Validator Key

- **Attack:** An attacker gains access to a validator's Ed25519 private key
  through filesystem compromise, backup exfiltration, or supply-chain attack.
- **Impact:** Attacker can sign conflicting votes (equivocate), stall
  consensus by refusing to vote, or vote for malicious blocks.
- **Existing Defense:** Equivocation detection; admin can deactivate and
  replace the compromised validator. Validator keys are not used for
  transaction signing, so funds are not directly at risk.
- **Missing Defense:** No built-in key rotation ceremony. No automatic
  mechanism to revoke a validator key without admin intervention.
- **Mitigation:** Store validator keys in an HSM or secure enclave. Never
  keep plaintext keys on disk. Run validators on dedicated, hardened hosts.
  Pre-authorize a replacement validator with a future `activation_height`.

### Threat: Stolen Admin Key

- **Attack:** The admin key (a BIP-39 mnemonic) is exfiltrated — through
  phishing, physical theft, or backup compromise.
- **Impact:** Full control of the validator set. Attacker can remove all
  honest validators, install malicious ones, and censor or rewrite the chain
  from the point of takeover.
- **Existing Defense:** The admin key is held by a single trusted party and
  is never loaded onto a validator node. Admin ops require explicit on-chain
  transactions with a future activation height.
- **Missing Defense:** No multisig. No threshold admin (e.g., 3-of-5).
  Single point of failure.
- **Mitigation:** Store the admin mnemonic offline (paper backup in a safe,
  metal backup for fire/flood). Consider air-gapped signing for all admin
  commands. Long-term: implement BLS threshold admin so no single party holds
  the full key.

### Threat: RPC Abuse (DDoS, Unauthorized Admin Ops)

- **Attack:** An attacker floods the JSON-RPC endpoint with requests
  (DDoS) or sends forged admin commands to disrupt the validator set.
- **Impact:** Validator RPC becomes unresponsive; legitimate clients cannot
  submit transactions. Forged admin ops that bypass auth could deactivate
  validators.
- **Existing Defense:** API key authentication; TLS encryption; token-bucket
  rate limiting per IP and per key. Admin ops require a valid admin signature
  and are rejected without it.
- **Missing Defense:** No Web Application Firewall (WAF) integration by
  default. Rate-limit thresholds may need tuning per deployment.
- **Mitigation:** Place the RPC endpoint behind a reverse proxy (nginx, HAProxy)
  with connection limits. Rotate API keys regularly. Monitor
  `augecoin_rpc_ratelimit_hits_total`.

### Threat: DDoS (P2P Network Flood)

- **Attack:** An attacker opens thousands of libp2p connections to each
  validator, exhausting file descriptors and bandwidth.
- **Impact:** Validators cannot communicate; consensus stalls; network halts.
- **Existing Defense:** libp2p connection limits per peer. Noise handshake
  has computational cost for the attacker.
- **Missing Defense:** No application-level IP reputation. No automatic
  blackholing of high-connection-rate peers.
- **Mitigation:** Configure kernel-level connection tracking limits
  (`conntrack`). Use iptables/nftables to rate-limit new inbound connections.
  Deploy validators behind load balancers with SYN-cookie protection.

### Threat: Replay Attack (Old Operations)

- **Attack:** An attacker captures a valid, signed message (transaction or
  admin command) and resubmits it later to repeat the operation.
- **Impact:** Duplicate transactions; unintended repeated admin actions
  (e.g., removing the same validator twice).
- **Existing Defense:** Every transaction includes a **nonce** (sequential
  counter per account) that is checked by the state machine. Once consumed,
  the nonce cannot be reused. Blocks include a monotonically increasing
  height.
- **Missing Defense:** None identified. Nonce + block height provide strong
  replay protection.
- **Mitigation:** No additional action required. Ensure clients never reuse
  nonces and always query the latest account state before signing.

### Threat: Equivocation (Leader and Non-Leader)

- **Attack:** The **leader** proposes two different blocks for the same
  height. A **non-leader** validator votes for two conflicting proposals at
  the same height.
- **Impact:** Both cases create a fork. If 3 validators equivocate, a
  conflicting block could be finalized.
- **Existing Defense:** The consensus layer (PBFT-style) detects duplicate
  proposals and duplicate votes. Any validator can construct an equivocation
  proof from two signed conflicting messages. The proof is gossiped network-wide.
- **Missing Defense:** No automatic slashing transaction. Detection does
  not imply punishment without the admin.
- **Mitigation:** Same as Byzantine Validator threat. Ensure monitoring
  catches equivocation events immediately. Pre-authorize replacement
  validators.

### Threat: Database Corruption

- **Attack:** A validator's local RocksDB (or equivalent) becomes corrupted
  due to disk failure, bit-rot, or an operational mistake (e.g., `rm -rf data/`).
- **Impact:** Validator cannot participate in consensus; if 2 validators are
  affected simultaneously, finality stalls.
- **Existing Defense:** Validators can resync from a healthy peer. The
  protocol records committed blocks deterministically, so a fresh node can
  rebuild its entire state from genesis.
- **Missing Defense:** No built-in snapshot/backup mechanism. No checksum
  verification of on-disk blocks.
- **Mitigation:** Operators should schedule regular filesystem snapshots
  (ZFS, LVM). Run at least one standby validator that stays synchronized.
  Monitor disk SMART metrics.

### Threat: Network Partition

- **Attack:** A network-level partition splits the 4 validators into two
  groups (e.g., 2 + 2, or 3 + 1).
- **Impact:** With 2+2, neither group reaches the 3-vote quorum — **chain
  halts**. With 3+1, the group of 3 continues finalizing while the lone
  validator diverges, then resyncs when the partition heals.
- **Existing Defense:** The BFT consensus model is partition-aware.
  Validators that fall behind detect a gap in block height and trigger a
  state sync from peers.
- **Missing Defense:** No automatic partition detection alert. The network
  relies on external monitoring (Prometheus, Grafana, ICMP pings).
- **Mitigation:** Deploy validators across at least 3 physically separate
  data centers or cloud regions. Monitor inter-validator latency. Use
  redundant network paths (BGP, dual uplinks).

### Threat: Mempool Spam

- **Attack:** An attacker submits millions of validly-signed but
  economically useless transactions (e.g., 0-value transfers) to exhaust
  mempool memory.
- **Impact:** Validator memory usage spikes; legitimate transactions are
  dropped or delayed; block creation slows down.
- **Existing Defense:** Mempool has a configurable size limit. Transactions
  pay a minimum fee, raising the cost of spam. Nonce ordering prevents the
  attacker from filling blocks with their own transactions exclusively.
- **Missing Defense:** No fee-bumping (RBF). No priority queue beyond fee
  per byte. No mempool transaction eviction based on age or utility.
- **Mitigation:** Set a conservative `mempool_max_size` and `min_fee`.
  Monitor mempool depth via `augecoin_mempool_transactions_total`. Rate-limit
  transaction submission at the RPC layer.

### Threat: Invalid Serialization (Malformed Blocks / Transactions)

- **Attack:** An attacker sends a block or transaction with deliberately
  malformed binary encoding — invalid varint lengths, oversized fields,
  or type-confused payloads.
- **Impact:** If the deserializer panics or exhibits undefined behavior,
  the validator process crashes (DoS). In the worst case, a memory-safety
  bug could allow remote code execution.
- **Existing Defense:** Deserialization uses Rust's `serde` with strict
  schema validation. All variable-length fields have maximum bounds enforced
  at parse time. `bincode` / custom `Read` impls reject oversized payloads
  early.
- **Missing Defense:** No fuzz-testing harness documented in CI. No
  canary/guard-page protections specific to deserialization buffers.
- **Mitigation:** Run validators under a process supervisor (systemd) with
  auto-restart. Fuzz the deserializer with `cargo fuzz`. Reject any peer
  that sends repeatedly invalid messages.

### Threat: Supply Inflation (Emission Bug)

- **Attack:** A software bug (intentional or accidental) causes the emission
  schedule to mint more coins than the protocol intends — e.g., an integer
  overflow or missing cap check.
- **Impact:** The total supply exceeds the hard cap, devaluing all existing
  coins. Trust in the network collapses.
- **Existing Defense:** Emission is computed deterministically per block.
  The supply cap is enforced in the state-machine transition logic. Block
  validation on all nodes rejects blocks that mint more than the allowed
  amount.
- **Missing Defense:** No independent supply-verification tool shipped to
  non-validator nodes. The emission scheme is not formally verified.
- **Mitigation:** Run periodic supply audits (`augecoin-cli supply audit`).
  Configure a Prometheus alert on `augecoin_total_supply > EXPECTED_CAP`.
  All validator operators should independently verify the emission logic
  in the open-source code before deploying.

### Threat: State Divergence (Non-Determinism)

- **Attack:** A non-deterministic operation in the state-machine execution
  (e.g., floating-point math, map iteration order, platform-specific
  behavior) causes validators that processed the same block to arrive at
  different state roots.
- **Impact:** No block can be committed because validators disagree on the
  post-state. Consensus stalls permanently at that height.
- **Existing Defense:** The state machine is written in Rust with
  deterministic data structures (`BTreeMap` for iteration order; integer-only
  arithmetic). Block execution is single-threaded and must produce identical
  results on all platforms.
- **Missing Defense:** No differential state-fuzzing that compares the
  state root across different CPU architectures. No deterministic replay
  tool for CI.
- **Mitigation:** Run validators on the same Rust toolchain version (pinned
  via `rust-toolchain.toml`). Never introduce `HashMap` or floating-point
  operations in consensus code. CI should verify state roots against golden
  fixtures for a fixed set of known blocks.

### Threat: 0-Confirmation Double Spend

- **Attack:** A user broadcasts two conflicting transactions spending the
  same coins to different recipients before either is included in a block
  (0-conf race).
- **Impact:** One recipient sees an unconfirmed transaction and accepts
  it as payment, but the other transaction gets included in the block. The
  first recipient loses funds.
- **Existing Defense:** The state machine enforces strict nonce ordering
  per account — only one transaction per nonce can be committed. The
  mempool and block proposer both select the first-seen transaction for
  a given nonce.
- **Missing Defense:** No first-seen cryptographic commitment across
  validators. No fraud proof for 0-conf double-spend attempts.
- **Mitigation:** Merchants and exchanges should wait for finality (1 block
  confirmation, ~10 minutes in the current block cadence). Do not accept
  0-conf transactions for high-value transfers. Consider tracking the
  `augecoin_double_spend_detections_total` metric for network health.
