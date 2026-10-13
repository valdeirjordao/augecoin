# Proposta técnica — State root incremental (protocolo v6)

Status: **implementado e desativado por padrão**. A implementação definitiva
(árvore compacta, persistida, versionada e com ativação explícita) está em
`crates/augecoin-storage/src/state_tree.rs` e descrita em
`docs/STATE_ROOT_SMT_V6.md`. A mitigação segura (cache do root + hash do SafeBox
adiado) continua em produção; a nova definição de root só é usada quando
`AUGECOIN_STATE_ROOT_V6_ACTIVATION` é configurado (mainnet: não configurado).
As seções abaixo permanecem como registro do desenho original.

## 1. Semântica atual (a preservar ou versionar)

Implementação: `crates/augecoin-storage/src/state_root.rs`
(`compute_consensus_state_root`) e o valor consumido como
`OperationBlockHeader::initial_safe_box_hash` (`crates/augecoin-node/src/consensus.rs`).

O root atual é um único BLAKE3-512 sobre a concatenação, sem separador de byte
entre registros, de:

1. Prefixo de domínio `b"AUGECOIN_CONSENSUS_STATE_V1"`.
2. Para cada column family em [`CONSENSUS_COLUMN_FAMILIES`] **não vazia**, na
   ordem da lista:
   - `len(cf_name) as u32 BE` + `cf_name`;
   - `len(records) as u64 BE`;
   - para cada `(key, value)` em ordem lexicográfica RocksDB
     (`IteratorMode::Start`): `len(key) as u32 BE` + `key` + `len(value) as u64 BE`
     + `value`.
3. Se todas as CFs estiverem vazias → `[0u8; 64]` (`EMPTY_STATE_ROOT`).

CFs incluídas: `accounts`, `validator_set` (inclui as chaves `vset` **e**
`height`), `contracts`, `contract_metadata`, `contract_balances`,
`contract_codes`, `contract_code_hash`.

Propriedades: determinístico, ordem total por bytes, sem exclusões parciais
(a ausência de uma chave é "não contribui"), sensível a qualquer mudança de
byte em qualquer CF incluída.

Custo atual: **O(estado total)** por chamada. Em `1.059.626` contas
(139,5 MiB), medido em 947 ms; chamado 2× por bloco por validador
(`verify_block_pre_state` + fim de `execute_block`).

## 2. Objetivo

Reduzir a atualização da raiz de `O(total de contas)` para
`O(k · log n)` (k = contas tocadas no bloco, n = total), mantendo:

- um root **autenticado** e determinístico entre os 4 validadores;
- prova de inclusão/estado para light clients (opcional, mas desejável);
- atualização atômica junto ao `WriteBatch` do bloco;
- rollback/recuperação consistentes.

## 3. Alternativas avaliadas

| Estrutura | Update | Consulta | Prova | Persistência |
|---|---:|---:|---|---|
| Flat hash (atual) | O(n) | — | não | — |
| Merkle binário em memória | O(n) (reordenamento) | O(1) raiz | O(log n) | serializar árvore inteira |
| **Sparse Merkle Tree (SMT)** | O(log n) | O(log n) | O(log n) | nós em CF `state_nodes` |
| Merkle Patricia Trie | O(log n) | O(log n) | O(log n) | nós em CF `state_nodes` |

A `SafeBox` já mantém uma Merkle root **sobre contas** (`compute_safe_box_hash`,
`crates/augecoin-core/src/safe_box.rs`), mas ela é posicional (BTreeMap) e
recalculada por inteiro a cada `add_account`; não é incremental e não cobre
validadores/contratos. Serve como referência de corretude, não como base.

**Escolha: Sparse Merkle Tree com chave determinística por registro.**

## 4. Desenho proposto

- Espaço de chaves de 256 bits. Chave do registro:
  `H(cf_name_bytes || key_bytes)` (BLAKE3-512 truncado para 256 bits).
- Folha: `H(0x00 || key256 || H(value_bytes))`; vazio: `H(0x01)`.
  A inclusão de `cf_name` e `key` no hash impede reinterpretação de registros
  entre CFs.
- Nó interno: `H(0x02 || left || right)` (sem colisão com folhas/vazios).
- Root: raiz da SMT; estado vazio → `[0u8;64]` (mantém o valor sentinela).
- Persistência: cada nó da SMT é gravado na CF dedicada `state_nodes`, chaveado
  por `path` (bits percorridos). Apenas os nós no caminho das folhas alteradas
  (`O(k · 256)` no pior caso, na prática `O(k · log n)`) são regravados.
- Atualização atômica: os `put`/`delete` de `state_nodes` entram no **mesmo**
  `WriteBatch` já usado por `commit_block_atomic_with_contracts`
  (`crates/augecoin-storage/src/lib.rs`), junto com contas, bloco, altura,
  validadores e contratos. Um crash não deixa root e estado divergentes.
- Cache do root: reutiliza a geração já introduzida (`ConsensusRootCache`);
  a SMT mantém o valor em memória e o invalida por geração.

### 4.1 Inserção / remoção / atualização

- `put(cf, key, value)`: calcular `key256`, ler o caminho até a folha, regravar
  as folhas/nós do caminho e recomputar os hashes até a raiz.
- `delete(cf, key)`: gravar folha vazia e recomputar o caminho.
- `height`/`vset` continuam em `validator_set`, tratados como quaisquer registros.

### 4.2 Determinismo

Todas as operações são função pura de `(cf_name, key, value)` e da ordem de
inserção por `cf_name` + `key`, idênticas em todos os nós. O root não depende de
tempo, ordem de execução de operações dentro do bloco (a execução é sequencial e
determinística) nem de estrutura de memória (bitmap/hashmap). Não há aleatoriedade.

### 4.3 Rollback e recuperação

- Como `state_nodes` é gravado no mesmo batch do bloco, "voltar" um bloco não
  aplicado é não gravar o batch. Não há rollback in-place.
- Recuperação após crash: a SMT é reconstruível a partir das CFs de estado
  (contas/validadores/contratos) — propriedade exigida pelos testes de
  recuperação já existentes (`safebox_recovers_without_snapshot`,
  `consensus_root_is_stable_across_reopen`). Fornece-se um
  `rebuild_state_tree()` usado no primeiro boot pós-migração e em auditoria.
- O cache de root só publica um valor sob a mesma geração (já implementado),
  evitando corrida com escrita concorrente.

### 4.4 Compatibilidade e ativação

- O novo root **muda de valor**. Ativação via `protocol_version`: blocos com
  `protocol_version < 6` usam `compute_consensus_state_root` (flat); a partir
  de 6 usam a SMT. O nó valida a raiz pelo método correspondente à versão do
  bloco, permitindo sincronização de blocos históricos.
- Migração: no primeiro boot com protocolo 6, construir a SMT a partir do estado
  e persistir em `state_nodes` (tarefa única, alinhada ao intervalo de snapshot).
- **Não ativar na mainnet** nesta tarefa: implementar em branch
  `feat/state-root-smt`, validar em testnet de 4 validadores por ≥N alturas,
  comparar roots e latência, e só então propor a ativação (bump de
  `CT_BUILD_PROTOCOL`/`CT_MAX_PROTOCOL` e ADR).

## 5. Impacto esperado

- `consensus_state_root`: de ~947 ms (1,06 M contas) para `O(k·log n)` —
  dezenas de microssegundos por bloco com poucas contas tocadas.
- `execute_block` (bloco vazio): de ~1,0 s para <10 ms.
- Raiz desacoplada do tamanho do estado: elimina a degradação crescente do tempo
  de bloco (o estado cresce ~3 contas/bloco).
- Ganho opcional: provas Merkle de conta para light clients.

## 6. Estratégia de testes

1. Igualdade de sequência: para um conjunto de blocos de referência, o root
   v6 (SMT) converge com o root v5 (flat) quando o estado é idêntico **não** se
   aplica (definições diferentes) — em vez disso, testar:
   - determinismo SMT entre instâncias independentes;
   - `insert`/`update`/`delete` produzem a mesma raiz que um rebuild completo;
   - roundtrip de prova de inclusão/ausência.
2. Migração: build SMT a partir de estado existente == rebuild; reboots idempotentes.
3. Falhas: crash no meio do batch não deixa SMT inconsistente; recuperação reconstrói.
4. Compatibilidade: blocos v5 continuam validando com o flat root; blocos v6 com SMT.
5. Carga: 1 M de contas, 1 k operações/bloco, latência p50/p95/p99.

## 7. Riscos

- Complexidade criptográfica nova (SMT): exige revisão dedicada e vetores de
  teste (fuzzing) antes de qualquer ativação.
- Tamanho em disco de `state_nodes` (nós internos) — mitigado por batch e
  possível compactação.
- Hard fork: nós antigos não validam v6; requer coordenação de upgrade e
  `protocol_available`.

## 8. O que **não** faz parte desta entrega

- Implementação da SMT na mainnet.
- Alteração de `CT_CHAIN_ID_MAINNET`, genesis ou chaves.
- Migração irreversível de dados de produção.
