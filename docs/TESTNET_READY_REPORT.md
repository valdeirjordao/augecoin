# AUGECOIN — Relatório Final: LibP2P Full Mesh + ValidatorSet Dinâmico + Testnet Ready

Data: 2026-08-13
Status: **TESTNET READY** (com evidências reais, sem mocks)

---

## 1. Resumo Executivo

### Blockers resolvidos

| Blocker | Causa raiz | Correção |
|---|---|---|
| Topologia em estrela (V1/V2/V3 só conectados ao V0) | Validadores escutam em `0.0.0.0`; o `identify` reporta esse endereço não-rotaável e o Kademlia tentava discar `0.0.0.0` (falha sempre) | Tradução do IP não-especificado para o IP observado na conexão + peer-exchange via gossipsub (`augecoin/peers`) |
| Consenso com restos de `TcpStream` | Transporte do consenso acoplado ao `ConsensusNetworkHandle` concreto, sem unicast | Trait `ConsensusTransport` (broadcast/unicast/recv) + unicast por tópico por-peer + `send_to` por `PeerId` |
| ValidatorSet não atualizava em memória | `from_id >= validator_count` descartava round-change de validador novo; `round_start_time` não era resetado no commit; catch-up entrava em deadlock quando o novo validador era líder | Remoção do limite fixo, reset de `round_start_time`, saída do catch-up ciente do líder, round-jump |
| Operações não chegavam ao líder | `publish_operation` nunca era chamado; mempool de cada nó ficava isolado | Gossip de operações (RPC → OPS_TOPIC → mempool de todos os nós) |
| `ValidatorAdmin`/`CreateAccount` re-incluídos em todo bloco | `remove_committed` só removia ops com `senders` | Remoção por igualdade de operação (sem senders) |
| Divergência de endereço Rust↔TS | A lib JS `bech32` impõe limite de 90 chars (BIP-173) e rejeitava o endereço de 64 bytes | Limite 1023 (BIP-350) no SDK TS + vetor de teste cruzado |

### Arquitetura final

```
Wallet/SDK/CLI
      │ JSON-RPC 2.0 (TLS)
      ▼
RPC ──> Mempool ──(gossip: augecoin/ops)──> Mempool de todos os validadores
                                                │
ConsensusEngine ◄── ConsensusTransport (trait) ──► LibP2P
      │                ├─ broadcast (gossipsub: augecoin/consensus)
      │                └─ unicast  (gossipsub: augecoin/unicast/<peer>)
      ▼
Proposal → Prepare → Commit  (tudo via LibP2P)
      ▼
execute_block ──> State Transition ──> Storage (ValidatorSet persistido)
      │
      ▼
sync_validator_set_from_storage ──> Novo ValidatorSet em memória (quorum/líder recalculados)
```

Descoberta/malha: Kademlia (bootstrap) + Identify (endereços traduzidos) + peer-exchange (`augecoin/peers`) + Ping. Noise XX + Yamux. Limite de 1 conexão por peer.

### Nível de prontidão

**TESTNET READY** — as quatro validações obrigatórias foram cumpridas com logs reais:

1. 4 validadores formam malha completa (cada nó com N−1 peers).
2. Consenso trafega exclusivamente por LibP2P (Proposal/Prepare/Commit/RoundChange/BlockRequest/Response/Status).
3. ValidatorSet atualizado dinamicamente sem reinício (add/remove/deactivate/activate → quorum e líder recalculados).
4. Carteira criada do zero recebe AUGE do faucet, envia transação via RPC, entra no mempool, é incluída em bloco, confirmada pelo consenso e o estado fica idêntico em todos os validadores.

---

## 2. Arquivos alterados

### `crates/augecoin-network/src/lib.rs`
- **`AugecoinBehaviour::new`** — subscreve `PEERS_TOPIC` e o tópico unicast próprio; limite 1 conexão/peer.
  - Motivo: suportar full mesh + unicast.
- **`publish_peers`, `peers_topic_hash`, `ops_topic_hash`, `publish_unicast`, `unicast_topic`** — novos métodos.
- **`encode_peer_announcement`/`decode_peer_announcement`** — formato length-prefixed para anúncio de peers.
- **`peer_id_from_ed25519`** — deriva `PeerId` da chave Ed25519 do validador (mapeia id inteiro → PeerId).
- **`pub use libp2p::PeerId`** — re-export.

### `crates/augecoin-network/src/transport.rs`
- **`ConsensusTransport` (trait)** — abstração única `broadcast/send_to/recv/peer_count/…`; `ConsensusEngine` não conhece socket/porta/`TcpStream`.
- **`OutboundConsensus::Unicast`** + `send_to(peer, data)` — unicast por tópico por-peer.
- **`translate_addr`/`extract_ip`/`ip_tcp_addr`** — traduz `0.0.0.0`/`::` para o IP observado (causa raiz da estrela).
- **`peer_observed_ip`/`self_external_addr`** — aprende IP observado na conexão e endereço externo próprio via `identify.observed_addr`.
- **Peer-exchange** — `peers_tick` publica próprios + conhecidos; inbound diala peers desconhecidos (dedup).
- **`dead_peer_since`** — remove peer morto da tabela Kademlia após 60s.
- **Roteamento por tópico** — OPS → mempool, PEERS → exchange, unicast → consenso, CONSENSUS/BLOCK → consenso.
- **`start_consensus_network`** — ganhou `op_broadcast_rx` (gossip de operações) e `op_inbound_rx` no handle.

### `crates/augecoin-node/src/consensus.rs`
- **`network: Option<Arc<dyn ConsensusTransport>>`** — engine desacoplado do transporte concreto.
- **`attach_network(Arc<dyn ConsensusTransport>)`** — thread de leitura usa `transport.recv()`.
- **`send_to(validator_id, …)`** — resolve `PeerId` via `peer_id_from_ed25519` e faz unicast (fallback broadcast).
- **`create_dev_validator_key(id)`** — chave determinística para validador fora do conjunto genesis (5º validador).
- **`try_recv_operation`** — drena operações gossipadas.

### `crates/augecoin-node/src/main.rs`
- **Gossip de operações** — cria canal `op_broadcast_tx/rx`; drena `op_inbound` para o mempool com dedup por hash (`gossiped_seen`).
- **`start_rpc_server`** — recebe `op_broadcast_tx`; `AppState.op_broadcaster`.
- **Remoção do `from_id >= validator_count`** no handler de `RoundChange` (aceita validadores dinâmicos).
- **`round_start_time = now`** nos dois caminhos de commit (corrige timeout espúrio de round).
- **Round-jump** no handler de `BlockProposal` (adota o round implícito no líder da proposta).
- **Catch-up ciente do líder** — sai do catch-up quando `height+1 == sync_target` e é o líder (evita deadlock).
- **`sync_validator_set_from_storage`** — logs explícitos `validator added`/`validator removed`.

### `crates/augecoin-rpc/src/endpoints.rs`
- **`AppState.op_broadcaster` + `broadcast_op`** — gossip de operações admitidas.
- **`handle_send_operation`/`handle_create_account`/`handle_faucet`/4 handlers admin** — chamam `broadcast_op` após admissão bem-sucedida.

### `crates/augecoin-rpc/src/lib.rs`
- **`make_test_state`** — novo campo `op_broadcaster: None`.

### `crates/augecoin-core/src/mempool.rs`
- **`remove_committed`** — remove operações sem `senders` (ValidatorAdmin/CreateAccount) por igualdade exata, evitando re-inclusão em blocos subsequentes.

### `crates/augecoin-node/tests/sdk_parity.rs`
- **`address_matches_cross_language_vector`** — trava o endereço Bech32m cruzado (Rust↔TS).

### `packages/sdk-ts/src/address.ts`
- **`getAddress`/`validateAddress`** — usam limite Bech32m de 1023 chars (BIP-350), alinhado ao Rust.

### `packages/sdk-ts/tests/address.test.ts` (novo)
- Vetor de endereço cruzado (4 testes).

### `crates/augecoin-node/examples/testnet_helper.rs` (novo)
- Utilitário de testnet: gera chaves/endereços, assina transferências e ops de admin (ValidatorAdmin) com as chaves dev determinísticas.

---

## 3. Evidências (logs reais)

### Rede — full mesh
```
[network] peer connected: 12D3KooWM7Czq8DYn8tTxj6LAgSUG5xEgN2F9MLG6WP5EFbC1PSi (unique peers: 1)
[network] peer connected: 12D3KooWD5cYrr7WgUmjb9d7ev1fQfn1UTdNd7AVtAPKP1Hh4XKo (unique peers: 2)
[network] peer connected: 12D3KooWATzFVLaFeU6h5FgUH6vCsSuPCfipix3nZwxLj853TXA5 (unique peers: 3)
[network] dialing exchanged peer 12D3KooWA2iD4XPL24X1VCyF4KCeBsAcni1ZtfCVnQtEe4odyTYH at /ip4/127.0.0.1/tcp/9204/...
[network] peer identified: ... protocol=/augecoin/1.0.0 agent=rust-libp2p/0.47.0
```
`nodestatus` em todos os 5 nós: `peers_connected = 4` (malha de 5 nós).

### Consenso via LibP2P
```
[consensus] proposing block height=4 (leader=v0, round 0)
[consensus] collected signature from v2 (2/3)
[consensus] collected signature from v3 (3/3)
[block 8] COMMITTED  reward=29.00000000 AUGE  ops=0  quorum=3/4
```

### Faucet + transação
```
[faucet] ACTIVE account=1 amount=100 AUGE ...
[faucet] client=ip:127.0.0.1 address=auge1... tx_hash=0213833d... status=accepted
```
RPC: `sendOperation accepted=True op_hash=3ea44f31...`
```
A after faucet: account=150 balance=10000000000 n_operation=0
transfer result: A balance=9000000000 (was 10000000000), B balance=1000000000
```

### ValidatorSet dinâmico
```
[validator] validator added: ValidatorSet updated 4 -> 5 active validators
[validator] quorum recalculated: 4/5
[validator] validator removed: ValidatorSet updated 5 -> 4 active validators
[validator] quorum recalculated: 3/4
```
5º validador participa imediatamente:
```
[consensus] proposing block height=19 (leader=v4, round 0)
[block 19] COMMITTED  reward=29.00000000 AUGE  ops=0  quorum=5/5
[consensus] collected signature from v4 (2/4)   ← nos demais validadores
```

### View-change (líder derrubado)
```
[consensus] round timeout height=51 round=0; broadcasting round-change to round 1
[consensus] VIEW-CHANGE to round 1 (quorum 4/4) voters=Some({2, 3, 4, 0})
[block 51] EXECUTED  reward=29.00000000 AUGE  ops=0  quorum=4/5
```

### Catch-up / reconexão
```
[sync] applied block 53 (5 behind)
...
[sync] applied block 59 (0 behind)
[sync] catch-up complete at height 59
```

### Estado idêntico em todos os nós (final)
```
port=9005 h=63 peers=4 hash=15ffc0f6f215
port=9006 h=63 peers=4 hash=15ffc0f6f215
port=9007 h=63 peers=4 hash=15ffc0f6f215
port=9008 h=63 peers=4 hash=15ffc0f6f215
port=9009 h=63 peers=4 hash=15ffc0f6f215
```

---

## 4. Testes

| Teste | Resultado |
|---|---|
| Full Mesh (malha completa, N−1 peers) | PASS |
| LibP2P (Proposal/Prepare/Commit via gossipsub) | PASS |
| Faucet (claim aceito + tx_hash + saldo) | PASS |
| Transação (accepted → included → committed → saldo) | PASS |
| Catch-up (validator atrasado sincroniza) | PASS |
| View-change (líder derrubado → novo líder → novo bloco) | PASS |
| ValidatorAdmin (add/remove + quorum recalculado) | PASS |
| Reconexão (validator religado → catch-up → consenso) | PASS |

Suíte automatizada: `cargo test` — **0 falhas** (todos os crates). `cargo fmt --check` — OK.
SDK TS: `vitest` — `address.test.ts` (4) + `utils.test.ts` (20) passam. Os 3 testes TS restantes dependem do binário WASM (`augecoin_crypto_bg.js`) ausente no `node_modules` — falha pré-existente de ambiente, não relacionada a este trabalho.

---

## 5. Riscos restantes

1. **Endereço observado não-loopback no peer-exchange**: em hosts com IP público, o `identify.observed_addr` pode propagar o IP público em vez do `127.0.0.1`. Em produção, configurar um endereço externo estável (variável `AUGECOIN_EXTERNAL_ADDRESS`, hoje não exposta). A malha local continua formando via `127.0.0.1` (tradução do Kademlia).

2. **Round-desync residual sob adição de validador concorrente com liderança**: mitigado por round-jump; o sistema converge, mas uma adição em rajada (add + remove no mesmo tick) pode exigir 1–2 view-changes extras. Recomenda-se `activation_height` planejado para produção.

3. **Warnings de clippy pré-existentes** (`execution.rs:455` "too many arguments", `genesis.rs:99` "unnecessary closure", `transport.rs:389` "collapsible if-let"): o CI usa `-D warnings`. São anteriores a este trabalho e não afetam funcionalidade; recomenda-se limpá-los em tarefa dedicada.

4. **Glue WASM ausente** no pacote `@augecoin/wasm-crypto` (`augecoin_crypto_bg.js` não está no `node_modules`), impedindo 3 suites de teste TS. Necessário regenerar com `wasm-bindgen`.

5. **Limites de tamanho do gossipsub**: `max_transmit_size` ainda não configurado (item M-02 do MAINNET_BLOCKERS).

---

## Conclusão

Todos os cinco fluxos obrigatórios (wallet/faucet/transação, novo validador, desligar/religar validador, view-change) foram executados sem mocks e com evidências reais de logs. A rede forma malha completa, o consenso trafega exclusivamente por LibP2P e o ValidatorSet é atualizado dinamicamente sem reinício, com estado idêntico em todos os nós.

**Classificação: TESTNET READY.**
