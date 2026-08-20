# AUGECOIN — PROMPTS.md
### Cole UM microprompt por vez no OpenCode, na ordem. Não pule, não junte dois. Confira `PROGRESS.md` para saber qual é o próximo.

Cada microprompt já inclui, no topo, a instrução de releitura de
contexto — você não precisa colar `SPEC.md`/`PROJECT_RULES.md` de novo
a cada vez, só garanta que eles estão no repositório.

---

## FASE-00A — Monorepo e crates vazios

```
Leia PROJECT_RULES.md, SPEC.md (seção 6) e PROGRESS.md antes de começar.

TAREFA: criar o monorepo Rust (cargo workspace) com os crates vazios
listados em SPEC.md seção 6 — apenas Cargo.toml + lib.rs esqueleto
compilável, SEM lógica de protocolo.

ESCOPO PERMITIDO:
- Cargo.toml raiz (workspace)
- crates/augecoin-crypto, augecoin-core, augecoin-storage,
  augecoin-consensus, augecoin-network, augecoin-rpc, augecoin-cli,
  augecoin-node (só Cargo.toml + src/lib.rs ou src/main.rs vazios)
- rust-toolchain.toml
- docs/DECISIONS.md e docs/OPEN_QUESTIONS.md (copiar os arquivos deste
  kit para dentro do repo, na pasta docs/)

PROIBIDO: qualquer lógica de protocolo, qualquer dependência além do
essencial para compilar um crate vazio.

ENTREGÁVEIS: estrutura de diretórios completa, workspace compilável.

TESTES: nenhum ainda (não há lógica).

COMANDOS DE VERIFICAÇÃO:
cargo build --workspace

Ao final: preencha ADR-001, ADR-002 e ADR-004 em DECISIONS.md com a
data de hoje. Atualize PROGRESS.md marcando FASE-00A como concluída.
```

---

## FASE-00B — CI

```
Leia PROJECT_RULES.md e PROGRESS.md (confirme FASE-00A concluída).

TAREFA: configurar CI (GitHub Actions ou equivalente) rodando build,
test, clippy e fmt em todo push/PR.

ESCOPO PERMITIDO: .github/workflows/ci.yml (ou equivalente da
plataforma usada).

PROIBIDO: alterar qualquer crate criado na FASE-00A.

ENTREGÁVEIS: workflow de CI funcional.

COMANDOS DE VERIFICAÇÃO (devem estar todos no workflow, e você deve
rodá-los localmente para confirmar antes de commitar):
cargo build --workspace
cargo clippy --workspace -- -D warnings
cargo fmt --all -- --check

Ao final: atualize PROGRESS.md marcando FASE-00B como concluída.
```

---

## FASE-01A — Assinatura híbrida (Ed25519 + Dilithium)

```
Leia PROJECT_RULES.md, SPEC.md (seção 2) e PROGRESS.md.

TAREFA: implementar, dentro de augecoin-crypto, o KeyPair híbrido:
geração, assinatura e verificação usando Ed25519 (ed25519-dalek) E
Dilithium/ML-DSA (pqcrypto-dilithium ou oqs), com struct
HybridSignature { classic, post_quantum } cujo verify() exige AMBAS
válidas.

ESCOPO PERMITIDO: crates/augecoin-crypto/ apenas (módulo de
assinatura).

PROIBIDO: tocar em qualquer outro crate. Não implementar hashing,
hdkeys ou endereços ainda (isso é FASE-01B/C/D).

ENTREGÁVEIS: struct HybridSignature, geração de par de chaves híbrido,
assinatura, verificação.

TESTES OBRIGATÓRIOS:
- assinatura válida verifica com sucesso
- assinatura inválida (classic corrompida) é rejeitada
- assinatura inválida (post_quantum corrompida) é rejeitada
- assinatura de mensagem diferente é rejeitada

COMANDOS DE VERIFICAÇÃO:
cargo test -p augecoin-crypto -- --nocapture
cargo bench -p augecoin-crypto

Ao final: preencha ADR-003 em DECISIONS.md com a data de hoje.
Atualize PROGRESS.md.
```

---

## FASE-01B — Hashing BLAKE3-512 (XOF)

```
Leia PROJECT_RULES.md, SPEC.md (seção 2) e PROGRESS.md (confirme
FASE-01A concluída).

TAREFA: implementar módulo de hashing blake3_512(data: &[u8]) -> [u8;
64] usando o modo XOF do BLAKE3, dentro de augecoin-crypto.

ESCOPO PERMITIDO: crates/augecoin-crypto/ (novo módulo de hashing, não
tocar no módulo de assinatura da FASE-01A).

ENTREGÁVEIS: função blake3_512 pura e testável.

TESTES OBRIGATÓRIOS:
- vetor de teste fixo conhecido (input → output determinístico)
- inputs diferentes produzem outputs diferentes
- mesmo input sempre produz mesmo output

COMANDOS DE VERIFICAÇÃO:
cargo test -p augecoin-crypto -- --nocapture

Ao final: atualize PROGRESS.md.
```

---

## FASE-01C — hdkeys (derivação determinística)

```
Leia PROJECT_RULES.md, SPEC.md (seção 2) e PROGRESS.md (confirme
FASE-01A e FASE-01B concluídas).

TAREFA: implementar o módulo hdkeys — derivação
seed (BIP39) → HKDF-SHA3-512 → sementes por índice → par
determinístico (Ed25519 + Dilithium), conforme SPEC.md seção 2.

ESCOPO PERMITIDO: crates/augecoin-crypto/ (novo módulo hdkeys, usa os
módulos das FASE-01A/B mas não os modifica).

ENTREGÁVEIS: função de derivação determinística, com vetores de teste
fixos DOCUMENTADOS (salvos em arquivo próprio, ex:
crates/augecoin-crypto/tests/vectors/hdkeys.json) para reprodutibilidade
futura entre Rust core, TS SDK e mobile (usado na FASE-08C).

TESTES OBRIGATÓRIOS:
- mesma seed + mesmo índice → sempre a mesma chave (determinismo)
- índices diferentes → chaves diferentes
- os vetores de teste fixos batem exatamente com o resultado da função

COMANDOS DE VERIFICAÇÃO:
cargo test -p augecoin-crypto -- --nocapture

Ao final: atualize PROGRESS.md, citando o caminho do arquivo de
vetores de teste (será reusado na FASE-08C).
```

---

## FASE-01D — Endereços Bech32m

```
Leia PROJECT_RULES.md, SPEC.md (seção 2) e PROGRESS.md (confirme
FASE-01A/B/C concluídas).

TAREFA: implementar o módulo address — pubkeys (Ed25519 + Dilithium) →
endereço Bech32m com prefixo auge1..., incluindo checksum.

ESCOPO PERMITIDO: crates/augecoin-crypto/ (novo módulo address).

ENTREGÁVEIS: função de derivação de endereço a partir do par de chaves
híbrido (usa blake3_512 da FASE-01B).

TESTES OBRIGATÓRIOS:
- endereço gerado a partir de par de chaves fixo bate com valor
  esperado (vetor de teste)
- endereço corrompido/com typo é corretamente rejeitado na validação
- dois pares de chaves diferentes nunca geram o mesmo endereço (dentro
  do universo de teste)

COMANDOS DE VERIFICAÇÃO:
cargo test -p augecoin-crypto -- --nocapture

Ao final: atualize PROGRESS.md. Esta é a última sub-fase de
augecoin-crypto — confirme que TODOS os módulos (assinatura, hash,
hdkeys, address) têm cobertura de teste antes de avançar para FASE-02A.
```

---

## FASE-02A — AugeAccount

```
Leia PROJECT_RULES.md, SPEC.md (seções 3-4) e PROGRESS.md. Se a
Pergunta 4 de OPEN_QUESTIONS.md ainda não foi respondida, PARE e
pergunte ao responsável do projeto antes de decidir a lógica do campo
`name`.

TAREFA: definir e implementar, em augecoin-core, o tipo AugeAccount:
account_number: u64, pubkeys (do augecoin-crypto), balance: u64
(augesat), name: Option<String>, metadata: Vec<u8>, op_sequence: u64,
created_at_height: u64.

ESCOPO PERMITIDO: crates/augecoin-core/ apenas.

PROIBIDO: implementar Transaction, Block ou block_reward ainda (fases
seguintes). Não modificar augecoin-crypto.

ENTREGÁVEIS: struct AugeAccount com serialização canônica
determinística (formato documentado byte-a-byte em SPEC.md — se ainda
não estiver, adicione a documentação do formato exato).

TESTES OBRIGATÓRIOS:
- criar conta
- alterar saldo (incremento/decremento com checked arithmetic, nunca
  overflow silencioso)
- incrementar op_sequence
- serializar e deserializar (round-trip idêntico)
- hash da conta é determinístico

COMANDOS DE VERIFICAÇÃO:
cargo test -p augecoin-core -- --nocapture

Ao final: atualize PROGRESS.md.
```

---

## FASE-02B — Transaction (tipos de operação)

```
Leia PROJECT_RULES.md, SPEC.md e PROGRESS.md (confirme FASE-02A
concluída).

TAREFA: definir o tipo Transaction em augecoin-core, com os tipos de
operação: Transfer, ChangeKey, ClaimAccount (só a estrutura de dados —
a LÓGICA de execução de ClaimAccount é da FASE-06C, não implemente
regras de negócio aqui), SetAccountName, AttachData, ValidatorAdmin
(add/remove/activate/deactivate).

ESCOPO PERMITIDO: crates/augecoin-core/ (novo módulo transaction, usa
AugeAccount da FASE-02A mas não o modifica).

ENTREGÁVEIS: enum/struct Transaction com todos os tipos de operação
como variantes explícitas (não um formato genérico demais), incluindo
o campo sig_scheme_version, serialização canônica determinística.

TESTES OBRIGATÓRIOS:
- cada variante de operação serializa/deserializa corretamente
  (round-trip)
- transação malformada é rejeitada na deserialização

COMANDOS DE VERIFICAÇÃO:
cargo test -p augecoin-core -- --nocapture

Ao final: atualize PROGRESS.md.
```

---

## FASE-02C — Block/BlockHeader + serialização canônica

```
Leia PROJECT_RULES.md, SPEC.md e PROGRESS.md (confirme FASE-02A/B
concluídas).

TAREFA: definir Block { header, transactions, leader_signature,
quorum_signatures: Vec<HybridSignature> } e BlockHeader { height,
prev_hash (blake3-512), state_root, tx_merkle_root, timestamp,
leader_id, round_reward: u64, new_account_number: u64 }.

ESCOPO PERMITIDO: crates/augecoin-core/ (novo módulo block, usa
Transaction da FASE-02B e HybridSignature do augecoin-crypto, sem
modificar nenhum dos dois).

ENTREGÁVEIS: structs Block/BlockHeader, serialização canônica
determinística documentada byte-a-byte em SPEC.md (o hash do bloco
depende de bytes exatos — isso é crítico e deve ser explícito).

TESTES OBRIGATÓRIOS:
- serialização/deserialização round-trip de um bloco completo
- hash do bloco é determinístico e muda se qualquer campo do header
  mudar
- bloco malformado é rejeitado

COMANDOS DE VERIFICAÇÃO:
cargo test -p augecoin-core -- --nocapture

Ao final: atualize PROGRESS.md.
```

---

## FASE-02D — block_reward() e prova de emissão total

```
Leia PROJECT_RULES.md, SPEC.md (seção 3) e PROGRESS.md (confirme
FASE-02A/B/C concluídas).

TAREFA: implementar a função pura fn block_reward(height: u64) -> u64
exatamente conforme a regra determinística de SPEC.md seção 3
(divisão inteira truncada + distribuição do resto nos primeiros blocos
+ zero após TOTAL_EMISSION_BLOCKS).

ESCOPO PERMITIDO: crates/augecoin-core/ (novo módulo emission).

ENTREGÁVEIS: função block_reward(), com as constantes de SPEC.md seção
3 definidas como consts nomeadas (não números mágicos espalhados).

TESTES OBRIGATÓRIOS:
- block_reward(0), block_reward(1), block_reward(100),
  block_reward(1_000_000)
- block_reward(TOTAL_EMISSION_BLOCKS) — último bloco com recompensa
- block_reward(TOTAL_EMISSION_BLOCKS + 1) e além — sempre 0
- TESTE DE PROPRIEDADE (crate proptest) somando block_reward(h) para
  todo h de 1 até TOTAL_EMISSION_BLOCKS e verificando que o total é
  EXATAMENTE TOTAL_SUPPLY_AUGESAT — nem um augesat a mais ou a menos

COMANDOS DE VERIFICAÇÃO:
cargo test -p augecoin-core -- --nocapture
cargo test -p augecoin-core --test emission_total_supply -- --nocapture

Ao final: atualize PROGRESS.md. Este é o teste mais importante do
projeto inteiro em termos de correção monetária — não avance sem ele
passando de forma determinística (rodar 2-3 vezes para garantir que
não há flakiness no proptest).
```

---

## FASE-03A — RocksDB schema + accounts CF

```
Leia PROJECT_RULES.md, SPEC.md (seção 6) e PROGRESS.md (confirme todas
as fases 02 concluídas).

TAREFA: implementar, em augecoin-storage, o backend RocksDB com column
families separadas: accounts, blocks, tx_index, validator_set,
equivocation_proofs. Implementar apenas a CF accounts nesta sub-fase
(as demais ficam com schema definido mas sem lógica de leitura/escrita
ainda — serão preenchidas nas fases que as usam: blocks na FASE-06,
validator_set na FASE-04, equivocation_proofs na FASE-04D).

ESCOPO PERMITIDO: crates/augecoin-storage/ apenas. Usa AugeAccount de
augecoin-core, sem modificá-lo.

ENTREGÁVEIS: abertura de DB com as 5 CFs declaradas, funções
get_account/put_account/delete_account operando sobre a CF accounts.

TESTES OBRIGATÓRIOS:
- escrever conta e ler de volta (round-trip real em disco, não em
  memória)
- ler conta inexistente retorna None/erro apropriado, não panic
- sobrescrever conta existente atualiza corretamente

COMANDOS DE VERIFICAÇÃO:
cargo test -p augecoin-storage -- --nocapture

Ao final: atualize PROGRESS.md.
```

---

## FASE-03B — state_root via Merkle tree

```
Leia PROJECT_RULES.md, SPEC.md e PROGRESS.md (confirme FASE-03A
concluída).

TAREFA: implementar cálculo de state_root via Merkle tree (ou
Merkle-Patricia — decidir e REGISTRAR a escolha como novo ADR em
DECISIONS.md antes de implementar, já que SPEC.md deixa isso como
decisão de implementação) sobre todas as contas na CF accounts,
recalculado a cada bloco.

ESCOPO PERMITIDO: crates/augecoin-storage/ (novo módulo state_root, usa
a CF accounts da FASE-03A sem modificar sua interface pública).

ENTREGÁVEIS: função compute_state_root() determinística.

TESTES OBRIGATÓRIOS:
- state_root é determinístico para o mesmo conjunto de contas
- state_root muda se qualquer conta mudar
- state_root de um conjunto vazio de contas é um valor bem definido
  (não panic)

COMANDOS DE VERIFICAÇÃO:
cargo test -p augecoin-storage -- --nocapture

Ao final: adicione o ADR da escolha Merkle tree vs Merkle-Patricia em
DECISIONS.md. Atualize PROGRESS.md.
```

---

## FASE-03C — Checkpoints/snapshots

```
Leia PROJECT_RULES.md, SPEC.md e PROGRESS.md (confirme FASE-03A/B
concluídas).

TAREFA: implementar checkpoints periódicos (snapshot do estado a cada N
blocos, N parametrizável) para permitir que um nó novo sincronize sem
re-executar todo o histórico.

ESCOPO PERMITIDO: crates/augecoin-storage/ (novo módulo checkpoint).

ENTREGÁVEIS: função create_checkpoint(height) e
restore_from_checkpoint(height), formato de checkpoint versionado
(documentar em SPEC.md ou DECISIONS.md o formato exato).

TESTES OBRIGATÓRIOS:
- criar checkpoint e restaurar reproduz exatamente o mesmo estado
- restaurar de checkpoint inexistente falha de forma explícita, não
  silenciosa

COMANDOS DE VERIFICAÇÃO:
cargo test -p augecoin-storage -- --nocapture

Ao final: atualize PROGRESS.md.
```

---

## FASE-03D — Testes de reorg/corrupção/recuperação

```
Leia PROJECT_RULES.md, SPEC.md e PROGRESS.md (confirme FASE-03A/B/C
concluídas).

TAREFA: implementar e testar cenários de robustez de storage: rollback
de N blocos (reorg simples), corrupção simulada de arquivo, e
recuperação a partir do último checkpoint válido.

ESCOPO PERMITIDO: crates/augecoin-storage/ (testes de integração, pode
adicionar funções auxiliares de rollback se necessário, mas não alterar
a interface pública das FASE-03A/B/C sem necessidade comprovada pelo
teste).

TESTES OBRIGATÓRIOS:
- rollback de N blocos retorna o estado exato de antes
- corrupção de arquivo de DB é detectada (não silenciosamente
  ignorada)
- recuperação a partir do último checkpoint válido após corrupção
  funciona de ponta a ponta

COMANDOS DE VERIFICAÇÃO:
cargo test -p augecoin-storage -- --nocapture

Ao final: atualize PROGRESS.md. Esta é a última sub-fase de
augecoin-storage antes do consenso (FASE-04) — confirme cobertura
completa antes de avançar.
```

---

## FASE-04A — ValidatorSet + seleção de líder

```
Leia PROJECT_RULES.md, SPEC.md (seção 5) e PROGRESS.md. CONFIRME que a
Pergunta 2 e a Pergunta 3 de OPEN_QUESTIONS.md já foram respondidas —
se não, PARE e pergunte ao responsável do projeto antes de codificar
(o formato de assinatura de ValidatorAdmin depende da resposta da
Pergunta 3; o teto de crescimento do conjunto depende da Pergunta 2).

TAREFA: implementar ValidatorSet em augecoin-consensus, com estado
on-chain (add/remove/activate/deactivate só via operação
ValidatorAdmin assinada pela chave/multisig administradora, com
activation_height futuro obrigatório), e seleção determinística de
líder: leader_for_height(h) = active_validators[h %
active_validators.len()].

ESCOPO PERMITIDO: crates/augecoin-consensus/ apenas. Usa Transaction
(variante ValidatorAdmin) de augecoin-core sem modificá-la.

ENTREGÁVEIS: struct ValidatorSet, leader_for_height(), aplicação de
ValidatorAdmin com activation_height.

TESTES OBRIGATÓRIOS:
- leader_for_height é determinístico e correto para vários heights
- add validador com activation_height futuro não afeta a seleção antes
  da altura de ativação
- remove/deactivate segue a mesma regra de altura futura
- operação ValidatorAdmin sem assinatura administradora válida é
  rejeitada

COMANDOS DE VERIFICAÇÃO:
cargo test -p augecoin-consensus -- --nocapture

Ao final: registre a resposta das Perguntas 2 e 3 como novos ADRs em
DECISIONS.md e remova-as de OPEN_QUESTIONS.md (mova para a seção
"Perguntas já respondidas"). Atualize PROGRESS.md.
```

---

## FASE-04B — Máquina de estados Propose/Verify/Sign/Commit

```
Leia PROJECT_RULES.md, SPEC.md (seção 5) e PROGRESS.md (confirme
FASE-04A concluída).

TAREFA: implementar a máquina de estados da rodada de consenso:
Propose → Verify → Sign → Commit, com timeout configurável (padrão
120s = 2x BLOCK_TIME_SECONDS) e ViewChange para o próximo validador
quando o líder não propõe a tempo.

ESCOPO PERMITIDO: crates/augecoin-consensus/ (novo módulo round, usa
ValidatorSet da FASE-04A sem modificar sua interface pública).

ENTREGÁVEIS: enum/struct RoundState com as transições Propose→Verify→
Sign→Commit, lógica de timeout e ViewChange.

TESTES OBRIGATÓRIOS:
- transição normal Propose→Verify→Sign→Commit completa com sucesso
- timeout sem proposta do líder dispara ViewChange para o próximo
  validador da lista
- proposta inválida (ex: assinatura errada) não avança de Verify

COMANDOS DE VERIFICAÇÃO:
cargo test -p augecoin-consensus -- --nocapture

Ao final: atualize PROGRESS.md.
```

---

## FASE-04C — Verificação de quórum 2/3+1 e finalidade

```
Leia PROJECT_RULES.md, SPEC.md (seção 5) e PROGRESS.md (confirme
FASE-04A/B concluídas).

TAREFA: implementar a verificação de quórum (3 assinaturas válidas de
4 finalizam um bloco, generalizado para N validadores como 2/3+1) antes
de marcar um bloco como Finalized.

ESCOPO PERMITIDO: crates/augecoin-consensus/ (integra com a máquina de
estados da FASE-04B, sem reescrevê-la).

ENTREGÁVEIS: função verify_quorum(signatures, validator_set) -> bool e
marcação de bloco como Finalized só quando o quórum é atingido.

TESTES OBRIGATÓRIOS:
- exatamente 2/3+1 assinaturas válidas finaliza o bloco
- menos que o quórum não finaliza
- assinaturas de validadores fora do ValidatorSet ativo não contam
  para o quórum

COMANDOS DE VERIFICAÇÃO:
cargo test -p augecoin-consensus -- --nocapture

Ao final: atualize PROGRESS.md.
```

---

## FASE-04D — Detecção de equivocation

```
Leia PROJECT_RULES.md, SPEC.md (seção 5) e PROGRESS.md (confirme
FASE-04A/B/C concluídas).

TAREFA: implementar detecção de equivocation — se dois blocos
assinados pelo mesmo validador na mesma altura chegam à rede, gerar
EquivocationProof (contendo as duas assinaturas conflitantes) e
persistir na CF equivocation_proofs de augecoin-storage (criada como
schema vazio na FASE-03A).

ESCOPO PERMITIDO: crates/augecoin-consensus/ (novo módulo
equivocation) + crates/augecoin-storage/ (implementar leitura/escrita
na CF equivocation_proofs já existente, sem alterar as demais CFs).

ENTREGÁVEIS: struct EquivocationProof, função detect_equivocation(),
persistência da prova.

TESTES OBRIGATÓRIOS:
- dois blocos conflitantes do mesmo validador na mesma altura geram
  EquivocationProof correta
- blocos não conflitantes (alturas diferentes, ou mesmo bloco
  duplicado) não geram falso positivo
- prova persistida é recuperável do storage

COMANDOS DE VERIFICAÇÃO:
cargo test -p augecoin-consensus -- --nocapture
cargo test -p augecoin-consensus --test equivocation_detection -- --nocapture

Ao final: atualize PROGRESS.md.
```

---

## FASE-04E — Testes de integração 4 nós (liveness + equivocation)

```
Leia PROJECT_RULES.md, SPEC.md e PROGRESS.md (confirme FASE-04A/B/C/D
concluídas).

TAREFA: implementar teste de integração com 4 validadores em processos
separados (mesma máquina, ambiente de teste controlado — isto NÃO é a
simulação proibida pela Regra 1 de PROJECT_RULES.md, é validação de
integração padrão), incluindo cenário de 1 validador offline (deve
continuar finalizando com os 3 restantes) e cenário de equivocation
deliberada em teste (deve ser detectada e provada).

ESCOPO PERMITIDO: crates/augecoin-consensus/tests/ (arquivo de teste de
integração novo, não modificar módulos de produção das fases
anteriores a menos que o teste revele um bug real — se revelar,
corrigir e documentar em PROGRESS.md).

TESTES OBRIGATÓRIOS:
- 4 validadores finalizam blocos consecutivos corretamente
- 1 validador offline: os 3 restantes continuam finalizando (quórum
  ainda atingível: 3 de 4)
- 2 validadores offline: rede NÃO finaliza (quórum não atingível) —
  este teste confirma que o sistema falha de forma segura, não que ele
  "quebra"
- equivocation deliberada é detectada e a prova é gerada

COMANDOS DE VERIFICAÇÃO:
cargo test -p augecoin-consensus --test four_node_liveness -- --nocapture
cargo test -p augecoin-consensus --test equivocation_detection -- --nocapture

Ao final: atualize PROGRESS.md. Esta é a última sub-fase de
augecoin-consensus antes da rede (FASE-05).
```

---

## FASE-05A — libp2p transporte + Noise

```
Leia PROJECT_RULES.md, SPEC.md (seção 7 implícita — rede não tem seção
própria numerada, referencie a arquitetura de crates da seção 6) e
PROGRESS.md (confirme FASE-04 completa).

TAREFA: implementar, em augecoin-network, o transporte libp2p com
criptografia obrigatória via Noise Protocol (corrigir desde o primeiro
commit a vulnerabilidade de rede sem criptografia identificada na
análise de referência do PascalCoin).

ESCOPO PERMITIDO: crates/augecoin-network/ apenas.

ENTREGÁVEIS: configuração de transporte libp2p com Noise, handshake
funcional entre dois peers.

TESTES OBRIGATÓRIOS:
- handshake Noise entre dois peers reais (não mockado) completa com
  sucesso
- peer sem suporte a Noise é rejeitado na conexão

COMANDOS DE VERIFICAÇÃO:
cargo test -p augecoin-network -- --nocapture

Ao final: atualize PROGRESS.md.
```

---

## FASE-05B — gossipsub (tx e blocos)

```
Leia PROJECT_RULES.md e PROGRESS.md (confirme FASE-05A concluída).

TAREFA: implementar propagação de transações e blocos via gossipsub
sobre o transporte da FASE-05A.

ESCOPO PERMITIDO: crates/augecoin-network/ (novo módulo gossip, não
modificar o transporte da FASE-05A).

ENTREGÁVEIS: tópicos gossipsub para tx e blocos, publish/subscribe
funcionais.

TESTES OBRIGATÓRIOS:
- transação publicada por um peer é recebida por outro peer inscrito
- bloco publicado é recebido por todos os peers inscritos
- mensagem malformada no tópico não derruba o nó

COMANDOS DE VERIFICAÇÃO:
cargo test -p augecoin-network -- --nocapture

Ao final: atualize PROGRESS.md.
```

---

## FASE-05C — Descoberta de peers + autenticação de canal

```
Leia PROJECT_RULES.md e PROGRESS.md (confirme FASE-05A/B concluídas).

TAREFA: implementar descoberta de peers via lista estática de
bootnodes (decisão: para uma rede fechada de 4 validadores, lista
estática é suficiente e mais auditável que Kademlia DHT — registrar
como ADR se ainda não estiver decidido) e autenticação de peer: só nós
com chave de validador reconhecida participam do canal de consenso;
outros peers só recebem dados públicos via RPC.

ESCOPO PERMITIDO: crates/augecoin-network/ (novo módulo peer_auth +
config de bootnodes).

ENTREGÁVEIS: lista de bootnodes configurável, verificação de
credencial de validador no handshake de canal de consenso.

TESTES OBRIGATÓRIOS:
- peer com chave de validador reconhecida acessa canal de consenso
- peer sem credencial é limitado a dados públicos (RPC), não recebe
  mensagens do canal de consenso

COMANDOS DE VERIFICAÇÃO:
cargo test -p augecoin-network -- --nocapture

Ao final: registre o ADR da escolha lista estática vs DHT em
DECISIONS.md, se ainda não estiver lá. Atualize PROGRESS.md.
```

---

## FASE-05D — Sync inicial (fast sync via checkpoint)

```
Leia PROJECT_RULES.md e PROGRESS.md (confirme FASE-05A/B/C concluídas
e FASE-03C — checkpoints — concluída).

TAREFA: implementar protocolo de sincronização inicial: fast sync via
checkpoint mais recente (augecoin-storage FASE-03C) + replay dos blocos
restantes.

ESCOPO PERMITIDO: crates/augecoin-network/ (novo módulo sync, integra
com augecoin-storage sem modificar sua interface pública).

ENTREGÁVEIS: função de sync que traz um nó novo do zero até a altura
atual da rede via checkpoint + replay.

TESTES OBRIGATÓRIOS:
- nó novo sincroniza corretamente a partir de um checkpoint + blocos
  subsequentes reais
- sync interrompido no meio pode ser retomado sem corromper o estado

COMANDOS DE VERIFICAÇÃO:
cargo test -p augecoin-network -- --nocapture

Ao final: atualize PROGRESS.md.
```

---

## FASE-05E — Rate limiting e banimento de peers

```
Leia PROJECT_RULES.md e PROGRESS.md (confirme FASE-05A/B/C/D
concluídas).

TAREFA: implementar rate limiting e banimento temporário de peers que
enviam dados inválidos repetidamente (mitigação básica de DoS).

ESCOPO PERMITIDO: crates/augecoin-network/ (novo módulo rate_limit).

ENTREGÁVEIS: contador de infrações por peer, banimento temporário
configurável.

TESTES OBRIGATÓRIOS:
- peer enviando dados inválidos repetidamente é banido temporariamente
- peer bem comportado nunca é afetado pelo rate limit
- ban expira corretamente após o tempo configurado

COMANDOS DE VERIFICAÇÃO:
cargo test -p augecoin-network -- --nocapture

Ao final: atualize PROGRESS.md. Última sub-fase de augecoin-network
antes da execução de blocos (FASE-06).
```

---

## FASE-06A — Mempool com validação completa

```
Leia PROJECT_RULES.md, SPEC.md e PROGRESS.md (confirme FASE-02, 03 e
04 completas).

TAREFA: implementar mempool em augecoin-node, com validação de
assinatura híbrida, op_sequence (anti-replay), saldo suficiente e taxa
mínima antes de aceitar uma transação.

ESCOPO PERMITIDO: crates/augecoin-node/ apenas. Usa Transaction,
AugeAccount, HybridSignature das fases anteriores sem modificá-los.

ENTREGÁVEIS: estrutura de mempool, função validate_and_admit(tx).

TESTES OBRIGATÓRIOS:
- transação válida é admitida
- transação com assinatura inválida é rejeitada
- transação com op_sequence reutilizado (replay) é rejeitada
- transação com saldo insuficiente é rejeitada
- transação com taxa abaixo do mínimo é rejeitada

COMANDOS DE VERIFICAÇÃO:
cargo test -p augecoin-node -- --nocapture

Ao final: atualize PROGRESS.md.
```

---

## FASE-06B — execute_block() atômico

```
Leia PROJECT_RULES.md, SPEC.md (seção 4) e PROGRESS.md (confirme
FASE-06A concluída).

TAREFA: implementar execute_block(): aplica todas as tx do bloco,
credita block_reward(N) + soma das taxas ao validador líder, recalcula
state_root, tudo em uma transação atômica de banco de dados (se
qualquer etapa falhar, nada é persistido). NÃO implemente ainda a
criação da nova AugeAccount #N com sua regra de atribuição — isso
depende da FASE-06C.

ESCOPO PERMITIDO: crates/augecoin-node/ (novo módulo execution, integra
augecoin-storage/augecoin-core/augecoin-consensus sem modificar suas
interfaces públicas).

ENTREGÁVEIS: função execute_block() com atomicidade garantida.

TESTES OBRIGATÓRIOS:
- bloco válido: todas as tx aplicadas, saldo do validador líder
  atualizado com reward + taxas, state_root recalculado corretamente
- falha no meio da execução (simular com um erro real, ex: tx
  malformada no meio do lote): nada é persistido, estado permanece
  como antes do bloco

COMANDOS DE VERIFICAÇÃO:
cargo test -p augecoin-node -- --nocapture

Ao final: atualize PROGRESS.md.
```

---

## FASE-06C — Decisão de produto: ClaimAccount

```
Leia PROJECT_RULES.md, SPEC.md (seção 4) e OPEN_QUESTIONS.md (Pergunta
1). Esta fase SÓ pode começar depois que a Pergunta 1 for respondida
pelo responsável do projeto — se ainda estiver em aberto, PARE aqui e
pergunte antes de qualquer código.

TAREFA (após resposta): implementar a lógica de atribuição da nova
AugeAccount #N criada a cada bloco, conforme a resposta da Pergunta 1:
(a) auto-atribuição ao validador da rodada dentro de execute_block(), ou
(b) operação ClaimAccount disponível a qualquer usuário mediante taxa.

ESCOPO PERMITIDO: crates/augecoin-core/ (lógica de execução de
ClaimAccount, se opção b) e/ou crates/augecoin-node/ (integração em
execute_block() da FASE-06B).

ENTREGÁVEIS: dependem da opção escolhida — documentar a implementação
final em SPEC.md seção 4 (substituindo a referência a "ver
OPEN_QUESTIONS.md").

TESTES OBRIGATÓRIOS: cobertura completa do caminho escolhido (ex: se
(b), testar ClaimAccount bem-sucedido, tentativa de claim duplo,
saldo insuficiente para a taxa).

COMANDOS DE VERIFICAÇÃO:
cargo test -p augecoin-core -- --nocapture
cargo test -p augecoin-node -- --nocapture

Ao final: mova a Pergunta 1 para "Perguntas já respondidas" em
OPEN_QUESTIONS.md, registre o ADR correspondente em DECISIONS.md, e
atualize SPEC.md seção 4. Atualize PROGRESS.md.
```

---

## FASE-06D — Teste end-to-end de execução de bloco

```
Leia PROJECT_RULES.md e PROGRESS.md (confirme FASE-06A/B/C
concluídas).

TAREFA: implementar teste de integração ponta-a-ponta: enviar
transação real → aparece no mempool → é incluída no próximo bloco →
saldo atualizado corretamente → nova conta criada com número
sequencial correto → validador recebe recompensa + taxa exatas.

ESCOPO PERMITIDO: crates/augecoin-node/tests/ (arquivo de teste de
integração; correções em módulos de produção só se o teste revelar bug
real, documentado em PROGRESS.md).

COMANDOS DE VERIFICAÇÃO:
cargo test -p augecoin-node --test end_to_end_block_execution -- --nocapture

Ao final: atualize PROGRESS.md. Marco importante: a partir daqui a
blockchain funciona localmente sem P2P (equivalente ao "Ciclo A" da
metodologia).
```

---

## FASE-07A — JSON-RPC 2.0 + gRPC sobre TLS (scaffold)

```
Leia PROJECT_RULES.md, SPEC.md (seção 6) e PROGRESS.md (confirme
FASE-06 completa).

TAREFA: montar o scaffold de augecoin-rpc: servidor JSON-RPC 2.0 e
gRPC, ambos rodando obrigatoriamente sobre TLS (rejeitar conexão HTTP
pura em modo produção). Apenas o scaffold — endpoints reais são da
FASE-07B.

ESCOPO PERMITIDO: crates/augecoin-rpc/ apenas.

ENTREGÁVEIS: servidor RPC subindo com TLS, rejeitando conexão não-TLS.

TESTES OBRIGATÓRIOS:
- servidor aceita conexão TLS válida
- servidor rejeita conexão HTTP pura

COMANDOS DE VERIFICAÇÃO:
cargo test -p augecoin-rpc -- --nocapture

Ao final: atualize PROGRESS.md.
```

---

## FASE-07B — Endpoints RPC completos

```
Leia PROJECT_RULES.md, SPEC.md e PROGRESS.md (confirme FASE-07A
concluída).

TAREFA: implementar os endpoints: getAccount, getBlock,
getBlockByHeight, sendTransaction, getMempool, getValidatorSet,
getNetworkStatus, getValidatorEarnings — todos integrando com
augecoin-node/augecoin-storage/augecoin-consensus sem modificar suas
interfaces públicas.

ESCOPO PERMITIDO: crates/augecoin-rpc/ (novo módulo endpoints).

TESTES OBRIGATÓRIOS: cada endpoint testado com caso de sucesso e pelo
menos um caso de erro (ex: getAccount de conta inexistente,
sendTransaction malformada).

COMANDOS DE VERIFICAÇÃO:
cargo test -p augecoin-rpc -- --nocapture

Ao final: atualize PROGRESS.md.
```

---

## FASE-07C — Auth de endpoints administrativos + rate limiting

```
Leia PROJECT_RULES.md e PROGRESS.md (confirme FASE-07A/B concluídas).

TAREFA: implementar autenticação/API key para endpoints administrativos
(ValidatorAdmin), separada dos endpoints públicos de leitura, e rate
limiting por IP/API key.

ESCOPO PERMITIDO: crates/augecoin-rpc/ (novo módulo auth).

TESTES OBRIGATÓRIOS:
- endpoint administrativo sem API key válida é rejeitado
- endpoint público não exige API key
- rate limit bloqueia excesso de requisições do mesmo IP/API key

COMANDOS DE VERIFICAÇÃO:
cargo test -p augecoin-rpc -- --nocapture
grpcurl -plaintext=false localhost:9443 augecoin.v1.NodeService/GetNetworkStatus

Ao final: atualize PROGRESS.md. Última sub-fase de augecoin-rpc.
```

---

## FASE-08A — augecoin-crypto → WASM

```
Leia PROJECT_RULES.md, SPEC.md (seção 7) e PROGRESS.md (confirme
augecoin-crypto — FASE-01 completa — e FASE-07 completas).

TAREFA: compilar augecoin-crypto para WASM via wasm-pack, expondo como
pacote npm @augecoin/wasm-crypto.

ESCOPO PERMITIDO: crates/augecoin-crypto/ (bindings WASM, sem alterar a
lógica interna já testada nas FASE-01A-D) + packages/wasm-crypto/
(config do pacote npm).

TESTES OBRIGATÓRIOS: build WASM bem-sucedido; teste smoke chamando
geração de chave + assinatura + verificação a partir de JS.

COMANDOS DE VERIFICAÇÃO:
wasm-pack build crates/augecoin-crypto --target web

Ao final: atualize PROGRESS.md.
```

---

## FASE-08B — SDK TS de alto nível + codegen do .proto

```
Leia PROJECT_RULES.md e PROGRESS.md (confirme FASE-08A concluída).

TAREFA: implementar @augecoin/sdk: criação de carteira HD, assinatura
de transação, chamadas RPC tipadas (geradas a partir do .proto do gRPC
da FASE-07A), utilitários de formatação de saldo (augesat ↔ AUGE).

ESCOPO PERMITIDO: packages/sdk-ts/ apenas.

TESTES OBRIGATÓRIOS: criação de wallet, assinatura de transação de
teste, formatação augesat↔AUGE (incluindo casos de borda como 0 e
valor máximo do supply).

COMANDOS DE VERIFICAÇÃO:
npm --prefix packages/sdk-ts test

Ao final: atualize PROGRESS.md.
```

---

## FASE-08C — Paridade de vetores de teste Rust↔TS

```
Leia PROJECT_RULES.md e PROGRESS.md (confirme FASE-08A/B concluídas e
o caminho dos vetores de teste da FASE-01C anotado).

TAREFA: usar os MESMOS vetores de teste fixos da FASE-01C
(crates/augecoin-crypto/tests/vectors/hdkeys.json) para confirmar que a
derivação de chave em TS bate byte-a-byte com a do Rust.

ESCOPO PERMITIDO: packages/sdk-ts/tests/ (novo arquivo de teste,
consumindo o JSON de vetores existente, sem duplicá-lo).

TESTES OBRIGATÓRIOS: cada vetor do arquivo JSON produz, em TS, o mesmo
resultado byte-a-byte que em Rust.

COMANDOS DE VERIFICAÇÃO:
npm --prefix packages/sdk-ts test

Ao final: atualize PROGRESS.md. Última sub-fase do SDK TS.
```

---

## FASE-09A — Wallet Web

```
Leia PROJECT_RULES.md, SPEC.md (seção 7) e PROGRESS.md (confirme
FASE-08 completa).

TAREFA: implementar wallet web (React + TypeScript + @augecoin/
wasm-crypto): geração de carteira com seed BIP39 exibida uma única vez
com confirmação obrigatória, armazenamento IndexedDB criptografado
(Argon2id + AES-256-GCM), tela de saldo/histórico/envio-recebimento com
QR code Bech32m.

ESCOPO PERMITIDO: apps/wallet-web/ apenas.

PROIBIDO: qualquer chamada que envie chave privada ou seed para um
servidor — nenhum dado sensível sai do dispositivo, só chamadas RPC via
TLS.

TESTES OBRIGATÓRIOS: fluxo completo de criação de carteira, cifragem/
decifragem do keystore local, exibição e verificação de QR code.

COMANDOS DE VERIFICAÇÃO:
npm --prefix apps/wallet-web run build && npm --prefix apps/wallet-web test

Ao final: atualize PROGRESS.md.
```

---

## FASE-09B — Wallet Desktop (Tauri)

```
Leia PROJECT_RULES.md e PROGRESS.md (confirme FASE-09A concluída).

TAREFA: implementar wallet desktop via Tauri (Rust) usando
augecoin-crypto diretamente (sem WASM), reaproveitando o design system
do wallet-web. Keystore do SO (Keychain/Credential Manager/libsecret).

ESCOPO PERMITIDO: apps/wallet-desktop/ apenas.

PROIBIDO: mesmo princípio da FASE-09A — nenhum dado sensível sai do
dispositivo.

COMANDOS DE VERIFICAÇÃO:
npm --prefix apps/wallet-desktop run tauri build

Ao final: atualize PROGRESS.md.
```

---

## FASE-09C — Wallet Mobile (React Native)

```
Leia PROJECT_RULES.md e PROGRESS.md (confirme FASE-09A/B concluídas).

TAREFA: implementar wallet mobile via React Native + módulo nativo para
operações criptográficas sensíveis (evitar WASM em RN por
performance). Keystore/Secure Enclave nativos.

ESCOPO PERMITIDO: apps/wallet-mobile/ apenas.

PROIBIDO: mesmo princípio das FASE-09A/B.

COMANDOS DE VERIFICAÇÃO:
npm --prefix apps/wallet-mobile test

Ao final: atualize PROGRESS.md. Última sub-fase de wallets.
```

---

## FASE-10A — CLI: comandos de validador

```
Leia PROJECT_RULES.md, SPEC.md (seção 7) e OPEN_QUESTIONS.md (Pergunta
3 — deve já estar resolvida desde a FASE-04A; se não, pare e resolva
antes). Leia PROGRESS.md (confirme FASE-04 e FASE-07 completas).

TAREFA: implementar em augecoin-cli: validator add/remove/activate/
deactivate/list, exigindo assinatura da chave/multisig administradora
(conforme decisão da Pergunta 3) para add/remove/activate/deactivate.

ESCOPO PERMITIDO: crates/augecoin-cli/ apenas.

ENTREGÁVEIS: comandos com saída tabular e --json, autocompletar de
shell via clap.

TESTES OBRIGATÓRIOS: cada comando com caso de sucesso; comando
administrativo sem assinatura válida é rejeitado pela CLI.

COMANDOS DE VERIFICAÇÃO:
cargo test -p augecoin-cli -- --nocapture
augecoin-cli --help
augecoin-cli validator list --json | jq .

Ao final: atualize PROGRESS.md.
```

---

## FASE-10B — CLI: status e earnings

```
Leia PROJECT_RULES.md e PROGRESS.md (confirme FASE-10A concluída).

TAREFA: implementar augecoin-cli status (altura, peers, sincronização,
saúde do quórum) e validator earnings --id/--all (total AUGE ganho em
recompensa + taxas, com ranking para --all).

ESCOPO PERMITIDO: crates/augecoin-cli/ (novos comandos, não modificar
os da FASE-10A).

COMANDOS DE VERIFICAÇÃO:
cargo test -p augecoin-cli -- --nocapture

Ao final: atualize PROGRESS.md.
```

---

## FASE-10C — CLI: comandos de segurança

```
Leia PROJECT_RULES.md e PROGRESS.md (confirme FASE-10A/B concluídas e
FASE-04D — equivocation — e FASE-05E — banimento — completas).

TAREFA: implementar security equivocations (lista provas registradas),
security banned-peers, security alerts --tail (stream em tempo real).

ESCOPO PERMITIDO: crates/augecoin-cli/ (novos comandos).

COMANDOS DE VERIFICAÇÃO:
cargo test -p augecoin-cli -- --nocapture

Ao final: atualize PROGRESS.md. Última sub-fase da CLI.
```

---

## FASE-11A — Métricas Prometheus

```
Leia PROJECT_RULES.md, SPEC.md (seção 8) e PROGRESS.md (confirme
FASE-06/07 completas).

TAREFA: expor métricas Prometheus pelo nó: altura do bloco, tempo médio
de bloco, peers conectados, tamanho do mempool, taxa de equivocation,
latência de propagação de bloco.

ESCOPO PERMITIDO: crates/augecoin-node/ (novo módulo metrics).

COMANDOS DE VERIFICAÇÃO:
curl -s localhost:9100/metrics | grep augecoin_

Ao final: atualize PROGRESS.md.
```

---

## FASE-11B — Dashboard Grafana versionado

```
Leia PROJECT_RULES.md e PROGRESS.md (confirme FASE-11A concluída).

TAREFA: criar dashboard Grafana pré-configurado (arquivo .json
versionado no repo) mostrando: saúde do quórum, ganhos por validador ao
longo do tempo, eventos de segurança.

ESCOPO PERMITIDO: infra/observability/ (arquivo .json de dashboard +
docker-compose de Prometheus/Grafana).

COMANDOS DE VERIFICAÇÃO:
docker compose -f infra/observability.yml up -d

Ao final: atualize PROGRESS.md.
```

---

## FASE-11C — Detecção ativa de comportamento malicioso

```
Leia PROJECT_RULES.md e PROGRESS.md (confirme FASE-11A/B, FASE-04D e
FASE-05E completas).

TAREFA: implementar alertas ativos: equivocation (já detectada na
FASE-04D, aqui é o alerta), peer malicioso repetido (auto-ban já existe
na FASE-05E, aqui é o log estruturado + alerta), validador ausente além
do timeout, tentativa de op_sequence reutilizado. Integração opcional
de webhook genérico (Telegram/Slack/Discord) via arquivo de config, não
hardcoded.

ESCOPO PERMITIDO: crates/augecoin-node/ (novo módulo alerts, integra
com os módulos já existentes sem modificá-los).

COMANDOS DE VERIFICAÇÃO:
cargo test -p augecoin-node -- --nocapture

Ao final: atualize PROGRESS.md. Última sub-fase de observabilidade.
```

---

## FASE-12A — docker-compose 4 nós + script de gênesis

```
Leia PROJECT_RULES.md, SPEC.md e PROGRESS.md (confirme TODAS as fases
anteriores — 00 a 11 — completas).

TAREFA: criar docker-compose com 4 nós validadores reais + 1 nó
observador + Prometheus/Grafana, cada nó com identidade criptográfica
própria e volume persistente. Script de genesis real: define os 4
validadores iniciais, timestamp de gênesis, parâmetros de SPEC.md
seção 3, gerando o bloco 0 determinístico.

ESCOPO PERMITIDO: infra/testnet-4node.yml, scripts/genesis/.

COMANDOS DE VERIFICAÇÃO:
docker compose -f infra/testnet-4node.yml up --build -d
augecoin-cli status --endpoint https://localhost:9443

Ao final: atualize PROGRESS.md.
```

---

## FASE-12B — Hardening (audit, fuzz, load test, partição)

```
Leia PROJECT_RULES.md e PROGRESS.md (confirme FASE-12A concluída).

TAREFA: rodar e documentar o checklist de hardening: auditoria de
dependências, fuzzing da desserialização de transações/blocos, revisão
de gerenciamento de chaves dos validadores (nunca em arquivo de config
versionado), teste de carga (taxa máxima de tx/s sem degradar o tempo
de bloco de 60s), teste de partição de rede (2 validadores isolados dos
outros 2 — quórum não deve ser atingido, rede deve se recuperar
automaticamente quando a partição se resolve).

ESCOPO PERMITIDO: correções pontuais em qualquer crate SE o hardening
revelar um bug real — cada correção deve ser documentada em
PROGRESS.md com referência ao teste que a motivou. Não usar esta fase
para adicionar funcionalidade nova.

COMANDOS DE VERIFICAÇÃO:
cargo audit
cargo fuzz run tx_deserialize -- -max_total_time=300

Ao final: atualize PROGRESS.md com os resultados de cada item do
checklist (não só "passou/falhou" — anotar números reais: tx/s
suportado, tempo de recuperação de partição, etc.).
```

---

## FASE-13 — Checklist final pré-mainnet

```
Leia PROJECT_RULES.md, SPEC.md, DECISIONS.md, OPEN_QUESTIONS.md
(deve estar vazio ou só com itens não-bloqueantes) e PROGRESS.md
(TODAS as fases devem estar [x]).

TAREFA: percorrer e confirmar, item a item, antes de gerar o bloco
gênesis real:

- [ ] Todos os testes das fases 00-12 passam em CI, sem flakiness
      (rodar a suíte completa 2-3 vezes seguidas para confirmar)
- [ ] SPEC.md e DECISIONS.md refletem exatamente o que foi
      implementado (nenhuma divergência entre spec e código)
- [ ] OPEN_QUESTIONS.md não tem nenhuma pergunta bloqueante pendente
- [ ] Chaves dos 4 validadores geradas em ambiente isolado, nunca
      transmitidas em texto puro, com backup testado (restore real)
- [ ] Modelo de custódia da chave administradora revisado uma última
      vez (ADR correspondente confirmado)
- [ ] Auditoria externa (mesmo que informal) de augecoin-crypto e
      augecoin-consensus
- [ ] Plano de resposta a incidentes documentado (validador
      comprometido, bug de consenso em produção)

Não gerar o bloco gênesis de produção enquanto qualquer item acima
estiver incompleto.

Ao final: atualize PROGRESS.md marcando FASE-13 como concluída — este
é o marco de prontidão para mainnet.
```
