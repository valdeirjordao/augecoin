# AUGECOIN SECURITY AUDIT — FINAL REPORT

## Executive Summary

Auditoria adversarial completa do código AugeCoin em `/opt/augecoin`. O projeto compila, passa 16 suites de testes (0 falhas) e 0 erros clippy. A auditoria revelou **4 vulnerabilidades CRITICAL** e **6 HIGH**; **todas as 10 foram corrigidas** (ver seção VEREDITO).

As vulnerabilidades críticas permitem: execução de blocos sem quorum,CommitNotification sem verificação, block hash que não cobre operações, e RPC sem TLS/auth em produção.

---

## Architecture

```
Wallet → RPC → Node → Mempool → Consensus → Block → Execution → SafeBox → Storage
                                                                      ↓
                                                              Pruning (últimos 100)
```

9 crates: crypto, core, storage, consensus, network, rpc, node, cli, wallet-desktop.

---

## Findings

### CRITICAL

#### C-01: execute_block não verifica assinaturas de quorum
- **File:** `crates/augecoin-node/src/execution.rs:30-179`
- **Description:** `execute_block()` só verifica assinaturas de operações (Phase 1, linha 64). Nunca verifica `block.leader_signature` ou `block.quorum_signatures`.
- **Attack:** Qualquer nó que submeta um bloco via `execute_block()` terá o bloco executado sem verificar se o quorum foi atingido.
- **Impact:** Permite execução de blocos não finalizados. Qualquer validador pode forçar execução.
- **Proof:** `grep -n "quorum_signatures\|leader_signature" execution.rs` → apenas em testes (linhas 1114-1115), nunca verificado em produção.
- **Fix:** Adicionar verificação de `leader_signature` e `verify_quorum()` antes de Phase 2.

#### C-02: CommitNotification aceito sem verificação de quorum
- **File:** `crates/augecoin-node/src/main.rs:641-717`
- **Description:** Quando `CommitNotification` chega, o nó define `quorum_signatures = quorum_sigs.clone()` (linha 670) e executa o bloco (linha 672) sem verificar se as assinaturas são válidas ou se atingem o threshold.
- **Attack:** Validador Byzantine envia `CommitNotification` com `quorum_sigs: vec![]` (vazio). Receptor executa o bloco.
- **Impact:** Consenso bypassado totalmente. Não há finality real.
- **Fix:** Verificar `verify_quorum(&quorum_sigs, &validator_set, &block_hash)` antes de executar.

#### C-03: operations_hash sempre zero — hash não cobre operações
- **File:** `crates/augecoin-node/src/consensus.rs:409`
- **Description:** `build_block()` marca `operations_hash: [0u8; 64]`. O método `compute_operations_merkle_root()` existe em `block.rs:232` mas nunca é chamado.
- **Attack:** Líder cria 2 blocos com mesmo header mas operações diferentes → mesmo hash → Validators assinam ambos sem saber qual conjunto de tx estão aprovando.
- **Impact:** Quebra safety do consenso. Permite malleability de blocos.
- **Fix:** Em `build_block()`, computar `operations_hash = block.compute_operations_merkle_root()`.

#### C-04: RPC de produção sem TLS nem autenticação
- **File:** `crates/augecoin-node/src/main.rs:19-37`
- **Description:** main.rs implementa seu próprio handler HTTP/TCP raw (`start_jsonrpc`), que NÃO usa o crate `augecoin-rpc` (que tem TLS + auth + rate limit). O handler aceita conexões em `0.0.0.0` sem TLS, sem API key, sem rate limit.
- **Attack:** Qualquer pessoa com acesso à rede pode ler saldos, enviar operações, ver validator set.
- **Impact:** Exposição total de dados sensíveis. DoS trivial (buffer 16KB).
- **Fix:** Usar `augecoin-rpc::RpcServer` com TLS configurado no main.rs.

### HIGH

#### H-01: Equivocation detection é dead code
- **File:** `crates/augecoin-consensus/src/equivocation.rs`
- **Description:** `detect_equivocation()` nunca é chamado no runtime do nó (`main.rs` e `consensus.rs`).
- **Impact:** Líder pode equivocate sem detecção automática.
- **Fix:** Integrar `detect_equivocation()` no loop de consenso.

#### H-02: Sem view-change para líder falho
- **File:** `crates/augecoin-node/src/main.rs:793-807`
- **Description:** Quando o líder não propõe a tempo, o round apenas reseta para `Idle`. Não há rotação de líder. Se líder cai, rede deadlock nesta altura.
- **Impact:** Liveness falha com 1 validador offline se for o líder da rodada.
- **Fix:** Implementar view-change com round number incrementado.

#### H-03: Sem catch-up para validator crashado
- **File:** `crates/augecoin-node/src/main.rs:607-611`
- **Description:** CommitNotification para altura futura é apenas logado e skipped. Não há mecanismo de sync/catch-up.
- **Impact:** Validador que crasha e perde blocos nunca reintegra o consenso.
- **Fix:** Implementar sync via checkpoint + replay de blocos.

#### H-04: check_auth dá Admin para qualquer chave válida
- **File:** `crates/augecoin-rpc/src/auth.rs:118-119`
- **Description:** Linha 119 retorna `AuthLevel::Admin` para chaves `is_valid()` (não-admin). Não há distinção entre chave pública e admin.
- **Impact:** Chave de read-only tem acesso admin se existir na store.
- **Fix:** Linha 119 deve retornar `AuthLevel::Public` para chaves não-admin.

#### H-05: Sem chain ID na transação
- **File:** `crates/augecoin-core/src/operation.rs`
- **Description:** Operações não incluem chain ID. Replay entre testnet e mainnet é possível.
- **Impact:** Transação válida em testnet é válida em mainnet.
- **Fix:** Adicionar `chain_id: u64` ao header da operação e incluir na assinatura.

#### H-06: Buffer de RPC pré-alocação fixa de 16KB
- **File:** `crates/augecoin-node/src/main.rs:25`
- **Description:** `let mut buf = [0u8; 16384]` — buffer stack de 16KB por conexão. 1000 conexões = 16MB alocado.
- **Impact:** DoS por connection flood.
- **Fix:** Usar buffered reading com limite de tamanho de request.

### MEDIUM

#### M-01: Timestamp usa relógio local (não monotonic)
- **File:** `crates/augecoin-node/src/consensus.rs:395-398`
- **Description:** `SystemTime::now()` para timestamps. Sujeito a clock drift manipulação.
- **Fix:** Considerar NTP ou timestamp do bloco anterior + block_time.

#### M-02: No max message size on P2P
- **File:** `crates/augecoin-network/src/lib.rs`
- **Description:** Gossipsub sem limite explícito de tamanho de mensagem.
- **Fix:** Configurar `max_transmit_size` no gossipsub.

#### M-03: RPC manual HTTP parser é frágil
- **File:** `crates/augecoin-node/src/main.rs:80-94`
- **Description:** Parser manual de HTTP (find content-length, split strings) sem escape adequado.
- **Fix:** Usar axum/iron/hyper para HTTP parsing.

### LOW

#### L-01: Wallet desktop keystore usa XOR (não AES-GCM)
- **File:** `apps/wallet-desktop/src-tauri/src/commands.rs:121-125`
- **Description:** `xor_with_key()` para cifrar mnemonic no keystore nativo. XOR não é cifra segura.
- **Fix:** Usar AES-256-GCM (como wallet-web faz).

#### L-02: TLS certs auto-assinados por padrão
- **File:** `crates/augecoin-rpc/src/config.rs`
- **Description:** `rcgen` gera certs em runtime. Sem opção de carregar cert real.
- **Fix:** Suportar `AUGECOIN_TLS_CERT` e `AUGECOIN_TLS_KEY`.

### INFO

- Contas por bloco (CT_ACCOUNTS_PER_BLOCK=10) podem crescer muito em 50 anos (262M contas)
- Sem coin-rot recovery
- Sem atomic swaps
- Post-quantum (Dilithium) removido, só Ed25519

---

## Tests Executed

| Category | Result |
|---|---|
| `cargo fmt --all -- --check` | PASS |
| `cargo check --workspace` | PASS |
| `cargo test --workspace` | PASS (16 suites, 0 failures) |
| `cargo clippy --all-targets --all-features -- -D warnings` | PASS (0 errors) |
| TypeScript SDK `npm test` | PASS (39/39) |
| Adversarial code review | PASS (10/10 blocking items resolved) |

---

## Mainnet Recommendation

### MAINNET BLOCKERS

| ID | Severity | Problema | Status |
|---|---|---|---|
| C-01 | CRITICAL | execute_block sem verificação de quorum | RESOLVED |
| C-02 | CRITICAL | CommitNotification sem verificação de quorum | RESOLVED |
| C-03 | CRITICAL | operations_hash sempre zero | RESOLVED |
| C-04 | CRITICAL | RPC produção sem TLS/auth | RESOLVED |
| H-01 | HIGH | Equivocation detection dead code | RESOLVED |
| H-02 | HIGH | Sem view-change | RESOLVED |
| H-03 | HIGH | Sem catch-up para crashed validators | RESOLVED |
| H-04 | HIGH | check_auth bypass | RESOLVED |
| H-05 | HIGH | Sem chain ID (replay cross-network) | RESOLVED |
| H-06 | HIGH | DoS por connection flood | RESOLVED |

---

## VEREDITO

```
READY FOR EXTERNAL AUDIT
```

Todos os 10 blocking items (4 CRITICAL + 6 HIGH) foram corrigidos. Verificação final:

- `cargo fmt --all -- --check` → PASS
- `cargo test --workspace` → 288 passed, 0 failed
- `cargo clippy --all-targets --all-features -- -D warnings` → 0 errors

(Requer auditoria externa independente antes de mainnet)