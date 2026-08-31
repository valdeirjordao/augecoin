/**
 * Smart contract abstractions for the AUGECOIN TypeScript SDK (Fase 16).
 *
 * Encoders here replicate the exact big-endian binary layout produced by
 * `augecoin_contracts::{TokenInit,TokenCall}::to_bytes` (see
 * crates/augecoin-contracts/src/codec.rs). Do not change the byte layout:
 * it must stay wire-compatible with the WASM runtime on the node.
 */
import type { RpcClient, ContractExecuteResult, ContractStoreCodeResult, ContractCreateResult } from './rpc';
import {
  bytesToHex,
  hexToBytes,
  u64ToBeHex,
  codeIdToBeHex,
  addressToBeHex,
} from './hex';

export type Address = number;
export type CodeId = number;

/** AUGE20 token instantiation parameters (mirrors `augecoin_contracts::TokenInit`). */
export interface TokenInit {
  name: string;
  symbol: string;
  decimals: number;
  initialSupply: bigint | number;
  maxSupply: bigint | number;
  mintEnabled: boolean;
  burnEnabled: boolean;
}

/** AUGE20 action (mirrors `augecoin_contracts::TokenCall`). */
export type TokenCall =
  | { kind: 'transfer'; to: Address; amount: bigint | number }
  | { kind: 'mint'; to: Address; amount: bigint | number }
  | { kind: 'burn'; amount: bigint | number };

/**
 * Typed view of `augecoin_contracts::ContractOp` (the on-chain operation
 * envelope). The node serializes this internally when a {@link ContractClient}
 * write call is admitted to the mempool, so SDK consumers only need the
 * decoded shape for inspection/logging.
 */
export type ContractOp =
  | { opType: 'store_code'; wasmCode: Uint8Array }
  | { opType: 'create_contract'; codeId: CodeId; instantiateData: Uint8Array }
  | { opType: 'execute_contract'; contractId: string; callData: Uint8Array };

/** Low-level deterministic writer matching crates/augecoin-contracts/src/codec.rs. */
class Writer {
  private chunks: Uint8Array[] = [];
  private total = 0;

  u8(v: number): this {
    this.push(new Uint8Array([v & 0xff]));
    return this;
  }

  u32(v: number): this {
    const b = new Uint8Array(4);
    new DataView(b.buffer).setUint32(0, v, false);
    this.push(b);
    return this;
  }

  u64(v: bigint | number): this {
    const b = new Uint8Array(8);
    new DataView(b.buffer).setBigUint64(0, BigInt(v), false);
    this.push(b);
    return this;
  }

  bool(v: boolean): this {
    return this.u8(v ? 1 : 0);
  }

  bytes(v: Uint8Array): this {
    this.u32(v.length);
    this.push(v);
    return this;
  }

  string(v: string): this {
    return this.bytes(new TextEncoder().encode(v));
  }

  private push(b: Uint8Array): void {
    this.chunks.push(b);
    this.total += b.length;
  }

  toBytes(): Uint8Array {
    const out = new Uint8Array(this.total);
    let off = 0;
    for (const c of this.chunks) {
      out.set(c, off);
      off += c.length;
    }
    return out;
  }
}

/** Encode a {@link TokenInit} to the on-chain instantiation payload (hex-ready). */
export function encodeTokenInit(init: TokenInit): Uint8Array {
  const w = new Writer();
  w.u8(init.decimals & 0xff);
  w.bool(init.mintEnabled);
  w.bool(init.burnEnabled);
  w.u64(init.initialSupply);
  w.u64(init.maxSupply);
  w.string(init.name);
  w.string(init.symbol);
  return w.toBytes();
}

/** Encode a {@link TokenCall} to the on-chain execute payload (hex-ready). */
export function encodeTokenCall(call: TokenCall): Uint8Array {
  const w = new Writer();
  switch (call.kind) {
    case 'transfer':
      w.u8(0);
      w.u64(call.to);
      w.u64(call.amount);
      break;
    case 'mint':
      w.u8(1);
      w.u64(call.to);
      w.u64(call.amount);
      break;
    case 'burn':
      w.u8(2);
      w.u64(call.amount);
      break;
  }
  return w.toBytes();
}

export interface ContractClientOptions {
  /** Gas limit hex (optional; defaults to chain DEFAULT_GAS_LIMIT). */
  gasLimitHex?: string;
}

/**
 * High-level client for deploying and invoking WASM smart contracts.
 *
 * Write operations (storeCode/create/execute) require the node admin API key,
 * passed either via {@link RpcClientConfig.apiKey} or explicitly per call. The
 * key is forwarded as `sender_hex`, which the node validates against its
 * configured `AUGECOIN_ADMIN_API_KEYS`.
 */
export class ContractClient {
  constructor(
    private readonly rpc: RpcClient,
    private readonly apiKey: string = '',
    private readonly sender: Address = 1
  ) {}

  /** Store raw WASM bytecode. Returns the assigned code id (hex). */
  async storeCode(wasmHex: string, options: ContractClientOptions = {}): Promise<ContractStoreCodeResult> {
    return this.rpc.contractStoreCode(wasmHex, addressToBeHex(this.sender), options.gasLimitHex);
  }

  /** Instantiate an AUGE20 token from {@link TokenInit}. Returns the contract id (hex). */
  async create(
    codeId: CodeId,
    init: TokenInit,
    options: ContractClientOptions = {}
  ): Promise<ContractCreateResult> {
    const instantiateHex = bytesToHex(encodeTokenInit(init));
    return this.rpc.contractCreate(codeIdToBeHex(codeId), instantiateHex, addressToBeHex(this.sender), options.gasLimitHex);
  }

  /** Execute an arbitrary {@link TokenCall} (or any pre-encoded hex payload) on a contract. */
  async execute(
    contractIdHex: string,
    call: TokenCall | string,
    options: ContractClientOptions = {}
  ): Promise<ContractExecuteResult> {
    const callHex = typeof call === 'string' ? call : bytesToHex(encodeTokenCall(call));
    return this.rpc.contractExecute(contractIdHex, callHex, addressToBeHex(this.sender), options.gasLimitHex);
  }

  async get(contractIdHex: string) {
    return this.rpc.contractGet(contractIdHex);
  }

  async balanceOf(contractIdHex: string, address: Address): Promise<bigint> {
    const res = await this.rpc.contractBalance(contractIdHex, addressToBeHex(address));
    return BigInt(res.balance);
  }

  async tokenInfo(contractIdHex: string) {
    return this.rpc.contractTokenInfo(contractIdHex);
  }
}

/**
 * Convenience client for AUGE20 token contracts. Builds {@link TokenCall}
 * payloads and reads token state via {@link ContractClient}.
 */
export class TokenClient {
  constructor(
    private readonly rpc: RpcClient,
    private readonly apiKey: string = '',
    private readonly sender: Address = 1
  ) {}

  async send(contractIdHex: string, to: Address, amount: bigint | number, options: ContractClientOptions = {}): Promise<ContractExecuteResult> {
    return this.execute(contractIdHex, { kind: 'transfer', to, amount }, options);
  }

  async mint(contractIdHex: string, to: Address, amount: bigint | number, options: ContractClientOptions = {}): Promise<ContractExecuteResult> {
    return this.execute(contractIdHex, { kind: 'mint', to, amount }, options);
  }

  async burn(contractIdHex: string, amount: bigint | number, options: ContractClientOptions = {}): Promise<ContractExecuteResult> {
    return this.execute(contractIdHex, { kind: 'burn', amount }, options);
  }

  async balanceOf(contractIdHex: string, address: Address): Promise<bigint> {
    const res = await this.rpc.contractBalance(contractIdHex, addressToBeHex(address));
    return BigInt(res.balance);
  }

  /** Total supply of the token (from `contract_token_info`). */
  async totalSupply(contractIdHex: string): Promise<bigint> {
    const info = await this.rpc.contractTokenInfo(contractIdHex);
    if (!info.token) throw new Error(info.error ?? 'contract is not an AUGE20 token');
    return BigInt(info.token.total_supply);
  }

  async getInfo(contractIdHex: string) {
    return this.rpc.contractTokenInfo(contractIdHex);
  }

  private execute(contractIdHex: string, call: TokenCall, options: ContractClientOptions): Promise<ContractExecuteResult> {
    const client = new ContractClient(this.rpc, this.apiKey, this.sender);
    return client.execute(contractIdHex, call, options);
  }
}

// Re-export decoder helpers for callers that need to inspect raw payloads.
export { hexToBytes };
