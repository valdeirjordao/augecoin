# AUGECOIN Technical Architecture

## Slide 1: System Scope

AUGECOIN is a Rust workspace blockchain implementation with TypeScript/WASM
SDKs, wallet applications, validator operations services, and browser-facing
administrative tooling.

The architecture separates protocol execution from operational and user
interface layers.

## Slide 2: Workspace Layers

| Layer | Main components | Responsibility |
|---|---|---|
| Cryptography | `augecoin-crypto`, `augecoin-core` | Keys, signatures, hashes, wire types, shared protocol rules |
| State and consensus | `augecoin-storage`, `augecoin-consensus`, `augecoin-node` | RocksDB state, validator primitives, block lifecycle |
| Connectivity | `augecoin-network`, `augecoin-rpc` | P2P transport, RPC API, request admission |
| Contracts | `augecoin-contracts`, `augecoin-wasm-runtime`, `augecoin-auge20-wasm` | Contract engine, sandbox, built-in token contract |
| External clients | SDK, wallets, CLI, Tauri apps | Local signing, RPC access, operator workflows |
| Operations | `augecoin-ops`, `operacional/`, `infra/`, `scripts/` | Licenses, validator monitoring, deployment and observability |

## Slide 3: Transaction Path

```text
Wallet / SDK / CLI
  -> signed Operation
  -> RPC sendoperation or P2P operation gossip
  -> Mempool::validate_and_admit
  -> ConsensusEngine::build_block
  -> proposal, validation, votes and quorum
  -> execute_block
  -> Storage::commit_block_atomic_with_contracts
```

`Operation` is the active production transaction envelope. The legacy
`Transaction` type is not the node execution path.

## Slide 4: Mempool And Admission

- `Mempool::validate_and_admit` checks chain ID, duplicate hash/nonce,
  signatures, nonce order, balance, and minimum fee.
- `precheck_for_block` revalidates selected operations before proposal.
- `remove_committed` removes finalized operations after successful execution.
- RPC and P2P inbound operations converge at the same mempool boundary.

## Slide 5: Block Lifecycle

```text
leader_for
  -> ConsensusEngine::build_block
  -> light/full proposal broadcast
  -> accept_proposal: pre-state check and dry execution
  -> validator votes
  -> quorum verification
  -> execute_block atomically
  -> CommitNotification and follower execution
```

`ValidatorSet` and quorum primitives live in `augecoin-consensus`. The active
runtime state machine is `NodePhase` in `augecoin-node/src/main.rs`.

## Slide 6: State And Persistence

`augecoin-storage` persists the canonical chain state in RocksDB column
families for accounts, blocks, SafeBox, operation/address indexes, validator
set, equivocation proofs, faucet claims, and contract records.

One `WriteBatch` commits changed accounts, the block, height, validator state,
and contract writes. `consensus_state_root` covers account, validator, and
contract state.

## Slide 7: Fees And Rewards

- `MIN_FEE_AUGESAT` defines the minimum operation fee.
- Mempool admission validates affordability and fee policy.
- Block execution aggregates operation fees.
- Contract execution reserves gas affordability and charges actual gas used.
- The block leader receives `block_reward(height) + total_fees` during block
  execution.

Protocol constants must be verified from source before changing any economic
rule.

## Slide 8: P2P And RPC

P2P uses libp2p TCP, Noise, Yamux, Gossipsub, Identify, Kademlia, peer
exchange, and per-peer unicast topics.

RPC dispatches account, block, operation, validator, lifecycle, and contract
methods. State-changing requests become signed operations, pass through the
mempool, and only change chain state after consensus execution. Faucet claim
rate-limit records are the exception: they are persisted immediately.

## Slide 9: AUGE20 Creation Path

```text
Wallet / SDK
  -> ContractOp::CreateContract
  -> mempool fee and gas-reserve validation
  -> execute_operation
  -> ContractEngine::create_contract
  -> AUGE20 built-in WASM (code ID 1)
  -> sandboxed host state writes
  -> atomic contract-state commit
```

The AUGE20 contract supports initialization, transfer, mint, burn, balance,
total-supply, and token-info queries.

## Slide 10: WASM Security Boundary

The runtime accepts only the `auge` host module. Its callable host functions
provide authenticated sender, block height, contract ID, scoped storage
read/write/delete, and event emission.

The runtime validates WASM size, imports, and memory limits; execution is
fuel-metered. Contract writes are buffered and applied only after successful
execution.

## Slide 11: Token Metadata

The current implementation has no global AUGE20 token-name or ticker/symbol
uniqueness validation.

Deployment deterministically derives a contract ID from owner, code ID, and
initialization bytes, and rejects only a duplicate contract ID. The account
`name_index` is unrelated to token metadata.

Any future uniqueness feature spans the contract store/engine, AUGE20 WASM,
node execution, storage/state-root logic, RPC clients, SDKs, wallets, and
tests.

## Slide 12: Critical Hubs

| Classification | Components |
|---|---|
| Critical hub | `Storage`, `Mempool`, `ConsensusEngine`, `ContractEngine`, `AppState`, `execute_operation` |
| High impact | `Operation`, `OperationBlock`, `ValidatorSet`, `ContractOverlay`, `WasmHost`, P2P transport, `execute_block` |
| Medium impact | Crypto, RPC handlers, SafeBox/state root, SDK and CLI serializers |
| Low impact | UI rendering, dashboards, standalone scripts, benchmarks |

## Slide 13: Protocol Change Discipline

Consensus, transactions, fees, rewards, tokens, cryptography, storage, network,
and genesis changes require impact analysis, source verification, an explicit
implementation plan, tests, and regression review.

No protocol rule should be changed based only on documentation or an inferred
relationship. The implementation and its tests must confirm the actual
behavior.

## Slide 14: Current Documentation Risk

The current implementation and `docs/SPEC.md`/ADR-015 differ materially on
block interval, block reward, emission-block count, and hard-cap values.

This presentation describes the implemented structure. It does not resolve or
modify the discrepancy. Protocol changes require explicit review, source
verification, tests, and the project decision process.

## References

- `docs/PROJECT_RULES.md`
- `docs/SPEC.md`
- `docs/DECISIONS.md`
