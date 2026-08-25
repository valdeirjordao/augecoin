// AUGECOIN Explorer — supply / emission page.

import { getNodeStatus, getLatestBlocks } from '../api/client.js';
import { CONFIG } from '../config.js';
import { skeleton, card } from '../components.js';
import { esc, fmtNum } from '../utils.js';
import { lineChart, donutChart, registerChartsForTheme } from '../charts.js';
import { createPoller } from '../realtime.js';

export async function render() {
  const root = document.getElementById('page-root');
  root.innerHTML = `
    <main class="container page">
      <div class="page-head"><h1>Supply & Emission</h1><p class="muted">AUGECOIN monetary policy — linear emission, 7.25 AUGE per block.</p></div>
      <section class="grid-stats" id="supply-cards">${skeleton(2, 4)}</section>
      <div class="grid-2">
        <div class="card chart-card">
          <h3 class="card-title">Emission over time (projection)</h3>
          <canvas id="emission-chart" height="260"></canvas>
        </div>
        <div class="card chart-card">
          <h3 class="card-title">Supply distribution</h3>
          <canvas id="supply-donut" height="260"></canvas>
          <div id="donut-legend" class="legend"></div>
        </div>
      </div>
    </main>`;

  let refresh = async () => {};

  try {
    const status = await getNodeStatus();
    const height = status.current_height;
    const circulating = Math.min(height + 1, CONFIG.TOTAL_EMISSION_BLOCKS) * CONFIG.BLOCK_REWARD_AUGE;
    const remaining = CONFIG.TOTAL_SUPPLY_AUGE - circulating;
    const pct = (circulating / CONFIG.TOTAL_SUPPLY_AUGE) * 100;
    const blocksLeft = Math.max(0, CONFIG.TOTAL_EMISSION_BLOCKS - (height + 1));
    const yearsLeft = ((blocksLeft * CONFIG.BLOCK_TIME_SECONDS) / (86400 * 365.25)).toFixed(1);

    document.getElementById('supply-cards').innerHTML =
      card({ label: 'Total Supply', value: `${fmtNum(CONFIG.TOTAL_SUPPLY_AUGE)} AUGE`, icon: 'coins', sub: 'Hard cap' }) +
      card({ label: 'Circulating', value: `${fmtNum(circulating)} AUGE`, icon: 'clock', sub: `${pct.toFixed(2)}% emitted` }) +
      card({ label: 'Remaining', value: `${fmtNum(remaining)} AUGE`, icon: 'cube', sub: `${yearsLeft} years left` }) +
      card({ label: 'Block reward', value: `${CONFIG.BLOCK_REWARD_AUGE} AUGE`, icon: 'users', sub: 'linear, no halving' });

    // Emission rate constants
    const perDay = CONFIG.BLOCKS_PER_DAY * CONFIG.BLOCK_REWARD_AUGE;
    const perMonth = perDay * 30;
    const perYear = perDay * 365;
    insertRateCards(perDay, perMonth, perYear);

    refresh = () => {
      drawEmissionChart(height);
      drawDonut(circulating, remaining);
    };
    refresh();
    registerChartsForTheme([refresh]);
    const poller = createPoller(async () => {
      const s = await getNodeStatus();
      const circ = Math.min(s.current_height + 1, CONFIG.TOTAL_EMISSION_BLOCKS) * CONFIG.BLOCK_REWARD_AUGE;
      drawDonut(circ, CONFIG.TOTAL_SUPPLY_AUGE - circ);
    }, CONFIG.POLL_INTERVAL_MS, false);
    window.addEventListener('beforeunload', () => poller.stop());
  } catch (err) {
    root.querySelector('.page-head').insertAdjacentHTML('afterend', `<div class="empty-state error"><h3>RPC unreachable</h3><p>${esc(err.message)}</p></div>`);
  }
}

function insertRateCards(perDay, perMonth, perYear) {
  const host = document.getElementById('supply-cards');
  host.insertAdjacentHTML('afterend', `
    <section class="section"><div class="section-head"><h2>Emission rate</h2></div>
    <div class="mini-grid">
      <div class="mini-stat"><span class="mini-label">Emitted today</span><span class="mini-value">${fmtNum(perDay)} AUGE</span></div>
      <div class="mini-stat"><span class="mini-label">Emitted this month</span><span class="mini-value">${fmtNum(perMonth)} AUGE</span></div>
      <div class="mini-stat"><span class="mini-label">Emitted this year</span><span class="mini-value">${fmtNum(perYear)} AUGE</span></div>
      <div class="mini-stat"><span class="mini-label">Blocks per day</span><span class="mini-value">${fmtNum(CONFIG.BLOCKS_PER_DAY)}</span></div>
    </div></section>`);
}

function drawEmissionChart(currentHeight) {
  const points = [];
  const N = 40;
  for (let i = 0; i <= N; i++) {
    const h = Math.round((currentHeight + 1) * (i / N));
    const emitted = Math.min(h, CONFIG.TOTAL_EMISSION_BLOCKS) * CONFIG.BLOCK_REWARD_AUGE;
    points.push({ label: '#' + h, value: emitted });
  }
  lineChart(document.getElementById('emission-chart'), points);
}

function drawDonut(circulating, remaining) {
  donutChart(document.getElementById('supply-donut'), [
    { label: 'Circulating', value: circulating, color: '#F97316' },
    { label: 'Remaining', value: remaining, color: '#334155' },
  ]);
  const legend = document.getElementById('donut-legend');
  if (legend) {
    legend.innerHTML = `
      <span class="legend-item"><i style="background:#F97316"></i>Circulating ${fmtNum(circulating)} AUGE</span>
      <span class="legend-item"><i style="background:#334155"></i>Remaining ${fmtNum(remaining)} AUGE</span>`;
  }
}
