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
}
