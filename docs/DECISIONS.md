# AUGECOIN — DECISIONS.md
### Log de ADRs (Architecture Decision Records). Toda decisão de design nova vira uma entrada aqui, no formato abaixo. Nunca editar ou apagar uma entrada existente — só adicionar um novo ADR que a substitui, marcando a antiga como `Superseded by ADR-XXX`.

## Formato

```
## ADR-XXX

Data: AAAA-MM-DD
Fase relacionada: FASE-XXA (id do microprompt em PROMPTS.md)
Decision: <o que foi decidido>
Reason: <por que>
Alternativas consideradas: <opções descartadas e por quê>
Status: Accepted | Superseded by ADR-YYY
```

---

## ADR-001

Data: 2026-08-09
Fase relacionada: FASE-00A
Decision: A implementação será feita em Rust (workspace de crates), com
SDK de superfície em TypeScript/WASM.
Reason: Segurança de memória, ecossistema maduro para blockchains
(Substrate, Solana), performance para verificação criptográfica híbrida.
Alternativas consideradas: Go (descartado — menor ecossistema
criptográfico PQC maduro em produção).
Status: Accepted

## ADR-002

Data: 2026-08-09
Fase relacionada: FASE-00A
Decision: Nenhum código do PascalCoin (Object Pascal) é traduzido linha
a linha. O PascalCoin é usado só como referência conceitual de
protocolo (contas numeradas, SafeBox, PoA de quórum).
Reason: Evitar herdar dívida arquitetural de um codebase histórico de
~1.750 commits em uma stack diferente; a própria documentação de
referência recomenda não reaproveitar o código original em produção
moderna.
Alternativas consideradas: Port direto assistido por IA (descartado —
alto risco de bugs sutis de tradução automática entre paradigmas muito
diferentes).
Status: Accepted

## ADR-003

Data: 2026-08-09
Fase relacionada: FASE-01A
Decision: Assinatura híbrida obrigatória (Ed25519 + Dilithium/ML-DSA)
desde o bloco gênesis; ambas as assinaturas devem verificar para uma
transação/bloco ser válido.
Reason: Postura de crypto-agility com defesa em profundidade para uma
rede de 50 anos de vida útil — quebrar Ed25519 via computador quântico
futuro não é suficiente sozinho para forjar uma transação.
Alternativas consideradas: Só Ed25519 com migração futura via hard fork
(descartado — expõe as contas ao risco de "harvest now, decrypt later"
por 50 anos); só Dilithium (descartado — sem histórico de auditoria tão
extenso quanto Ed25519, e mais lento).
Status: Accepted

## ADR-004

Data: 2026-08-09
Fase relacionada: FASE-00A
Decision: Consenso PoA (não PoS, não PoW), quórum 2/3+1, 4 validadores
no gênesis.
Reason: Especificação de produto definida pelo responsável do projeto
antes do início da implementação.
Alternativas consideradas: N/A — parâmetro de entrada, não decisão de
engenharia.
Status: Accepted

## ADR-005

Data: 2026-08-09
Fase relacionada: FASE-06C
Decision: A nova Conta AUGE #N criada a cada bloco é auto-atribuída ao
validador líder da rodada, dentro de execute_block(). Não há operação
ClaimAccount; a atribuição é automática e sem taxa adicional.
Reason: Decisão de produto do responsável do projeto. Simplifica a
execução do bloco e elimina a necessidade de um mecanismo de leilão/claim.
Alternativas consideradas: Opção (b) — ClaimAccount por qualquer usuário
mediante taxa (descartada pelo responsável).
Status: Accepted

## ADR-006

Data: 2026-08-20
Fase relacionada: SPRINT-01 (validator ecosystem — backend)
Decision: O serviço de backend da camada operacional (`crates/augecoin-ops`,
binário `augecoin-ops`) é implementado em Rust com `axum` 0.8 e persistência
em PostgreSQL via `sqlx`. A autenticação administrativa usa API key
compartilhada (header `x-api-key`, comparada por hash BLAKE3) no mesmo
padrão do node RPC. O serviço não guarda chaves privadas nem saldos — é uma
ponte entre o SaaS (licenças/ativação/heartbeat) e o consenso on-chain.
Reason: Reuso do stack e das convenções do core (Rust, env-var config,
`x-api-key`), garantias de memória para um serviço que manipula chaves
públicas e hashes de máquina, e PostgreSQL pela integridade referencial +
auditoria imutável + índices temporais para métricas.
Alternativas consideradas: Node/TypeScript (reusaria o `wallet-web-server`,
descartado por separar a autoridade operacional em duas runtimes); PHP
(descartado — não existe PHP no repositório).
Status: Accepted

## ADR-007

Data: 2026-08-20
Fase relacionada: SPRINT-01 (validator ecosystem — licenças)
Decision: A chave de licença (32 caracteres Crockford Base32, agrupados
8×4) é persistida somente como hash BLAKE3-256 (`license_key_hash`) + prefixo
de 4 caracteres para exibição. A chave em texto claro é retornada uma única
vez no momento da emissão e nunca é recuperável do banco.
Reason: Vazamento do banco não expõe chaves ativas; validação por lookup do
hash mantém comparação auditável e determinística. Padrão equivalente a
chaves de API de provedores de pagamento.
Alternativas consideradas: armazenar `license_key CHAR(32)` em texto claro
(descartado — risco direto de comprometimento em caso de leitura do banco);
assinatura digital da chave (descartado para este domínio — entropia de
160 bits dispensa assinatura; assinaturas ficam reservadas a atualizações
OTA e operações on-chain).
Status: Accepted

## ADR-008

Data: 2026-08-20
Fase relacionada: SPRINT-02 (validator ecosystem — validadores/ativação/heartbeat)
Decision: O domínio de validadores separa **binding** de **estado operacional**.
Os dados de ligação (machine_hash, public_key, augeid, ip) vivem na licença
(fonte única da verdade); a tabela `validators` guarda apenas a chave pública e
métricas espelhadas (uptime, blocks, blocks_lost, leadership, total_rewards,
last_seen, status). Presença (online/offline) é **derivada** de `last_seen`
contra a janela de heartbeat (120s), nunca armazenada — mesmo princípio do
`expired` derivado de `expires_at` no domínio de licenças.
Reason: Evita escrita dupla e divergência entre duas tabelas (ex: troca de
AUGEID), mantém o blockchain como autoridade das métricas, e trata online como
fato transitório e não como estado persistente.
Alternativas consideradas: denormalizar binding na tabela `validators`
(descartado — risco de divergência na troca de AUGEID/IP); armazenar presença
como coluna `online BOOLEAN` (descartado — desatualiza entre heartbeats sem um
job de reconciliação).
Status: Accepted

## ADR-009

Data: 2026-08-20
Fase relacionada: SPRINT-02 (validator ecosystem — bridge consenso)
Decision: O `augecoin-ops` é uma ponte, nunca um ator de consenso. `approve`,
`suspend` e `revoke` chamam os endpoints administrativos do node RPC
(`validatoradd`, `validatordeactivate`, `validatorremove`), que **assinam** a
`ValidatorAdminOp` com a chave mestre do node e a submetem pelo consenso. O
serviço ops não possui chave privada alguma; sem o node RPC configurado, as
transições que exigem efeito on-chain retornam erro (`NodeNotConfigured`).
Reason: Preserva a regra fundamental de que a participação no consenso acontece
apenas por transações administrativas assinadas pela chave mestre; a autoridade
do SaaS é somente autorização, monitoramento, licenciamento e auditoria.
Alternativas consideradas: importar a chave mestre no serviço ops e assinar lá
(descartado — amplia a superfície de exposição da chave mestre); manipular o
conjunto de validadores diretamente no storage do node (descartado — viola a
regra de nunca modificar consenso diretamente).
Status: Accepted

## ADR-010

Data: 2026-08-20
Fase relacionada: SPRINT-03 (validator ecosystem — dashboard financeiro)
Decision: Recompensas são persistidas num **livro-razão append-only**
(`validator_rewards`), espelho do on-chain, alimentado por um worker de sync
com cursor resumível (`sync_state`). A atribuição usa `leader_id` (header do
bloco) → `validators.node_validator_id`; cada linha registra `auge`
(reward + fees, em augesat), `fees` e `augeids` (10 por bloco). A idempotência
é garantida por `UNIQUE(validator_id, block_number)` + `ON CONFLICT DO NOTHING`.
As agregações por período (hora/dia/semana/mês/ano/total) são computadas em SQL
sobre `block_ts` (bucket de calendário em UTC), não em memória.
Reason: O blockchain é a autoridade dos saldos; o serviço só espelha. O cursor
+ UNIQUE tornam o replay determinístico e auditável (sem contagem dupla), e a
agregação em SQL evita drift de cálculo em memória entre processos.
Alternativas consideradas: assinar evento "novo bloco" do node (descartado —
mais frágil e complexo; polling com cursor é igualmente correto e mais simples);
manter `validators.total_rewards` como única fonte (descartado — não suporta
decomposição por período nem AUGE/AUGEID separados).
Status: Accepted

## ADR-011

Data: 2026-08-20
Fase relacionada: SPRINT-04 (validator ecosystem — OTA, monitoramento, auditoria)
Decision: OTA e monitoramento seguem a mesma disciplina de "o servidor não
detém chaves privadas". (1) Releases: a assinatura Ed25519 cobre o hash BLAKE3
do artefato; o servidor **verifica** a assinatura contra a chave pública de
release configurada (`AUGECOIN_OPS_RELEASE_PUBLIC_KEY`) antes de publicar, mas a
chave privada vive no pipeline de build. O cliente re-verifica com a chave
embutida antes de instalar. (2) Remoção PoA automática (heartbeat >10min) é
feita pelo monitor chamando o bridge do node (`validatordeactivate`), nunca
manipulando o conjunto diretamente. (3) Alertas são append-only com dedup por
`(validator, kind)` via índice único parcial (`resolved_at IS NULL`).
Reason: Nunca atualizar binários sem assinatura válida e nunca modificar
consenso diretamente são regras fundamentais; a verificação em duas pontas
(servidor + cliente) garante defesa em profundidade sem expor chave privada.
Alternativas consideradas: assinar releases com a chave mestre do node
(descartado — acopla OTA ao consenso e amplia exposição); auto-remover do PoA
sem o bridge (descartado — violaria a regra de consenso).
Status: Accepted

## ADR-012

Data: 2026-08-20
Fase relacionada: SPRINT-05 (validator ecosystem — painel operacional)
Decision: O painel operacional (`operacional.augeco.in`) fala **somente** com a
API do `augecoin-ops` (via proxy nginx `/api/`), nunca com o node RPC
diretamente. Aprovar/suspender/revogar validadores são endpoints do backend
(`/v1/validators/{id}/…`) que, por sua vez, chamam o bridge do node. Valores
monetários em augesat são serializados como **string** na API para evitar perda
de precisão acima de 2^53 no JavaScript.
Reason: Mantém a autoridade de consenso centralizada no node e a superfície
administrativa no backend (autorização/monitoramento/licenciamento/auditoria),
evita expor a API key do node ao browser e garante exatidão financeira.
Alternativas consideradas: painel falar direto com o node RPC (descartado —
exporia a chave admin do node no browser e duplicaria lógica de licenciamento);
serializar augesat como JSON number (descartado — perde precisão para totais de
rede acima de 2^53).
Status: Accepted

## ADR-013

Data: 2026-08-20
Fase relacionada: SPRINT-03/05 (validator ecosystem — wallet validador + pagamentos)
Decision: A seção Validador da wallet web usa o **servidor** da wallet como
bridge para o `augecoin-ops`. O servidor Node detém a chave admin do ops
(`AUGECOIN_OPS_ADMIN_KEY`) **server-side** (o browser nunca a vê) e expõe apenas
endpoints escopados ao usuário autenticado: pedidos de licença
(`validator_orders`), emissão (retorna a chave de licença em texto claro uma
única vez ao dono) e overview (licenças + validadores + recompensas). A
confirmação de pagamento é **manual** (operador, `x-confirm-key`): não há
aprovação automática nem gateway falso.
Reason: Separa identidade web (wallet) da autoridade operacional (ops) sem
expor a chave admin ao cliente; mantém a chave de licença efêmera (retornada
uma vez, nunca persistida); evita "mock permanente" de pagamento — o estado é
real e auditável, com integração de gateway (Pix PSP, USDT, AUGE on-chain) como
incremento posterior.
Alternativas consideradas: wallet falar direto com o ops a partir do browser
(descartado — exporia a chave admin ou exigiria credenciais por usuário);
auto-aprovar pagamento sem verificação (descartado — fraude e violação de
"nunca produzir mock permanente").
Status: Accepted

## ADR-014

Data: 2026-08-20
Fase relacionada: SPRINT-06/07 (validator ecosystem — Validator Desktop)
Decision: O Validator Desktop é uma app Tauri v2 com núcleo Rust que embute o
`augecoin-node` como **sidecar** (`externalBin`) e o executa com a seed Ed25519
gerada localmente. A chave privada é persistida em arquivo `0600` no diretório
de dados do app e passada ao node apenas por env var; o backend recebe somente a
chave pública e o `machine_id` (BLAKE3 de `install_id || hardware`, sem
persistir dados brutos). OTA é verificação **client-side** de assinatura Ed25519
sobre o hash do artefato (`RELEASE_PUBLIC_KEY_HEX` embutida no build); sem
assinatura válida, nunca instala. O node é re-spawnado no boot (`resume()`).
Reason: Nunca enviar a chave privada ao servidor; nunca atualizar binário sem
assinatura válida; anti-clonagem via machine binding determinístico e auditável.
Alternativas consideradas: implementar o validador como binário único Rust sem
UI (descartado — o produto pede dashboard desktop); armazenar a chave privada no
keychain do SO (descartado para o seed do node — o processo precisa lê-lo como
arquivo/env; o arquivo 0600 + diretório do usuário é o limite de segurança
prático para operadores de nó).
Status: Accepted

## ADR-015

Data: 2026-08-23
Fase relacionada: Pré-mainnet (reconciliação SPEC v1 ↔ implementação)
Decision: O parâmetro **tempo de bloco é 15 segundos** — a SPEC v1 (60 s)
estava desatualizada e foi corrigida. A SPEC.md passa a refletir exatamente
os valores congelados em `crates/augecoin-core/src/constants.rs`
(fonte autoritativa usada pelo consenso):
- `CT_BLOCK_TIME_SECONDS = 15` (round timeout = block_time + 5 s);
- recompensa **fixa** de 7,25 AUGE/bloco (`CT_BLOCK_REWARD_AUGESAT`),
  linear, sem halving, sem cauda;
- emissão de 3 contas (AUGEIDs) por bloco (`CT_ACCOUNTS_PER_BLOCK = 3`);
- `TOTAL_EMISSION_BLOCKS = 105_120_000` (50 anos × 2.102.400 blocos/ano
  a 15 s);
- hard cap de emissão/supply = **762.120.000 AUGE**
  (105.120.000 × 7,25 AUGE, verificado por teste).
A divergência já havia sido registrada no WHITEPAPER_V2 §1/§15; este ADR
formaliza a reconciliação exigida pelo item "SPEC.md synchronized" do
MAINNET_READINESS.
Reason: A política monetária do código (constantes + proptest) é a fonte
da verdade; documentar 60 s criava contradição pública antes do mainnet.
Os múltiplos de emissão (210_240 = 2.102.400/10) só fecham com blocos de
15 s.
Alternativas consideradas: alterar o código para 60 s (descartado — a
rede chain_id=1 e todos os vetores de teste já operam a 15 s; mudança
quebraria a emissão projetada).
Status: Accepted
