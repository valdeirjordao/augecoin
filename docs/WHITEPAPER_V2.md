# AUGECOIN — Whitepaper Completo

**Versão 2.0 — fundamentada no código-fonte real (crates/ + apps/)**

> **Nota de fidelidade.** Este documento descreve o protocolo **como efetivamente
> implementado** no repositório (fonte da verdade: `crates/augecoin-core`,
> `augecoin-consensus`, `augecoin-crypto`, `augecoin-node`, `augecoin-rpc`,
> `augecoin-network`, `augecoin-storage`). Onde a implementação diverge do
> desenho original (SPEC.md / AUGECOIN_WHITEPAPER.md), esta versão registra
> **ambos** os valores e marca a divergência, para não confundir leitores
> técnicos nem mascarar diferenças de design.

---

## Resumo Executivo

AUGECOIN é uma blockchain de camada 1 permissionada, escrita em Rust, com
contas nativas numeradas e permanentes (modelo conceitualmente inspirado no
PascalCoin — **sem** reuso de código Pascal). Seu diferencial é a combinação
de **previsibilidade monetária absoluta** (emissão linear determinística, sem
pré-mineração, sem halving) com **simplicidade operacional** (Proof of
Authority com quórum 2/3+1) e **segurança de longo prazo** (hashes BLAKE3-512
e estrutura de carteira HD determinística).

---

## 1. Identidade do Protocolo

| Campo | Valor (implementado) |
|---|---|
| Nome da rede | AUGECOIN |
| Símbolo | AUGE |
| Unidade mínima | `augesat` — 1 AUGE = 100.000.000 augesat (8 casas decimais) |
| Tipo de conta | Conta nativa numerada, permanente, sem expiração |
| Consenso | Proof of Authority (PoA), quórum 2/3+1, sem staking, sem lock |
| Tempo de bloco | **15 s** (constante `CT_BLOCK_TIME_SECONDS = 15`) |
| Suprimento total | **762.120.000 AUGE** (`TOTAL_SUPPLY_AUGE`) |
| Recompensa por bloco | **29 AUGE** fixos (`CT_BLOCK_REWARD_AUGE = 29`) |
| Modelo de emissão | Linear, **sem halving** |
| Pré-mineração | Nenhuma |
| Divisão de recompensa | 0% desenvolvedor / 100% validador |
| Divisão de taxas | 100% validador |
| Duração da emissão | Contagem por blocos: **26.280.000 blocos** |
| Chain IDs | mainnet=1, testnet=2, devnet=3 |
| Hash de estado/bloco | BLAKE3 (XOF) saída de 512 bits |
| Assinatura | Ed25519 (ver notas em §4.1) |

> **Divergência de design registrada.** A SPEC v1 e o whitepaper v1
> especificavam tempo de bloco de **60 s**, suprimento de **750.000.000 AUGE**
> e recompensa **variável** por bloco (`TOTAL_SUPPLY / 26.298.000 ≈ 2,85 AUGE`).
> A implementação atual adota **15 s**, **762.120.000 AUGE** e recompensa
> **fixa de 29 AUGE**. Este documento segue o código implementado.

---

## 2. Modelo de Contas

### 2.1 Estrutura da conta (`AugeAccount`)

Cada conta recebe um número sequencial único e perpétuo. A estrutura em
`crates/augecoin-core/src/account.rs` expõe:

| Campo | Tipo | Descrição |
|---|---|---|
| `account_number` | `u64` | Número sequencial da conta |
| `public_key` | `[u8; 32]` | Chave pública Ed25519 (32 bytes) |
| `balance` | `u64` | Saldo em augesat |
| `name` | `Option<String>` | Nome reservado |
| `metadata` / `account_data` | `Vec<u8>` | Dados arbitrários (limite `CT_MAX_ACCOUNT_DATA = 32`) |
| `op_sequence` / `nonce` | `u64` | Contador anti-replay |
| `created_at_height` | `u64` | Altura do bloco de criação |
| `state` | enum | `Owned` / `Normal` / `ForSale` / `Reserved` |

### 2.2 Criação de contas

A cada bloco finalizado, **3 contas novas** são criadas
(`CT_ACCOUNTS_PER_BLOCK = 3`), com numeração sequencial. As contas são
auto-atribuídas ao validador líder da rodada durante `execute_block()`.

**Propriedades:**
- Conta nunca expira; não há operação de "Recover" por inatividade.
- Perda de chave privada = saldo permanentemente inacessível.
- Conta pode ser **transferida** (mudança de chave pública), **vendida**,
  **presenteada** e **aceita** via operações on-chain.

---

## 3. Tokenomics e Economia Monetária

### 3.1 Parâmetros fundamentais (implementados em `constants.rs`)

```
DECIMALS               = 8
ONE_AUGE               = 100_000_000  (augesat)

CT_BLOCK_REWARD_AUGE   = 29
CT_BLOCK_REWARD_AUGESAT = 2_900_000_000  (29 AUGE em augesat)

TOTAL_SUPPLY_AUGE      = 762_120_000
TOTAL_SUPPLY_AUGESAT   = 76_212_000_000_000_000

TOTAL_EMISSION_BLOCKS  = 26_280_000

MIN_FEE_AUGESAT        = 1_000  (0,00001 AUGE por operação)
```

### 3.2 Função de recompensa por bloco (`emission.rs`)

```rust
pub fn block_reward(block_number: u64) -> u64 {
    if block_number < TOTAL_EMISSION_BLOCKS {
        CT_BLOCK_REWARD_AUGESAT   // 29 AUGE
    } else {
        0
    }
}
```

- Cada bloco de `0` a `26.279.999` produz **exatamente 29 AUGE**.
- A partir de `TOTAL_EMISSION_BLOCKS`, a recompensa é **0** para sempre —
  **sem cauda perpétua**.
- **Sem halving** e **sem variação**: `block_reward(0) == block_reward(26.279.999) == 29 AUGE`.

### 3.3 Prova de suprimento máximo (verificada por teste)

```
26.280.000 blocos × 29 AUGE = 762.120.000 AUGE  (exato, hard cap)
```

Testes determinísticos em `emission.rs` confirmam:
- `TOTAL_EMISSION_BLOCKS × CT_BLOCK_REWARD_AUGESAT == TOTAL_SUPPLY_AUGESAT`
- `block_reward` é sempre 29 AUGE no período de emissão;
- `block_reward` é 0 após o período;
- `developer_reward == 0` (sem alocação automática a desenvolvedores).

### 3.4 Distribuição de recompensa e taxas

A cada bloco `N` finalizado, ocorrem atomicamente:

```
BLOCO N
 ├── 10 novas Contas AUGE criadas         → Contas #(N·10) … #(N·10+9)
 ├── Recompensa de emissão                → 29 AUGE
 └── Taxas de todas as operações do bloco → soma(fee_i) augesat
        ↓
 Validador líder da rodada N recebe: 29 AUGE + soma(fee_i)
```

- **100% da recompensa + 100% das taxas** vão para o validador líder da rodada.
- **0%** para fundação/desenvolvedores (`developer_reward` é no-op retornando 0).
- **Sem pré-mineração**: todo AUGE em circulação vem exclusivamente de
  recompensa de bloco.
- Taxa mínima por operação: `MIN_FEE_AUGESAT = 1.000` augesat (0,00001 AUGE).

### 3.5 Curva de oferta e tempo até o hard cap

Como a recompensa é fixa (29 AUGE) e o tempo de bloco é de 15 s, a emissão é
**linear** até o hard cap. A contagem canônica é **por blocos**, não por
relógio; a duração em tempo de parede depende do tempo médio de bloco real.

**Cálculo (com bloco de 15 s, produção contínua):**

```
Blocos até o cap     = 26.280.000
Tempo total          = 26.280.000 × 15 s
                     = 394.200.000 s
                     = 4.562,5 dias
                     ≈ 12,5 anos
```

| Período | AUGE emitidos |
|---|---|
| 1 minuto (4 blocos) | 116 AUGE |
| 1 hora (240 blocos) | 6.960 AUGE |
| 1 dia (5.760 blocos) | 167.040 AUGE |
| 1 ano (2.102.400 blocos) | 60.969.600 AUGE |
| **Hard cap (26.280.000 blocos)** | **762.120.000 AUGE** |

O hard cap de **762.120.000 AUGE** é alcançado em **26.280.000 blocos**,
equivalente a **~12,5 anos** a 15 s/bloco. A partir daí a recompensa é `0`
para sempre (sem cauda perpétua).

---

## 4. Criptografia

### 4.1 Assinatura

A implementação atual (`crates/augecoin-crypto/src/signature.rs`) usa
**Ed25519** (`ed25519-dalek`):

| Esquema | Implementação atual |
|---|---|
| Clássico | **Ed25519** (`ed25519-dalek`) — assinaturas de 64 bytes, chaves de 32 bytes |

Os tipos `HybridKeyPair`, `HybridPublicKey` e `HybridSignature` existem como
**alias legados** de Ed25519 (`pub type HybridSignature = Ed25519Signature`),
preservando compatibilidade de API.

> **Nota de implementação (honestidade técnica).** A SPEC v1 previa assinatura
> **híbrida dual-sign** (Ed25519 **+** ML-DSA/Dilithium) obrigatória desde o
> gênesis. Na versão atual do crate, **apenas Ed25519 está concretamente
> implementado**; os tipos híbridos são aliases. A infraestrutura de migração
> (`sig_scheme_version`) permanece o caminho planejado para adicionar PQC em
> hard fork governado pelos validadores.

### 4.2 Hashing

- **Algoritmo:** BLAKE3 em modo XOF.
- **Saída:** 512 bits (64 bytes) — `blake3_512(data) -> [u8; 64]`.
- **Aplicações:** hash de bloco, state root, prova de Merkle, endereços.
- Vetores de teste fixos garantem determinismo byte-a-byte.

### 4.3 Endereços

A carteira web deriva endereços de recebimento a partir da chave pública
Ed25519:

- **Endereço canônico (bech32m):** `hash = blake3_512(pubkey)` codificado em
  Bech32m com prefixo `auge1…` (carteira `deriveAddress`, BIP-350).
- **Endereço curto (Base58Check):** `Base58(blake3_512(pubkey)[0..24] ||
  checksum[0..4])`, com checksum
  `blake3_512("AUGECOIN-SHORT-ADDRESS-V1" || payload)[0..4]` — usado para
  exibição/pagamento externo (~39 caracteres).

### 4.4 Carteira HD (Hierarchical Deterministic)

- **Seed:** BIP39 (12/16 palavras — a wallet usa 16).
- **Derivação:** a WASM (`augecoin_crypto`) expõe `WasmHdWallet`, com
  `ed25519_public_key_hex(index)` e `sign_hex(index, messageHex)`.
- **Determinismo:** mesma seed + mesmo índice = mesmas chaves, reproduzível
  entre core Rust, WASM/TS SDK e wallets.

> **Nota.** A SPEC previa `seed → HKDF-SHA3-512 → sementes por índice → par
> Ed25519+Dilithium`. A implementação atual deriva pares **Ed25519**; o
> mecanismo de derivação PQC permanece planejado.

---

## 5. Consenso — Proof of Authority (PoA)

### 5.1 Parâmetros

| Parâmetro | Valor (implementado) |
|---|---|
| Validadores iniciais (gênesis) | Definidos no config de gênese |
| Quórum | `(n*2)/3 + 1` (`quorum_threshold`) — ex.: 4 validadores → 3 |
| Seleção de líder | Round-robin determinístico (`leader_for_height_is_deterministic`) |
| Finalidade | Bloco marcado como `Finalized` ao atingir quórum |
| Autoridade admin | Single-key (`admin_keypair`) |

### 5.2 Cálculo de quórum (`quorum.rs`)

```rust
pub fn quorum_threshold(validator_count: u64) -> u64 {
    if validator_count == 0 { return 0; }
    (validator_count * 2) / 3 + 1
}
```

- `4 validadores → 3` (testado)
- `2 validadores → 2`
- `7 validadores → 5`
- `1 validador → 1`

A verificação conta assinaturas de **validadores ativos** distintas sobre o
hash do bloco; o bloco só finaliza se `valid_count >= threshold`.

### 5.3 Máquina de estados da rodada

```
Propose → Verify → Sign → Commit (Finalized)
```

| Fase | Descrição |
|---|---|
| **Propose** | Líder monta o bloco (mempool + recompensa + taxas) e propõe |
| **Verify** | Validadores verificam assinaturas, saldos, `op_sequence`, estado |
| **Sign** | Validadores assinam o hash do bloco |
| **Commit** | Bloco aplicado atomicamente e marcado como `Finalized` ao atingir quórum |

### 5.4 Governança de validadores (`validator.rs`)

Apenas o **administrador** (single-key) pode executar operações sobre o
conjunto de validadores:

| Operação | Descrição |
|---|---|
| `add_validator` | Adiciona com `activation_height` **futuro** obrigatório |
| `remove_validator` | Remove |
| `activate` / `deactivate` | Ativa / desativa sem remover do conjunto |

Toda mudança é registrada on-chain e entra em vigor em `activation_height`
futuro — **nunca imediato**. Operações de admin exigem assinatura do admin
(`invalid_admin_signature_rejected`, `non_validator_admin_op_rejected`).

### 5.5 Tolerância a falhas

- Com 4 validadores e 3 online → rede continua finalizando (quórum 3/4).
- Com 4 validadores e 2 online → rede para de finalizar (falha segura).
- Partição 2 vs 2 → nenhum lado atinge quórum; consenso retorna ao resolver.

---

## 6. Rede P2P e Comunicação

- **Biblioteca:** libp2p (Rust).
- **Criptografia de canal:** Noise Protocol (peers sem Noise são rejeitados).
- **Descoberta:** lista estática de bootnodes (mais auditável que DHT em rede fechada).
- **Propagação:** gossipsub; canais de consenso restritos a validadores reconhecidos.
- **Fast sync:** download do checkpoint mais recente + replay dos blocos restantes.
- **Anti-DoS:** rate limiting por peer, ban temporário configurável.

---

## 7. Tipos de Operação

As operações implementadas (serialização byte-a-byte espelhada em
`crates/augecoin-core/src/operation.rs` e no wallet `tx.js`):

| Operação | Tag | Descrição |
|---|---|---|
| **Transaction** | `0x01` | Transferência de AUGE entre contas |
| **ChangeKey** | `0x02` | Altera a chave pública de uma conta |
| **ListAccountForSale** | `0x04` | Coloca conta à venda |
| **DelistAccount** | `0x05` | Cancela venda |
| **BuyAccount** | `0x06` | Compra uma conta à venda |
| **ChangeAccountInfo** | `0x08` | Altera nome/tipo/dados da conta |
| **MultiOperation** | `0x09` | Multipla operação (até 100 remetentes / 1000 destinatários) |
| **GiftAccount** | `0x0d` | Presenteia uma conta (key transfer) |
| **AcceptGift** | `0x0e` | Aceita uma conta recebida |

### Validação de operação (mempool)

1. **Assinatura** — deve verificar (Ed25519).
2. **op_sequence** — deve ser `current_sequence + 1` (anti-replay).
3. **Saldo suficiente** — `balance >= amount + fee`.
4. **Taxa mínima** — `fee >= MIN_FEE_AUGESAT` (1.000 augesat).

---

## 8. Estrutura do Bloco

### BlockHeader

| Campo | Tipo | Descrição |
|---|---|---|
| `height` | `u64` | Altura do bloco |
| `prev_hash` | `[u8; 64]` | Hash BLAKE3-512 do bloco anterior |
| `state_root` | `[u8; 64]` | Raiz da Merkle tree do estado de contas |
| `tx_merkle_root` | `[u8; 64]` | Raiz da Merkle tree das operações |
| `timestamp` | `u64` | Timestamp UNIX (tolerância futura `CT_MAX_FUTURE_BLOCK_TIMESTAMP_SECONDS = 30`) |
| `leader_id` | `u64` | Conta do validador líder |
| `round_reward` | `u64` | Recompensa da rodada (emissão + taxas) |

### Block

| Campo | Tipo | Descrição |
|---|---|---|
| `header` | `BlockHeader` | Cabeçalho |
| `operations` | `Vec<Operation>` | Operações incluídas |
| `leader_signature` | `HybridSignature` | Assinatura do líder |
| `quorum_signatures` | `Vec<HybridSignature>` | Assinaturas de quórum |

A serialização é **canônica e determinística byte-a-byte** — o hash do bloco
depende de cada byte exato.

---

## 9. Armazenamento e Estado (`augecoin-storage`)

- **Backend:** RocksDB.
- **SafeBox:** o estado de contas (equivalente moderno ao "SafeBox" do
  PascalCoin) é persistido; entre snapshots completos (`SAFEBOX_SNAPSHOT_INTERVAL_*`),
  só as contas modificadas são gravadas (otimização local; não afeta o hash de
  consenso).
- **Pruning:** blocos antigos são podados; o SafeBox retém **todo** o estado de
  contas (`PRUNE_KEEP_BLOCKS = 100`).
- **State root:** Merkle Tree sobre as contas — determinística.
- **Atomicidade:** `execute_block()` aplica operações, credita recompensa+taxas
  e recalcula o state root em **uma transação atômica**; se qualquer etapa
  falhar, nada é persistido.
- **Checkpoints:** snapshots a cada N blocos para fast sync e recuperação.

---

## 10. Interface RPC (`augecoin-rpc`)

### 10.1 Protocolos

- **JSON-RPC 2.0** — endpoints públicos de consulta e envio de operações.
- Endpoints usados pelo wallet/explorer: `getaccount`, `getblockcount`,
  `nodestatus`, `getblock`, `getblockoperations`, `getoperations`,
  `getoperationbyhash`, `getpendings`, `findaccounts`,
  `listaccountsforsale`, `listvalidatorinventory`, `listpendinggifts`,
  `sendoperation`, `buyaccount`, `sellaccount`, `giftaccount`, `acceptgift`,
  `cancelsale`, `createaccount`, `faucet`.

### 10.2 Segurança da API

- Endpoints administrativos exigem autenticação separada.
- Rate limiting por IP/API key.
- Canais criptografados (TLS) em produção.

---

## 11. Ecossistema de Software

### 11.1 Crates (Rust workspace)

```
augecoin-crypto      → assinatura (Ed25519), hashing BLAKE3-512, HD keys, endereços
augecoin-core        → AugeAccount, Operation, Block, BlockHeader, block_reward(), SafeBox
augecoin-storage     → RocksDB, SafeBox, state_root, checkpoints, pruning
augecoin-consensus   → ValidatorSet, quórum, seleção de líder, PoA, equivocation
augecoin-network     → libp2p, Noise, gossipsub, sync
augecoin-rpc         → JSON-RPC 2.0 endpoints
augecoin-cli         → CLI administrativo (clap)
augecoin-node        → binário principal, mempool, execute_block(), gênese
augecoin-bench       → benchmarks
```

### 11.2 Pacotes e aplicações

```
packages/
 ├── wasm-crypto/    → augecoin-crypto → WASM (blake3 + Ed25519 + BIP39 HD)
 └── sdk-ts/         → SDK TypeScript de alto nível

apps/
 ├── wallet-web/     → Wallet web estático (vanilla JS + WASM, sem build step)
 ├── wallet-desktop/ → Tauri + Rust nativo
 └── wallet-mobile/  → React Native + módulo nativo
```

### 11.3 Wallets

| Plataforma | Stack | Armazenamento de chaves |
|---|---|---|
| **Web** | Vanilla JS + WASM | IndexedDB + PBKDF2-SHA256 (600k) + AES-256-GCM |
| **Desktop** | Tauri + Rust nativo | Keystore do SO |
| **Mobile** | React Native + módulo nativo | Secure Enclave / Keystore nativo |

**Princípio:** a chave privada/seed **nunca** sai do dispositivo; apenas
chamadas RPC via rede. Nenhum dado sensível é transmitido a servidor.

### 11.4 Explorer

`explorer/` — block explorer estático com páginas de conta, bloco, transação,
validadores, richlist, supply, busca, e gráficos em tempo real via poller RPC.

---

## 12. Observabilidade

- **Métricas Prometheus:** altura, tempo médio de bloco, peers, mempool,
  equivocation, latência de propagação.
- **Dashboard Grafana** versionado no repositório.
- **Detecção ativa:** equivocation, peer malicioso repetido, validador ausente
  além do timeout, tentativa de gasto duplo (`op_sequence` reutilizado).

---

## 13. Deploy e Infraestrutura

- **Testnet padrão:** docker-compose com 4 nós validadores com identidades
  criptográficas próprias e volumes persistentes + nó observador +
  Prometheus/Grafana (`infra/testnet-4node.yml`).
- **Script de gênese:** geração determinística do bloco 0 a partir de um
  `GenesisConfig` TOML (chain_id, treasury, faucet, validators, admin),
  idêntico em qualquer máquina com os mesmos parâmetros.

---

## 14. Segurança

- **Gestão de chaves:** chaves de validadores nunca em arquivo versionado
  (referenciadas por caminho externo no gênese).
- **Fuzzing:** desserialização de operações e blocos (`cargo fuzz`).
- **Auditoria de dependências:** `cargo audit`.
- **Plano de resposta a incidentes** (`INCIDENT_RESPONSE.md`): validador
  comprometido, bug de consenso, rollback/recuperação.

---

## 15. Divergências entre SPEC v1 e Implementação (resumo)

| Parâmetro | SPEC v1 / WP v1 | Implementação (v2) |
|---|---|---|
| Tempo de bloco | 60 s | **15 s** |
| Suprimento total | 750.000.000 AUGE | **762.120.000 AUGE** |
| Recompensa | variável (~2,85 AUGE) | **fixa, 29 AUGE** |
| Emissão | 26.298.000 blocos (~50 anos) | **26.280.000 blocos** |
| Assinatura | Ed25519 + Dilithium (dual-sign) | **Ed25519** (híbrido = alias) |
| Contas/bloco | 1 | **10** |
| Desenvolvimento | — | `developer_reward = 0` |

> Estas diferenças devem ser reconciliadas via ADR em `DECISIONS.md` antes do
> mainnet, para que a documentação pública e o protocolo estejam alinhados.

---

## 16. Conclusão

AUGECOIN entrega uma blockchain PoA determinística e auditável, com
tokenomics linear e previsível (29 AUGE/bloco, hard cap de 762,12M AUGE, sem
pré-mineração e sem split para desenvolvedores) e um ecossistema completo de
ferramentas (SDK TS/WASM, wallets web/desktop/mobile, CLI administrativo,
explorer e observabilidade). A trajetória pós-quântica (adição de ML-DSA via
`sig_scheme_version`) permanece como evolução planejada sobre a base Ed25519
atual.

---

*Whitepaper v2 gerado a partir da análise do repositório via graphify +
leitura dos crates de implementação. Alterações de parâmetros monetários ou
criptográficos exigem ADR em `DECISIONS.md`.*
