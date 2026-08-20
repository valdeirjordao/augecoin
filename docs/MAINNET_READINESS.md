# AUGECOIN Mainnet Readiness Checklist

## Build & Tests
- [ ] cargo build --workspace --release
- [ ] cargo test --workspace (0 failures)
- [ ] cargo clippy --workspace --all-targets --all-features -- -D warnings
- [ ] cargo fmt --all -- --check
- [ ] TypeScript SDK tests (0 failures)
- [ ] CI green

## Consensus
- [ ] 4-node liveness test passes
- [ ] Adversarial tests pass (equivocation, timeout, quorum)
- [ ] Partition test passes
- [ ] Determinism test passes (multi-node state root match)
- [ ] Load test (10k+ operations within block time)

## Security
- [ ] Validator keys generated via CSPRNG (not deterministic seed)
- [ ] Admin key separated from consensus keys
- [ ] RPC admin endpoints require authentication
- [ ] TLS configured with real certificates (not self-signed)
- [ ] No private keys in repository
- [ ] Dependency audit clean
- [ ] Fuzz targets run for 5+ minutes without panics

## Storage
- [ ] Corruption detection verified
- [ ] Checkpoint/restore tested
- [ ] Rollback tested
- [ ] All 6 column families verified

## Network
- [ ] Noise encryption verified
- [ ] Rate limiting active
- [ ] Maximum message size limits enforced
- [ ] Peer authentication for validator channel

## Operations
- [ ] Genesis block reproducible
- [ ] Docker build reproducible
- [ ] Backup/restore documented
- [ ] Monitoring configured (Prometheus + Grafana)
- [ ] Incident response plan documented
- [ ] Key generation tool available

## Documentation
- [ ] SPEC.md synchronized with implementation
- [ ] SECURITY.md complete
- [ ] THREAT_MODEL.md complete
- [ ] OPERATIONS.md complete
- [ ] DECISIONS.md up to date
- [ ] OPEN_QUESTIONS.md resolved

## Protocol
- [ ] Total supply verified mathematically
- [ ] Emission schedule verified
- [ ] No overflow in reward calculation
- [ ] Protocol constants frozen

## External
- [ ] Crypto audit (ed25519, BLAKE3, HD keys)
- [ ] Consensus audit (BFT, quorum, equivocation)
- [ ] Network security audit (libp2p, Noise, TLS)
- [ ] Penetration test (RPC, P2P)
