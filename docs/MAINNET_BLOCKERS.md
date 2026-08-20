# AUGECOIN — MAINNET BLOCKERS

## BLOCKING (deve resolver antes de mainnet)

| ID | Severity | Problema | Evidência | Correção | Status |
|---|---|---|---|---|---|
| C-01 | CRITICAL | `execute_block()` não verifica `leader_signature` nem `quorum_signatures` | `execution.rs` — grep confirma que estes campos só aparecem em testes | Adicionar verificação de leader sig + `verify_quorum()` antes de Phase 2 | RESOLVED |
| C-02 | CRITICAL | `CommitNotification` executado sem verificar quorum | `main.rs:670` — `quorum_sigs.clone()` sem `verify_quorum()` | Chamar `verify_quorum(&quorum_sigs, &validator_set, &block_hash)` antes de `execute_block()` | RESOLVED |
| C-03 | CRITICAL | `operations_hash` hardcoded `[0u8;64]` no `build_block()` | `consensus.rs:409` — `operations_hash: [0u8; 64]` | Computar `compute_operations_merkle_root()` e atribuir | RESOLVED |
| C-04 | CRITICAL | main.rs RPC raw TCP sem TLS, sem auth, em `0.0.0.0` | `main.rs:19-37` — `TcpListener::bind("0.0.0.0")` sem TLS | Substituir `start_jsonrpc()` por `augecoin-rpc::RpcServer` com TLS real | RESOLVED |
| H-01 | HIGH | `detect_equivocation()` nunca chamado no runtime | `equivocation.rs` — não importado em `main.rs` nem `consensus.rs` | Integrar no loop de consenso | RESOLVED |
| H-02 | HIGH | Sem view-change para líder offline | `main.rs:793-807` — round timeout apenas reseta, não rotaciona líder | Implementar `RoundState::view_change()` com round number | RESOLVED |
| H-03 | HIGH | Crashed validator não reintegra consenso | `main.rs:607-611` — commit futuro é skipped, sem sync | Implementar sync via checkpoint + replay | RESOLVED |
| H-04 | HIGH | `check_auth()` dá Admin para qualquer chave `is_valid()` | `auth.rs:119` — `AuthLevel::Admin` para non-admin | Retornar `AuthLevel::Public` para chaves não-admin | RESOLVED |
| H-05 | HIGH | Operações não incluem chain ID | `operation.rs` — sem campo `chain_id` | Adicionar `chain_id` ao payload e incluir na assinatura | RESOLVED |
| H-06 | HIGH | Buffer stack 16KB por conexão RPC | `main.rs:25` — `[0u8; 16384]` | Usar buffered reading com `max_request_size` | RESOLVED |

## NON-BLOCKING (recomendado mas não impede mainnet)

| ID | Severity | Problema | Correção |
|---|---|---|---|
| M-01 | MEDIUM | Timestamp usa `SystemTime::now()` (clock drift) | Considerar timestamp do bloco anterior |
| M-02 | MEDIUM | Gossipsub sem `max_transmit_size` | Configurar limite (RESOLVED — 1 MiB + message_id_fn/duplicate_cache_time/heartbeat/mesh) |
| M-03 | MEDIUM | Parser HTTP manual frágil | Usar framework HTTP |
| L-01 | LOW | Wallet desktop usa XOR não AES-GCM | Substituir por AES-256-GCM |
| L-02 | LOW | TLS certs auto-assinados por padrão | Suportar `AUGECOIN_TLS_CERT` env var |