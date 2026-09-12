# augecoin-ops

Backend da camada operacional da AUGECOIN — licenciamento, ativação, heartbeat,
monitoramento e APIs administrativas consumidas por `operacional.augeco.in`.

**Não** guarda chaves privadas nem saldos. A autoridade de consenso permanece
on-chain: a única forma de um validador entrar no conjunto é via operação
administrativa assinada pela chave mestre e submetida pelo node RPC. Este
serviço é a ponte SaaS ↔ consenso, nunca um ator de consenso.

## Sprint 1 — domínio de licenças (entregue)

- Geração de chave de licença: 20 bytes de entropia (CSPRNG) → Crockford Base32,
  32 caracteres, agrupados em 8 blocos de 4 (`XXXX-XXXX-...-XXXX`).
- Persistência com segurança: apenas `BLAKE3-256(chave)` + prefixo de 4 chars.
- Estados: `active`, `expired` (derivado de `expires_at`), `suspended`, `revoked`.
- Planos: `monthly` (30d), `semiannual` (180d), `annual` (365d).
- Trilha de auditoria imutável (`license_events`), escrita atomicamente com cada
  mutação.

## Sprint 2 — domínio de validadores + ativação + heartbeat (entregue)

- **Ativação** (`POST /activate`): resolve a licença, valida `machine_id`
  (BLAKE3-256 hex) e `public_key` (Ed25519 hex), aplica **anti-clonagem** (a
  licença só pode ligar a uma máquina e uma chave) e registra o validador como
  `pending`. Reativação na mesma máquina é idempotente.
- **Heartbeat** (`POST /validator/heartbeat`): autenticado pela chave de licença;
  atualiza `uptime`/`cpu`/`ram`/`block`/`last_seen` e insere uma amostra em
  `heartbeats`. Presença (`online`) é **derivada** de `last_seen` (janela de
  120s), nunca armazenada.
- **Bridge consenso** (`validatoradd`/`validatorremove`/`validatordeactivate`):
  `approve`/`suspend`/`revoke` pedem ao node para assinar e submeter a operação
  administrativa. O serviço nunca assina nada — não possui a chave mestre.
- Métricas espelhadas (`uptime`, `blocks`, `blocks_lost`, `leadership`,
  `total_rewards`) — o blockchain continua sendo a autoridade dos valores.
- Trilha de auditoria imutável (`validator_events`).

## Sprint 3 — dashboard financeiro (entregue)

- **Livro-razão de recompensas** (`validator_rewards`, append-only): cada bloco
  atribuído ao validador líder registra `auge` (reward + fees, em augesat),
  `fees`, `augeids` (10 por bloco) e `block_ts`.
- **Sync worker** (`reward/sync.rs`): cursor persistente (`sync_state`), caminha
  a chain via `getblock`, mapeia `leader_id → validator` por
  `node_validator_id` e insere idempotente (`ON CONFLICT DO NOTHING`). Roda a
  cada 30s quando o bridge do node está configurado.
- **Agregação por período** (hora/dia/semana/mês/ano/total) em SQL sobre
  `block_ts` — uma query indexada por validador ou para a rede inteira.
- **Endpoints**: `GET /validator/stats` e `GET /v1/validators/{id}` passam a
  incluir `rewards` (buckets por período); novo `GET /metrics` (admin) com os
  KPIs de rede (AUGE distribuídos hoje/mês/ano, blocos, AUGEIDs, contagens de
  validadores). A lista de validadores inclui `total_rewards` vindo do livro.

## Sprint 4 — OTA, monitoramento e auditoria (entregue)

- **OTA (`releases`)** — publicação de releases com assinatura Ed25519 sobre o
  hash BLAKE3 do artefato. `POST /releases` (admin) **verifica a assinatura**
  contra a chave pública de release configurada (`AUGECOIN_OPS_RELEASE_PUBLIC_KEY`)
  antes de persistir; `GET /updates?platform=&current_version=` retorna o último
  release + `signature` + `artifact_hash` para o cliente verificar antes de
  instalar. A chave privada de release vive no pipeline de build, nunca aqui.
- **Monitor de presença (`monitor`)** — tiers de staleness por heartbeat
  (≤2min online, >2min warning, >5min critical, >10min remoção do PoA).
  Registra um alerta não-resolvido por (validador, tier) e, acima de 10min,
  suspende o validador pelo bridge do node (`validatordeactivate`).
- **Auditoria unificada** — `GET /audit` (admin) mescla `license_events`,
  `validator_events` e `alerts` num fluxo imutável paginado.
- **Retenção** — heartbeat samples podados diariamente (janela de 7 dias).

## Configuração (variáveis de ambiente)

| Variável | Obrigatória | Padrão | Descrição |
|---|---|---|---|
| `AUGECOIN_OPS_DATABASE_URL` | sim | — | URL PostgreSQL |
| `AUGECOIN_OPS_ADMIN_KEY` | sim | — | API key administrativa (hash BLAKE3 retido) |
| `AUGECOIN_OPS_LISTEN_ADDR` | não | `0.0.0.0:8790` | Endereço de bind HTTP |
| `AUGECOIN_OPS_MAX_CONNECTIONS` | não | `5` | Conexões máximas no pool |
| `AUGECOIN_OPS_NODE_RPC_URL` | não | — | URL do node JSON-RPC (habilita o bridge) |
| `AUGECOIN_OPS_NODE_ADMIN_KEY` | não | — | API key administrativa do node |
| `AUGECOIN_OPS_RELEASE_PUBLIC_KEY` | não | — | Chave pública Ed25519 (64 hex) para publicar releases |

## Endpoints

### Licenças

| Método | Rota | Auth | Descrição |
|---|---|---|---|
| `POST` | `/v1/licenses` | admin | Emite licença (retorna a chave uma única vez) |
| `GET` | `/v1/licenses` | admin | Lista licenças |
| `GET` | `/v1/licenses/{id}` | admin | Detalhe de licença |
| `POST` | `/v1/licenses/{id}/suspend` | admin | Suspende licença |
| `POST` | `/v1/licenses/{id}/revoke` | admin | Revoga licença (terminal) |
| `POST` | `/v1/licenses/validate` | — | Valida chave (sem vazar existência) |

### Validador (público — desktop e wallet)

| Método | Rota | Descrição |
|---|---|---|
| `POST` | `/activate` | Ativa licença numa máquina (liga máquina + chave) |
| `POST` | `/validator/heartbeat` | Heartbeat a cada 30s |
| `GET` | `/validator/stats?license=<key>` | Estatísticas do validador |

### Validador (admin)

| Método | Rota | Descrição |
|---|---|---|
| `GET` | `/v1/validators` | Lista validadores (com `total_rewards` do livro) |
| `GET` | `/v1/validators/{id}` | Detalhe + `rewards` por período |
| `POST` | `/v1/validators/{id}/approve` | Aprova → `validatoradd` on-chain |
| `POST` | `/v1/validators/{id}/suspend` | Suspende → `validatordeactivate` on-chain |
| `POST` | `/v1/validators/{id}/revoke` | Revoga → `validatorremove` on-chain (terminal) |
| `GET` | `/metrics` | KPIs de rede (AUGE distribuídos, contagens) |
| `GET` | `/audit?limit=&offset=` | Auditoria unificada (imutável) |
| `POST` | `/releases` | Publica release (verifica assinatura Ed25519) |
| `GET` | `/releases` | Lista releases |

### OTA (público)

| Método | Rota | Descrição |
|---|---|---|
| `GET` | `/updates?platform=&current_version=` | Último release + assinatura + hash |

Auth admin: header `x-api-key`.

### Exemplos

```bash
# Ativar uma licença (IP é derivado da conexão TCP, nunca do corpo)
curl -X POST http://localhost:8790/activate \
  -H 'Content-Type: application/json' \
  -d '{"license_key":"8F2K-X91M-A7QP-5NLD-3R8C-HJ4T-Z6WV-PQ2X",
       "machine_id":"<64 hex>","public_key":"<64 hex>",
       "augeid":"AUGE123","os":"linux","cpu":32,"ram":48,"version":"1.0.0"}'

# Heartbeat
curl -X POST http://localhost:8790/validator/heartbeat \
  -H 'Content-Type: application/json' \
  -d '{"license":"8F2K-X91M-A7QP-5NLD-3R8C-HJ4T-Z6WV-PQ2X",
       "uptime":86400,"cpu":32,"ram":48,"block":325000}'

# Aprovar um validador (exige o bridge do node configurado)
curl -X POST http://localhost:8790/v1/validators/<id>/approve \
  -H 'Content-Type: application/json' -H 'x-api-key: <admin>' \
  -d '{"activation_height": 325000}'
```

## Executar

```bash
export AUGECOIN_OPS_DATABASE_URL=postgres://user:pass@host:5432/augecoin_ops
export AUGECOIN_OPS_ADMIN_KEY=change-me
export AUGECOIN_OPS_NODE_RPC_URL=http://127.0.0.1:9005
export AUGECOIN_OPS_NODE_ADMIN_KEY=node-admin-key
cargo run -p augecoin-ops
```

As migrações são aplicadas automaticamente na inicialização.

## Testes

Testes de unidade (sempre rodam, sem banco):

```bash
cargo test -p augecoin-ops --lib
```

Testes de integração (exigem PostgreSQL; cada teste isola o estado):

```bash
export AUGECOIN_OPS_DATABASE_URL=postgres://postgres:postgres@localhost:5432/augecoin_ops_test
cargo test -p augecoin-ops
```

## Modelo de dados

```
licenses(id, license_key_hash, license_key_prefix, user_id, plan,
         augeid, machine_hash, public_key, ip, expires_at, status,
         created_at, updated_at)
license_events(id, license_id, event, actor, data, created_at)  -- append-only

validators(id, license_id, public_key, node_validator_id, os, cpu, ram, version,
           uptime, blocks, blocks_lost, leadership, total_rewards, last_seen,
           status, created_at, updated_at)
heartbeats(id, validator_id, cpu, ram, block, uptime, created_at)   -- append-only
validator_events(id, validator_id, event, actor, data, created_at)  -- append-only

validator_rewards(id, validator_id, block_number, leader_id, auge, fees,
                  augeids, block_ts, created_at)                    -- append-only, UNIQUE(validator_id, block_number)
sync_state(key, value, updated_at)                                  -- cursor resumível

releases(id, version, platform, artifact_url, artifact_hash, signature,
         notes, published_at)                                       -- UNIQUE(version, platform)
alerts(id, validator_id, kind, message, resolved_at, created_at)    -- append-only
```

`user_id` é referência suave ao usuário da wallet web (que vive no SQLite do
`wallet-web-server`) — fronteiras de persistência separadas, sem FK. Da mesma
forma, `augeid`/`ip`/`machine_hash` vivem na licença (fonte da verdade do
binding); o validador guarda apenas o estado operacional e a chave pública.

## Próximo passo (roadmap)

Backend completo (Sprints 1–12) e painel operacional entregue. Restam os
**frontends de usuário**: a seção Validador da wallet web (planos/compra/licença/
painel de ganhos) e o app desktop (Tauri) ao fluxo de ativação/heartbeat/OTA.
