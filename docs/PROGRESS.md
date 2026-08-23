# AUGECOIN — PROGRESS.md
### Atualizar ao fim de CADA microprompt. É este arquivo que diz ao agente (e a você) exatamente onde o projeto parou.

Legenda: `[ ]` não iniciado · `[~]` em andamento · `[x]` concluído e
verificado (comandos de verificação passaram) · `[!]` bloqueado (ver
motivo na coluna)

| ID | Microprompt | Status | Branch | Motivo se bloqueado |
|---|---|---|---|---|
| FASE-00A | Monorepo + crates vazios | [x] | phase-00a-workspace-setup | |
| FASE-00B | CI (build/test/clippy/fmt) | [x] | phase-00b-ci | |
| FASE-01A | Assinatura híbrida (Ed25519+Dilithium) | [x] | phase-01a-hybrid-signature | |
| FASE-01B | Hashing BLAKE3-512 XOF | [x] | phase-01b-hashing | |
| FASE-01C | hdkeys (derivação determinística) | [x] | phase-01c-hdkeys | |
| FASE-01D | Endereços Bech32m | [x] | phase-01d-addresses | |
| FASE-02A | AugeAccount | [x] | phase-02a-account | |
| FASE-02B | Transaction (tipos de operação) | [x] | phase-02b-transaction | |
| FASE-02C | Block/BlockHeader + serialização canônica | [x] | phase-02c-block | |
| FASE-02D | block_reward() + proptest de emissão total | [x] | phase-02d-emission | |
| FASE-03A | RocksDB schema + accounts CF | [x] | phase-03a-storage | |
| FASE-03B | state_root via Merkle tree | [x] | phase-03b-state-root | |
| FASE-03C | Checkpoints/snapshots | [x] | phase-03c-checkpoints | |
| FASE-03D | Testes de reorg/corrupção/recuperação | [x] | phase-03d-robustness | |
| FASE-04A | ValidatorSet + seleção de líder | [x] | phase-04a-validator-set | |
| FASE-04B | Máquina de estados Propose/Verify/Sign/Commit | [x] | phase-04b-round | |
| FASE-04C | Verificação de quórum 2/3+1 e finalidade | [x] | phase-04c-quorum | |
| FASE-04D | Detecção de equivocation | [x] | phase-04d-equivocation | |
| FASE-04E | Testes de integração 4 nós (liveness + equivocation) | [x] | phase-04e-integration | |
| FASE-05A | libp2p transporte + Noise | [x] | phase-05a-noise | |
| FASE-05B | gossipsub (tx e blocos) | [x] | phase-05b-gossipsub | |
| FASE-05C | Descoberta de peers + autenticação de canal | [x] | phase-05c-peers | |
| FASE-05D | Sync inicial (fast sync via checkpoint) | [x] | phase-05d-sync | |
| FASE-05E | Rate limiting e banimento de peers | [x] | phase-05e-ratelimit | |
| FASE-06A | Mempool com validação completa | [x] | phase-06a-mempool | |
| FASE-06B | execute_block() atômico | [x] | phase-06b-execute-block | |
| FASE-06C | Decisão de produto: ClaimAccount | [x] | phase-06c-claim-account | |
| FASE-06D | Teste end-to-end de execução de bloco | [x] | phase-06d-e2e | |
| FASE-07A | JSON-RPC 2.0 + gRPC sobre TLS (scaffold) | [x] | phase-07a-rpc-scaffold | |
| FASE-07B | Endpoints RPC completos | [x] | phase-07b-endpoints | |
| FASE-07C | Auth de endpoints administrativos + rate limiting | [x] | phase-07c-auth | |
| FASE-08A | augecoin-crypto → WASM | [x] | phase-08a-wasm | |
| FASE-08B | SDK TS de alto nível + codegen do .proto | [x] | phase-08b-sdk-ts | |
| FASE-08C | Paridade de vetores de teste Rust↔TS | [x] | phase-08c-parity | |
| FASE-09A | Wallet Web | [x] | phase-09a-wallet-web | |
| FASE-09B | Wallet Desktop (Tauri) | [x] | phase-09b-wallet-desktop | |
| FASE-09C | Wallet Mobile (React Native) | [x] | phase-09c-wallet-mobile | |
| FASE-10A | CLI: comandos de validador | [x] | phase-10a-cli-validator | |
| FASE-10B | CLI: status e earnings | [x] | phase-10b-status-earnings | |
| FASE-10C | CLI: comandos de segurança | [x] | phase-10c-security | |
| FASE-11A | Métricas Prometheus | [x] | phase-11a-prometheus-metrics | |
| FASE-11B | Dashboard Grafana versionado | [x] | phase-11b-grafana-dashboard | |
| FASE-11C | Detecção ativa de comportamento malicioso | [x] | phase-11c-alerts | |
| FASE-12A | docker-compose 4 nós + script de gênesis | [x] | phase-12a-testnet | |
| FASE-12B | Hardening (audit, fuzz, load test, partição) | [x] | phase-12b-hardening | |
| FASE-13 | Checklist final pré-mainnet | [x] | phase-13-pre-mainnet | |

## Próximo passo

**Status: TODAS AS 38 FASES CONCLUÍDAS.** Bateria MAINNET_READINESS executada
em 2026-08-23 (410 testes, clippy/fmt limpos, fuzz limpo pós-fix OOM,
cargo audit sem vulnerabilidades, TLS real Let's Encrypt nos validadores
chain1). Pendências de lançamento: CI verde no remote, monitoring apontado
aos nós finais, auditoria externa + pentest. Ver
`docs/MAINNET_READINESS.md`.

## Validator Ecosystem (pós-mainnet)

| ID | Sprint | Status | Notas |
|---|---|---|---|
| SPRINT-01 | Licenças (banco + geração + hash BLAKE3) | [x] | `augecoin-ops` domínio de licenças (ADR-006/007) |
| SPRINT-02 | Planos (monthly/semiannual/annual) | [x] | enum `Plan` + duração determinística |
| SPRINT-03 | Pagamentos (PIX/USDT/AUGE) | [~] | pedidos + confirmação manual (operador); integração com gateway PSP/on-chain pendente (ADR-013) |
| SPRINT-04 | Geração da licença | [x] | `LicenseService::issue` retorna a chave uma única vez |
| SPRINT-05 | API de ativação | [x] | `POST /activate` + anti-clonagem (machine binding) (ADR-008) |
| SPRINT-06 | Desktop Windows | [~] | código completo (Tauri); empacotamento `.msi` exige host Windows/CI (ADR-014) |
| SPRINT-07 | Desktop Linux | [~] | código completo; `.deb`/`.AppImage` gerados via `BUILD.sh` (ADR-014) |
| SPRINT-08 | Heartbeat | [x] | `POST /validator/heartbeat` + presença derivada |
| SPRINT-09 | Painel operacional | [x] | SPA vanilla religada à API do `augecoin-ops` (dashboard, validadores, licenças, ganhos, monitoramento, auditoria, OTA) (ADR-012) |
| SPRINT-10 | RPC administrativos | [~] | bridge node `validatoradd/remove/deactivate` (ADR-009); endpoints admin `/v1/validators/*` |
| SPRINT-11 | Dashboard financeiro | [x] | livro-razão `validator_rewards` + worker de sync + agregação por período (ADR-010) |
| SPRINT-12 | OTA, auditoria e otimizações | [x] | releases assinados (Ed25519), monitor de presença + remoção PoA, `/audit`, retenção de heartbeats (ADR-011) |

## Notas de contexto entre sessões

*(o agente adiciona aqui, no máximo 3-4 linhas por microprompt
concluído, qualquer coisa que a próxima fase precise saber e que não
está óbvia em SPEC.md/DECISIONS.md — ex: nome exato de um trait
público criado, formato de erro adotado, etc. Não duplicar o que já
está documentado em outro lugar.)*

- FASE-09A: Wallet Web (apps/wallet-web). React 19 + TypeScript + Vite. Fluxo completo: (1) criação de carteira com seed BIP39 de 12 palavras exibida UMA vez com checkbox de confirmação, (2) tela de verificação (digitar palavra N), (3) senha (mín 8 chars, confirmação). Keystore: IndexedDB + PBKDF2-SHA256(600k iters) + AES-256-GCM (salt 32B + IV 12B). Telas: saldo, envio (conta destino + valor), recebimento (endereço + QR code via biblioteca `qrcode`). Dados sensíveis nunca saem do dispositivo. 20 testes: 9 wallet (criação/validação/importação), 6 keystore (save/load/decrypt senha errada/remove), 5 app (fluxo UI: create flow, import, password validation). Build: tsc + vite build (443KB bundle).
- FASE-09C: Wallet Mobile (apps/wallet-mobile). React Native (Expo SDK 52). Mesmo fluxo de telas da wallet-web adaptado para mobile (Welcome/Create/Confirm/Import/Password/Home com tabs Saldo/Enviar/Receber). Keystore nativo via expo-secure-store (Keychain iOS / EncryptedSharedPreferences Android) com camada adicional PBKDF2-SHA256 + AES-256-GCM (@noble/hashes + @noble/ciphers). QR code via react-native-qrcode-svg. 42 testes (17 wallet, 9 keystore, 16 app UI). Comando: npm --prefix apps/wallet-mobile test.
- FASE-10A: CLI (crates/augecoin-cli). clap derive com subcomandos validator add/remove/activate/deactivate/list. Admin commands exigem --mnemonic ou --key-file; CLI deriva HybridKeyPair via HdWallet::from_mnemonic, assina Transaction com ValidatorAdminOp, serializa canonicamente, envia via JSON-RPC 2.0 (sendTransaction) sobre TLS (reqwest+rustls). Saida tabular (box-drawing Unicode) e --json. Shell autocomplete via clap. 23 testes: 10 CLI parsing, 8 transacao admin (construcao/assinatura/verificacao/rejeicao sem chave), 5 output (tabela/JSON/vazio).
- SPRINT-01/02/04/05/08/10 (augecoin-ops): módulos `license` e `validator` + `node`. Endpoints: `POST /activate` (anti-clonagem: machine_hash/public_key ligados na licença), `POST /validator/heartbeat`, `GET /validator/stats?license=`, admin `/v1/validators/*` (approve→`validatoradd`, suspend→`validatordeactivate`, revoke→`validatorremove`). Presença `online` derivada de `last_seen` (janela 120s). Bridge do node é opcional (env `AUGECOIN_OPS_NODE_RPC_URL` + `AUGECOIN_OPS_NODE_ADMIN_KEY`); sem ele, transições on-chain retornam `NodeNotConfigured`. `validators` guarda só estado operacional; binding vive na `licenses`. 33 testes (17 unidade + 6 licenças + 10 validadores, sendo o ciclo approve/suspend/revoke coberto com node mockado via axum). Comando: `AUGECOIN_OPS_DATABASE_URL=... cargo test -p augecoin-ops`.
- SPRINT-11 (augecoin-ops): módulo `reward` (livro-razão `validator_rewards` + `RewardSync` com cursor `sync_state`). Worker caminha a chain (`getblock`) e atribui `leader_id → validator` por `node_validator_id`; cada bloco = `auge` (7,25 AUGE + fees) + `augeids` (10). Idempotente (`UNIQUE(validator_id, block_number)`). Agregação SQL por período (hora/dia/semana/mês/ano/total) sobre `block_ts` em UTC. `GET /validator/stats` e `GET /v1/validators/{id}` incluem `rewards`; `GET /metrics` (admin) retorna KPIs de rede; `list` inclui `total_rewards` do livro. 37 testes no total. Comando: `AUGECOIN_OPS_DATABASE_URL=... cargo test -p augecoin-ops`.
- SPRINT-12 (augecoin-ops): módulos `release`, `monitor`, `audit`. OTA: `POST /releases` verifica assinatura Ed25519 sobre o hash BLAKE3 do artefato contra `AUGECOIN_OPS_RELEASE_PUBLIC_KEY`; `GET /updates?platform=` retorna release+assinatura. Monitor: tiers de staleness (120/300/600s) + alertas append-only (dedup por índice único parcial) + auto-suspensão via `validatordeactivate` após 10min. `GET /audit` mescla license/validator events + alerts. Retenção de heartbeats (7 dias, diária). 45 testes no total.
- SPRINT-09 (operacional/): SPA vanilla reescrita para consumir a API ops (`/api` via nginx). Views: dashboard (KPIs + gráfico), validadores (tabela + filtros + aprovar/suspender/revogar), página individual (info + uptime + ganhos AUGE/AUGEID), licenças (emitir/suspender/revogar), ganhos, monitoramento (heartbeat verde/amarelo/vermelho + alertas), auditoria paginada, configurações (releases OTA). `x-api-key` em sessionStorage; augesat serializado como string no backend (precisão 2^53) e parseado com BigInt no frontend. Deploy: systemd `augecoin-ops.service` (porta 8790) + nginx `/api/` proxy. 45 testes backend + clippy/fmt limpos.
- Wallet Validador (apps/wallet-web): seção "Validador" no menu (intro + planos + licença + painel). Servidor Node ganha `validator_orders` (SQLite), endpoints `/api/validator/{plans,orders,orders/:id/confirm,orders/:id/issue,overview,downloads}`; `ops.js` faz o bridge para o `augecoin-ops` com a chave admin **server-side** (nunca no browser). Fluxo e2e verificado via curl: registro → pedido (pending) → confirmação do operador (`x-confirm-key`) → emissão da licença (retorna a chave uma única vez) → overview. Frontend React: 4 páginas + tipos + serviço `validator.ts` + CSS. Build `tsc && vite build` ok, 21 testes ok. Ops ganha filtros `GET /v1/licenses?user_id=` e `GET /v1/validators?license_id=`.
- Validator Desktop (apps/validator-desktop, Tauri v2): núcleo Rust (`machine.rs` machine_id=BLAKE3(install_id||hardware) sem persistir dados brutos; `ops.rs` cliente público activate/heartbeat/stats/updates; `state.rs` estado + chave privada 0600; `commands.rs` activate/get_dashboard/node_status/check_update + `resume()` re-spawna o node no boot). Sidecar `augecoin-node` embutido (`externalBin`). Frontend React: tela de ativação + dashboard (status, sincronização, último bloco, ganhos, licença/expiração, indicador "Você é o Validador Chefe", OTA com verificação de assinatura). `cargo clippy` limpo, `cargo check`/`test` ok (machine_id), `tsc && vite build` ok. Empacotamento via `BUILD.sh` (`.deb`/`.AppImage`; `.msi` requer Windows/CI).

