# AUGECOIN — Auditoria de Segurança Fase 19 (WASM Smart Contracts)

Data: 2026-08-27
Status: **CONCLUÍDO** (correções aplicadas) — SE-1 (CRÍTICA), SE-3, SE-6, SE-9 e SE-10 corrigidos; SE-4/SE-5 aceitos; reentrança ausente (positivo). Harness de testes `contract_tests` da RPC reescrito e **100% verde** (27 testes do módulo; 67 no crate). Testes de `augecoin-contracts`: 135 passam; `cargo clippy` limpo.

Escopo: subsistema de contratos inteligentes WASM
(`crates/augecoin-contracts`, `crates/augecoin-wasm-runtime`,
`crates/augecoin-node/src/execution.rs`, `crates/augecoin-rpc/src/endpoints_contract.rs`).

---

## 1. Resumo Executivo

| ID | Severidade | Problema | Estado |
|---|---|---|---|
| SE-1 | **CRÍTICA** | Escape de isolamento de escrita: contrato WASM escrevia chaves cruas em Column Families privilegiadas (ex.: `next_code_id`, `bal/...`, `supply/...`) | **CORRIGIDO** |
| SE-2 | ALTA | `snapshot` (caminho live) varria prefixo `cid` cru em vez de `state/{cid}/` → contrato custom não enxergava próprio estado | Corrigido junto de SE-1 |
| SE-3 | MÉDIA | Divergência de layout AUGE20 nativo (helper) vs WASM (`state/{cid}/...`); `handle_contract_token_info` lê nativo | **CORRIGIDO** |
| SE-4 | MÉDIA | Gás cobrado só no sucesso (ops falhas pagam 0) | Aceito (admin-gated) |
| SE-5 | MÉDIA | `store_code` ignora `_sender`; deploy só bloqueado na camada RPC admin | Aceito (centralização conhecida) |
| SE-6 | BAIXA | Eventos de contrato genérico (não-AUGE20) descartados em `harvest_wasm` | **CORRIGIDO** |
| SE-9 | ALTA | RPC de contrato lia via `state::ContractStateStore`/`state::CodeRegistry` (chave `cid(id)`/`code_id_key`) enquanto o node escreve via `store::*` (chave `contract+id`/`key_code`) → `contract_get`/`code`/`state` retornavam "contract not found" em produção | **CORRIGIDO** |
| SE-10 | ALTA | `StorageContractReader` (leitor usado pelos handlers RPC/estado) não implementava `scan_prefix`; o snapshot WASM montado em `query_auge20_wasm`/`execute_auge20_wasm` ficava **vazio** para storage RocksDB real → AUGE20 built-in não enxergava estado em disco (saldo/token info zerados). Mascarado porque os testes de equivalência usam `MemoryContractStore` (que implementa `scan_prefix`) | **CORRIGIDO** |
| SE-7 | BAIXA | `max_call_depth: 1` é config morto (sem `call_contract`) | Inofensivo |
| SE-8 | INFO | Reentrança: **ausente** (host API fixa `auge`, sem `call_contract`/fs/network/clock) | Positivo |

---

## 2. SE-1 — Escape de isolamento de escrita (CRÍTICO) — CORRIGIDO

### Raiz
Três pipelines de execução coexistiam:
- Fase 5 `execute_wasm` + `commit_overlay` → **namespace** `state/{contract_id}/` (correto);
- `execute` (caminho *live* do node, `engine.rs:373`) + `harvest_wasm`
  (`engine.rs:830`) → colhia as escritas do host como **chaves cruas**;
- `execute_code_id` + `commit_overlay` → namespaced (correto).

O node (`crates/augecoin-node/src/execution.rs:1533`) chama `ContractEngine::execute`
e aplica os `WriteOp` via `ContractOverlay::put/flush`
(`execution.rs:147`). `ContractOverlay::flush` roteia cada chave por
`contract_cf_for_key(w.0)` (`store.rs`): qualquer chave crua like
`b"next_code_id"`, `b"bal"+...`, `b"supply"+...`, `b"code"+...` cai em Column
Families **privilegiadas** (`CF_CONTRACT_CODES`, `CF_CONTRACT_BALANCES`, ...).

Um contrato malicioso que fizesse `storage_write(b"next_code_id", ...)`:
1. sobrescrevia o contador global de `code_id` (corrupção de todo o registry);
2. ou sobrescrevia metadados/balanços/supply de *outros* contratos.

### Correção (`crates/augecoin-contracts/src/engine.rs`)
- `snapshot(store, &contract_id)` agora varre prefixo `state/{cid}/` e **strip** do prefixo (consistente com `scan_contract_state` da Fase 5).
- `WasmHost::take_log` (`engine.rs:985`) namespaceia **toda** chave escrita/deletada via `key_state(&self.contract_id, &k)`.

Resultado: nenhuma escrita de contrato atinge CF privilegiada; tudo fica confinado
em `state/{cid}/...` → CF genérica.

### Evidência (regression tests)
Módulo `mod isolation_tests` em `engine.rs` com contratos WAT atacantes:
- `contract_cannot_clobber_next_code_id` — escreve a chave crua `next_code_id`;
  afirma que a chave privilegiada **não** foi sobrescrita pelo payload do atacante
  e que a escrita vive em `state/{cid}/next_code_id`.
- `contract_cannot_clobber_other_contract_balance` — escreve a chave crua `bal`;
  afirma que `b"bal"` permanece ausente e a escrita está confinada ao namespace.

```
cargo test -p augecoin-contracts isolation_tests
test engine::isolation_tests::contract_cannot_clobber_next_code_id ... ok
test engine::isolation_tests::contract_cannot_clobber_other_contract_balance ... ok
cargo test -p augecoin-contracts   → 135 passed; 0 failed
cargo clippy -p augecoin-contracts -- -D warnings → clean
```

Antes da correção, o mesmo teste falhava (o atacante efetivamente escrevia a chave
privilegiada `next_code_id`).

---

## 3. SE-2 — Prefixo de snapshot inconsistente (ALTA)

No caminho *live*, `snapshot(store, &contract_id)` (`engine.rs`) varria
`scan_prefix(contract_id)` — i.e. só chaves que *começam* pelo `cid` cruo. O
estado genérico persistido vive, corretamente, sob `state/{cid}/...`. Um contrato
custom que lesse seu próprio estado via `storage_read` no pipeline live não
encontrava nada. Corrigido em conjunto com SE-1 (agora varre `state/{cid}/` e
stripa). A Fase 5 (`execute_wasm` → `scan_contract_state`) já fazia isso; a
divergência era só no caminho `harvest_wasm`.

---

## 4. SE-3 — Divergência de layout AUGE20 nativo vs WASM (MÉDIA) — CORRIGIDO

Helpers nativos (`crates/augecoin-contracts/src/lib.rs`) usam
`bal/{cid}/{addr}`, `supply/{cid}`, `token/{cid}` (CFs dedicadas). O contrato
AUGE20 **built-in** (`AUGE20_CODE_ID = 1`), porém, roda como WASM de verdade e
mantém todo o seu estado em `state/{cid}/(cid || tag || addr)` (ver
`crates/augecoin-auge20-wasm/src/lib.rs`: `build_key` → `cid || tag || addr`;
config em `tag='T'`, saldos em `tag='B'`, supply em `tag='S'`).

`handle_contract_token_info` (`endpoints_contract.rs`) lia `get_token_info`
(nativo, `token/{cid}`) — chave que o contrato WASM built-in **nunca** escreve.
Em produção, `token_info` de um AUGE20 on-chain retornaria "token info not found".

### Correção
`handle_contract_token_info` agora roteia por `engine.query(&reader, id,
&TokenQuery::TokenInfo.to_bytes())` e converte `QueryResult::Info` para a view
RPC. Assim a leitura vem do mesmo estado WASM que o contrato mantém. `balance`
(`handle_contract_balance`) já usava `query` e continua correto.

---

## 5. SE-4 — Gás só no sucesso (MÉDIA)

`crates/augecoin-node/src/execution.rs` cobra `gas_used` apenas quando a operação
retorna `Ok`; uma op que falha (`Err`) retorna sem débito de gás. Mitigado porque
as ops de contrato são admin-gated na RPC; recomenda-se cobrar gás mínimo também
em falha para evitar loop de retries baratas.

---

## 6. SE-5 — `store_code` ignora `_sender` (MÉDIA)

`ContractEngine::store_code(store, _sender, ...)` não usa o sender; o controle de
deploy está só na camada RPC (`handle_contract_store_code` exige `sender_hex`
== admin). Centralização conhecida; se a intenção for permitir deploy por
endereço autorizado on-chain, o sender deve ser validado no engine.

---

## 7. SE-6 — Eventos de contrato genérico descartados (BAIXA)

`harvest_wasm` ignora `call.events` para o caminho de WASM armazenado genérico
(só `execute_auge20_wasm` copia eventos). `WasmHost::emit_event` é no-op e os
eventos capturados em `HostState.events` não são expostos. Recomendado propagar
`outcome.events` também para o caminho genérico.

---

## 8. SE-7 / SE-8 — Config morto e reentrança

- `max_call_depth: 1` (`RuntimeLimits`) é inerte: não existe host fn de
  chamada cross-contrato, logo não há profundidade real. Inofensivo.
- **Reentrança: ausente.** O runtime exporta apenas a API fixa `auge`
  (`storage_read/write/remove`, `sender`, `block_height`, `emit_event`,
  `crypto_*`). Não há `call_contract`, acesso a fs/network/clock, nem leitura de
  estado de outro contrato. Um contrato só toca `state/{cid}/...`.

---

## 8. SE-9 — RPC de contrato usava API de leitura inconsistente com o node (ALTA) — CORRIGIDO

### Raiz
`crates/augecoin-rpc/src/endpoints_contract.rs` importava `ContractStateStore` e
`CodeRegistry` do **re-export do crate** (`augecoin_contracts::{...}`), que aponta
para o módulo `state`. O módulo `state` usa esquema de chaves diferente do módulo
`store` usado pelo node/engine para escrever:

| dado | `state` (lido pela RPC) | `store` (escrito pelo node) |
|---|---|---|
| meta de contrato | `cid(id)` (só o id) | `b"contract" + id` |
| código | `code_id_key(id)` | `b"code" + id` / `b"codehash" + hash` |
| token info | `cid(id) + ".token"` | `b"token" + id` |
| estado genérico | `cid(id) + ".state"` | `state/{cid}/<key>` |

Como o node aplica operações via `ContractEngine` (escreve com chaves do módulo
`store`), um contrato on-chain real **nunca** era encontrado pelas rotas RPC
`contract_get`, `contract_code_by_id`, `contract_code_by_hash` e `contract_state`
— todas retornavam "contract not found"/"code not found". Bug de produção
silencioso (os testes de RPC não compilavam, então passou despercebido).

### Correção
`endpoints_contract.rs` agora usa `augecoin_contracts::store::{ContractStateStore,
CodeRegistry}` (mesmo esquema de chaves do node) e `handle_contract_state` passa
a chamar `get_contract_state(&id, &key)` (2 args, valor bruto), consistente com o
namespace `state/{cid}/...` aplicado em SE-1.

> Nota: os testes unitários `contract_tests` do RPC ainda têm **falhas de
> concepção do harness** (pré-existentes, não relacionadas à auditoria): o estado
> de teste não configura `admin_keypair` (logo `submit_contract_op` falha), um
> teste usa wasm inválido (`vec![1,2,3]`), e `put_auge20` escreve CFs nativos
> enquanto o AUGE20 built-in lê estado WASM (`state/{cid}/...`). Esses testes
> exigem reescrita do harness para usarem o pipeline on-chain do engine e estão
> fora do escopo desta fase; a correção de produção (SE-9) é independente deles.

## 8.1 SE-10 — `StorageContractReader` sem `scan_prefix` (ALTA) — CORRIGIDO

### Raiz
Os handlers RPC e o caminho de leitura de estado usam `StorageContractReader`
(`crates/augecoin-contracts/src/store.rs:237`) para alimentar o `engine`. O
`engine` monta o **snapshot** legível pelo WASM chamando `store.scan_prefix(...)`
(em `query_auge20_wasm`/`execute_auge20_wasm`, via `snapshot()`). `StorageContractReader`
só implementava `get`; seu `scan_prefix` caía no *default* da trait (vazio), então
o snapshot para storage RocksDB real ficava **vazio**.

Consequência em produção: o AUGE20 built-in (`code_id == 1`) é executado pela
pipeline WASM, que lê saldo/token-info via `state/{cid}/...`; com snapshot vazio,
`read_u64`/`read_token_state` não encontravam nada → saldo e token info retornavam
**0** (e `query` devolvia `InvalidSerialization`). Os testes de equivalência
`native_vs_wasm_equivalence` usam `MemoryContractStore` (que implementa
`scan_prefix`), mascarando o bug.

### Correção (`crates/augecoin-contracts/src/store.rs`)
`StorageContractReader` agora implementa `scan_prefix`, delegando a
`storage.contract_scan_prefix(prefix)` (mesma CF `CF_CONTRACTS` usada por `get`).

### Evidência
Após a correção, o harness `contract_tests` (que exercita AUGE20 sobre storage real)
passa integralmente: `contract_balance_existing` (saldo 1_000_000), `contract_token_info_existing`
(token info correta), `contract_query_existing`, `contract_execute_existing`,
`contract_simulate_existing`, `contract_estimate_gas_existing` etc.

---

## 9. Arquivos alterados (nesta fase)

| Caminho | Função | Motivo |
|---|---|---|
| `crates/augecoin-contracts/src/engine.rs` | `snapshot` | varrer `state/{cid}/` + strip (SE-1/SE-2) |
| `crates/augecoin-contracts/src/engine.rs` | `WasmHost::take_log` | namespacear todas as chaves (SE-1) |
| `crates/augecoin-contracts/src/engine.rs` | `harvest_wasm` | propagar `call.events` (SE-6) |
| `crates/augecoin-contracts/src/engine.rs` | `mod isolation_tests` (novo) | regression tests do escape (SE-1) |
| `crates/augecoin-rpc/src/endpoints_contract.rs` | `handle_contract_token_info` | rotear por `engine.query(TokenInfo)` (SE-3) |
| `crates/augecoin-rpc/src/endpoints_contract.rs` | imports `store::{ContractStateStore, CodeRegistry}` | chaves consistentes com o node (SE-9) |
| `crates/augecoin-rpc/src/endpoints_contract.rs` | `handle_contract_state` | `get_contract_state(&id, &key)` (SE-9) |
| `crates/augecoin-contracts/src/store.rs` | `StorageContractReader::scan_prefix` | implementar `scan_prefix` (delega `contract_scan_prefix`) para snapshot WASM completo em storage real (SE-10) |
| `crates/augecoin-rpc/src/endpoints_contract.rs` | `mod contract_tests` | reescrito: `make_state` registra admin (conta+saldo) p/ `submit_contract_op`; `put_code_at` grava em `code_id` exato; `put_auge20` faz deploy real via `ContractEngine`+`ContractOverlay`+`flush`; wasm de contrato válido via `wat` |

---

## 10. Próximos passos recomendados

 1. (Opcional) SE-4: cobrar gás mínimo em falha de contrato.
 2. ~~Reescrever o harness de testes `contract_tests` da RPC...~~ **CONCLUÍDO** —
    o harness agora usa o pipeline on-chain real (`make_state` com admin registrado,
    `put_code_at` em `code_id` exato, `put_auge20` via `ContractEngine`+`ContractOverlay`+`flush`,
    wasm válido via `wat`); 27 testes do módulo passam e 67 no crate.
 3. Unificar/remover os helpers nativos mortos (`create_auge20`/`execute_auge20`)
    uma vez confirmado que nenhum caminho de produção os usa.
