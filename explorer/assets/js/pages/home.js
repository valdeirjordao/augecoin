// AUGECOIN Explorer — home / dashboard page.

import { getNodeStatus, getLatestBlocks, getOperations, getValidators } from '../api/client.js';
import { CONFIG } from '../config.js';
import { card, badge, opBadge, skeleton, statusDot } from '../components.js';
import { esc, shortHash, auge, augeShort, fmtNum, timeAgo, qs } from '../utils.js';
import { createPoller, createTpsTracker } from '../realtime.js';

const RECENT_BLOCK_COUNT = 8;

function rewardInfo() {
  // Linear emission, no halving: reward stays 7.25 AUGE until block 105,119,999.
  return { label: 'Block reward', value: `${CONFIG.BLOCK_REWARD_AUGE} AUGE`, icon: 'coins' };
}

export async function render() {
  const root = document.getElementById('page-root');
  root.innerHTML = `
    <section class="hero">
      <div class="container hero-inner">
        <h1>AUGECOIN <span class="text-accent">Explorer</span></h1>
        <p class="hero-sub">Search blocks, transactions, AUGEIDs and accounts on the AUGECOIN testnet.</p>
        <form class="hero-search" data-search>
          <span class="hero-search-icon">🔍</span>
          <input name="q" type="text" autocomplete="off" placeholder="Buscar bloco, transação, AUGEID ou hash…" aria-label="Search" />
          <button class="btn btn-primary" type="submit">Search</button>
        </form>
        <div class="hero-stats" id="hero-stats">
          <div class="hero-stat"><span id="hero-height">—</span><label>Height</label></div>
          <div class="hero-stat"><span id="hero-peers">—</span><label>Peers</label></div>
          <div class="hero-stat"><span id="hero-accounts">—</span><label>Accounts</label></div>
          <div class="hero-stat"><span id="hero-tps">—</span><label>TPS</label></div>
        </div>
      </div>
    </section>

    <main class="container">
      <section class="grid-stats" id="stat-cards">
        ${skeleton(2, 4)}
      </section>

      <section class="section">
        <div class="section-head">
          <h2>Latest Blocks</h2>
          <a class="link" href="blocks.html">View all →</a>
        </div>
        <div class="card table-card" id="recent-blocks">${skeleton(6, 6)}</div>
      </section>

      <section class="section">
        <div class="section-head">
          <h2>Latest Operations</h2>
          <span class="muted">from recent blocks</span>
        </div>
        <div class="card table-card" id="recent-ops">${skeleton(6, 6)}</div>
      </section>
    </main>`;

  const tps = createTpsTracker();
  const poller = createPoller(refresh, CONFIG.POLL_INTERVAL_MS);

  async function refresh() {
    try {
      const [status, blocks, validators] = await Promise.all([
        getNodeStatus(),
        getLatestBlocks(RECENT_BLOCK_COUNT),
        getValidators(),
      ]);

      tps.sample(status.current_height);
      const { tps: tpsVal, avgBlockTime } = tps.compute();

      // hero quick stats
      setText('hero-height', fmtNum(status.current_height));
      setText('hero-peers', fmtNum(status.peers_connected));
      setText('hero-accounts', fmtNum(status.total_accounts));
      setText('hero-tps', tpsVal.toFixed(2));

      // stat cards
      const circulating = circulatingSupply(status.current_height);
      document.getElementById('stat-cards').innerHTML =
        card({ label: 'Total Supply', value: `${fmtNum(CONFIG.TOTAL_SUPPLY_AUGE)} AUGE`, icon: 'coins', sub: 'Hard cap' }) +
        card({ label: 'Circulating Supply', value: `${fmtNum(circulating)} AUGE`, icon: 'clock', sub: `${pctSupply(circulating)}% emitted` }) +
        card({ label: 'Last Block', value: `#${fmtNum(status.current_height)}`, icon: 'cube', sub: timeAgo(latestTimestamp(blocks)), href: 'block.html?block=' + status.current_height }) +
        card({ label: 'Validators', value: fmtNum(validators.total ?? validators.active.length), icon: 'users', sub: `${fmtNum(validators.active.length)} active`, href: 'validators.html' });

      renderRecentBlocks(blocks);
      await renderRecentOps(blocks);

      // secondary metrics row
      updateSecondary(status, tpsVal, avgBlockTime);
    } catch (err) {
      // keep skeleton; show subtle error
      const el = document.getElementById('recent-blocks');
      if (el && el.dataset.loaded !== '1') el.innerHTML = `<div class="empty-state error"><h3>RPC unreachable</h3><p>${esc(err.message)}</p></div>`;
    }
  }

  function renderRecentBlocks(blocks) {
    const el = document.getElementById('recent-blocks');
    el.dataset.loaded = '1';
    if (!blocks.length) { el.innerHTML = '<div class="empty-state"><h3>No blocks yet</h3></div>'; return; }
    el.innerHTML = `
      <div class="table-wrap">
        <table class="table">
          <thead><tr>
            <th>Height</th><th>Hash</th><th>Validator</th><th>Age</th><th>Reward</th><th>Size</th>
          </tr></thead>
          <tbody>
            ${blocks.map((b) => `
              <tr>
                <td><a class="mono link" href="block.html?block=${esc(b.block_number)}">${fmtNum(b.block_number)}</a></td>
                <td><span class="mono hash" title="${esc(b.operations_hash_hex)}">${shortHash(b.operations_hash_hex, 12)}</span></td>
                <td><a class="link" href="account.html?id=${esc(b.leader_id)}">${badge('muted', 'v' + esc(b.leader_id))}</a></td>
                <td class="muted">${timeAgo(b.timestamp)}</td>
                <td>${auge(b.reward, { symbol: false })} AUGE</td>
                <td class="muted">${fmtNum(blockSize(b))} B</td>
              </tr>`).join('')}
          </tbody>
        </table>
      </div>`;
  }

  async function renderRecentOps(blocks) {
    const el = document.getElementById('recent-ops');
    // Fetch only a small page from each recent block, in parallel. Full block
    // operation lists belong on the block detail page, not the dashboard.
    const responses = await Promise.all(blocks.map(async (b) => {
      // Empty blocks have an all-zero operations merkle root — skip them to
      // avoid a per-block RPC call.
      if (/^0+$/.test(b.operations_hash_hex || '')) return [];
      try {
        const ops = await getOperations(b.block_number, 0, CONFIG.RECENT_TRANSACTIONS);
        return ops.map((op, idx) => ({ ...op, block_number: b.block_number, index: idx, timestamp: b.timestamp }));
      } catch { return []; }
    }));
    const ops = responses.flat();
    if (!ops.length) { el.innerHTML = '<div class="empty-state"><h3>No operations yet</h3></div>'; return; }

    const recent = ops.slice(0, CONFIG.RECENT_TRANSACTIONS);
    el.innerHTML = `
      <div class="table-wrap">
        <table class="table">
          <thead><tr><th>Operation</th><th>Type</th><th>From</th><th>To</th><th>Amount</th><th>Fee</th><th>Block</th><th>Age</th></tr></thead>
          <tbody>
            ${recent.map((op) => opRow(op)).join('')}
          </tbody>
        </table>
      </div>`;
  }

  function opRow(op) {
    const p = op.payload || {};
    const senders = p.senders || [];
    const receivers = p.receivers || [];
    const from = senders[0] ? senders[0].account : '—';
    const to = receivers[0] ? receivers[0].account : '—';
    const amount = senders[0] ? senders[0].amount : 0;
    const fee = p.fee || 0;
    return `
      <tr>
        <td><a class="mono link" href="transaction.html?block=${esc(op.block_number)}&op=${esc(op.index)}">${esc(op.block_number)}·${esc(op.index)}</a></td>
        <td>${opBadge(op.op_type_name)}</td>
        <td><a class="mono link" href="account.html?id=${esc(from)}">${esc(from)}</a></td>
        <td><a class="mono link" href="account.html?id=${esc(to)}">${esc(to)}</a></td>
        <td>${auge(amount)} AUGE</td>
        <td>${auge(fee)} AUGE</td>
        <td><a class="link" href="block.html?block=${esc(op.block_number)}">${fmtNum(op.block_number)}</a></td>
        <td class="muted">${timeAgo(op.timestamp)}</td>
      </tr>`;
  }

  function updateSecondary(status, tpsVal, avgBlockTime) {
    let el = document.getElementById('secondary-metrics');
    if (!el) {
      const main = document.querySelector('main.container');
      el = document.createElement('section');
      el.className = 'section';
      el.id = 'secondary-metrics';
      main.insertBefore(el, main.querySelector('.section:nth-child(3)'));
    }
    const remaining = remainingBlocks(status.current_height);
    el.innerHTML = `
      <div class="section-head"><h2>Network</h2></div>
      <div class="mini-grid">
        ${miniStat('Current height', '#' + fmtNum(status.current_height))}
        ${miniStat('Avg block time', avgBlockTime.toFixed(1) + 's')}
        ${miniStat('TPS', tpsVal.toFixed(3))}
        ${miniStat('Accounts created', fmtNum(status.total_accounts))}
        ${miniStat('Block reward', CONFIG.BLOCK_REWARD_AUGE + ' AUGE')}
        ${miniStat('Emission remaining', fmtNum(remaining) + ' blocks')}
        ${miniStat('Mempool', fmtNum(status.mempool_size))}
        ${miniStat('Chain ID', String(status.chain_id))}
      </div>`;
  }

  // cleanup handled by page unload; poller runs forever (browser manages).
  window.addEventListener('beforeunload', () => poller.stop());
}

function setText(id, text) {
  const el = document.getElementById(id);
  if (el) el.textContent = text;
}

function circulatingSupply(height) {
  const blocks = Math.min(height + 1, CONFIG.TOTAL_EMISSION_BLOCKS);
  return blocks * CONFIG.BLOCK_REWARD_AUGE;
}

function pctSupply(circulating) {
  return ((circulating / CONFIG.TOTAL_SUPPLY_AUGE) * 100).toFixed(2);
}

function remainingBlocks(height) {
  return Math.max(0, CONFIG.TOTAL_EMISSION_BLOCKS - (height + 1));
}

function latestTimestamp(blocks) {
  return blocks && blocks.length ? blocks[0].timestamp : null;
}

function blockSize(b) {
  // Approximate serialized size: header + payload + merkle root.
  return 96 + (b.block_payload_hex ? b.block_payload_hex.length / 2 : 0);
}

function miniStat(label, value) {
  return `<div class="mini-stat"><span class="mini-label">${esc(label)}</span><span class="mini-value">${esc(value)}</span></div>`;
}
