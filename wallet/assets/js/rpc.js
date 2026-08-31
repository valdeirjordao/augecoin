// AUGECOIN Wallet — JSON-RPC client (single source of RPC access).

import { CONFIG } from './config.js';

let _id = 1;

// The node enforces a per-client rate limit. Under load (e.g. the dashboard's
// history backfill bursts hundreds of getBlockOperations calls) individual
// read queries — like contractBalance, which feeds the token list — can be
// throttled. Retry those transient rejections with a short backoff so a
// single throttled response never silently drops a balance/token lookup.
const RPC_MAX_RETRIES = 4;
const RPC_TIMEOUT_MS = 15_000;
const isRateLimit = (body) =>
  body && body.error && (body.error.code === -32001 || /rate limit/i.test(body.error.message || ''));

export async function rpc(method, params = {}) {
  let lastErr;
  for (let attempt = 0; attempt <= RPC_MAX_RETRIES; attempt++) {
    const controller = new AbortController();
    const timeout = setTimeout(() => controller.abort(), RPC_TIMEOUT_MS);
    let res;
    try {
      res = await fetch(CONFIG.RPC_URL, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ jsonrpc: '2.0', id: _id++, method, params }),
        signal: controller.signal,
      });
    } catch (err) {
      lastErr = new Error(err.name === 'AbortError' ? `RPC timeout after ${RPC_TIMEOUT_MS / 1000}s` : err.message);
      if (attempt === RPC_MAX_RETRIES) throw lastErr;
      await new Promise((r) => setTimeout(r, 250 * (attempt + 1)));
      continue;
    } finally {
      clearTimeout(timeout);
    }
    if (!res.ok) {
      lastErr = new Error(`HTTP ${res.status}`);
      if (res.status === 429 || attempt === RPC_MAX_RETRIES) throw lastErr;
      await new Promise((r) => setTimeout(r, 250 * (attempt + 1)));
      continue;
    }
    const body = await res.json();
    if (isRateLimit(body)) {
      lastErr = new Error(body.error.message || 'rate limit exceeded');
      if (attempt === RPC_MAX_RETRIES) throw lastErr;
      await new Promise((r) => setTimeout(r, 250 * (attempt + 1)));
      continue;
    }
    if (body && body.error) throw new Error(body.error.message || `RPC error ${body.error.code}`);
  return body && 'result' in body ? body.result : body;
  }
  throw lastErr || new Error('RPC failed');
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

/** Read-only: list every account whose owner key equals the given Ed25519
 *  public key (hex). Used to auto-discover accounts received via direct
 *  ChangeKey transfers (which never enter GiftPending). */
export const listAccountsByKey = (ed25519PublicKeyHex) => rpc('listaccountsbykey', { ed25519_public_key_hex: ed25519PublicKeyHex });

// ── Lifecycle operations (signed) ─────────────────────────────────────

export const sendOperation = (hex) => rpc('sendoperation', { hex });

// ── Smart contracts (consultas públicas) ───────────────────────────────

export const contractGet = (idHex) => rpc('contract_get', { contract_id_hex: idHex });
export const contractBalance = (idHex, addrHex) =>
  rpc('contract_balance', { contract_id_hex: idHex, address_hex: addrHex });
export const contractTokenInfo = (idHex) => rpc('contract_token_info', { contract_id_hex: idHex });
export async function contractTokenDetails(idHex) {
  const [contract, tokenInfo] = await Promise.all([
    contractGet(idHex),
    contractTokenInfo(idHex),
  ]);
  return { contract, token: tokenInfo?.token, error: tokenInfo?.error };
}
export const listAuge20 = (start = 0, limit = 100) =>
  rpc('contract_list_auge20', { start: Number(start), limit: Number(limit) });
export const contractEvents = (idHex, fromHeight = 0, maxEvents = 100) =>
  rpc('contract_events', {
    contract_id_hex: idHex,
    from_height: Number(fromHeight),
    max_events: Number(maxEvents),
  });
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
  const resolved = await resolveAddress(trimmed);
  // `resolve_address` returns `account`, whereas account lookups return
  // `account_number`. Keep a single shape for every wallet send screen.
  return resolved?.exists && Number.isInteger(resolved.account)
    ? { ...resolved, account_number: resolved.account }
    : resolved;
}

/** True when no on-chain account currently holds `name` (case-insensitive). */
export async function isNameAvailable(name) {
  const trimmed = String(name || '').trim();
  if (!trimmed) return false;
  const res = await findAccounts({ name: trimmed, max: 20 });
  return !res.accounts.some((a) => (a.name || '').toLowerCase() === trimmed.toLowerCase());
}
