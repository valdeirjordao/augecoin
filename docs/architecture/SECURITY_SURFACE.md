# AUGECOIN — Security Surface

> Mapeamento da superfície de ataque a partir do grafo (`graph.json`) e leitura
> de `rpc/lib.rs`, `rpc/auth.rs`, `execution.rs`, `consensus.rs`, `crypto/*`.

## Superfícies de ataque

### 1. RPC HTTP (porta configurável, exposta na rede)
- `POST /`, `/createaccount`, `/faucet`, `GET /health`.
- Vetores: rate-limit (mitigado por `RateLimiter` global + sensível), auth
  (métodos admin exigem `x-api-key` válido), `createaccount`/`faucet` são
  sensíveis e têm limitador dedicado de 5 req/60s.

### 2. libp2p / rede (gossipsub + Kademlia)
- Vetores: nós maliciosos, mensagens oversized (mitigado por
  `max_transmit_size=1MiB`), spoofing de endereço (mitigado por
  `AUGECOIN_EXTERNAL_ADDRESS` e rejeição de `0.0.0.0`/`::`).
- `deserialize_consensus_msg`, `read_u32_be`/`read_u64_be` — limites de
  comprimento são críticos para evitar OOM/overflow.

### 3. Validação de assinaturas
- `verify_operation_signatures` (Ed25519) sobre `to_bytes_stripped()`.
- `verify_block_quorum`: assinatura do líder + quórum 2/3 + `operations_hash`.
- `Ed25519KeyPair`/`HybridSignature` — nunca usar chave com verificação frouxa.

### 4. Serialização crítica
- `Account::to_bytes/from_bytes`, `Operation::to_bytes_stripped`,
  `SafeBox::to_bytes/from_bytes`, `OperationBlock::from_bytes`,
  `ValidatorSet::to_bytes/from_bytes`.
- Vetor: dados malformados → `from_bytes` deve falhar fechado (retornam
  `Option`/`Result`, nunca panic em input externo).

### 5. Faucet
- `faucet_claims` persistido em RocksDB (`faucet_claims` CF); 1 claim por
  AUGEID por 24h. `faucet_account` controlado por `faucet_keypair`.
- Destino por `account_number` (AUGEID); `Reserved`/`GiftPending` não recebem.

### 6. Marketplace e doação (novos vetores)
- `buyaccount`/`sellaccount`/`giftaccount`/`acceptgift`/`cancelsale` são
  assinados (Ed25519) e validados em `verify_operation_signatures`.
- `CreateAccount` exige assinatura do admin/líder e só ativa AUGEID `Reserved`.
- Proteção anti-replay via `n_operation`.
- Regras de consenso: só o proprietário vende; só o líder recebe emissão;
  `Reserved`/`GiftPending` não movimentam saldo; dupla venda/doação são
  rejeitadas pelo estado da conta.

### 7. Chaves e secrets
- `AUGECOIN_VALIDATOR_KEY_HEX` / `_FILE`, `AUGECOIN_FAUCET_KEY_HEX`.
- Nunca logar chaves; `genesis.rs` valida `public_key != [0u8;32]`.

## Caminhos RPC privilegiados (admin)

`validator_add`, `validator_remove`, `validator_activate`, `validator_deactivate`
→ exigem `AuthLevel::Admin` via `check_auth`. Qualquer bypass do `ApiKeyStore`
é uma escalada de privilégio crítica (controle do conjunto de validadores).
`createaccount` é assinado pelo admin (não cria números, só ativa `Reserved`).

## Código sensível

- `crypto/signature.rs` — Ed25519 + assinatura híbrida.
- `crypto/hash.rs` — `blake3_512`.
- `crypto/address.rs` (bech32) — `derive_address`/`validate_address`.
- `crypto/address.rs` (Base58Check curto) — `derive_short_address` é somente
  uma representação externa; o endereço canônico Bech32m continua sendo a
  identidade usada pelo protocolo.
- `core/safe_box.rs` — Merkle proof + hash de consenso.
- `node/execution.rs` — execução determinística + emissão (hard cap).

## Wallet Web — superfície do backend de plataforma (`apps/wallet-web/server`)

Serviço separado da blockchain (HTTP `/api/*`). Nunca detém saldo, estado do
SafeBox ou chave privada — apenas identidade + registro de vínculos.

| Controle | Implementação |
|---|---|
| Senha | `crypto.scrypt` (N=16384, r=8, p=1), salt por usuário, `timingSafeEqual` |
| Sessão | JWT access (15 min, HttpOnly/SameSite=Lax) + refresh token opaco rotativo, armazenado como `sha256` |
| Logout global | revoga todos os refresh tokens do usuário |
| CSRF | double-submit (`X-CSRF-Token` vs cookie não-HttpOnly) + verificação de Origin |
| Rate limit login | em memória: 5/60s por IP e por e-mail |
| Chave privada | **nunca** enviada ao backend; o mnemônico é gerado e criptografado (IndexedDB keystore, Argon/PBKDF2 + AES-GCM) apenas no cliente |
| Registro | cria somente `platform_users` + `user_preferences` — nunca um AUGEID |

Tabelas: `platform_users`, `user_preferences`, `linked_wallets`, `refresh_tokens`.
`linked_wallets` é a camada de ponte (Wallet Registry); a autoridade sobre
propriedade/saldo/transferência continua exclusiva do SafeBox.

Pré-requisitos de produção: definir `AUGECOIN_WALLET_JWT_SECRET` e
`AUGECOIN_WALLET_ORIGINS`; ativar `AUGECOIN_WALLET_SECURE_COOKIES=1` sob TLS.
Suporte futuro planejado: WebAuthn e 2FA (sem alterar o consenso).

## Recomendações

1. Fuzz continuado em `from_bytes` (existe `crates/augecoin-core/fuzz`).
2. Manter `rate_limiter`/`sensitive_rate_limiter` calibrados sob carga real.
3. Revisar `deserialize_consensus_msg` para limites de tamanho de cada campo.
4. Monitorar `augecoin_equivocation_events_total` (equivocação) via Prometheus.
