// AUGECOIN Explorer — JSON-RPC client layer.
//
// Single source of truth for all network calls. Never duplicate RPC code in
// page scripts: import the functions you need from here.

import { CONFIG } from '../config.js';

let _id = 1;

/**
 * Low-level JSON-RPC 2.0 call with exponential-backoff retry.
 * @param {string} method
 * @param {object} [params]
 * @returns {Promise<any>} the JSON-RPC `result`.
 */
export async function rpc(method, params = {}) {
  const { RPC_URL, RPC_TIMEOUT_MS, RPC_RETRIES } = CONFIG;
  let lastError = null;

  for (let attempt = 1; attempt <= RPC_RETRIES; attempt++) {
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), RPC_TIMEOUT_MS);
    try {
      const res = await fetch(RPC_URL, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ jsonrpc: '2.0', id: _id++, method, params }),
        signal: controller.signal,
      });

      if (!res.ok) {
        throw new Error(`HTTP ${res.status} ${res.statusText}`);
      }

      const body = await res.json();

      if (body && body.error) {
        throw new Error(body.error.message || `RPC error ${body.error.code}`);
      }

      return body && 'result' in body ? body.result : body;
    } catch (e) {
      lastError = e;
      if (attempt === RPC_RETRIES) break;
      // Exponential backoff: 300ms, 900ms, ...
      await new Promise((r) => setTimeout(r, 300 * Math.pow(3, attempt - 1)));
    } finally {
      clearTimeout(timer);
    }
  }

  throw lastError || new Error(`rpc(${method}) failed`);
}

/** Batch several calls in parallel (still one HTTP request each). */
async function parallel(fns) {
  return Promise.all(fns.map((f) => f()));
}

// ── Node / status ──────────────────────────────────────────────────────

export function getNodeStatus() {
  return rpc('nodestatus');
}

export function getBlockCount() {
  return rpc('getblockcount');
}

export function getAccountCount() {
  return rpc('getaccountcount');
}

export function ping() {
  return rpc('ping');
}

// ── Blocks ─────────────────────────────────────────────────────────────

export function getBlock(blockNumber) {
  return rpc('getblock', { block_number: Number(blockNumber) });
}

export function getBlockOperations(blockNumber) {
  return rpc('getblockoperations', { block_number: Number(blockNumber) });
}

export function getOperations(blockNumber, start = 0, limit = null) {
  const params = { block_number: Number(blockNumber), start: Number(start) };
  if (limit != null) params.limit = Number(limit);
  return rpc('getoperations', params);
}

/** Latest N block headers (height, height-1, …). Skips missing blocks. */
export async function getLatestBlocks(n = CONFIG.RECENT_BLOCKS) {
  const height = await getBlockCount();
  const numbers = [];
  for (let i = 0; i < n && height - i >= 0; i++) numbers.push(height - i);
  const blocks = await parallel(numbers.map((bn) => () => getBlock(bn)));
  return blocks.map((b, i) => ({ ...b, height: numbers[i] }));
}

// ── Accounts ───────────────────────────────────────────────────────────

export function getAccount(params) {
  return rpc('getaccount', params);
}

export function getAccountByNumber(number) {
  return getAccount({ account_number: Number(number) });
}

export function getAccountByAddress(address) {
  return getAccount({ address });
}

export function findAccounts(params = {}) {
  return rpc('findaccounts', params);
}

/** Return the account that owns a given name, or null. */
export async function findAccountByName(name) {
  const res = await findAccounts({ name, start: 0, max: 10 });
  if (!res || !res.accounts || res.accounts.length === 0) return null;
  const target = name.toLowerCase();
  const hit = res.accounts.find((a) => a.name && a.name.toLowerCase() === target);
  return hit || res.accounts[0];
}

// ── Validators ─────────────────────────────────────────────────────────

export function getValidators() {
  return rpc('getvalidatorset');
}

// ── Mempool ────────────────────────────────────────────────────────────

export function getPendings() {
  return rpc('getpendings');
}

// ── Search helpers ─────────────────────────────────────────────────────

/**
 * Resolve a block by hash (header hash, operations merkle root, safe-box hash
 * or proof-of-work). Delegates to the node's server-side `getblockbyhash`.
 */
export function getBlockByHash(hashHex, maxBlocks) {
  return rpc('getblockbyhash', { hash_hex: hashHex, max_blocks: maxBlocks });
}

/**
 * Resolve an operation by its blake3-512 hash. Returns
 * `{ block_number, op_index, op_hash_hex, operation }` from the node's
 * server-side reverse scan (`getoperationbyhash`).
 */
export function getOperationByHash(hashHex, maxBlocks) {
  return rpc('getoperationbyhash', { hash_hex: hashHex, max_blocks: maxBlocks });
}
