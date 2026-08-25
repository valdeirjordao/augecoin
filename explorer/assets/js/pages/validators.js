// AUGECOIN Explorer — validators page.

import { getValidators, getNodeStatus, getLatestBlocks } from '../api/client.js';
import { skeleton, badge, statusDot } from '../components.js';
import { esc, shortHash, fmtNum, timeAgo } from '../utils.js';
import { barChart, registerChartsForTheme } from '../charts.js';
import { createPoller } from '../realtime.js';
import { CONFIG } from '../config.js';

export async function render() {
  const root = document.getElementById('page-root');
  root.innerHTML = `
    <main class="container page">
      <div class="page-head"><h1>Validators</h1><p class="muted">Active validator set.</p></div>
      <div class="card table-card" id="validators-table">${skeleton(6, 5)}</div>
      <div class="section">
        <div class="section-head"><h2>Block production</h2><span class="muted">last ${CONFIG.RECENT_BLOCKS} blocks</span></div>
        <div class="card chart-card"><canvas id="prod-chart" height="220"></canvas></div>
      </div>
    </main>`;

  let refresh = async () => {};

  try {
    const [vs, status] = await Promise.all([getValidators(), getNodeStatus()]);
    const active = vs.active || [];

    const table = document.getElementById('validators-table');
    table.innerHTML = `
      <div class="table-wrap">
        <table class="table">
          <thead><tr><th>ID</th><th>Status</th><th>Public key</th><th>Peer ID</th></tr></thead>
          <tbody>
            ${active.map((v) => `
              <tr>
                <td><a class="mono link" href="account.html?id=${esc(v.id)}">${fmtNum(v.id)}</a></td>
                <td>${v.status === 'active' ? statusDot(true, 'Active') : badge('muted', esc(v.status))}</td>
                <td><span class="mono hash" title="${esc(v.ed25519_public_key_hex)}">${shortHash(v.ed25519_public_key_hex, 14)}</span></td>
                <td class="muted">—</td>
              </tr>`).join('')}
          </tbody>
        </table>
      </div>
      <p class="table-foot muted">${fmtNum(active.length)} active · quorum ${quorum(active.length)}/${fmtNum(active.length)}</p>`;

    refresh = async () => {
      const blocks = await getLatestBlocks(CONFIG.RECENT_BLOCKS);
      const counts = {};
      blocks.forEach((b) => { counts[b.leader_id] = (counts[b.leader_id] || 0) + 1; });
      const bars = active.map((v) => ({ label: 'v' + v.id, value: counts[v.id] || 0 }));
      barChart(document.getElementById('prod-chart'), bars);
    };

    await refresh();
    registerChartsForTheme([refresh]);
    const poller = createPoller(refresh, CONFIG.POLL_INTERVAL_MS * 2, false);
    window.addEventListener('beforeunload', () => poller.stop());
  } catch (err) {
    document.getElementById('validators-table').innerHTML = `<div class="empty-state error"><h3>Failed to load validators</h3><p>${esc(err.message)}</p></div>`;
  }
}

function quorum(n) {
  return Math.floor(n * 2 / 3) + 1;
}
