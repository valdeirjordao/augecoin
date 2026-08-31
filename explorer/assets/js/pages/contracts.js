// AUGECOIN Explorer — smart contracts page (AUGE20 token registry).

import { listAuge20Contracts } from '../api/client.js';
import { skeleton, pagination, badge, copyButton, bindCopyButtons, errorState, emptyState } from '../components.js';
import { esc, shortHash, auge, fmtNum } from '../utils.js';

const PAGE_SIZE = 25;

export async function render() {
  const root = document.getElementById('page-root');
  root.innerHTML = `
    <main class="container container-wide page">
      <div class="page-head">
        <h1>Smart Contracts</h1>
        <p class="muted">AUGE20 token contracts deployed on the AUGECOIN network.</p>
      </div>
      <div class="card table-card" id="contracts-table">${skeleton(8, 7)}</div>
      <div id="pagination"></div>
    </main>`;

  try {
    await renderPage(1);
  } catch (err) {
    document.getElementById('contracts-table').innerHTML = errorState(err.message);
  }
}

async function renderPage(page) {
  const res = await listAuge20Contracts({ start: (page - 1) * PAGE_SIZE, limit: PAGE_SIZE });
  const contracts = res.contracts || [];
  const total = Number(res.total || 0);

  if (contracts.length === 0 && page === 1) {
    document.getElementById('contracts-table').innerHTML = emptyState('No contracts yet', 'AUGE20 tokens deployed on the network will appear here.');
    document.getElementById('pagination').innerHTML = '';
    return;
  }

  const rows = contracts.map((c, i) => {
    const rank = (page - 1) * PAGE_SIZE + i + 1;
    const ownerNum = parseInt(c.owner_hex, 16);
    const supply = auge(c.total_supply, { decimals: c.decimals, fixed: 2 });
    const max = auge(c.max_supply, { decimals: c.decimals, fixed: 2 });
    return `
      <tr>
        <td class="muted">${rank}</td>
        <td>
          <a class="mono link" href="contract.html?id=${esc(c.contract_id_hex)}">${shortHash(c.contract_id_hex, 8)}</a>
          ${copyButton(c.contract_id_hex, 'sm')}
        </td>
        <td><a class="link" href="contract.html?id=${esc(c.contract_id_hex)}"><strong>${esc(c.symbol)}</strong></a></td>
        <td>${esc(c.name)}</td>
        <td class="muted">${c.decimals}</td>
        <td class="mono">${esc(supply)} <span class="muted">${esc(c.symbol)}</span></td>
        <td class="mono">${esc(max)}</td>
        <td><a class="link" href="account.html?id=${ownerNum}">${fmtNum(ownerNum)}</a></td>
        <td>${c.mint_enabled ? badge('success', 'Mint') : badge('muted', 'No mint')}</td>
        <td>${c.burn_enabled ? badge('warning', 'Burn') : badge('muted', 'No burn')}</td>
        <td class="muted">${fmtNum(c.created_height)}</td>
      </tr>`;
  });

  document.getElementById('contracts-table').innerHTML = `
    <div class="table-wrap">
      <table class="table">
        <thead>
          <tr>
            <th>#</th><th>Contract</th><th>Symbol</th><th>Name</th><th>Dec</th>
            <th>Supply</th><th>Max</th><th>Owner</th><th>Mint</th><th>Burn</th><th>Height</th>
          </tr>
        </thead>
        <tbody>${rows.join('')}</tbody>
      </table>
    </div>`;

  const pg = pagination({
    page,
    pageSize: PAGE_SIZE,
    total,
    onChange: (np) => { renderPage(np); window.scrollTo({ top: 0 }); },
    label: 'contracts',
  });
  const host = document.getElementById('pagination');
  host.innerHTML = pg.html;
  pg.bind(host);
  bindCopyButtons(document.getElementById('contracts-table'));
}
