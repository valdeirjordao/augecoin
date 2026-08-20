// AUGECOIN Explorer — rich list page (top 100 accounts by balance).

import { findAccounts, getAccountCount } from '../api/client.js';
import { CONFIG } from '../config.js';
import { skeleton, pagination } from '../components.js';
import { esc, auge, fmtNum, deriveAddress } from '../utils.js';

const PAGE_SIZE = 25;

export async function render() {
  const root = document.getElementById('page-root');
  root.innerHTML = `
    <main class="container page">
      <div class="page-head"><h1>Rich List</h1><p class="muted">Top ${CONFIG.RICH_LIST_MAX} accounts by balance.</p></div>
      <div class="card table-card" id="richlist-table">${skeleton(8, 5)}</div>
      <div id="pagination"></div>
    </main>`;

  try {
    const accounts = await fetchAllAccounts(CONFIG.RICH_LIST_MAX);
    const sorted = accounts.sort((a, b) => Number(b.balance) - Number(a.balance)).slice(0, CONFIG.RICH_LIST_MAX);
    const totalSupply = CONFIG.TOTAL_SUPPLY_AUGE * CONFIG.AUGESAT_PER_AUGE;

    await renderPage(sorted, 1, totalSupply);
  } catch (err) {
    document.getElementById('richlist-table').innerHTML = `<div class="empty-state error"><h3>Failed to load accounts</h3><p>${esc(err.message)}</p></div>`;
  }
}

async function fetchAllAccounts(limit) {
  const out = [];
  let start = 0;
  while (out.length < limit) {
    const res = await findAccounts({ start, max: CONFIG.FIND_ACCOUNTS_PAGE_SIZE });
    if (!res.accounts || !res.accounts.length) break;
    out.push(...res.accounts);
    if (res.accounts.length < CONFIG.FIND_ACCOUNTS_PAGE_SIZE) break;
    start += res.accounts.length;
    if (start >= 100000) break; // safety
  }
  return out;
}

async function renderPage(sorted, page, totalSupply) {
  const total = sorted.length;
  const pages = Math.max(1, Math.ceil(total / PAGE_SIZE));
  const p = Math.min(page, pages);
  const slice = sorted.slice((p - 1) * PAGE_SIZE, p * PAGE_SIZE);

  const rows = [];
  for (const acc of slice) {
    const pct = (Number(acc.balance) / totalSupply) * 100;
    const rank = (p - 1) * PAGE_SIZE + rows.length + 1;
    rows.push(`
      <tr>
        <td class="muted">${rank}</td>
        <td><a class="mono link" href="account.html?id=${esc(acc.account_number)}">${fmtNum(acc.account_number)}</a></td>
        <td>${acc.name ? esc(acc.name) : '<span class="muted">—</span>'}</td>
        <td class="mono">${auge(acc.balance)} AUGE</td>
        <td class="muted">${pct.toFixed(4)}%</td>
      </tr>`);
  }

  document.getElementById('richlist-table').innerHTML = `
    <div class="table-wrap">
      <table class="table">
        <thead><tr><th>#</th><th>AUGEID</th><th>Name</th><th>Balance</th><th>% of supply</th></tr></thead>
        <tbody>${rows.join('')}</tbody>
      </table>
    </div>`;

  const pg = pagination({ page: p, pageSize: PAGE_SIZE, total, onChange: (np) => { renderPage(sorted, np, totalSupply); window.scrollTo({ top: 0 }); }, label: 'accounts' });
  const host = document.getElementById('pagination');
  host.innerHTML = pg.html;
  pg.bind(host);
}
