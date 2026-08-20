# AUGECOIN — Kit de Execução para OpenCode 1.18 + DeepSeek V4 Pro

## Por que este kit existe

O prompt mestre original (14 fases) funciona conceitualmente, mas colado
inteiro — ou mesmo uma fase inteira de uma vez — ele sobrecarrega o agente:
o DeepSeek V4 Pro tem janela de 1.000.000 tokens, mas janela grande **não
é a mesma coisa** que precisão grande. Em tarefas de geração de código,
quanto mais contexto irrelevante o modelo carrega, maior a chance de
"lost in the middle": ele mistura decisões antigas, reintroduz padrões
que você já rejeitou, ou modifica arquivos fora do escopo.

A solução não é reduzir a especificação — é **separar o que é permanente
do que é da tarefa do momento**, e nunca colar as duas coisas juntas em
excesso.

## Os 6 arquivos do kit

| Arquivo | O que é | Quando muda |
|---|---|---|
| `PROJECT_RULES.md` | A "constituição" do agente — regras absolutas, fluxo de trabalho, o que ele nunca pode fazer | Praticamente nunca |
| `SPEC.md` | A especificação técnica congelada do protocolo AUGECOIN | Só via decisão formal registrada em `DECISIONS.md` |
| `DECISIONS.md` | Log de ADRs (Architecture Decision Records) — toda escolha de design vira uma entrada aqui | Cresce a cada decisão tomada |
| `OPEN_QUESTIONS.md` | Perguntas de produto ainda sem resposta — o agente é proibido de assumir, só de perguntar | Encolhe conforme você responde |
| `PROGRESS.md` | Estado atual do projeto: qual microprompt foi concluído, o que passou nos testes | Atualizado ao fim de **cada** microprompt |
| `PROMPTS.md` | A sequência de ~38 microprompts prontos para colar, um de cada vez, em ordem | Fixo — é o roteiro |

## Como usar, na prática

1. Crie o repositório e cole `PROJECT_RULES.md`, `SPEC.md`, `DECISIONS.md`,
   `OPEN_QUESTIONS.md` e `PROGRESS.md` na raiz (pasta `docs/` sugerida).
2. Abra `PROMPTS.md` e copie **apenas o próximo microprompt pendente**
   (veja `PROGRESS.md` para saber qual é) — nunca vários de uma vez.
3. Cole no OpenCode. Cada microprompt já instrui o agente a reler os 4
   documentos de estado antes de começar — você não precisa colar a spec
   inteira de novo a cada vez.
4. Rode os comandos de verificação listados no próprio microprompt.
5. Se passar: peça ao agente para atualizar `PROGRESS.md` (marcar
   concluído) e, se alguma decisão de design foi tomada no processo,
   `DECISIONS.md` (novo ADR). Faça commit nomeando a branch pela fase
   (ex: `phase-01c-hdkeys`).
6. Se não passar: corrija dentro do mesmo microprompt. Não avance de fase
   com testes quebrados — essa é a regra mais importante do processo
   inteiro.

## Por que quebrar em ~38 microprompts em vez de 14 fases

Cada uma das 14 fases originais mistura, em uma tarefa só, várias
decisões de design independentes (ex: a FASE 1 de criptografia junta
assinatura híbrida + hashing + derivação de chave + endereços — quatro
módulos com testes próprios). Pedir tudo isso de uma vez aumenta a chance
do agente:

- Reaproveitar/inventar interface entre módulos que ainda não foi
  decidida por você;
- Deixar de testar um dos quatro módulos porque "já gastou o orçamento
  de atenção" nos outros três;
- Alterar arquivos de um módulo enquanto implementa outro, quebrando o
  princípio de escopo fechado do `PROJECT_RULES.md`.

Cada microprompt em `PROMPTS.md` entrega **um artefato verificável por
vez**, com escopo de arquivos explícito (permitido/proibido) e comandos
de teste próprios. Isso é mais lento em número de mensagens, mas muito
mais confiável — é a mesma lógica usada em revisão de código humana:
PRs pequenos revisam melhor que PRs gigantes.

## Ordem de leitura recomendada para você (não para o agente)

1. `PROJECT_RULES.md` — entenda as regras antes de rodar qualquer coisa.
2. `SPEC.md` — confirme que reflete exatamente o que você quer (ela é a
   fonte da verdade; qualquer erro aqui se propaga para todas as fases).
3. `OPEN_QUESTIONS.md` — responda o que puder agora; o que ficar em
   aberto será perguntado pelo agente na fase correspondente (marcado em
   `PROMPTS.md`).
4. `PROMPTS.md` — comece pelo microprompt `FASE-00A`.

## Public Testnet Accounts

Use `scripts/genesis/public-genesis.toml` with `AUGECOIN_GENESIS_CONFIG`.
The file references private-key files outside the repository; the node stores
only derived public keys. Configure the faucet signer separately with
`AUGECOIN_FAUCET_KEY_HEX` and `AUGECOIN_FAUCET_ACCOUNT`.

The RPC exposes `POST /createaccount` with `{ "public_key_hex": "..." }` and
`POST /faucet` with `{ "address": "auge1..." }`. Faucet claims are persisted
in RocksDB and limited to one claim per address per 24 hours.

## Networking hardening

- **Explicit external address** — set `AUGECOIN_EXTERNAL_ADDRESS` (e.g.
  `/ip4/203.0.113.10/tcp/9200`) to force a stable, dialable multiaddr for NAT /
  VPS / multi-interface hosts. Priority: external address → `identify.observed_addr`
  → local address. `0.0.0.0` / `::` are rejected.
- **Gossipsub limits** — `max_transmit_size=2097152` (2 MiB) plus pinned
  `message_id_fn`, `duplicate_cache_time`, `heartbeat_interval`,
  `history_length`/`history_gossip` and mesh parameters. Oversized messages are
  rejected before transmission.
- **WASM crypto** — `packages/wasm-crypto` is regenerated from
  `crates/augecoin-crypto` via `wasm-bindgen` (`augecoin_crypto_bg.js` glue
  included). See `packages/sdk-ts` (`npm test`) and `crates/augecoin-node/tests/sdk_parity.rs`
  for the cross-language parity vectors.

See `.env.example` for the full configuration reference.
