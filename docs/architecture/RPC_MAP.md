# AUGECOIN — RPC Map

> Fonte: `crates/augecoin-rpc/src/lib.rs`, `crates/augecoin-rpc/src/endpoints.rs`,
> `crates/augecoin-rpc/src/auth.rs`. Nós do grafo: `dispatch_method`, `AppState`,
> `check_auth`, `handle_*`.

## Endpoints HTTP

| Rota | Handler | Finalidade |
|---|---|---|
| `POST /` | `jsonrpc_handler` | JSON-RPC 2.0 genérico |
| `POST /createaccount` | `create_account_handler` | cria conta (sensível) |
| `POST /faucet` | `faucet_handler` | faucet (sensível) |
| `GET /health` | `health_handler` | liveness |

## Autenticação / rate limiting

- Header `x-api-key` → `auth::check_auth` → `AuthLevel::{Public, Admin}`.
- `ApiKeyStore` valida chaves conhecidas.
- `RateLimiter` global (100 req/60s) + `sensitive_rate_limiter` (5 req/60s)
  para `createaccount`/`faucet`.
- Métodos admin (`validator_*`) exigem `AuthLevel::Admin`.

## Métodos JSON-RPC (`dispatch_method`)

| Método | Handler (`endpoints.rs`) | Acesso |
|---|---|---|
| `getaccount` | `handle_get_account` | público |
| `createaccount` | `handle_create_account` | público (rate-limited) |
| `getblock` | `handle_get_block` | público |
| `getblockoperations` | `handle_get_block_operations` | público |
| `getoperations` | `handle_get_operations` | público |
| `getoperationbyhash` | `handle_get_operation_by_hash` | público |
| `getblockbyhash` | `handle_get_block_by_hash` | público |
| `getpendings` | `handle_get_pendings` | público |
| `sendoperation` | `handle_send_operation` | público |
| `findaccounts` | `handle_find_accounts` | público |
| `getaccountcount` | `handle_get_account_count` | público |
| `getblockcount` | `handle_get_block_count` | público |
| `getnodestatus` | `handle_get_node_status` | público |
| `getvalidatorset` | `handle_get_validator_set` | público |
| `validator_add` | `handle_validator_add` | admin |
| `validator_remove` | `handle_validator_remove` | admin |
| `validator_activate` | `handle_validator_activate` | admin |
| `validator_deactivate` | `handle_validator_deactivate` | admin |
| `listaccountsforsale` | `handle_list_accounts_for_sale` | público |
| `listvalidatorinventory` | `handle_list_validator_inventory` | público |
| `listpendinggifts` | `handle_list_pending_gifts` | público |
| `buyaccount` | `handle_buy_account` | público (assinado) |
| `sellaccount` | `handle_sell_account` | público (assinado) |
| `giftaccount` | `handle_gift_account` | público (assinado) |
| `acceptgift` | `handle_accept_gift` | público (assinado) |
| `cancelsale` | `handle_cancel_sale` | público (assinado) |

> **Nota sobre `createaccount`:** agora recebe `account_number` + `public_key_hex`
> e apenas *ativa* um AUGEID `Reserved` existente (assinado pelo admin/líder).
> Nunca cria números novos.

## Marketplace e ciclo de vida do AUGEID

```
listaccountsforsale → Storage::list_for_sale() (sale_index residente)
listvalidatorinventory → Storage::safebox() filtrando por validator_public_key
buyaccount   → op BuyAccount  → Reserved/ForSale → Owned (atomic)
sellaccount  → op ListAccountForSale → (Reserved|Owned|Normal) → ForSale
giftaccount  → op GiftAccount → → GiftPending
acceptgift   → op AcceptGift  → GiftPending → Owned
cancelsale   → op DelistAccount → ForSale → Owned
```

## Estado compartilhado (`AppState`)

```
AppState {
  storage: Arc<Storage>              // RocksDB
  mempool: Arc<Mutex<Mempool>>       // pool de operações pendentes
  node_status: Arc<NodeStatus>       // métricas/status
  faucet_keypair, faucet_account, faucet_amount
  faucet_claims: Mutex<HashMap>      // claims por endereço (24h)
  api_keys: ApiKeyStore
  rate_limiter, sensitive_rate_limiter
  admin_keypair
  op_broadcast: sender               // gossip p/ operações admitidas
}
```

## Fluxo de `sendoperation` (caminho até o consenso)

```
POST / (sendoperation)
  → dispatch_method → handle_send_operation
  → valida assinatura + nonce + saldo
  → mempool.validate_and_admit
  → op_broadcast (gossip para demais validadores)
  → (no próximo bloco) execute_block consome do mempool
```

## Wallet Web — quais RPCs o frontend consome

O frontend (`apps/wallet-web/src/services/rpc.ts`) é a **única** porta de acesso à
blockchain e consome **apenas** os métodos oficiais abaixo — não há RPC inventada
na UI nem regra de consenso duplicada:

- Consulta: `getaccount`, `findaccounts`, `listaccountsforsale`,
  `listvalidatorinventory`, `listpendinggifts`, `getnodestatus`.
- Ciclo de vida (assinado): `buyaccount`, `sellaccount`, `giftaccount`,
  `acceptgift`, `cancelsale`.
- Operações brutas: `sendoperation`.

> O `server/` da Wallet Web **não** é uma RPC da blockchain: ele é um serviço de
> identidade/registro (Conta da Plataforma + `linked_wallets`), exposto em
> `/api/*` (HTTP + cookies), que nunca detém saldo, estado ou chaves privadas.
> Ele apenas responde "quais AUGEIDs pertencem a este usuário?" — o saldo/nome/
> estado de cada AUGEID é sempre lido do SafeBox via `getaccount`.
