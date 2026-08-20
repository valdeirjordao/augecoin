# AUGECOIN — SafeBox Flow

> Fonte: `crates/augecoin-core/src/safe_box.rs`, `crates/augecoin-storage/src/lib.rs`,
> `crates/augecoin-node/src/execution.rs`. Nós do grafo: `SafeBox`,
> `SafeboxCache`, `compute_safe_box_hash`, `commit_safebox_incremental`.

## Estrutura

```
SafeBox {
  header: SafeBoxHeader { protocol, start_block, end_block, blocks_count, safe_box_hash }
  accounts: BTreeMap<u64, Account>          // ordenado por número
  name_index: BTreeMap<String, u64>         // nome → número de conta (índice de nomes)
}
```

## Ciclo de vida do AUGEID (AccountState)

```
Unknown → (emissão do bloco) → Reserved → (venda/doação/ativação) → Owned → Normal
                                        ↘ GiftPending (aguardando aceite)
```

| Estado | Descrição |
|---|---|
| `Reserved` | Emitido ao líder do bloco; sem chave definitiva; não envia/recebe AUGE |
| `Owned` | Tem proprietário (`account_key`, `n_operation=0`, `account_seal`) |
| `ForSale` | Anunciado no marketplace (`sale_index`) |
| `GiftPending` | Doação pendente (`gift_index`); aguarda aceite |
| `Normal` | Conta operacional |

A numeração é determinística: **AUGEID = bloco × 10 + offset (0..9)**. Cada bloco
emite exatamente 10 AUGEIDs, todos `Reserved` e pertencentes ao líder.

## Hash de consenso

`compute_safe_box_hash()` = raiz Merkle (blake3_512) sobre:
1. `accounts.values().map(|a| a.hash())` — folha por conta;
2. `name_index` — `blake3(name || number)` por entrada.

- **Empty SafeBox → `[0u8; 64]`.**
- O hash é gravado no header do bloco como `initial_safe_box_hash` e realimentado
  no bloco seguinte (`prev_safe_box_hash`). **É parte do consenso.**
- `sale_index` e `gift_index` são índices residentes derivados (NÃO entram no hash).

## Persistência incremental

Todas as escritas de um bloco (contas modificadas + bloco + altura + validator_set
+ snapshot periódico) são consolidadas num único `commit_block_atomic` (WriteBatch).

```
execute_block (por bloco)
  └─ commit_block_atomic(&modified, block, validator_set_bytes)
        ├─ WriteBatch: put_account(contas) + put_block + put_height + put_validator_set
        ├─ atualiza SafeboxCache residente (accounts + name_index + pubkey_index
        │   + sale_index + gift_index) — O(k)
        └─ snapshot completo periódico (intervalo configurável)
```

## Índices residentes (O(1), sincronizados incrementalmente)

| Índice | Mapeamento | Uso |
|---|---|---|
| `name_index` | nome → AUGEID | resolução `resolve_name("CarlosPay")` |
| `pubkey_index` | pubkey → refcount | detecção de chave duplicada |
| `sale_index` | AUGEID → `SaleListing` | marketplace (`list_for_sale`) |
| `gift_index` | AUGEID → `GiftListing` | doações pendentes (`list_pending_gifts`) |

## Snapshot periódico

- Intervalo: `SAFEBOX_SNAPSHOT_INTERVAL_DEV = 100` / `_MAINNET = 1000`
  (override `AUGECOIN_SAFEBOX_SNAPSHOT_INTERVAL`).
- `flush_safebox_snapshot()` — força snapshot (shutdown/export/recovery).

## Recuperação

- O hash é **sempre** reconstruído do CF `accounts` (fonte de verdade), nunca de
  um snapshot potencialmente atrasado — crash entre snapshots é seguro.

## Invariantes

- `SafeBox.accounts` ≡ conteúdo do CF `accounts` após o commit.
- `name_index` ≡ derivado dos `account.name`.
- A emissão é fixa: exatamente 10 AUGEIDs `Reserved` por bloco, todos do líder.
- `CreateAccount` nunca cria número novo — apenas ativa um `Reserved` existente.
