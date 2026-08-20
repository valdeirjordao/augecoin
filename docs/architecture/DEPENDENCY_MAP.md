# AUGECOIN — Dependency Map

> Derivado de `graphify-out/graph.json`. Referencie `ARCHITECTURE_GRAPH.md` para o grafo completo.

## Grafo de dependências entre crates

```
augecoin-crypto
    ↑
    ├── augecoin-core ──────────────┐
    │      ↑                        │
    │      ├── augecoin-storage ────┤ (Account, OperationBlock, SafeBox)
    │      ├── augecoin-consensus ──┤
    │      │      ↑                 │
    │      │      └── augecoin-network
    │      ├── augecoin-rpc ────────┤
    │      └── augecoin-cli ────────┤
    │                               │
    └── augecoin-node (depende de core, crypto, storage, consensus, network, rpc)
```

## Relações extraídas pelo grafo (amostra de alta confiança)

| Origem | Relação | Destino |
|---|---|---|
| `endpoints.rs` | imports_from | `Account` |
| `execution.rs` | calls | `execute_operation`, `commit_changes`, `verify_operation_signatures` |
| `main.rs` | calls (INFERRED) | `identity_from_ed25519_seed`, `start_consensus_network` |
| `consensus.rs` | references | `OperationBlock`, `Mempool`, `ValidatorSet`, `Storage` |
| `storage/lib.rs` | contains | `SafeboxCache`, `Storage` |
| `safe_box.rs` | references | `Account`, `blake3_512` |
| `wallet-desktop` | depends_on | `augecoin-crypto` |

## Comunidades arquiteturais relevantes (Graphify)

| Community | Conteúdo identificado |
|---|---|
| 0 | Handlers RPC de leitura (`getAccount`, `findAccounts`, `getBlock`…) |
| 1 | Tipos RPC + `AppState`, `NodeStatus` |
| 2 | Assinaturas/cripto (`Ed25519KeyPair`, `dummy_signature`, `block_reward`) |
| 3 | Equivocação + bloco (`EquivocationProof`, `detect_equivocation`) |
| 7 | Storage/tests (`cleanup`, `delete_account`, `prune_blocks`) |
| 8 | libp2p (`AugecoinBehaviour`, `Bootnodes`, `build_validator_swarm`) |
| 11 | Operações (`HybridSignature`, roundtrips de operação) |
| 12 | Node bootstrap (`main`, `leader_for`, `load_safe_box_hash`) |
| 15 | RPC HTTP (`dispatch_method`, handlers, `first_forwarded_ip`) |
| 18 | Consenso transport (`ConsensusEngine`, `ConsensusEvent`) |
| 23 | Checkpoint (`AccountSnapshot`, restauração) |
| 26 | SafeBox (`MerkleProof`, `compute_safe_box_hash`) |
| 47 | SafeBox/Account core (`SafeBox`, `SafeBoxHeader`, `Account`) |
| 71 | Workspace crates (lista de membros Cargo) |
| 77 | `blake3_512` (hash) |

## Acoplamento (bridges de alta centralidade)

- `Account` — entre centrality 0.100, liga comunidades 1,2,7,10,17,18,23,25,26,37,47,62,77.
- `blake3_512()` — 0.072, liga 25,26,3,77,78.
- `derive_address()` — 0.071, liga 78,77.

## Pontos únicos de falha (SPOF) estruturais

1. **`Storage`** — toda persistência passa por um DB RocksDB (arquivo único).
2. **`ConsensusEngine`** — orquestração central do consenso em `node/consensus.rs`.
3. **`AppState`** — estado compartilhado de todos os handlers RPC.
4. **`SafeBox`** — o hash de consenso depende de `compute_safe_box_hash` correto.
5. **`main()`** — o loop de consenso concentra propor/validar/confirmar.

## Ciclos

- Único ciclo reportado: self-loop benigno em `crates/augecoin-consensus/src/quorum.rs`
  (referência reflexiva, sem impacto estrutural).
