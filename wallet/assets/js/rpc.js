// AUGECOIN Wallet — JSON-RPC client (single source of RPC access).

import { CONFIG } from './config.js';

let _id = 1;

export async function rpc(method, params = {}) {
  const res = await fetch(CONFIG.RPC_URL, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ jsonrpc: '2.0', id: _id++, method, params }),
  });
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  const body = await res.json();
  if (body && body.error) throw new Error(body.error.message || `RPC error ${body.error.code}`);
  return body && 'result' in body ? body.result : body;
}

// ── Queries ────────────────────────────────────────────────────────────

export const getAccount = (params) => rpc('getaccount', params);
export const getAccountByNumber = (n) => rpc('getaccount', { account_number: Number(n) });
export const resolveAddress = (address) => rpc('resolve_address', { address });
export const createAccount = (p) => rpc('createaccount', p);
export const getBlockCount = () => rpc('getblockcount');
export const getNodeStatus = () => rpc('nodestatus');
export const getBlock = (blockNumber) => rpc('getblock', { block_number: Number(blockNumber) });
export const getBlockOperations = (blockNumber) => rpc('getblockoperations', { block_number: Number(blockNumber) });
export const getOperations = (blockNumber, start = 0, limit = 50) => rpc('getoperations', { block_number: Number(blockNumber), start: Number(start), limit: Number(limit) });
export const getOperationByHash = (hashHex, maxBlocks = 10000) => rpc('getoperationbyhash', { hash_hex: String(hashHex), max_blocks: Number(maxBlocks) });
export const getPendings = () => rpc('getpendings');
export const findAccounts = (params = {}) => rpc('findaccounts', params);
export const listAccountsForSale = () => rpc('listaccountsforsale', {});
export const listValidatorInventory = (validatorPublicKeyHex) => rpc('listvalidatorinventory', { validator_public_key_hex: validatorPublicKeyHex });
export const listPendingGifts = (recipientPublicKeyHex) => rpc('listpendinggifts', { recipient_public_key_hex: recipientPublicKeyHex });

// ── Lifecycle operations (signed) ─────────────────────────────────────

export const sendOperation = (hex) => rpc('sendoperation', { hex });
export const buyAccount = (p) => rpc('buyaccount', p);
export const sellAccount = (p) => rpc('sellaccount', p);
export const giftAccount = (p) => rpc('giftaccount', p);
export const acceptGift = (p) => rpc('acceptgift', p);
export const cancelSale = (p) => rpc('cancelsale', p);

// ── Helpers ────────────────────────────────────────────────────────────

/** Resolve a name or AUGEID number to the on-chain account, or null. */
export async function resolveName(input) {
  const trimmed = String(input || '').trim();
  if (!trimmed) return null;
  if (/^\d+$/.test(trimmed)) {
    try { return await getAccountByNumber(trimmed); } catch { return null; }
  }
  const res = await findAccounts({ name: trimmed, max: 20 });
  const exact = res.accounts.find((a) => (a.name || '').toLowerCase() === trimmed.toLowerCase());
  return exact || res.accounts[0] || null;
}

export async function resolveDestination(input) {
  const trimmed = String(input || '').trim();
  if (/^AUGE-?\d+$/i.test(trimmed) || /^\d+$/.test(trimmed)) {
    return getAccountByNumber(trimmed.replace(/^AUGE-/i, ''));
  }
  return resolveAddress(trimmed);
}

/** True when no on-chain account currently holds `name` (case-insensitive). */
export async function isNameAvailable(name) {
  const trimmed = String(name || '').trim();
  if (!trimmed) return false;
  const res = await findAccounts({ name: trimmed, max: 20 });
  return !res.accounts.some((a) => (a.name || '').toLowerCase() === trimmed.toLowerCase());
}
