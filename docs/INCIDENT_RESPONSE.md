# AUGECOIN — Incident Response Plan

## Overview

This document defines the detection, response, and recovery procedures for
security and operational incidents on the AUGECOIN network. All validators
and the admin key holder must have access to this document.

---

## 1. Validator Key Compromise

**Detection:**
- Equivocation alert fired (`augecoin_equivocation_events_total` > 0)
- CLI alert: `augecoin-cli security alerts --tail` shows `equivocation` event
- Grafana dashboard: Security Events panel turns red

**Response:**
1. Verify equivocation proof via `augecoin-cli security equivocations`
2. Deactivate compromised validator:
   ```
   augecoin-cli validator deactivate --id <ID> --activation-height <N+10> --mnemonic <ADMIN>
   ```
3. Remove validator permanently:
   ```
   augecoin-cli validator remove --id <ID> --activation-height <N+20> --mnemonic <ADMIN>
   ```
4. Generate new validator keys on isolated machine
5. Add new validator with future activation_height

**Recovery time:** ~20 blocks (20 minutes)

---

## 2. Consensus Bug (Block Finalization Stops)

**Detection:**
- Block height stops incrementing on Prometheus/Grafana
- CLI: `augecoin-cli status` shows `healthy: false`
- Validator logs show repeated ViewChange without Commit

**Response:**
1. Check active validators: `augecoin-cli validator list`
2. Check quorum: `augecoin-cli status` shows `quorum.active_validators < quorum.quorum_threshold`
3. If ≥3 validators active: wait for timeout recovery (automatic via ViewChange)
4. If <3 validators active: admin must activate/reactivate validators
5. If consensus logic bug suspected: halt all validators, investigate code, deploy fix

**Recovery time:** 2-5 minutes (auto-recovery) or hours (code fix)

---

## 3. Network Partition

**Detection:**
- Peers connected drops: `augecoin_peers_connected` < 4
- CLI: `augecoin-cli status` shows low peer count
- Block production continues but finalization may stall if <3 validators connected

**Response:**
1. Identify partition boundaries (which validators can see each other)
2. Automatic behavior:
   - 3+ validators connected → quorum reached, blocks finalize normally
   - 2 validators connected → blocks NOT finalized (fail-safe), no double-spend risk
3. When partition resolves: validators reconnect, sync missed blocks via fast-sync (FASE-05D)
4. No manual intervention needed for standard partition recovery

**Recovery time:** Automatic upon network restoration

---

## 4. Replay Attack Attempt

**Detection:**
- Mempool rejects transaction with `InvalidOpSequence`
- Alert: `[ALERT-INFO] replay_attempt | sender <N>, op_sequence <M> reused`
- CLI: `augecoin-cli security alerts --tail` shows replay events

**Response:**
1. Replay is automatically rejected by mempool (FASE-06A)
2. No funds at risk — op_sequence anti-replay is enforced at protocol level
3. If persistent from same peer: auto-ban triggers (FASE-05E)
4. Review banned peers: `augecoin-cli security banned-peers`
5. Rate limit prevents flooding

**Recovery time:** Immediate (automatic rejection)

---

## 5. Storage Corruption

**Detection:**
- RocksDB returns checksum error on read
- Node logs: `[ERROR] storage corruption detected`
- Block execution fails with `Storage` error

**Response:**
1. Stop the affected node immediately
2. Identify last valid checkpoint: `augecoin-cli status` to get current height
3. Delete corrupted data directory
4. Restart node with fast-sync (FASE-05D):
   - Node downloads latest checkpoint from peers (FASE-03C)
   - Replays remaining blocks
   - Returns to full sync automatically
5. If all nodes corrupted simultaneously: restore from backup, regenerate from genesis

**Recovery time:** Minutes (checkpoint + fast-sync) to hours (genesis replay)

---

## 6. DoS / Resource Exhaustion

**Detection:**
- Rate limit exceeded logs on RPC
- Mempool size spikes on Prometheus/Grafana
- Peer connections spike

**Response:**
1. Rate limiting automatically throttles excessive clients (FASE-07C)
2. Peers sending invalid data repeatedly are auto-banned (FASE-05E)
3. Review: `augecoin-cli security banned-peers`
4. If attack persists: firewall-level IP blocking at infrastructure layer
5. Scale horizontally: add observer nodes for read-only RPC load

**Recovery time:** Immediate (automatic rate limiting + banning)

---

## 7. Admin Key Compromise

**Detection:**
- Unauthorized validator changes observed on-chain
- Unexpected `ValidatorAdmin` transactions in blocks
- `augecoin-cli validator list` shows unknown validators

**Response (CRITICAL — highest severity):**
1. Immediately halt all validators
2. Audit validator set: identify unauthorized changes
3. Cannot rotate admin key on current chain (single-key model)
4. **Requires genesis fork:**
   a. Generate new admin keypair on air-gapped machine
   b. Create new genesis.json with new admin_public_key
   c. Export current state (all account balances)
   d. Create new chain from new genesis, importing state
   e. All validators upgrade to new chain
5. This is a worst-case scenario — admin key security is paramount

**Prevention:**
- Admin mnemonic stored offline (hardware wallet, paper backup in safe)
- Never used on internet-connected machine except for signing via CLI
- CLI signs locally, only signed transaction leaves device

---

## Communication Plan

| Channel | Purpose | Audience |
|---|---|---|
| `augecoin-cli security alerts --tail` | Real-time alert stream | Validator operators |
| Prometheus Alertmanager (future) | Automated alerting | On-call engineers |
| Grafana dashboard | Visual monitoring | All operators |
| Out-of-band (Signal/Telegram) | Emergency coordination | Validator operators + admin |

---

## Escalation Matrix

| Severity | Condition | Response Time | Escalation |
|---|---|---|---|
| Critical | Admin key compromise, consensus halt >10 blocks | Immediate | All validators + admin |
| High | Equivocation detected, storage corruption | <5 minutes | Affected validator + admin |
| Medium | Rate limit triggers, peer banned | <30 minutes | Operator review |
| Low | Replay attempt, single tx rejection | Next business day | Automated |
