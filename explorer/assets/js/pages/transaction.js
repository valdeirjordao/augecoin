// AUGECOIN Explorer — single operation (transaction) page.

import { getBlockOperations, getBlockCount } from '../api/client.js';
import { skeleton, opBadge, copyButton, bindCopyButtons } from '../components.js';
import { esc, shortHash, auge, fmtNum, fmtTime, timeAgo, qs } from '../utils.js';

export async function render() {
  const root = document.getElementById('page-root');
  const blockNum = qs('block');
  const opIdx = qs('op');

  root.innerHTML = `
    <main class="container page">
      <div class="page-head">
        <h1>Operation <span class="text-accent">${esc(blockNum || '—')}·${esc(opIdx ?? '—')}</span></h1>
        <p class="muted" id="op-sub"></p>
      </div>
      <div id="op-content">${skeleton(6, 2)}</div>
    </main>`;

  if (!blockNum || opIdx === null || opIdx === '') {
    document.getElementById('op-content').innerHTML = '<div class="empty-state error"><h3>Invalid operation reference</h3></div>';
    return;
  }

  try {
    const [res, height] = await Promise.all([getBlockOperations(Number(blockNum)), getBlockCount()]);
    const op = res.operations[Number(opIdx)];
    if (!op) throw new Error(`operation ${opIdx} not found in block ${blockNum}`);

    const b = res.block;
    const confirmations = Math.max(0, height - b.block_number + 1);
    const ref = `${blockNum}-${opIdx}`;
    document.getElementById('op-sub').textContent = `Included in block #${blockNum} · ${fmtNum(confirmations)} confirmations`;

    const kv = (label, valueHTML) => `<div class="kv"><span class="kv-label">${esc(label)}</span><span class="kv-value">${valueHTML}</span></div>`;

    document.getElementById('op-content').innerHTML = `
      <div class="grid-2">
        <div class="card">
          <h3 class="card-title">Summary</h3>
          ${kv('Operation reference', `<span class="mono">${esc(ref)}</span> ${copyButton(ref)}`)}
          ${kv('Hash', op.op_hash_hex ? `<span class="hash">${shortHash(op.op_hash_hex, 16)}</span> ${copyButton(op.op_hash_hex)}` : '—')}
          ${kv('Type', opBadge(op.op_type_name))}
          ${kv('Block', `<a class="link" href="block.html?block=${esc(blockNum)}">#${fmtNum(blockNum)}</a>`)}
          ${kv('Confirmations', fmtNum(confirmations))}
          ${kv('Timestamp', fmtTime(b.timestamp))}
          ${kv('Signatures', fmtNum(op.signatures_count))}
        </div>
        ${opPayloadCard(op)}
      </div>
      <div class="section">
        <div class="section-head"><h2>Raw payload</h2></div>
        <div class="card"><pre class="code-block">${esc(JSON.stringify(op.payload, null, 2))}</pre></div>
      </div>`;
    bindCopyButtons();
  } catch (err) {
    document.getElementById('op-content').innerHTML = `<div class="empty-state error"><h3>Operation not found</h3><p>${esc(err.message)}</p></div>`;
  }
}

function opPayloadCard(op) {
  const p = op.payload || {};
  const kv = (label, valueHTML) => `<div class="kv"><span class="kv-label">${esc(label)}</span><span class="kv-value">${valueHTML}</span></div>`;

  let body = '';
  if (p.senders || p.receivers) {
    body += '<h3 class="card-title">Transfer</h3>';
    const senders = p.senders || [];
    const receivers = p.receivers || [];
    body += kv('From', senders.map((s) => `<a class="link mono" href="account.html?id=${esc(s.account)}">${esc(s.account)}</a>`).join(', ') || '—');
    body += kv('To', receivers.map((r) => `<a class="link mono" href="account.html?id=${esc(r.account)}">${esc(r.account)}</a>`).join(', ') || '—');
    if (senders[0]) { body += kv('Amount', auge(senders[0].amount, { symbol: true })); body += kv('Nonce (n_operation)', fmtNum(senders[0].n_operation)); }
    if (p.fee !== undefined) body += kv('Fee', auge(p.fee, { symbol: true }));
  } else if (p.account !== undefined) {
    body += '<h3 class="card-title">Account operation</h3>';
    body += kv('Account', `<a class="link mono" href="account.html?id=${esc(p.account)}">${esc(p.account)}</a>`);
    if (p.fee !== undefined) body += kv('Fee', auge(p.fee, { symbol: true }));
  } else if (p.type === 'ValidatorAdmin') {
    body += '<h3 class="card-title">ValidatorAdmin</h3><p class="muted">ValidatorSet governance operation.</p>';
  } else if (p.pubkey_hex) {
    body += '<h3 class="card-title">CreateAccount</h3>';
    body += kv('Public key', `<span class="mono">${esc(p.pubkey_hex)}</span>`);
  } else {
    body += '<h3 class="card-title">Details</h3><p class="muted">See raw payload.</p>';
  }

  return `<div class="card">${body}</div>`;
}
