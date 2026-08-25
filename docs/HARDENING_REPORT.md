# AUGECOIN — Relatório de Hardening (External Address + Gossipsub + WASM)

Data: 2026-08-13
Status: **CONCLUÍDO** (com evidências reais, sem mocks)

---

## 1. Resumo Executivo

Foram eliminados os três itens de maior prioridade remanescentes do
MAINNET_BLOCKERS, sem alterar consenso, formato de blocos, serialização,
`chain_id` ou a compatibilidade Rust ↔ WASM ↔ TypeScript.

| Item | Problema | Correção | Impacto |
|---|---|---|---|
| `AUGECOIN_EXTERNAL_ADDRESS` | Peer exchange usava só `identify.observed_addr`; em NAT/VPS/multi-interface o endereço observado pode ser o indesejado | Configuração explícita validada, com prioridade external → observed → local e rejeição de `0.0.0.0`/`::` | Rede anunciável de forma determinística em produção |
| Gossipsub `max_transmit_size` (M-02) | Sem limite de tamanho de mensagem → risco de DoS | `ConfigBuilder` com `max_transmit_size=1 MiB`, `message_id_fn`, `duplicate_cache_time`, `heartbeat_interval`, `history_length`/`history_gossip` e mesh params; rejeição explícita de mensagens oversized | DoS por mensagem gigante bloqueado; malha estável mantida |
| Glue WASM ausente (`augecoin_crypto_bg.js`) | 3 suites TS falhavam por falta do glue | Regeneração completa com `wasm-bindgen` (não manual) + `scripts/build-wasm.sh` + CI | SDK TS volta a passar 100% e mantém paridade com Rust |

Compatibilidade preservada: `cargo test` (workspace) e `npm test` (SDK) passam
integralmente; os vetores cruzados Rust↔WASM↔TS permanecem idênticos.

---

## 2. Arquivos alterados

| Caminho | Função alterada | Motivo | Impacto |
|---|---|---|---|
| `crates/augecoin-network/src/lib.rs` | `gossipsub_config()` (nova), `parse_external_address()` (nova), `MAX_TRANSMIT_SIZE` (novo), `publish_to_topic()` (novo) + `publish_*` delegando a ele | Centralizar a config do gossipsub e validar o endereço externo; rejeitar oversized antes de publicar | Hardening de rede; API `publish_*` inalterada para os chamadores |
| `crates/augecoin-network/src/lib.rs` | `AugecoinBehaviour::new` | Usar `gossipsub_config()` e logar `max_transmit_size` | Config explícita em vez do default |
| `crates/augecoin-network/src/transport.rs` | `start_consensus_network(..., external_address: Option<Multiaddr>)` | Aceitar endereço externo; registrá-lo via `add_external_address` e logar `external address configured`; prioridade external → observed → local | Anúncio determinístico do endereço |
| `crates/augecoin-node/src/main.rs` | parsing de `AUGECOIN_EXTERNAL_ADDRESS` + passagem a `start_consensus_network` | Ler/validar a variável (falha fatal em valor inválido) | Configuração de produção exposta |
| `packages/wasm-crypto/*` (regenerado) | saída de `wasm-bindgen --target bundler` | Regenerar `augecoin_crypto_bg.js`/`.wasm`/`.d.ts` | SDK TS volta a carregar o glue |
| `packages/wasm-crypto/package.json` | adicionado `types` e `files` completos | Expor tipos e todos os artefatos | Instalação via `npm ci` completa |
| `packages/wasm-crypto/.gitignore` | `*` → whitelist dos artefatos essenciais | Permitir commit do pacote regenerado (CI depende dele) | CI typescript reproduzível |
| `packages/sdk-ts/tests/hdkeys_parity.test.ts` | `require(esm)` → `import * as wasmModule` | `require()` de ESM+WASM não é suportado no Node | Teste de paridade passa |
| `scripts/build-wasm.sh` (novo) | fluxo cargo build → wasm-bindgen → pkg | Reproduzir a regeneração | Regeração automatizada |
| `.github/workflows/ci.yml` | job typescript: `wasm-pack` morto → `build-wasm.sh` | Garantir regeneração em CI | CI alinhado ao fluxo |
| `README.md`, `.env.example`, `docs/OPERATIONS.md`, `docs/MAINNET_BLOCKERS.md` | documentação de `AUGECOIN_EXTERNAL_ADDRESS` e gossipsub | Atualizar docs conforme exigido | Operadores sabem configurar |

---

## 3. Evidências reais

### External Address
```
[network] external address configured: /ip4/127.0.0.1/tcp/9200
[network] listening on /ip4/127.0.0.1/tcp/9200
[network] peer connected: 12D3KooWM7Czq8DYn8tTxj6LAgSUG5xEgN2F9MLG6WP5EFbC1PSi (unique peers: 1)
[network] peer identified: 12D3KooWM7Czq8DYn8tTxj6LAgSUG5xEgN2F9MLG6WP5EFbC1PSi protocol=/augecoin/1.0.0 agent=rust-libp2p/0.47.0
```
(segundo nó, IP diferente `127.0.0.2`):
```
[network] external address configured: /ip4/127.0.0.2/tcp/9201
[network] dialing bootnode /ip4/127.0.0.1/tcp/9200
[network] peer connected: 12D3KooWCXHmWLCT88znthQPc6KfNSQ4rww9JzLuQzJaG1ApsVRd (unique peers: 1)
```

### Gossipsub
```
[network] gossipsub max_transmit_size=1048576
[network] oversized message rejected: 1048577 bytes (max 1048576)
```

### Full mesh + consenso (4 validadores, sem regressão)
```
[network] peer connected: ... (unique peers: 3)   # em cada nó
[block 2] COMMITTED  reward=29.00000000 AUGE  ops=0  quorum=3/4
[block 3] COMMITTED  reward=29.00000000 AUGE  ops=0  quorum=3/4
```

### WASM
```
wasm-bindgen 0.2.127
[wasm] running wasm-bindgen -> packages/wasm-crypto
augecoin_crypto.js  augecoin_crypto_bg.js  augecoin_crypto_bg.wasm  ...
Test Files  5 passed (5)
     Tests  46 passed (46)
```

### Paridade Rust↔WASM↔TS
```
cargo test -p augecoin-node --test sdk_parity  -> 4 passed
npm test  (transaction/hdkeys_parity/address)  -> vetores idênticos aos do Rust
```

---

## 4. Testes

| Teste | Resultado |
|---|---|
| External Address (`/ip4/127.0.0.1/tcp/9200` e `127.0.0.2`) | PASS |
| Kademlia / peer exchange | PASS |
| Full Mesh (N−1 peers) | PASS |
| Gossipsub small/medium/at-limit | PASS |
| Oversized Reject (`max+1` bytes rejeitado) | PASS |
| Consenso não afetado (blocos commitados) | PASS |
| WASM (`npm test`, 46 testes) | PASS |
| SDK (`npm run build`) | PASS |
| Paridade (`sdk_parity.rs` + vetores TS) | PASS |
| `cargo test` workspace | PASS (0 falhas) |
| `cargo fmt --all --check` | PASS |

---

## 5. Riscos restantes

1. **`max_transmit_size` de 1 MiB vs. blocos muito grandes**: o limite comporta
   os blocos reais da testnet (proposal/block response com poucas operações),
   mas um bloco com milhares de operações (mempool `max_operations=10_000`)
   poderia, em tese, exceder 1 MiB e ser rejeitado pelo gossipsub antes do
   `MAX_CONSENSUS_MSG_SIZE` de 16 MiB. Recomenda-se monitorar o tamanho máximo
   de bloco e, se necessário, aumentar o limite (constante `MAX_TRANSMIT_SIZE`
   em `crates/augecoin-network/src/lib.rs`). Não afeta a testnet atual.

2. **`ExperimentalWarning: Importing WebAssembly module instances`**: o Node 22
   ainda marca a importação ESM de `.wasm` como experimental. É um aviso, não
   erro; os 46 testes passam. Alternativa de longo prazo: `--target nodejs`
   para o runtime Node (exigiria separar a build do browser).

3. **Warnings de clippy pré-existentes** (`execution.rs` "too many arguments",
   `genesis.rs` "unnecessary closure", `transport.rs` "collapsible if-let"):
   não foram introduzidos por este trabalho; com CI em `-D warnings`, devem ser
   limpos em tarefa dedicada (fora do escopo aqui).

4. **`crates/augecoin-crypto/pkg/` (output antigo `--target nodejs`)**: diretório
   legado não usado pelo SDK (que usa `packages/wasm-crypto`); deixado intacto
   para não remover funcionalidade, mas pode ser removido em limpeza futura.

---

## Conclusão

Os três itens de hardening foram implementados, testados e documentados com
evidências reais (logs de execução, não mocks). A rede continua formando malha
completa, o consenso continua commitando blocos via LibP2P, o SDK TypeScript
volta a passar integralmente e a paridade Rust ↔ WASM ↔ TypeScript permanece
verificada por vetores cruzados.

**Classificação: concluído — Testnet Ready preservada.**
