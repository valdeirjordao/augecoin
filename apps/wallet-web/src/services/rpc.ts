// AUGECOIN Wallet Web — JSON-RPC 2.0 client (single source of RPC access).

import { CONFIG } from './config';
import type {
  AccountInfo,
  InventoryResult,
  LifecycleOpResult,
  MarketplaceEntry,
  NodeStatus,
  PendingGiftEntry,
} from '../types';

let nextId = 1;

export class RpcError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'RpcError';
  }
}

async function call<T>(method: string, params: Record<string, unknown> = {}): Promise<T> {
  const res = await fetch(CONFIG.RPC_URL, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ jsonrpc: '2.0', id: nextId++, method, params }),
  });
  if (!res.ok) throw new RpcError(`HTTP ${res.status}`);
  const body = await res.json();
  if (body && body.error) {
    throw new RpcError(body.error.message || `RPC error ${body.error.code}`);
  }
  return (body && 'result' in body ? body.result : body) as T;
}

// ── Queries ────────────────────────────────────────────────────────────

export function getAccount(accountNumber: number): Promise<AccountInfo> {
  return call<AccountInfo>('getaccount', { account_number: accountNumber });
}

export function findAccounts(params: {
  name?: string;
  account_type?: number;
  min_balance?: number;
  max_balance?: number;
  start?: number;
  max?: number;
}): Promise<{ accounts: AccountInfo[]; total: number }> {
  return call('findaccounts', params);
}

export function listAccountsForSale(): Promise<{ entries: MarketplaceEntry[] }> {
  return call('listaccountsforsale', {});
}

export function listValidatorInventory(validatorPublicKeyHex: string): Promise<InventoryResult> {
  return call<InventoryResult>('listvalidatorinventory', {
    validator_public_key_hex: validatorPublicKeyHex,
  });
}

export function listPendingGifts(recipientPublicKeyHex: string): Promise<{ entries: PendingGiftEntry[] }> {
  return call('listpendinggifts', { recipient_public_key_hex: recipientPublicKeyHex });
}

export function getNodeStatus(): Promise<NodeStatus> {
  return call<NodeStatus>('nodestatus', {});
}

// ── Lifecycle operations (signed) ─────────────────────────────────────

export function buyAccount(params: {
  buyer_account: number;
  n_operation: number;
  account_to_purchase: number;
  amount: number;
  fee: number;
  new_public_key_hex: string;
  seller_account: number;
  signature_hex: string;
}): Promise<LifecycleOpResult> {
  return call<LifecycleOpResult>('buyaccount', params);
}

export function sellAccount(params: {
  account: number;
  n_operation: number;
  sale_price: number;
  account_to_pay: number;
  locked_until_block: number;
  fee: number;
  new_public_key_hex: string;
  signature_hex: string;
}): Promise<LifecycleOpResult> {
  return call<LifecycleOpResult>('sellaccount', params);
}

export function giftAccount(params: {
  account: number;
  n_operation: number;
  recipient_public_key_hex: string;
  fee: number;
  signature_hex: string;
}): Promise<LifecycleOpResult> {
  return call<LifecycleOpResult>('giftaccount', params);
}

export function acceptGift(params: {
  account: number;
  n_operation: number;
  fee: number;
  signature_hex: string;
}): Promise<LifecycleOpResult> {
  return call<LifecycleOpResult>('acceptgift', params);
}

export function cancelSale(params: {
  account: number;
  n_operation: number;
  fee: number;
  signature_hex: string;
}): Promise<LifecycleOpResult> {
  return call<LifecycleOpResult>('cancelsale', params);
}

export function sendOperation(hex: string): Promise<{ accepted: boolean; op_hash_hex: string; error: string | null }> {
  return call('sendoperation', { hex });
}

// ── Helpers ────────────────────────────────────────────────────────────

/** Resolve a name or AUGEID number to the on-chain account, or null. */
export async function resolveName(input: string): Promise<AccountInfo | null> {
  const trimmed = input.trim();
  if (!trimmed) return null;
  if (/^\d+$/.test(trimmed)) {
    try {
      return await getAccount(Number(trimmed));
    } catch {
      return null;
    }
  }
  const res = await findAccounts({ name: trimmed, max: 20 });
  const exact = res.accounts.find((a) => (a.name ?? '').toLowerCase() === trimmed.toLowerCase());
  return exact ?? res.accounts[0] ?? null;
}

/** True when no on-chain account currently holds `name` (case-insensitive). */
export async function isNameAvailable(name: string): Promise<boolean> {
  const trimmed = name.trim();
  if (!trimmed) return false;
  const res = await findAccounts({ name: trimmed, max: 20 });
  return !res.accounts.some((a) => (a.name ?? '').toLowerCase() === trimmed.toLowerCase());
}
