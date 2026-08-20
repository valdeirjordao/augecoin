// AUGECOIN Explorer — single block page.

import { getBlockOperations, getBlockCount } from '../api/client.js';
import { skeleton, opBadge, copyButton, bindCopyButtons } from '../components.js';
import { esc, shortHash, auge, fmtNum, fmtTime, timeAgo, qs, accountStateInfo } from '../utils.js';

export async function render() {
  const root = document.getElementById('page-root');
  const blockNum = qs('block');

  root.innerHTML = `
    <main class="container page">
      <div class="page-head">
        <h1>Block <span class="text-accent">#${esc(blockNum || '—')}</span></h1>
        <p class="muted" id="block-sub"></p>
      </div>
      <div id="block-content">${skeleton(6, 2)}</div>
    </main>`;

  if (!blockNum) {
    document.getElementById('block-content').innerHTML = '<div class="empty-state error"><h3>No block number</h3></div>';
    return;
  }

  try {
    const [res, height] = await Promise.all([getBlockOperations(Number(blockNum)), getBlockCount()]);
    const b = res.block;
    const ops = res.operations;
    const confirmations = Math.max(0, height - b.block_number + 1);

    document.getElementById('block-sub').textContent = `Mined ${timeAgo(b.timestamp)} · ${fmtNum(confirmations)} confirmations`;

    const kv = (label, valueHTML, mono = false) => `
      <div class="kv"><span class="kv-label">${esc(label)}</span><span class="kv-value ${mono ? 'mono' : ''}">${valueHTML}</span></div>`;

    document.getElementById('block-content').innerHTML = `
      <div class="grid-2">
        <div class="card">
          <h3 class="card-title">Overview</h3>
          ${kv('Height', '#' + fmtNum(b.block_number))}
          ${kv('Timestamp', fmtTime(b.timestamp))}
          ${kv('Validator (leader)', `<a class="link" href="account.html?id=${esc(b.leader_id)}">v${esc(b.leader_id)}</a>`)}
          ${kv('Reward', auge(b.reward, { symbol: true }))}
          ${kv('Total fees', auge(b.fee, { symbol: true }))}
          ${kv('Operations', fmtNum(ops.length))}
          ${kv('Confirmations', fmtNum(confirmations))}
        </div>
        <div class="card">
          <h3 class="card-title">Hashes</h3>
          ${kv('Block Hash', b.block_hash_hex ? `<span class="hash">${shortHash(b.block_hash_hex, 16)}</span> ${copyButton(b.block_hash_hex)}` : '—', true)}
          ${kv('Operations Merkle Root', `<span class="hash">${shortHash(b.operations_hash_hex, 16)}</span> ${copyButton(b.operations_hash_hex)}`, true)}
          ${kv('Safe Box Hash', `<span class="hash">${shortHash(b.initial_safe_box_hash_hex, 16)}</span> ${copyButton(b.initial_safe_box_hash_hex)}`, true)}
          ${kv('Previous Proof of Work', `<span class="hash">${shortHash(b.previous_proof_of_work_hex, 12)}</span> ${copyButton(b.previous_proof_of_work_hex)}`, true)}
          ${kv('Proof of Work', `<span class="hash">${shortHash(b.proof_of_work_hex, 12)}</span> ${copyButton(b.proof_of_work_hex)}`, true)}
          ${kv('Protocol', `v${esc(b.protocol_version)} (available v${esc(b.protocol_available)})`)}
        </div>
      </div>

      <div class="section">
        <div class="section-head"><h2>Operations (${fmtNum(ops.length)})</h2></div>
        <div class="card table-card">
          ${ops.length ? `
          <div class="table-wrap">
            <table class="table">
              <thead><tr><th>#</th><th>Type</th><th>Details</th><th>Signatures</th></tr></thead>
              <tbody>
                ${ops.map((op, i) => opRow(op, i, b.block_number)).join('')}
              </tbody>
            </table>
          </div>` : '<div class="empty-state"><h3>No operations in this block</h3></div>'}
        </div>
      </div>`;

    bindCopyButtons();
  } catch (err) {
    document.getElementById('block-content').innerHTML = `<div class="empty-state error"><h3>Block not found</h3><p>${esc(err.message)}</p></div>`;
  }
}

function opRow(op, i, blockNum) {
  const p = op.payload || {};
  let detail = '';
  const senders = p.senders || [];
  const receivers = p.receivers || [];
  if (senders.length || receivers.length) {
    const from = senders.map((s) => `<a class="link mono" href="account.html?id=${esc(s.account)}">${esc(s.account)}</a>`).join(', ') || '—';
    const to = receivers.map((r) => `<a class="link mono" href="account.html?id=${esc(r.account)}">${esc(r.account)}</a>`).join(', ') || '—';
    detail = `From ${from} → ${to}`;
    const amt = senders[0] ? senders[0].amount : 0;
    if (amt) detail += ` · ${auge(amt)} AUGE`;
    if (p.fee) detail += ` · fee ${auge(p.fee)} AUGE`;
  } else if (p.account !== undefined) {
    detail = `Account <a class="link mono" href="account.html?id=${esc(p.account)}">${esc(p.account)}</a>`;
  } else if (p.type === 'ValidatorAdmin') {
    detail = 'ValidatorSet change';
  }
  return `
    <tr>
      <td><a class="mono link" href="transaction.html?block=${esc(blockNum)}&op=${esc(i)}">${esc(blockNum)}·${esc(i)}</a></td>
      <td>${opBadge(op.op_type_name)}</td>
      <td>${detail || '—'}</td>
      <td class="muted">${fmtNum(op.signatures_count)}</td>
    </tr>`;
}
