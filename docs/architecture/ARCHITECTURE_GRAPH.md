# AUGECOIN — Architecture Graph

> Gerado a partir do grafo do Graphify (`graphify-out/graph.json`).
> Data da indexação: 2026-08-17. Modo: `--code-only`.
> Fonte de verdade: `graphify-out/GRAPH_REPORT.md` (3167 nós · 6966 arestas · 167 comunidades).

## 1. Crates principais (workspace)

| Crate | Papel | Depende de |
|---|---|---|
| `augecoin-crypto` | Primitivas: `blake3_512`, Ed25519/HybridSignature, Bech32, HD wallet (bip39/HKDF), hash | — (fundação) |
| `augecoin-core` | Modelo de domínio: `Account`, `OperationBlock`, `Operation`, `SafeBox`, `Mempool`, emissão, constantes | augecoin-crypto |
| `augecoin-storage` | RocksDB: `Storage`, column families, `SafeboxCache` (residente), checkpoints, state root | augecoin-core |
| `augecoin-consensus` | PoA: `ValidatorSet`, `ValidatorInfo`, `RoundState`, `quorum_threshold`, `EquivocationProof` | augecoin-core |
| `augecoin-network` | libp2p: `AugecoinBehaviour`, gossipsub, Kademlia, `ConsensusTransport`, sync | augecoin-consensus |
| `augecoin-rpc` | HTTP JSON-RPC: `dispatch_method`, handlers, auth (`ApiKeyStore`, `RateLimiter`) | augecoin-core/storage |
| `augecoin-cli` | Binário CLI (consulta/operações) | augecoin-core/crypto |
| `augecoin-node` | Binário do validador: `main`, `ConsensusEngine`, `execution`, `genesis`, `alerts`, `metrics` | todos acima |

Caminho crítico das dependências (Community 71 no grafo):
```
crypto ← core ← {storage, consensus}
              consensus ← network
              core ← rpc, cli
              node ← core, crypto, storage, consensus, network, rpc
```

## 2. Fluxo de inicialização do node (`crates/augecoin-node/src/main.rs`)

1. `parse_cli_args` / `parse_external_address` — argumentos e endereço externo.
2. `Storage::open(data_dir)` — abre RocksDB com tuning (WAL 1GiB, rotação LOG).
3. `set_safebox_snapshot_interval` — 100 (dev) / 1000 (mainnet).
4. `genesis::initialize` — contas genésis (treasury/faucet/validator/admin).
5. Carregar `ValidatorSet` do storage (ou gravar genésis).
6. `load_safe_box_hash` + `load_latest_block_hash` — restaura altura/estado.
7. `start_rpc_server`, `start_wallet_server`, `MetricsServer::start`.
8. `build_validator_swarm` + `start_consensus_network` (libp2p).
9. `ConsensusEngine::new` + `attach_network`.
10. Loop de consenso: descobre peers, propõe/valida/confirma blocos.

## 3. Fluxo RPC (`crates/augecoin-rpc/src/lib.rs`)

- Rotas HTTP: `POST /` (JSON-RPC 2.0), `POST /createaccount`, `POST /faucet`, `GET /health`.
- `jsonrpc_handler` → `check_auth` (x-api-key) → `dispatch_method`.
- Métodos: `getaccount`, `getblock`, `getblockoperations`, `getoperations`,
  `getpendings`, `sendoperation`, `findaccounts`, `getaccountcount`,
  `getblockcount`, `getnodestatus`, `getvalidatorset`, `validator_add/remove/activate/deactivate`.
- Sensíveis (`createaccount`, `faucet`): rate limiter dedicado (`sensitive_rate_limiter`).
- Métodos admin exigem `AuthLevel::Admin` (api key válida).

## 4. Consenso PoA (`crates/augecoin-consensus` + `node/src/consensus.rs`)

- Líder rotativo por `leader_for_height` (round-robin sobre `active_validators`).
- `RoundState`: propõe bloco, coleta assinaturas, `try_commit` com `quorum_threshold`
  (2/3), timeout → `view_change`.
- `EquivocationProof` / `detect_equivocation` detectam dupla proposição.
- `ConsensusEngine` (node) orquestra: `build_block`, `sign_block`, `verify_block_sig`,
  `broadcast_to_others`, `try_recv_operation`.

## 5. SafeBox (`crates/augecoin-core/src/safe_box.rs`)

- `SafeBox { header, accounts: BTreeMap<u64,Account>, name_index: BTreeMap<String,u64> }`.
- `compute_safe_box_hash` = raiz Merkle sobre `accounts.hash()` + `name_index` (blake3_512).
- `generate_merkle_proof` / `verify_merkle_proof` — provas de inclusão.
- O hash é **consenso** (gravado em `initial_safe_box_hash` no header do bloco).
- A serialização é **cache**: `SafeboxCache` em `augecoin-storage` mantém o SafeBox
  residente em memória; persiste snapshot só a cada N blocos (ver SAFEBOX_FLOW.md).
- **Emissão de AUGEIDs (consenso):** exatamente 3 por bloco, todos `Reserved`
  e do líder. Numeração `bloco × 3 + offset`. `CreateAccount` não cria números —
  apenas ativa um `Reserved` → `Owned` (assinado pelo admin/líder).
- Estados: `Reserved → Owned → Normal`; `ForSale` (marketplace); `GiftPending` (doação).

## 6. Storage RocksDB (`crates/augecoin-storage/src/lib.rs`)

- Column families: `accounts`, `blocks`, `safebox`, `op_index`, `validator_set`,
  `equivocation_proofs`, `faucet_claims`.
- `SafeboxCache` (residente em memória) + índices O(1): `pubkey_index`,
  `sale_index`, `gift_index` (e `name_index` no SafeBox).
- `commit_block_atomic` — consolida contas + bloco + altura + validator_set +
  snapshot num único `WriteBatch`; atualiza índices incrementalmente.
- Tuning: WAL 1GiB, LOG 64MiB/10 arquivos, memtable 64MiB, SST 64MiB, LZ4.

## 7. libp2p (`crates/augecoin-network`)

- `AugecoinBehaviour`: gossipsub (consenso) + Kademlia (descoberta).
- `build_validator_swarm`, `build_observer_swarm`.
- `ConsensusTransport`: serialização/deserialização de `ConsensusEvent`.
- `Bootnodes`, `parse_external_address`, `identity_from_ed25519_seed`.
- `SyncManager` (`sync.rs`): sincronização com buffer fora de ordem.

## 8. Wallets

- `apps/wallet-desktop` (Tauri + React/Vite) — binário `augecoin-wallet-desktop`.
- `apps/wallet-web` (Vite/React) + `apps/wallet-web/server` (Node/Express + SQLite).
- `apps/wallet-mobile` (React Native/Expo).
- `packages/sdk-ts` — SDK TypeScript cliente.
- `packages/wasm-crypto` — `augecoin-crypto` compilado para WASM (wasm-bindgen).
- `wallet/` — utilitários de carteira/keystore.

### 8.1 Wallet Web — três camadas estritamente separadas

A Wallet Web implementa a separação definitiva entre **Conta da Plataforma** e
**Carteira Blockchain**, com uma terceira camada lógica de ponte:

| Camada | Onde vive | Responsabilidades | NÃO faz |
|---|---|---|---|
| **1. Conta da Plataforma** | `wallet-web/server` (`platform_users`, `user_preferences`) + SPA | login, cadastro, perfil, sessão (JWT/refresh), preferências | não tem saldo; não cria AUGEID |
| **2. Carteira Blockchain** | SPA ↔ node RPC (`getaccount`, `buyaccount`, …) | saldo, nome, transferências, compra/venda/doação | só existe quando há AUGEID |
| **3. Wallet Registry** | `wallet-web/server` (`linked_wallets`) | vincula `user_id ↔ account_number` | não guarda saldo, estado ou chave privada |

Fluxo:

```
Visitante → Cadastro → Conta da Plataforma → (sem AUGEID)
  → Marketplace ou Doação → recebe AUGEID → Carteira Blockchain ativada
  → envia/recebe AUGE
```

- `buyaccount`/`acceptgift` retornam sucesso ⇒ o frontend cria automaticamente o
  vínculo em `linked_wallets` (vinculação automática).
- `useLinkedWallets` lê `linked_wallets` e enriquece cada AUGEID com `getaccount`;
  a autoridade sobre propriedade/saldo/transferências permanece no SafeBox.
- Frontend consome **apenas as RPCs oficiais** (`services/rpc.ts`); nenhuma regra
  de consenso é duplicada na UI.

## 9. God Nodes (top abstrações — fonte: GRAPH_REPORT.md)

| # | Nó | Grau |
|---|---|---|
| 1 | `Account` | 57 |
| 2 | `AppState` | 46 |
| 3 | `esc()` | 39 |
| 4 | `Storage` | 39 |
| 5 | `OperationBlock` | 38 |
| 6 | `Mempool` | 37 |
| 7 | `ConsensusEngine` | 32 |
| 8 | `StorageError` | 32 |
| 9 | `HybridSignature` | 29 |
| 10 | `ValidatorSet` | 29 |

## 10. Métricas do grafo

- **Nós:** 3167 · **Arestas:** 6966 · **Comunidades:** 167.
- **Extração:** 99% EXTRACTED · 1% INFERRED (42 arestas, confiança média 0.78).
- **Nós isolados (≤1 conexão):** 284 (majoritariamente UI/wallet/config).
- **Ciclo:** 1 (self-loop benigno em `quorum.rs`).
- **Arquivos mais conectados:** `account.rs`, `endpoints.rs`, `safe_box.rs`,
  `storage/lib.rs`, `consensus.rs`.
