# AUGECOIN — OPEN_QUESTIONS.md
### Perguntas de produto sem resposta. O agente NUNCA assume uma resposta aqui — só pergunta e espera. Quando respondida, mover para DECISIONS.md como ADR e apagar daqui.

## Pergunta 1 — ~~Destino da conta nova criada a cada bloco~~ → respondida, movida abaixo

## Pergunta 2 — Teto de crescimento do conjunto de validadores

O conjunto de validadores pode crescer além de 4 no futuro (ex: 7, 9)
mantendo quórum 2/3+1? Se sim, qual o teto máximo?

**Bloqueia:** FASE-04A (design de `ValidatorSet` precisa saber se o
tamanho é fixo ou dinâmico com teto).

## Pergunta 3 — Custódia da chave administradora

A chave do administrador (poder de add/remove/activate/deactivate
validador) será:

- (a) single-key, ou
- (b) multisig M-de-N (ex: 3 de 5)?

**Bloqueia:** FASE-04A (o formato de assinatura de `ValidatorAdmin`
muda dependendo da resposta) e FASE-10A (comandos da CLI).

## Pergunta 4 — Nome/atribuição de conta

Nome de conta (ex: `joao.auge`) é uma feature do MVP ou fica para uma
fase posterior?

**Bloqueia:** FASE-02A (se for MVP, o campo `name` em `AugeAccount`
precisa de validação/unicidade desde já; se não, fica reservado sem
lógica).

---

## Perguntas já respondidas (histórico)

### Pergunta 2 — Teto de crescimento do conjunto de validadores
Resposta: Pode crescer além de 4, sem limite de teto. Data: 2026-08-09.

### Pergunta 3 — Custódia da chave administradora
Resposta: Single-key. Data: 2026-08-09.

### Pergunta 4 — Nome/atribuição de conta
Resposta: Reservado sem lógica (campo `name: Option<String>` armazena mas sem validação/unicidade). Data: 2026-08-09.

### Pergunta 1 — Destino da conta nova criada a cada bloco
Resposta: Opção (a) — auto-atribuída ao validador líder da rodada. A nova Conta AUGE #N criada a cada bloco pertence automaticamente ao validador que liderou aquela rodada, dentro de `execute_block()`. Data: 2026-08-09.

---
