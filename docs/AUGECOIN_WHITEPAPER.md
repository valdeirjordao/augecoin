# AUGECOIN Whitepaper

**Versão 1.0 — 09 de Agosto de 2026**

---

## Resumo Executivo

AUGECOIN é uma blockchain permissionada de camada 1, projetada para redes institucionais fechadas com alta exigência de previsibilidade monetária, segurança de longo prazo e simplicidade operacional. Utiliza consenso Proof of Authority (PoA) com quórum de 2/3+1, assinaturas híbridas pós-quânticas obrigatórias desde o bloco gênesis, e emissão monetária determinística com suprimento total fixo de 750 milhões de AUGE distribuído ao longo de 50 anos, sem pré-mineração, sem taxas ocultas e sem fundação ou alocação especial de gênesis.

---

## 1. Identidade do Protocolo

| Campo | Valor |
|---|---|
| Nome da rede | **AUGECOIN** |
| Símbolo | **AUGE** |
| Unidade mínima | `augesat` (1 AUGE = 100.000.000 augesat, 8 casas decimais) |
| Tipo de conta | Conta nativa numerada, sem expiração |
| Consenso | Proof of Authority (PoA), quórum 2/3+1, sem staking, sem lock de tokens |
| Tempo de bloco | 60 segundos (tolerância de deriva ≤ 2s via NTP) |
| Suprimento total | 750.000.000 AUGE, sem pré-mineração |
| Duração da emissão | 50 anos, emissão determinística por bloco |
| Pós-quântico | Assinatura híbrida (clássica + PQC) desde o gênesis |

---

## 2. Modelo de Contas

AUGECOIN adota um modelo de contas numeradas permanentes, inspirado conceitualmente no PascalCoin. Cada conta recebe um número sequencial único e perpétuo.

### Estrutura da Conta (AugeAccount)

| Campo | Tipo | Descrição |
|---|---|---|
| `account_number` | `u64` | Número sequencial da conta |
| `pubkeys` | `HybridKeyPair` | Par de chaves públicas (Ed25519 + Dilithium) |
| `balance` | `u64` | Saldo em augesat |
| `name` | `Option<String>` | Nome reservado (sem lógica de unicidade no MVP) |
| `metadata` | `Vec<u8>` | Dados arbitrários anexados |
| `op_sequence` | `u64` | Contador de operações (anti-replay) |
| `created_at_height` | `u64` | Altura do bloco em que a conta foi criada |

### Criação de Contas

A cada bloco finalizado, uma nova conta numerada é criada automaticamente e atribuída ao validador líder da rodada. Não há custo adicional para recebê-la — a atribuição é automática e integrada à execução atômica do bloco.

**Propriedades:**
- Conta nunca expira
- Perda de chave privada = saldo permanentemente inacessível (sem operação de "Recover")
- Não há mecanismo de leilão ou claim — a atribuição é direta ao líder

---

## 3. Economia e Emissão Monetária

### 3.1 Parâmetros Fundamentais

```
DECIMALS               = 8
TOTAL_SUPPLY_AUGE       = 750.000.000
TOTAL_SUPPLY_AUGESAT    = 75.000.000.000.000.000

BLOCK_TIME_SECONDS      = 60
EMISSION_YEARS          = 50
BLOCKS_PER_YEAR         = 525.960
TOTAL_EMISSION_BLOCKS   = 26.298.000

BASE_REWARD_AUGESAT     = TOTAL_SUPPLY_AUGESAT / TOTAL_EMISSION_BLOCKS
                        = 2.852.383 (divisão inteira truncada)
REMAINDER_AUGESAT       = TOTAL_SUPPLY_AUGESAT % TOTAL_EMISSION_BLOCKS
```

### 3.2 Função de Recompensa por Bloco

`block_reward(height: u64) -> u64`

1. Se `height > TOTAL_EMISSION_BLOCKS`: retorna `0` — sem cauda perpétua.
2. Caso contrário: recompensa = `BASE_REWARD_AUGESAT`.
3. Se `height <= REMAINDER_AUGESAT`: adiciona 1 augesat extra.

**Propriedade fundamental:** a soma das recompensas de todos os blocos de emissão deve ser **exatamente** `TOTAL_SUPPLY_AUGESAT`, sem 1 augesat a mais ou a menos. Esta propriedade é verificada por teste de proptest determinístico.

### 3.3 O que Cada Bloco Produz

A cada bloco `N` finalizado, três operações ocorrem atomicamente:

```
BLOCO N
 ├── 1 nova Conta AUGE criada           → Conta #N
 ├── Recompensa de emissão              → block_reward(N) augesat
 └── Taxas de todas as tx do bloco      → soma(fee_i) augesat
        ↓
 Validador líder da rodada N recebe: block_reward(N) + soma(fee_i)
```

- **100% da recompensa + 100% das taxas** vão integralmente para o validador líder da rodada.
- Não há split para fundação, desenvolvedores, ou qualquer terceiro.
- Não há pré-mineração: todo AUGE em circulação foi gerado exclusivamente via recompensa de bloco.

---

## 4. Criptografia

### 4.1 Assinatura Híbrida Dual-Sign Obrigatória

Toda transação e bloco exige **duas assinaturas simultâneas**, ambas válidas, desde o bloco gênesis:

| Esquema | Algoritmo | Crate/Biblioteca |
|---|---|---|
| Clássico | **Ed25519** | `ed25519-dalek` |
| Pós-quântico | **ML-DSA** / CRYSTALS-Dilithium (NIST FIPS 204) | `pqcrypto-dilithium` |

**Estrutura `HybridSignature`:**
```
HybridSignature {
    classic: Ed25519Signature,
    post_quantum: DilithiumSignature,
}
```

A verificação exige **ambas** as assinaturas válidas. Quebrar Ed25519 via computador quântico futuro não é suficiente para forjar uma transação — defesa em profundidade contra "harvest now, decrypt later".

### 4.2 Hashing

- **Algoritmo:** BLAKE3 em modo XOF (eXtendable Output Function)
- **Saída:** 512 bits (64 bytes)
- **Aplicações:** hash de bloco, state root, derivação de endereços

### 4.3 Endereços

```
address = blake3_512(ed25519_pubkey || dilithium_pubkey)
endereço codificado em Bech32m, prefixo: auge1...
```

Exemplo: `auge1abc...def` — com checksum Bech32m integrado e validação de integridade.

### 4.4 Carteira HD (Hierarchical Deterministic)

Como Dilithium não é compatível com BIP32, a AUGECOIN implementa derivação própria:

```
seed (BIP39, 12/24 palavras)
  → HKDF-SHA3-512
    → sementes determinísticas por índice
      → par de chaves (Ed25519 + Dilithium)
```

- Vetores de teste fixos garantem reprodutibilidade entre Rust core, TypeScript SDK e wallets mobile.
- Mesma seed + mesmo índice = sempre as mesmas chaves (determinismo byte-a-byte).

### 4.5 Migração Criptográfica Futura

Cada transação inclui o campo `sig_scheme_version`. Em caso de necessidade futura (novo algoritmo PQC, fortalecimento de parâmetros), novos esquemas podem ser adicionados via hard fork governado pelo conjunto de validadores, sem invalidar transações antigas.

---

## 5. Consenso — Proof of Authority (PoA)

### 5.1 Arquitetura Geral

| Parâmetro | Valor |
|---|---|
| Validadores iniciais (gênesis) | 4 |
| Teto de crescimento | Sem limite |
| Quórum | 2/3 + 1 |
| Seleção de líder | Round-robin determinístico |
| Timeout de líder | 2 × BLOCK_TIME_SECONDS (120s) |
| Finalidade | Bloco marcado como `Finalized` ao atingir quórum |

### 5.2 Governança de Validadores

Apenas o administrador (single-key autoritativa) pode executar operações sobre o conjunto de validadores:

| Operação | Descrição |
|---|---|
| `add` | Adiciona novo validador com `activation_height` futuro obrigatório |
| `remove` | Remove validador com `deactivation_height` futuro obrigatório |
| `activate` | Reativa validador removido/inativo |
| `deactivate` | Desativa validador sem removê-lo do conjunto |

Toda mudança é registrada on-chain e só entra em vigor no bloco de altura especificada — nunca imediatamente, garantindo previsibilidade.

### 5.3 Seleção de Líder

```
leader_for_height(h) = active_validators[h % active_validators.len()]
```

Rotação round-robin estritamente determinística. Qualquer nó pode computar o líder esperado para qualquer altura de bloco.

### 5.4 Máquina de Estados da Rodada

```
Propose → Verify → Sign → Commit (Finalized)
```

| Fase | Descrição |
|---|---|
| **Propose** | Líder monta bloco com tx do mempool e propõe à rede |
| **Verify** | Validadores verificam assinaturas, saldos, op_sequence, estado |
| **Sign** | Validadores assinam o bloco com sua HybridSignature |
| **Commit** | Bloco é aplicado atomicamente e marcado como Finalized |

Se o líder não propuser dentro do timeout (120s), ocorre **ViewChange** automático para o próximo validador da lista.

### 5.5 Equivocation (Falta Dupla)

Se um mesmo validador assinar dois blocos diferentes na mesma altura:

1. **Prova on-chain:** `EquivocationProof` contendo as duas assinaturas conflitantes é gerada e persistida.
2. **Consequência:** Validador marcado como slashable, removível pelo administrador.
3. **Detecção:** Automática e ativa — a rede monitora e alerta em tempo real.

### 5.6 Tolerância a Falhas

- **4 validadores, 3 online:** Rede continua finalizando (quórum 3/4 atingível).
- **4 validadores, 2 online:** Rede para de finalizar (falha segura — quórum não é atingido).
- **Partição de rede (2 vs 2):** Nenhum lado atinge quórum; quando a partição se resolve, o consenso retorna automaticamente.

---

## 6. Rede P2P e Comunicação

### 6.1 Transporte

- **Biblioteca:** libp2p (Rust)
- **Criptografia de canal:** Noise Protocol obrigatório — peer sem Noise é rejeitado na conexão
- **Descoberta de peers:** Lista estática de bootnodes (mais auditável que DHT para redes fechadas)

### 6.2 Propagação

- Transações e blocos propagados via **gossipsub**
- Canais de consenso restritos a validadores reconhecidos
- Peers não validadores recebem apenas dados públicos via RPC

### 6.3 Autenticação de Peer

Só nós com chave de validador reconhecida participam do canal de consenso. A verificação ocorre no handshake da conexão.

### 6.4 Sincronização Inicial (Fast Sync)

Nós novos sincronizam via:
1. Download do checkpoint mais recente (snapshot de estado)
2. Replay dos blocos restantes até a altura atual

Checkpoints são gerados a cada N blocos (N parametrizável), permitindo fast sync sem re-executar todo o histórico.

### 6.5 Proteção contra DoS

- Rate limiting por peer
- Banimento temporário configurável para peers que enviam dados inválidos repetidamente
- Ban expira automaticamente após o período configurado

---

## 7. Tipos de Transação

| Tipo | Descrição |
|---|---|
| **Transfer** | Transferência de AUGE entre contas |
| **ChangeKey** | Altera o par de chaves públicas associado a uma conta |
| **ClaimAccount** | (Reservado — não implementado no MVP) |
| **SetAccountName** | Define nome público da conta |
| **AttachData** | Anexa dados arbitrários (metadata) a uma conta |
| **ValidatorAdmin** | Operações administrativas do conjunto de validadores |

### Validação de Transação (Mempool)

Antes de entrar no mempool, toda transação é validada contra:
1. **Assinatura híbrida** — ambas (Ed25519 + Dilithium) devem verificar
2. **op_sequence** — deve ser exatamente `current_sequence + 1` (anti-replay)
3. **Saldo suficiente** — `balance >= amount + fee`
4. **Taxa mínima** — `fee >= MIN_FEE`

---

## 8. Estrutura do Bloco

### BlockHeader

| Campo | Tipo | Descrição |
|---|---|---|
| `height` | `u64` | Altura do bloco |
| `prev_hash` | `[u8; 64]` | Hash BLAKE3-512 do bloco anterior |
| `state_root` | `[u8; 64]` | Raiz da Merkle tree do estado de contas |
| `tx_merkle_root` | `[u8; 64]` | Raiz da Merkle tree das transações do bloco |
| `timestamp` | `u64` | Timestamp UNIX |
| `leader_id` | `u64` | Número da conta do validador líder |
| `round_reward` | `u64` | Recompensa total da rodada (emissão + taxas) |
| `new_account_number` | `u64` | Número da nova conta criada neste bloco |

### Block

| Campo | Tipo | Descrição |
|---|---|---|
| `header` | `BlockHeader` | Cabeçalho do bloco |
| `transactions` | `Vec<Transaction>` | Lista de transações incluídas |
| `leader_signature` | `HybridSignature` | Assinatura do líder |
| `quorum_signatures` | `Vec<HybridSignature>` | Assinaturas de quórum dos validadores |

Toda serialização é canônica e determinística byte-a-byte — o hash do bloco depende de cada byte exato.

---

## 9. Armazenamento e Estado

### 9.1 Backend

- **Banco de dados:** RocksDB
- **Column Families:** `accounts`, `blocks`, `tx_index`, `validator_set`, `equivocation_proofs`

### 9.2 State Root

- **Estrutura:** Merkle Tree sobre todas as contas na CF `accounts`
- **Propriedade:** Determinístico — mesmo conjunto de contas = mesmo state_root
- Recalculado a cada bloco executado

### 9.3 Atomicidade

`execute_block()` aplica todas as transações do bloco, credita recompensa + taxas ao líder, e recalcula `state_root` em uma única transação atômica de banco de dados. Se qualquer etapa falhar, **nada é persistido** — o estado permanece idêntico ao de antes da tentativa.

### 9.4 Checkpoints e Recuperação

- Snapshots do estado a cada N blocos (N parametrizável)
- Formato versionado documentado
- Permite fast sync e recuperação de corrupção
- Rollback de N blocos suportado (reorg)

---

## 10. Interface RPC

### 10.1 Protocolos

| Protocolo | Descrição |
|---|---|
| **JSON-RPC 2.0** | Endpoints públicos de consulta e envio de transações |
| **gRPC** | Interface tipada de alto desempenho, codegen automático para SDK |

Ambos rodam **obrigatoriamente sobre TLS** — conexões HTTP puras são rejeitadas em modo produção (mitigação de sniffing de rede).

### 10.2 Endpoints

| Endpoint | Descrição |
|---|---|
| `getAccount` | Consulta conta por número |
| `getBlock` | Consulta bloco por hash |
| `getBlockByHeight` | Consulta bloco por altura |
| `sendTransaction` | Envia transação ao mempool |
| `getMempool` | Lista transações pendentes |
| `getValidatorSet` | Lista validadores ativos |
| `getNetworkStatus` | Status da rede (altura, peers, sincronização) |
| `getValidatorEarnings` | Ganhos acumulados por validador |

### 10.3 Segurança da API

- Endpoints administrativos (`ValidatorAdmin`) exigem API key/autenticação separada
- Rate limiting por IP/API key
- Endpoints públicos de leitura não exigem autenticação

---

## 11. Superfícies Externas e Ferramentas

### 11.1 SDK TypeScript (`@augecoin/sdk`)

- Compilação de `augecoin-crypto` para **WASM** (`@augecoin/wasm-crypto`)
- Carteira HD com BIP39
- Assinatura de transação (Ed25519 + Dilithium via WASM)
- Chamadas RPC tipadas via codegen do `.proto` gRPC
- Utilitários de formatação: `augesat ↔ AUGE`

### 11.2 Wallets

| Plataforma | Stack | Armazenamento de Chaves |
|---|---|---|
| **Web** | React + TypeScript + WASM | IndexedDB + Argon2id + AES-256-GCM |
| **Desktop** | Tauri + Rust nativo | Keystore do SO (Keychain / Credential Manager / libsecret) |
| **Mobile** | React Native + módulo nativo | Secure Enclave / Keystore nativo |

**Princípio fundamental:** chave privada e seed **nunca** saem do dispositivo. Somente chamadas RPC via TLS. Nenhum dado sensível é transmitido a servidor algum.

### 11.3 CLI Administrativo (`augecoin-cli`)

| Comando | Descrição |
|---|---|
| `validator add/remove/activate/deactivate/list` | Gestão do conjunto de validadores |
| `status` | Altura, peers, sincronização, saúde do quórum |
| `validator earnings --id/--all` | Ganhos por validador (ranking com `--all`) |
| `security equivocations` | Lista de provas de equivocation registradas |
| `security banned-peers` | Lista de peers banidos |
| `security alerts --tail` | Stream de alertas de segurança em tempo real |

- Saída tabular (box-drawing Unicode) ou `--json`
- Autocomplete de shell via `clap`
- Comandos administrativos exigem `--mnemonic` ou `--key-file`

---

## 12. Observabilidade e Monitoramento

### 12.1 Métricas Prometheus

| Métrica | Descrição |
|---|---|
| `augecoin_block_height` | Altura atual do bloco |
| `augecoin_block_time_avg` | Tempo médio entre blocos |
| `augecoin_peers_connected` | Número de peers conectados |
| `augecoin_mempool_size` | Tamanho do mempool |
| `augecoin_equivocation_rate` | Taxa de equivocation detectada |
| `augecoin_block_propagation_latency` | Latência de propagação de bloco |

### 12.2 Dashboard Grafana

Dashboard pré-configurado e versionado no repositório, com painéis para:
- Saúde do quórum
- Ganhos por validador ao longo do tempo
- Eventos de segurança

### 12.3 Detecção Ativa de Ameaças

Alertas em tempo real para:
- **Equivocation** — validador assinou dois blocos na mesma altura
- **Peer malicioso repetido** — auto-ban após múltiplas infrações
- **Validador ausente** — além do timeout sem propor bloco
- **Tentativa de replay** — `op_sequence` reutilizado

Integração opcional com webhooks (Telegram, Slack, Discord) via arquivo de configuração.

---

## 13. Arquitetura de Software

### 13.1 Crates (Rust Workspace)

```
augecoin-crypto      → assinatura híbrida, hashing, hdkeys, endereços
augecoin-core        → AugeAccount, Transaction, Block, BlockHeader, block_reward()
augecoin-storage     → RocksDB, state_root, checkpoints
augecoin-consensus   → ValidatorSet, seleção de líder, máquina de estados PoA, equivocation
augecoin-network     → libp2p, Noise, gossipsub, sync, rate limiting
augecoin-rpc         → JSON-RPC 2.0 + gRPC sobre TLS
augecoin-cli         → CLI administrativo (clap)
augecoin-node        → binário principal, mempool, execute_block()
```

### 13.2 Pacotes e Aplicações

```
packages/
 ├── wasm-crypto/    → @augecoin/wasm-crypto (augecoin-crypto → WASM)
 └── sdk-ts/         → @augecoin/sdk (SDK TypeScript de alto nível)

apps/
 ├── wallet-web/     → React + TypeScript + WASM
 ├── wallet-desktop/ → Tauri + Rust nativo
 └── wallet-mobile/  → React Native + módulo nativo
```

---

## 14. Deploy e Infraestrutura

### 14.1 Testnet Padrão

`docker-compose` com:
- 4 nós validadores com identidades criptográficas próprias e volumes persistentes
- 1 nó observador (sem poder de consenso)
- Prometheus + Grafana

### 14.2 Script de Gênesis

Geração determinística do bloco 0:
- Define os 4 validadores iniciais
- Timestamp de gênesis
- Parâmetros de emissão conforme SPEC
- Bloco gênesis é idêntico em qualquer máquina que execute o script com os mesmos parâmetros

---

## 15. Segurança e Auditoria

### 15.1 Checklist de Hardening

| Item | Descrição |
|---|---|
| Auditoria de dependências | `cargo audit` — zero vulnerabilidades conhecidas |
| Fuzzing | Desserialização de transações e blocos (`cargo fuzz`) |
| Gestão de chaves | Chaves de validadores nunca em arquivo versionado |
| Teste de carga | Taxa máxima de tx/s sem degradar tempo de bloco de 60s |
| Teste de partição | Partição 2 vs 2: nenhum lado finaliza; recuperação automática |

### 15.2 Plano de Resposta a Incidentes

Documentado em `INCIDENT_RESPONSE.md`, cobrindo:
- Validador comprometido
- Bug de consenso em produção
- Procedimentos de rollback e recuperação

---

## 16. Referência Conceitual

A arquitetura de contas numeradas permanentes, o conceito de checkpoint de estado (equivalente moderno ao "SafeBox") e o modelo PoA com quórum 2/3 utilizam o **PascalCoin** como referência conceitual de protocolo. **Nenhum código Object Pascal é reaproveitado** — a implementação é um design original em Rust/TypeScript, construído do zero com as melhores práticas modernas de engenharia de software.

---

## 17. Conclusão

AUGECOIN é uma blockchain projetada para durar 50 anos com segurança de longo prazo e previsibilidade econômica absoluta. Suas características distintivas — assinatura híbrida pós-quântica desde o gênesis, emissão determinística sem pré-mineração, PoA com quórum de 2/3+1, e ecossistema completo de ferramentas (SDK, wallets multiplataforma, CLI administrativo, observabilidade) — a posicionam como uma solução robusta para redes institucionais que exigem confiabilidade, simplicidade operacional e resistência a ameaças quânticas futuras.

---

*Este documento é a fonte da verdade do protocolo AUGECOIN v1.0. Qualquer alteração deve ser registrada como ADR em DECISIONS.md.*
