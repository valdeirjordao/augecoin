// AUGECOIN Explorer — search results page (fallback when a query can't be
// auto-routed, e.g. an unindexed hash).

import { getBlockByHash, getOperationByHash } from '../api/client.js';
import { esc, shortHash, fmtNum, timeAgo } from '../utils.js';
import { skeleton } from '../components.js';
import { CONFIG } from '../config.js';

export async function render() {
  const root = document.getElementById('page-root');
  const q = new URLSearchParams(location.search).get('q') || '';

  root.innerHTML = `
    <main class="container page">
      <div class="page-head">
        <h1>Search results</h1>
        <p class="muted">Query: <span class="mono">${esc(q)}</span></p>
      </div>
      <div id="search-results">${skeleton(5, 4)}</div>
    </main>`;

  if (!q) {
    document.getElementById('search-results').innerHTML = '<div class="empty-state"><h3>Enter a search term</h3></div>';
    return;
  }

  const results = [];

  // 1. block hash / operation hash lookup
  if (/^[0-9a-fA-F]{64,128}$/.test(q)) {
    const block = await getBlockByHash(q).catch(() => null);
    if (block) {
      results.push({ title: `Block #${block.block_number}`, desc: 'matches hash', href: `block.html?block=${block.block_number}` });
    } else {
      const op = await getOperationByHash(q).catch(() => null);
      if (op) results.push({ title: `Operation ${op.block_number}·${op.op_index}`, desc: 'matches hash', href: `transaction.html?block=${op.block_number}&op=${op.op_index}` });
    }
  }

  // 2. numeric → block link hint
  if (/^\d+$/.test(q)) {
    results.push({ title: `Block #${q}`, desc: 'view block', href: `block.html?block=${q}` });
    results.push({ title: `Account ${q}`, desc: 'view account', href: `account.html?id=${q}` });
  }

  if (!results.length) {
    document.getElementById('search-results').innerHTML = `
      <div class="empty-state">
        <h3>No results for "${esc(q)}"</h3>
        <p>Try a block height, an account number/name, a Base58 address, or a block hash.</p>
      </div>`;
    return;
  }

  document.getElementById('search-results').innerHTML = `
    <div class="card">
      ${results.map((r) => `<a class="result-row" href="${esc(r.href)}"><div><div class="result-title">${r.title}</div><div class="muted">${esc(r.desc)}</div></div><span class="link">→</span></a>`).join('')}
    </div>`;
}
