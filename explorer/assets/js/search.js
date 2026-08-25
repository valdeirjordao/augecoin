// AUGECOIN Explorer — intelligent search.
//
// Detects the query type automatically (block height, hash, AUGEID, account
// name or Base58 address) and routes to the correct page without asking the
// user to choose.

import {
  getBlockCount, getAccountByNumber, getAccountByAddress, findAccountByName,
  getBlockByHash, getOperationByHash,
} from './api/client.js';
import { toast } from './components.js';

const HEX128 = /^[0-9a-fA-F]{128}$/;
const HEX64 = /^[0-9a-fA-F]{64}$/;
const HEX = /^[0-9a-fA-F]+$/;
const AUGEID = /^(\d+)-(\d+)$/;
const INT = /^\d+$/;

/**
 * Classify and route a search query.
 * @param {string} raw
 * @returns {Promise<{page:string, params:URLSearchParams}|null>}
 */
export async function routeQuery(raw) {
  const q = (raw || '').trim();
  if (!q) return null;

  const go = (page, params = {}) => {
    const sp = new URLSearchParams(params);
    return { page, params: sp };
  };

  // AUGEID "635842-1"
  const augeid = q.match(AUGEID);
  if (augeid) return go('account.html', { id: augeid[1] });

  // Base58 address
  if (/^[1-9A-HJ-NP-Za-km-z]{32,42}$/.test(q)) return go('account.html', { address: q });

  // 128-char hex → block hash (blake3-512) or operation hash
  if (HEX128.test(q)) {
    try {
      const block = await getBlockByHash(q);
      if (block) return go('block.html', { block: block.block_number });
    } catch { /* not a block hash */ }
    try {
      const op = await getOperationByHash(q);
      if (op) return go('transaction.html', { block: op.block_number, op: op.op_index });
    } catch { /* not an operation hash */ }
    return go('search.html', { q });
  }

  // 64-char hex → block hash (proof-of-work) or an account public key
  if (HEX64.test(q)) {
    try {
      const block = await getBlockByHash(q);
      if (block) return go('block.html', { block: block.block_number });
    } catch { /* not a block hash */ }
    return go('search.html', { q });
  }

  // Pure integer → block height (or account number fallback)
  if (INT.test(q)) {
    const n = Number(q);
    try {
      const height = await getBlockCount();
      if (n <= height) return go('block.html', { block: n });
    } catch { /* node unreachable — fall through */ }
    try {
      await getAccountByNumber(n);
      return go('account.html', { id: n });
    } catch { /* not an account */ }
    return go('search.html', { q });
  }

  // Hex (short) → assume it might be an account public key; try address-less
  if (HEX.test(q)) return go('search.html', { q });

  // Otherwise: account name
  try {
    const acc = await findAccountByName(q);
    if (acc) return go('account.html', { id: acc.account_number });
  } catch { /* ignore */ }
  return go('search.html', { q });
}

/** Perform the search and navigate. */
export async function performSearch(raw) {
  const target = await routeQuery(raw);
  if (!target) return;
  if (target.page === 'search.html') {
    location.href = `search.html?${target.params.toString()}`;
  } else {
    location.href = `${target.page}?${target.params.toString()}`;
  }
}

export function bindSearchForms() {
  document.addEventListener('submit', async (e) => {
    const form = e.target.closest('[data-search]');
    if (!form) return;
    e.preventDefault();
    const input = form.querySelector('input[name="q"]');
    const q = input ? input.value : '';
    if (!q.trim()) return;
    toast('Searching…', 'info');
    try {
      await performSearch(q);
    } catch (err) {
      toast('Search failed: ' + err.message, 'error');
    }
  });
}
