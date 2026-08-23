# AUGECOIN — SPEC.md
### Fonte da verdade do protocolo. Qualquer mudança aqui exige um ADR em DECISIONS.md.

## 1. Identidade

| Campo | Valor |
|---|---|
| Nome da rede | AUGECOIN |
| Símbolo | AUGE |
| Unidade mínima | `augesat` (1 AUGE = 100.000.000 augesat, 8 casas decimais) |
| Tipo de conta | Conta nativa numerada, sem expiração |
| Consenso | Proof of Authority, quórum 2/3+1, sem staking, sem lock de tokens |
| Tempo de bloco | 15 segundos (tolerância de deriva ≤ 2s via NTP) |
| Suprimento total | 762.120.000 AUGE (hard cap de emissão), sem pré-mineração |
| Duração da emissão | 50 anos = 105.120.000 blocos, emissão linear determinística por bloco |
| Pós-quântico | Assinatura híbrida (clássica + PQC) desde o gênesis |

## 2. Criptografia

- **Assinatura híbrida dual-sign obrigatória desde o bloco gênesis:**
  `Ed25519` (crate `ed25519-dalek`) **+** `ML-DSA`/CRYSTALS-Dilithium
  (NIST FIPS 204, via `pqcrypto-dilithium` ou `oqs`). Transação/bloco só
  é válido se **ambas** as assinaturas verificarem.
- **Hash de estado e bloco:** `BLAKE3` em modo XOF, saída de 512 bits.
- **Endereços:** `hash(ed25519_pubkey || dilithium_pubkey)`, codificados
  em Bech32m, prefixo `auge1...`.
- **Endereço curto externo:** para pagamento e exibição, o SDK pode derivar
  `Base58(BLAKE3-512(pubkey)[0..24] || checksum[0..4])`. O checksum é
  `BLAKE3-512("AUGECOIN-SHORT-ADDRESS-V1" || payload)[0..4]`. Esse formato
  possui aproximadamente 39 caracteres e não substitui o endereço canônico
  `auge1...`, que continua sendo a identidade interna do protocolo.
- **HD wallet:** Dilithium não é compatível com BIP32. Derivação:
  `seed (BIP39) → HKDF-SHA3-512 → sementes determinísticas por índice →
  pares Ed25519 e Dilithium`. Módulo próprio `augecoin-hdkeys`, com
  vetores de teste fixos (mesma seed sempre gera as mesmas chaves,
  reproduzível entre Rust core, TS SDK e mobile).
- **Migração futura:** campo `sig_scheme_version` em cada transação,
  permitindo trocar/adicionar esquemas via hard fork governado pelo
  conjunto de validadores.

## 3. Emissão monetária

```
DECIMALS                 = 8
TOTAL_SUPPLY_AUGE        = 762_120_000
TOTAL_SUPPLY_AUGESAT     = 76_212_000_000_000_000

BLOCK_TIME_SECONDS       = 15
EMISSION_YEARS           = 50
BLOCKS_PER_YEAR          = 2_102_400
TOTAL_EMISSION_BLOCKS    = 105_120_000

CT_BLOCK_REWARD_AUGESAT  = 725_000_000   (7,25 AUGE por bloco, fixo)
```

Regra de implementação de `block_reward(height: u64) -> u64`:

1. Recompensa **fixa** de `725_000_000` augesat (7,25 AUGE) para todo
   bloco com `height < TOTAL_EMISSION_BLOCKS` — linear, sem halving,
   sem caso especial no gênesis.
2. A partir do bloco `TOTAL_EMISSION_BLOCKS`, retorna `0` para
   sempre — sem cauda perpétua.
3. Propriedade obrigatória (testes em `augecoin-core::emission`):
   `TOTAL_EMISSION_BLOCKS × CT_BLOCK_REWARD_AUGESAT ==
   TOTAL_SUPPLY_AUGESAT` exatamente (105.120.000 × 7,25 AUGE =
   762.120.000 AUGE).

## 4. O que cada bloco produz

A cada bloco finalizado, três coisas acontecem atomicamente:

```
BLOCO N
 ├── 3 novas contas (AUGEIDs) emitidas → nº N·3+0..2, estado `Reserved`
 │     (CT_ACCOUNTS_PER_BLOCK = 3, numeração determinística)
 ├── Recompensa de emissão           → block_reward(N) augesat
 └── Taxas de todas as tx do bloco   → soma(fee_i) augesat
        ↓
 Validador líder da rodada N recebe: block_reward(N) + soma(fee_i)
```

- Contas emitidas nascem vazias (saldo zero), em estado `Reserved`,
  ligadas à chave do validador líder da rodada dentro de
  execute_block() (ver ADR-005 em DECISIONS.md). Slots já ocupados no
  gênesis nunca são sobrescritos (guarda defensiva determinística).
- Conta nunca expira. Sem operação de "Recover" por inatividade. Perda
  de chave privada = saldo permanentemente inacessível.
- 100% da recompensa + 100% das taxas vão para o validador da rodada.
  Sem split para fundação/desenvolvedores (`developer_reward = 0`).

## 5. Consenso PoA

```
Genesis: 4 validadores autorizados pelo administrador
Quórum:  2/3 + 1 → 3 assinaturas válidas de 4 finalizam um bloco
Líder:   round-robin determinístico, height % validator_count
Timeout: BLOCK_TIME + 5 s (20 s com bloco de 15 s) → round-change
         automático
Equivocation: 2 assinaturas do mesmo validador na mesma altura →
         prova on-chain (as duas assinaturas conflitantes), validador
         marcado como slashable, removível pelo administrador
Governança de validadores: add/remove/activate/deactivate só pelo
         administrador, mudança registrada on-chain com
         activation_height futuro (nunca imediato)
```

Autoridade administradora: single-key (ver ADR em DECISIONS.md).

## 6. Arquitetura de crates (Rust workspace)

```
augecoin-crypto      → assinatura híbrida, hashing, hdkeys, endereços
augecoin-core        → AugeAccount, Transaction, Block, BlockHeader,
                        block_reward()
augecoin-storage      → RocksDB, state_root, checkpoints
augecoin-consensus    → ValidatorSet, seleção de líder, máquina de
                        estados PoA, equivocation
augecoin-network      → libp2p, Noise, gossipsub, sync
augecoin-rpc          → JSON-RPC 2.0 + gRPC sobre TLS
augecoin-cli          → CLI administrativo (clap)
augecoin-node         → binário principal, mempool, execute_block()
```

## 7. Superfícies externas

- **SDK TypeScript** (`@augecoin/sdk`): compila `augecoin-crypto` para
  WASM (`@augecoin/wasm-crypto`), expõe wallet HD, assinatura, chamadas
  RPC tipadas.
- **Wallets:** Web (React + WASM), Desktop (Tauri + Rust nativo),
  Mobile (React Native + módulo nativo). Chave nunca sai do
  dispositivo; armazenamento sempre criptografado (Argon2id + AES-256-
  GCM no web; keystore do SO no desktop/mobile).
- **CLI administrativo:** comandos `validator add/remove/activate/
  deactivate/list`, `status`, `validator earnings`, `security
  equivocations/banned-peers/alerts`.

## 8. Observabilidade

Métricas Prometheus (altura, tempo médio de bloco, peers, mempool, taxa
de equivocation, latência de propagação) + dashboard Grafana versionado
no repo + detecção ativa de: equivocation, peer malicioso repetido,
validador ausente além do timeout, tentativa de gasto duplo
(`op_sequence` reutilizado).

## 9. Referência de arquitetura (não de código)

A arquitetura de contas numeradas permanentes, o conceito de checkpoint
de estado (equivalente moderno ao "SafeBox") e o modelo PoA com quórum
2/3 usam o PascalCoin como referência conceitual de protocolo. **Nenhum
código Object Pascal é reaproveitado** — ver Regra 7 de
`PROJECT_RULES.md`.
