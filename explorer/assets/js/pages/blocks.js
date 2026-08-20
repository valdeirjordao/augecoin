// AUGECOIN Explorer — blocks list page (with pagination).

import { getBlockCount, getBlock } from '../api/client.js';
import { CONFIG } from '../config.js';
import { skeleton, pagination, badge } from '../components.js';
import { esc, shortHash, auge, fmtNum, timeAgo } from '../utils.js';

const PAGE_SIZE = 25;

export async function render() {
  const root = document.getElementById('page-root');
  const startPage = Math.max(1, parseInt(new URLSearchParams(location.search).get('page') || '1', 10));

  root.innerHTML = `
    <main class="container page">
      <div class="page-head">
        <h1>Blocks</h1>
        <p class="muted">All blocks on the AUGECOIN testnet.</p>
      </div>
      <div class="card table-card" id="blocks-table">${skeleton(8, 5)}</div>
      <div id="pagination"></div>
    </main>`;

  async function load(page) {
    const height = await getBlockCount();
    const total = height + 1;
    const pages = Math.max(1, Math.ceil(total / PAGE_SIZE));
    const p = Math.min(page, pages);
    const start = total - 1 - (p - 1) * PAGE_SIZE;
    const numbers = [];
    for (let i = start; i >= 0 && numbers.length < PAGE_SIZE; i--) numbers.push(i);

    const blocks = await Promise.all(numbers.map((n) => getBlock(n)));

    const host = document.getElementById('blocks-table');
    host.innerHTML = `
      <div class="table-wrap">
        <table class="table">
          <thead><tr>
            <th>Height</th><th>Hash (Merkle root)</th><th>Validator</th><th>Age</th><th>Reward</th><th>Fees</th>
          </tr></thead>
          <tbody>
            ${blocks.map((b) => `
              <tr>
                <td><a class="mono link" href="block.html?block=${esc(b.block_number)}">${fmtNum(b.block_number)}</a></td>
                <td><span class="mono hash" title="${esc(b.operations_hash_hex)}">${shortHash(b.operations_hash_hex, 14)}</span></td>
                <td><a class="link" href="account.html?id=${esc(b.leader_id)}">${badge('muted', 'v' + esc(b.leader_id))}</a></td>
                <td class="muted">${timeAgo(b.timestamp)}</td>
                <td>${auge(b.reward)} AUGE</td>
                <td>${auge(b.fee)} AUGE</td>
              </tr>`).join('')}
          </tbody>
        </table>
      </div>`;

    const pg = pagination({ page: p, pageSize: PAGE_SIZE, total, onChange: (np) => { load(np); window.scrollTo({ top: 0 }); }, label: 'blocks' });
    const pgHost = document.getElementById('pagination');
    pgHost.innerHTML = pg.html;
    pg.bind(pgHost);
  }

  await load(startPage);
}
