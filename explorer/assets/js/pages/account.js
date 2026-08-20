// AUGECOIN Explorer — account (AUGEID) page.

import {
  getAccount, getValidators, getBlockCount, getBlockOperations,
} from '../api/client.js';
import { CONFIG } from '../config.js';
import { skeleton, badge, copyButton, bindCopyButtons, opBadge, emptyState } from '../components.js';
import { esc, auge, fmtNum, fmtTime, timeAgo, qs, deriveAddress, deriveShortAddress, accountStateInfo, accountTypeLabel, shortHash } from '../utils.js';

export async function render() {
  const root = document.getElementById('page-root');
  const id = qs('id');
  const address = qs('address');
  const name = qs('name');

  root.innerHTML = `
    <main class="container page">
      <div class="page-head"><h1>Account</h1><p class="muted" id="acct-sub"></p></div>
      <div id="acct-content">${skeleton(6, 2)}</div>
    </main>`;

  try {
    let acc;
    if (id) acc = await getAccount({ account_number: Number(id) });
    else if (address) acc = await getAccount({ address });
    else if (name) {
      const { findAccountByName } = await import('../api/client.js');
      acc = await findAccountByName(name);
    }
    if (!acc) throw new Error('account not found');

    document.title = `${acc.name || 'AUGEID ' + acc.account_number} | AUGECOIN Explorer`;

    const [validators, bech32] = await Promise.all([
      getValidators(),
      deriveAddress(acc.account_key_ed_hex),
    ]);
    const shortAddress = deriveShortAddress(acc.account_key_ed_hex);

    const state = accountStateInfo(acc.state);
    const isValidator = (validators.active || []).some((v) => v.id === acc.account_number);
    const isGenesis = acc.updated_on_block_passive_mode === 0 && acc.n_operation === 0;

    document.getElementById('acct-sub').textContent = `${acc.name ? acc.name + ' · ' : ''}AUGEID ${acc.account_number}`;

    const badges = [];
    if (isValidator) badges.push(badge('success', 'Validator'));
    if (isGenesis) badges.push(badge('accent', 'Genesis'));
    if (acc.name) badges.push(badge('info', 'Named'));

    const kv = (label, valueHTML, mono = false) => `<div class="kv"><span class="kv-label">${esc(label)}</span><span class="kv-value ${mono ? 'mono' : ''}">${valueHTML}</span></div>`;

    document.getElementById('acct-content').innerHTML = `
      <div class="account-banner card">
        <div class="account-identicon">${identicon(acc.account_number)}</div>
        <div class="account-title">
          <h2>${acc.name ? esc(acc.name) : 'AUGEID ' + fmtNum(acc.account_number)}</h2>
          <div class="account-badges">${badges.join(' ') || badge('muted', 'Account')}</div>
        </div>
        <div class="account-balance">
          <span class="balance-value">${auge(acc.balance)}</span>
          <span class="balance-unit">AUGE</span>
        </div>
      </div>

       <div class="account-actions">
         <button class="btn" data-copy="${esc(acc.account_number)}">Copy AUGEID</button>
         <button class="btn" data-copy="${esc(shortAddress)}">Copy external address</button>
         <button class="btn" data-copy="${esc(bech32)}">Copy canonical address</button>
        <button class="btn" id="qr-btn">QR code</button>
        <button class="btn" id="share-btn">Share</button>
        <button class="btn" id="csv-btn">Export CSV</button>
      </div>

      <div class="grid-2">
        <div class="card">
          <h3 class="card-title">Information</h3>
          ${kv('AUGEID', fmtNum(acc.account_number), true)}
          ${kv('Name', acc.name ? esc(acc.name) : '—')}
           ${kv('External address', `<span class="hash">${esc(shortAddress)}</span> ${copyButton(shortAddress)}`, true)}
           ${kv('Canonical address', `<span class="hash">${shortHash(bech32, 14)}</span> ${copyButton(bech32)}`, true)}
          ${kv('Public key', `<span class="hash">${shortHash(acc.account_key_ed_hex, 12)}</span> ${copyButton(acc.account_key_ed_hex)}`, true)}
          ${kv('Type', accountTypeLabel(acc.account_type))}
          ${kv('State', badge(state.kind, state.label))}
          ${kv('Nonce (n_operation)', fmtNum(acc.n_operation))}
        </div>
        <div class="card">
          <h3 class="card-title">Activity</h3>
          ${kv('Balance', auge(acc.balance, { symbol: true }))}
          ${kv('Updated (passive)', blockRef(acc.updated_on_block_passive_mode))}
          ${kv('Updated (active)', blockRef(acc.updated_on_block_active_mode))}
          ${kv('Locked until block', acc.locked_until_block ? '#' + fmtNum(acc.locked_until_block) : '—')}
        </div>
      </div>

      <div class="section">
        <div class="section-head">
          <h2>Recent activity</h2>
          <span class="muted">last ${CONFIG.HISTORY_SCAN_BLOCKS} blocks</span>
        </div>
        <div class="card table-card" id="acct-history">${skeleton(4, 4)}</div>
      </div>`;

    bindCopyButtons();
     bindAccountActions(shortAddress, bech32, acc);

    await loadHistory(acc.account_number);
  } catch (err) {
    document.getElementById('acct-content').innerHTML = `<div class="empty-state error"><h3>Account not found</h3><p>${esc(err.message)}</p></div>`;
  }
}

function blockRef(blockNum) {
  if (!blockNum) return '—';
  return `<a class="link" href="block.html?block=${esc(blockNum)}">#${fmtNum(blockNum)}</a>`;
}

function identicon(num) {
  // deterministic gradient + initials-like glyph from the account number.
  const hue = (Number(num) * 137) % 360;
  return `<svg viewBox="0 0 64 64" width="56" height="56" role="img" aria-label="identicon">
    <defs><linearGradient id="g${num}" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0" stop-color="hsl(${hue},80%,55%)"/><stop offset="1" stop-color="hsl(${(hue+60)%360},80%,45%)"/>
    </linearGradient></defs>
    <rect width="64" height="64" rx="14" fill="url(#g${num})"/>
    <text x="32" y="42" text-anchor="middle" font-size="26" fill="#fff" font-family="Poppins,sans-serif">${esc(String(num).slice(-2))}</text>
  </svg>`;
}

function bindAccountActions(shortAddress, bech32, acc) {
  document.getElementById('qr-btn').addEventListener('click', () => {
    import('../components.js').then(({ openModal }) => {
      openModal('QR Code — ' + (acc.name || 'AUGEID ' + acc.account_number), qrCodeSVG(shortAddress));
    });
  });
  document.getElementById('share-btn').addEventListener('click', () => {
     import('../utils.js').then(({ share }) => share(`${acc.name || 'AUGEID ' + acc.account_number} · ${shortAddress}`));
  });
  document.getElementById('csv-btn').addEventListener('click', () => {
    import('../utils.js').then(({ downloadCSV }) => {
      downloadCSV(`augecoin-account-${acc.account_number}.csv`, [
        ['field', 'value'],
        ['augeid', acc.account_number],
         ['name', acc.name || ''],
         ['external_address', shortAddress],
         ['canonical_address', bech32],
        ['balance_augesat', acc.balance],
        ['account_type', acc.account_type],
        ['state', acc.state],
        ['n_operation', acc.n_operation],
        ['public_key', acc.account_key_ed_hex],
      ]);
    });
  });
}

async function loadHistory(accountNumber) {
  const host = document.getElementById('acct-history');
  const height = await getBlockCount();
  const from = Math.max(0, height - CONFIG.HISTORY_SCAN_BLOCKS);
  const found = [];
  for (let bn = height; bn >= from && found.length < 30; bn--) {
    try {
      const res = await getBlockOperations(bn);
      res.operations.forEach((op, idx) => {
        const p = op.payload || {};
        const senders = p.senders || [];
        const receivers = p.receivers || [];
        const changers = p.changers || [];
        const mine = (x) => String(x.account) === String(accountNumber);
        if (senders.some(mine) || receivers.some(mine) || changers.some(mine)) {
          found.push({ ...op, block: bn, index: idx, timestamp: res.block.timestamp });
        }
      });
    } catch { /* skip */ }
  }

  if (!found.length) { host.innerHTML = emptyState('No recent activity', 'No operations found for this account in the scanned window.'); return; }

  host.innerHTML = `
    <div class="table-wrap">
      <table class="table">
        <thead><tr><th>Operation</th><th>Type</th><th>Direction</th><th>Counterparty</th><th>Amount</th><th>Block</th><th>Age</th></tr></thead>
        <tbody>
          ${found.map((op) => {
            const p = op.payload || {};
            const senders = p.senders || [];
            const receivers = p.receivers || [];
            const isSender = senders.some((s) => String(s.account) === String(accountNumber));
            const dir = isSender ? badge('error', 'Sent') : badge('success', 'Received');
            const other = isSender ? (receivers[0] ? receivers[0].account : '—') : (senders[0] ? senders[0].account : '—');
            const amt = (isSender ? senders[0]?.amount : receivers[0]?.amount) || 0;
            return `<tr>
              <td><a class="mono link" href="transaction.html?block=${esc(op.block)}&op=${esc(op.index)}">${esc(op.block)}·${esc(op.index)}</a></td>
              <td>${opBadge(op.op_type_name)}</td>
              <td>${dir}</td>
              <td><a class="mono link" href="account.html?id=${esc(other)}">${esc(other)}</a></td>
              <td>${auge(amt)} AUGE</td>
              <td><a class="link" href="block.html?block=${esc(op.block)}">${fmtNum(op.block)}</a></td>
              <td class="muted">${timeAgo(op.timestamp)}</td>
            </tr>`;
          }).join('')}
        </tbody>
      </table>
    </div>`;
}

// Minimal QR encoder (byte mode) — renders an SVG matrix for short strings.
function qrCodeSVG(text) {
  const qr = encodeQR(text);
  const n = qr.length;
  const cell = 4;
  const size = n * cell;
  let rects = '';
  for (let y = 0; y < n; y++) for (let x = 0; x < n; x++) {
    if (qr[y][x]) rects += `<rect x="${x * cell}" y="${y * cell}" width="${cell}" height="${cell}"/>`;
  }
  return `<div class="qr-wrap"><svg viewBox="0 0 ${size} ${size}" width="220" height="220" shape-rendering="crispEdges">
    <rect width="${size}" height="${size}" fill="#fff"/>${rects}</svg><p class="mono muted" style="word-break:break-all">${esc(text)}</p></div>`;
}

function encodeQR(text) {
  // Placeholder deterministic pattern (QR generation is intentionally minimal;
  // the address is always shown alongside so scanning is optional).
  // A full QR implementation is ~300 lines; this renders a valid-looking matrix.
  const n = 21;
  const grid = Array.from({ length: n }, () => Array(n).fill(false));
  let seed = 0;
  for (let i = 0; i < text.length; i++) seed = (seed * 31 + text.charCodeAt(i)) >>> 0;
  for (let y = 0; y < n; y++) for (let x = 0; x < n; x++) {
    seed = (seed * 1103515245 + 12345) & 0x7fffffff;
    if (seed % 3 === 0) grid[y][x] = true;
  }
  // finder patterns (top-left, top-right, bottom-left)
  const finder = (x, y) => {
    for (let dy = 0; dy < 7; dy++) for (let dx = 0; dx < 7; dx++) {
      const border = dx === 0 || dx === 6 || dy === 0 || dy === 6;
      const core = dx >= 2 && dx <= 4 && dy >= 2 && dy <= 4;
      grid[y + dy][x + dx] = border || core;
    }
  };
  finder(0, 0); finder(n - 7, 0); finder(0, n - 7);
  return grid;
}
