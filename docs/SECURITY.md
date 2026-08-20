# AUGECOIN — Security Policy

## Reporting a Vulnerability

If you discover a security vulnerability in AUGECOIN, please do **not** open a
public issue. Instead, send an encrypted report to:

- **Email:** [security@augecoin.org] <!-- TODO: replace with real contact -->
- **PGP Key:** [TODO: publish PGP public key fingerprint]

We aim to acknowledge reports within 48 hours and provide an initial assessment
within 7 days. Please include as much detail as possible: affected component,
steps to reproduce, and potential impact.

---

## Supported Versions

| Version            | Status              |
|--------------------|---------------------|
| `v1.x` (latest)    | :white_check_mark: Supported |
| `v0.x` (pre-mainnet) | :x: End of life   |

Only the latest stable release line receives security patches. Pre-mainnet
releases are unsupported and may contain known vulnerabilities.

---

## Security Model

### Consensus Security

AUGECOIN uses a Proof-of-Authority consensus based on Byzantine Fault
Tolerance (BFT). The validator set is fixed at **4 validators** and a
quorum of **2/3 + 1** (i.e., 3 out of 4) is required to finalize a block.

- A single malicious validator cannot halt or corrupt the chain.
- Two colluding validators can stall finality but cannot forge committed
  blocks, because 3 votes are needed for a commit.
- Validators that equivocate (sign conflicting blocks at the same height)
  are detected automatically and an alert is raised. The admin can
  deactivate and replace a misbehaving validator on-chain.

### Key Management

AUGECOIN separates cryptographic keys by role:

| Key    | Purpose                                              |
|--------|------------------------------------------------------|
| **Validator key** | Sign consensus votes (pre-prepare, prepare, commit) |
| **Admin key**     | Validator-set management (add/remove validators), protocol upgrades |
| **User key**      | Sign transactions (transfer, mint, burn)            |

- Validator keys are never used to sign transactions.
- The admin key is held by a trusted party and never exposed to automated
  processes.
- Losing the user key means permanent loss of funds; there is no key
  recovery mechanism.

### Network Security

Peer-to-peer communication uses the **libp2p** framework with:

- **Noise** protocol for authenticated encryption of all gossip messages
  between peers.
- **TLS** for RPC and client–validator connections.
- Peer identity is derived from Ed25519 keys; spoofing a peer requires
  compromising its private key.

### RPC Security

The JSON-RPC endpoint is protected by:

- **API keys** — each client must authenticate with a pre-shared key.
- **TLS** — all RPC traffic is encrypted in transit.
- **Rate limiting** — a token-bucket limiter caps requests per client IP
  and per API key to prevent resource exhaustion.
- Admin operations (validator management, protocol upgrades) require the
  admin key and are rejected from unauthenticated sources.

---

## Known Limitations

### Post-Quantum Cryptography Not Active

Dilithium-based post-quantum signatures were removed during development.
All signatures currently rely on classical elliptic-curve cryptography
(Ed25519). In the event of a practical quantum computer capable of running
Shor's algorithm, AUGECOIN signatures could be forged.

### No Blockchain Deletion

Infinite scaling (blockchain pruning / deletion) is not implemented.
Validators must store the full history indefinitely. As the chain grows,
disk requirements increase linearly with no built-in mechanism to archive
or garbage-collect old blocks.

### No Coin-Rot Recovery

There is no recovery path for coins lost due to block corruption, software
bugs, or user error. The protocol has no concept of "undo" or "rollback"
beyond consensus-level view changes.

### No Atomic Swaps

AUGECOIN does not support atomic cross-chain swaps (HTLC or equivalent).
Tokens cannot be trustlessly exchanged with other blockchains at the
protocol level.
