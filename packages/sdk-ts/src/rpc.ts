/**
 * Typed JSON-RPC 2.0 client for AUGECOIN node.
 *
 * Method names and payloads match the node's RPC (crates/augecoin-rpc).
 */
import { serializeSignedOperation } from './transaction';
import type { Operation } from './transaction';

export interface RpcClientConfig {
  endpoint: string;
  apiKey?: string;
}

export interface AccountInfo {
  account_number: number;
  balance: number;
  n_operation: number;
  name: string | null;
  account_type: number;
  account_key_ed_hex: string;
  address: string;
}

export interface SendOperationResult {
  accepted: boolean;
  op_hash_hex: string;
  error: string | null;
}

export interface NodeStatus {
  current_height: number;
  latest_block_hash_hex: string;
  peers_connected: number;
  syncing: boolean;
  sync_target_height: number;
  total_accounts: number;
  total_blocks: number;
  mempool_size: number;
  current_round: number;
  current_view: number;
  validator_id: number;
  chain_id: number;
  uptime_seconds: number;
  last_consensus_error: string | null;
}

// ── Smart contract RPC types (Fase 16) ────────────────────────────────────

export interface ContractStoreCodeParams {
  wasm_hex: string;
  sender_hex: string;
  gas_limit_hex?: string | null;
}

export interface ContractStoreCodeResult {
  accepted: boolean;
  code_id_hex: string | null;
  op_hash_hex: string | null;
  error: string | null;
}

export interface ContractCreateParams {
  code_id_hex: string;
  instantiate_hex: string;
  sender_hex: string;
  gas_limit_hex?: string | null;
}

export interface ContractCreateResult {
  accepted: boolean;
  contract_id_hex: string | null;
  op_hash_hex: string | null;
  error: string | null;
}

export interface ContractExecuteParams {
  contract_id_hex: string;
  call_hex: string;
  sender_hex: string;
  gas_limit_hex?: string | null;
}

export interface ContractExecuteResult {
  success: boolean;
  gas_used: number | null;
  op_hash_hex: string | null;
  events: string[] | null;
  error: string | null;
}

export interface ContractGetResult {
  exists: boolean;
  owner_hex?: string;
  code_id_hex?: string;
  code_hash_hex?: string;
  created_height?: number;
  is_auge20?: boolean;
}

export interface ContractBalanceResult {
  contract_id_hex: string;
  address_hex: string;
  balance: number;
}

export interface TokenInfoView {
  name: string;
  symbol: string;
  decimals: number;
  total_supply: number;
  max_supply: number;
  owner: number;
  mint_enabled: boolean;
  burn_enabled: boolean;
}

export interface ContractTokenInfoResult {
  token: TokenInfoView | null;
  error?: string;
}

export class RpcClient {
  private endpoint: string;
  private apiKey?: string;
  private nextId = 1;

  constructor(config: RpcClientConfig) {
    this.endpoint = config.endpoint;
    this.apiKey = config.apiKey;
  }

  private async call<T>(method: string, params: unknown): Promise<T> {
    const headers: Record<string, string> = { 'Content-Type': 'application/json' };
    if (this.apiKey) headers['x-api-key'] = this.apiKey;

    const response = await fetch(this.endpoint, {
      method: 'POST',
      headers,
      body: JSON.stringify({ jsonrpc: '2.0', method, params, id: this.nextId++ }),
    });

    if (!response.ok) {
      throw new Error(`RPC error: ${response.status} ${response.statusText}`);
    }
    const data = await response.json();
    if (data.error) throw new Error(`RPC error: ${data.error.message}`);
    return data.result as T;
  }

  async getAccount(accountNumber: number): Promise<AccountInfo> {
    return this.call<AccountInfo>('getaccount', { account_number: accountNumber });
  }

  async getBlockCount(): Promise<number> {
    return this.call<number>('getblockcount', {});
  }

  async getAccountCount(): Promise<number> {
    return this.call<number>('getaccountcount', {});
  }

  async getNodeStatus(): Promise<NodeStatus> {
    return this.call<NodeStatus>('nodestatus', {});
  }

  /** Submit a signed operation (hex-encoded full operation bytes). */
  async sendOperation(op: Operation, signature: Uint8Array): Promise<SendOperationResult> {
    const bytes = serializeSignedOperation(op, signature);
    const hex = Buffer.from(bytes).toString('hex');
    return this.call<SendOperationResult>('sendoperation', { hex });
  }

  async getPendings(): Promise<{ size: number; operations: unknown[] }> {
    return this.call('getpendings', {});
  }

  async getValidatorSet(): Promise<unknown> {
    return this.call('getvalidatorset', {});
  }

  // ── Smart contract RPC methods (Fase 16) ─────────────────────────────────

  async contractStoreCode(
    wasmHex: string,
    senderHex: string,
    gasLimitHex?: string
  ): Promise<ContractStoreCodeResult> {
    return this.call('contract_store_code', {
      wasm_hex: wasmHex,
      sender_hex: senderHex,
      gas_limit_hex: gasLimitHex ?? null,
    });
  }

  async contractCreate(
    codeIdHex: string,
    instantiateHex: string,
    senderHex: string,
    gasLimitHex?: string
  ): Promise<ContractCreateResult> {
    return this.call('contract_create', {
      code_id_hex: codeIdHex,
      instantiate_hex: instantiateHex,
      sender_hex: senderHex,
      gas_limit_hex: gasLimitHex ?? null,
    });
  }

  async contractExecute(
    contractIdHex: string,
    callHex: string,
    senderHex: string,
    gasLimitHex?: string
  ): Promise<ContractExecuteResult> {
    return this.call('contract_execute', {
      contract_id_hex: contractIdHex,
      call_hex: callHex,
      sender_hex: senderHex,
      gas_limit_hex: gasLimitHex ?? null,
    });
  }

  async contractGet(contractIdHex: string): Promise<ContractGetResult> {
    return this.call('contract_get', { contract_id_hex: contractIdHex });
  }

  async contractBalance(contractIdHex: string, addressHex: string): Promise<ContractBalanceResult> {
    return this.call('contract_balance', {
      contract_id_hex: contractIdHex,
      address_hex: addressHex,
    });
  }

  async contractTokenInfo(contractIdHex: string): Promise<ContractTokenInfoResult> {
    return this.call('contract_token_info', { contract_id_hex: contractIdHex });
  }
}
