# AUGECOIN — Consensus Flow (PoA)

> Fonte: `crates/augecoin-consensus/*`, `crates/augecoin-node/src/consensus.rs`,
> `crates/augecoin-node/src/main.rs`. Nós do grafo: `ConsensusEngine`,
> `RoundState`, `ValidatorSet`, `quorum_threshold`, `EquivocationProof`.

## Modelo

- **PoA (Proof of Authority)** com líder rotativo round-robin.
- `ValidatorSet.active_validators()` ordena os validadores ativos.
- Líder da altura H: `leader_for(H, round, set)` = `active[(H+1+round) % len]`.
- Quórum: `quorum_threshold(n)` = 2/3 (ver `quorum.rs`).

## Fases de um round (`round.rs`)

1. **Propose** — líder constrói bloco (`ConsensusEngine::build_block`) com
   `initial_safe_box_hash` (hash do SafeBox anterior) e assina.
2. **Collect** — `add_signature` acumula assinaturas dos demais validadores.
3. **Commit** — `try_commit` quando assinaturas ≥ quórum.
4. **Timeout** — `check_timeout` dispara `view_change` para o próximo líder.

## Eventos de rede (`ConsensusEvent`, `ConsensusTransport`)

- `BlockProposal`, `BlockResponse`, `BlockRequest`, `StatusRequest`,
  `RoundChange`, `CommitNotification`, operações gossiped.
- Transporte: libp2p gossipsub (mesh de consenso) + Kademlia (descoberta).

## Verificação de bloco (`execution.rs::verify_block_quorum`)

1. Rejeita timestamp futuro além de `CT_MAX_FUTURE_BLOCK_TIMESTAMP_SECONDS`.
2. Verifica assinatura do líder (Ed25519) sobre `block.hash()`.
3. Verifica assinaturas de quórum dos validadores ativos.
4. Valida `operations_hash` == Merkle das operações do bloco.

## Fluxo de finalização (visão `main.rs`)

```
Idle → (se é líder) build_block → AwaitingSignatures
     → (threshold atingido) execute_block → Executed
     → grava bloco + altura + SafeBox incremental + prune
     → sync_validator_set_from_storage → próximo round
```

- Catch-up: `apply_catchup_block` (blocos recebidos fora de ordem são aplicados
  após verificação de quórum).
- Equivocação: `detect_equivocation` gera `EquivocationProof` persistido em
  `equivocation_proofs`.

## Determinismo

- `execute_block` é determinístico: mesmo bloco + mesmo estado → mesmo
  `safe_box_hash` (validado por `determinism_test.rs`).
- `compute_safe_box_hash` (Merkle) não mudou; a refatoração do SafeBox afetou
  apenas a persistência local, nunca o hash de consenso.
