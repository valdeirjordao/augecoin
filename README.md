# AUGECOIN

AUGECOIN é uma blockchain em Rust com consenso Proof of Authority, estado
persistido em RocksDB, rede P2P baseada em libp2p, API RPC, contratos WASM e
clientes TypeScript, web e desktop.

## Arquitetura

| Camada | Componentes | Responsabilidade |
|---|---|---|
| Criptografia e tipos | `augecoin-crypto`, `augecoin-core` | Chaves, assinaturas, hashes, contas, operações e blocos |
| Consenso e execução | `augecoin-consensus`, `augecoin-node` | Validadores, proposição, votação, quórum e transição de estado |
| Persistência | `augecoin-storage` | Estado canônico, blocos, índices e escrita atômica em RocksDB |
| Rede e RPC | `augecoin-network`, `augecoin-rpc` | libp2p, gossip, sincronização e interface JSON-RPC |
| Contratos | `augecoin-contracts`, `augecoin-wasm-runtime`, `augecoin-auge20-wasm` | Engine, sandbox WASM e token AUGE20 |
| Clientes e operação | SDK, wallets, CLI, `augecoin-ops`, `infra/` | Assinatura local, operação de nós e observabilidade |

## Fluxo de Transação

```text
Wallet / SDK / CLI
  -> Operation assinada
  -> RPC ou gossip P2P
  -> Mempool::validate_and_admit
  -> ConsensusEngine::build_block
  -> proposta, validação, votos e quórum
  -> execute_block
  -> Storage::commit_block_atomic_with_contracts
```

O mempool verifica chain ID, duplicidade, assinaturas, sequência de operação,
saldo e taxa mínima. A execução do bloco revalida as operações, aplica as
transições de estado e persiste contas, bloco, conjunto de validadores e estado
de contratos em um único `WriteBatch`.

## Consenso PoA

- O conjunto ativo é representado por `ValidatorSet`.
- O quórum é calculado como `floor(2n / 3) + 1`.
- O líder é escolhido deterministicamente entre validadores ativos por altura e
  round.
- O líder monta e assina o bloco; validadores verificam pré-estado, execução e
  assinaturas antes de votar.
- Um bloco é aplicado somente após verificação de quórum, assinatura do líder,
  raiz de operações e raiz de estado prévia.
- O runtime de consenso em produção está no loop de `augecoin-node`; o crate
  `augecoin-consensus` fornece as primitivas de conjunto, quórum e
  equivocation.

## Tokenomics Implementada

Os valores abaixo descrevem a implementação atual em
`crates/augecoin-core/src/constants.rs` e `emission.rs`.

| Parâmetro | Valor atual |
|---|---:|
| Unidade mínima | 1 AUGE = 100.000.000 augesat |
| Taxa mínima por operação | 1.000 augesat |
| Tempo de bloco | 5 segundos |
| Recompensa por bloco | 250.000.000 augesat (2,50 AUGE) |
| Blocos de emissão | 315.576.000 |
| Hard cap | 788.940.000 AUGE |
| Contas reservadas emitidas por bloco | 3 |
| Distribuição de taxas e recompensa | 100% para o líder do bloco |

`block_reward(height)` retorna a recompensa fixa durante o período de emissão
e zero após seu término. A execução soma as taxas das operações ao reward e
credita o total ao líder do bloco.

## Taxas De Operação E Contrato

A taxa mínima de qualquer operação admitida no mempool é
`MIN_FEE_AUGESAT = 1.000` augesat. Essa taxa plana é validada com o saldo e a
sequência da conta antes de a operação poder entrar em um bloco.

Operações de contrato pagam dois componentes:

```text
custo total = taxa plana da operação + (gas_used * GAS_PRICE_AUGESAT)
```

| Parâmetro | Valor atual |
|---|---:|
| Taxa plana mínima | 1.000 augesat |
| Preço do gás | 1 augesat por unidade de gás |
| Limite de gás usado pela execução de consenso | 100.000 unidades |
| Reserva exigida com a taxa mínima | 101.000 augesat |

Antes da execução, o node exige saldo suficiente para a taxa plana mais o
limite máximo de gás. Após execução bem-sucedida, cobra a taxa plana e apenas
o gás efetivamente consumido; o restante da reserva não é debitado. O consumo
de gás é determinístico: combina custo-base da operação, fuel WASM e chamadas
de host.

| Operação de contrato | Custo-base de gás |
|---|---:|
| Publicar código WASM | 50.000 |
| Criar contrato | 10.000 |
| Transferir token | 5.000 |
| Mint | 7.500 |
| Burn | 5.000 |

Taxa plana e gás cobrado são agregados às taxas do bloco e destinados ao líder
que finaliza o bloco. Não existe, na implementação atual, uma taxa fixa
adicional exclusiva para criar um token AUGE20.

> **Nota de protocolo:** `docs/SPEC.md` e ADR-015 descrevem parâmetros
> econômicos diferentes dos valores presentes na implementação atual. Esta
> documentação não altera nem resolve essa divergência.

## Contratos WASM E Token AUGE20

Contratos WASM são a plataforma genérica de execução contratual. AUGE20 é o
contrato fungível embutido que usa essa plataforma e está associado ao `code ID
1`; outros contratos WASM podem ser armazenados e executados com outros IDs.

```text
Contrato de criação assinado
  -> validação de taxa e reserva de gás
  -> execute_operation
  -> ContractEngine::create_contract
  -> AUGE20 WASM (code ID 1)
  -> writes em overlay
  -> commit atômico do estado de contrato
```

O runtime expõe somente o host `auge`, com acesso a sender autenticado, altura,
ID do contrato, armazenamento com escopo do contrato e eventos. Imports e
limites de memória são validados, e a execução é medida por fuel.

O AUGE20 implementa criação, transferência, mint, burn, consulta de saldo,
supply e metadados. Atualmente não existe validação global de unicidade para
nome ou ticker/símbolo de token; apenas IDs de contrato duplicados são
rejeitados.

### Criação De Um Token AUGE20

O criador monta um `ContractOp::CreateContract` para o `code ID 1`, com um
`TokenInit` serializado contendo, nesta ordem: `decimals`, flags de mint e
burn, `initial_supply`, `max_supply`, nome e símbolo.

```text
Conta assinante
  -> Operation::Contract com taxa plana
  -> validação de assinatura, sequência, saldo e reserva de gás
  -> ContractEngine::create_contract
  -> validação do TokenInit
  -> instanciação do AUGE20 WASM
  -> configuração, supply e saldo inicial gravados no overlay
  -> commit atômico com o bloco
```

Na criação, o engine exige nome e símbolo não vazios, `decimals <= 18` e
`initial_supply <= max_supply`. O ID é determinístico e deriva de
`owner + code_id + init_data`; por isso, uma segunda criação com exatamente os
mesmos dados e mesmo owner é rejeitada como contrato duplicado. Esse mecanismo
não impede tokens diferentes de compartilharem nome ou símbolo.

O caminho de wallet/SDK cria uma operação assinada pela conta do usuário. Já o
endpoint RPC administrativo `contract_create` assina a operação com a conta
admin configurada no node; nessa rota, o campo `sender_hex` da requisição não
define o owner on-chain. A execução canônica sempre usa o limite padrão de
100.000 unidades de gás, independentemente do limite usado na simulação RPC.

## Rede, Estado E RPC

- P2P: TCP, Noise, Yamux, Gossipsub, Identify, Kademlia e tópicos unicast.
- Estado: contas, blocos, SafeBox, índices, validadores, faucet e contratos em
  column families RocksDB.
- Integridade: `consensus_state_root` inclui estado de contas, validadores e
  contratos.
- RPC: métodos de leitura para contas, blocos, operações, validadores e
  contratos; métodos mutáveis entram no mempool e passam pelo consenso antes de
  modificar a cadeia.

## Estrutura Do Repositório

| Diretório | Conteúdo |
|---|---|
| `crates/` | Crates Rust de protocolo, node, rede, armazenamento e operação |
| `apps/` | Wallets web, mobile e desktop, além do validator desktop |
| `packages/` | SDK TypeScript e bindings WASM |
| `infra/` | Imagens, topologia de nós e observabilidade |
| `scripts/` | Gênesis, inicialização, monitoramento e operação |
| `docs/` | Especificação, decisões e documentação técnica |

## Documentação Técnica

- [Apresentação técnica](docs/TECHNICAL_PRESENTATION.md)
- [Especificação do protocolo](docs/SPEC.md)
- [Decisões arquiteturais](docs/DECISIONS.md)
- [Regras do projeto](docs/PROJECT_RULES.md)
- [Questões em aberto](docs/OPEN_QUESTIONS.md)

## Public Testnet Accounts

Use `scripts/genesis/public-genesis.toml` with `AUGECOIN_GENESIS_CONFIG`.
The file references private-key files outside the repository; the node stores
only derived public keys. Configure the faucet signer separately with
`AUGECOIN_FAUCET_KEY_HEX` and `AUGECOIN_FAUCET_ACCOUNT`.

The RPC exposes `POST /createaccount` with `{ "public_key_hex": "..." }` and
`POST /faucet` with `{ "address": "3Fb2QT3ngwCVZPSK9KEKVo241d7oJMxw91HzpMo" }`.
Faucet claims are persisted
in RocksDB and limited to one claim per address per 24 hours.

## Networking hardening

- **Explicit external address** — set `AUGECOIN_EXTERNAL_ADDRESS` (e.g.
  `/ip4/203.0.113.10/tcp/9200`) to force a stable, dialable multiaddr for NAT /
  VPS / multi-interface hosts. Priority: external address → `identify.observed_addr`
  → local address. `0.0.0.0` / `::` are rejected.
- **Gossipsub limits** — `max_transmit_size=2097152` (2 MiB) plus pinned
  `message_id_fn`, `duplicate_cache_time`, `heartbeat_interval`,
  `history_length`/`history_gossip` and mesh parameters. Oversized messages are
  rejected before transmission.
- **WASM crypto** — `packages/wasm-crypto` is regenerated from
  `crates/augecoin-crypto` via `wasm-bindgen` (`augecoin_crypto_bg.js` glue
  included). See `packages/sdk-ts` (`npm test`) and `crates/augecoin-node/tests/sdk_parity.rs`
  for the cross-language parity vectors.

See `.env.example` for the full configuration reference.
