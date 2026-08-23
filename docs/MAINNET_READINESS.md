# AUGECOIN Mainnet Readiness Checklist

**Última bateria completa:** 2026-08-23 (host: validador chain1, rust stable)

## Build & Tests
- [x] cargo build --workspace --release (PASS, 15m00s)
- [x] cargo test --workspace (PASS — 410 testes, 0 falhas)
- [x] cargo clippy --workspace --all-targets --all-features -- -D warnings (PASS)
- [x] cargo fmt --all -- --check (PASS)
- [x] TypeScript SDK tests (PASS — vitest, packages/sdk-ts)
- [ ] CI green (verificar último run no GitHub Actions após push)

## Consensus
- [x] 4-node liveness test passes (crates augecoin-node, suite de integração)
- [x] Adversarial tests pass (equivocation, timeout, quorum)
- [x] Partition test passes (2/4 não finaliza; 3/4 finaliza)
- [x] Determinism test passes (multi-node state root match)
- [x] Load test (10k+ operations within block time)

## Security
- [x] Validator keys generated via CSPRNG (v1–v4.env, fora do repo; gênesis guarda só chaves públicas)
- [x] Admin key separated from consensus keys (admin offline ≠ chaves dos validadores)
- [x] RPC admin endpoints require authentication (API key header)
- [x] TLS configured with real certificates (Let's Encrypt `augeco.in`, montado nos 4 validadores; hook de renovação em /etc/letsencrypt/renewal-hooks/deploy/augecoin-chain1.sh)
- [x] No private keys in repository (`infra/genesis1/*.env` e `tls/` gitignored — verificado)
- [x] Dependency audit clean (`cargo audit`: 0 vulnerabilidades; h2→0.4.18; exceções documentadas em `.cargo/audit.toml`)
- [x] Fuzz targets run without panics (tx_deserialize 11,4M execs; block_deserialize 1,9M execs pós-fix do OOM-02ab382a)

## Storage
- [x] Corruption detection verified
- [x] Checkpoint/restore tested
- [x] Rollback tested
- [x] All 6 column families verified

## Network
- [x] Noise encryption verified
- [x] Rate limiting active
- [x] Maximum message size limits enforced (gossipsub 2 MiB)
- [x] Peer authentication for validator channel
- [x] AUGECOIN_EXTERNAL_ADDRESS público por validador (/ip4/184.174.36.240/tcp/1910X)

## Operations
- [x] Genesis block reproducible (TOML determinístico; sha256 registrado no lançamento)
- [x] Docker build reproducible (multi-stage, rust:1.83-slim)
- [x] Backup/restore documented (docs/OPERATIONS.md)
- [ ] Monitoring configured (Prometheus + Grafana) apontado para os nós chain1 (dashboards versionados existem em infra/observability*)
- [x] Incident response plan documented (docs/INCIDENT_RESPONSE.md)
- [x] Key generation tool available (scripts/genesis/generate.sh)

## Documentation
- [x] SPEC.md synchronized with implementation (ADR-015: 15 s/bloco, 7,25 AUGE linear, cap 762.120.000 AUGE, 3 AUGEIDs/bloco)
- [x] SECURITY.md complete
- [x] THREAT_MODEL.md complete
- [x] OPERATIONS.md complete
- [x] DECISIONS.md up to date (ADR-015)
- [x] OPEN_QUESTIONS.md resolved

## Protocol
- [x] Total supply verified mathematically (105.120.000 × 7,25 AUGE = 76.212.000.000.000.000 augesat, teste `total_emission_equals_hard_cap`)
- [x] Emission schedule verified (linear, sem halving, zero após o período)
- [x] No overflow in reward calculation (aritmética saturante + proptests)
- [x] Protocol constants frozen (crates/augecoin-core/src/constants.rs)

## External
- [ ] Crypto audit independente (ed25519, BLAKE3, HD keys)
- [ ] Consensus audit independente (BFT, quorum, equivocation)
- [ ] Network security audit independente (libp2p, Noise, TLS)
- [ ] Penetration test (RPC, P2P)

> **Pendências para o lançamento:** CI verde no remote, monitoramento apontado
> para os nós finais e auditoria externa/pentest. Nada bloqueante técnico no
> código; os itens externos são decisão de governança/orçamento.
