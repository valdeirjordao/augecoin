// AUGECOIN Explorer — smart contract detail page (AUGE20 token).

import { getContract, getContractTokenInfo, getContractBalance } from '../api/client.js';
import { skeleton, badge, copyButton, bindCopyButtons, errorState, card } from '../components.js';
import { esc, shortHash, auge, fmtNum, qs } from '../utils.js';

function toAddressHex(input) {
  const v = String(input || '').trim();
  if (/^\d+$/.test(v)) return Number(v).toString(16).padStart(16, '0');
  const h = v.replace(/^0x/, '').toLowerCase();
  if (/^[0-9a-f]{16}$/.test(h)) return h;
  if (/^[0-9a-f]{1,15}$/.test(h)) return h.padStart(16, '0');
  throw new Error('Enter an AUGEID (account number) or a 16-hex address.');
}

export async function render() {
  const root = document.getElementById('page-root');
  const id = qs('id');
  if (!id) {
    root.innerHTML = errorState('Missing contract id');
    return;
  }

  root.innerHTML = `
    <main class="container page">
      <div class="page-head">
        <h1>Smart Contract</h1>
        <p class="mono muted">${shortHash(id, 12)}</p>
      </div>
      <div id="contract-body">${skeleton(6, 4)}</div>
    </main>`;

  try {
    const [meta, infoRes] = await Promise.all([getContract(id), getContractTokenInfo(id)]);
    if (!meta || !meta.exists) {
      document.getElementById('contract-body').innerHTML = errorState('Contract not found');
      return;
    }
    const info = infoRes && infoRes.token ? infoRes.token : null;
    renderBody(id, meta, info);
  } catch (err) {
    document.getElementById('contract-body').innerHTML = errorState(err.message);
  }
}

function renderBody(id, meta, info) {
  const ownerNum = parseInt(meta.owner_hex, 16);
  const isToken = !!info;

  const header = `
    <div class="contract-title">
      <div>
        <h2>${info ? esc(info.name) : 'Contract'} ${info ? `<span class="token-symbol">${esc(info.symbol)}</span>` : ''}</h2>
        <div class="mono muted">${shortHash(id, 14)} ${copyButton(id, 'sm')}</div>
      </div>
      <div class="contract-badges">
        ${isToken ? badge('info', 'AUGE20') : badge('muted', 'Contract')}
        ${isToken && info.mint_enabled ? badge('success', 'Mint enabled') : ''}
        ${isToken && !info.mint_enabled ? badge('muted', 'Mint disabled') : ''}
        ${isToken && info.burn_enabled ? badge('warning', 'Burn enabled') : ''}
        ${isToken && !info.burn_enabled ? badge('muted', 'Burn disabled') : ''}
      </div>
    </div>`;

  const statCards = isToken
    ? `
      ${card({ label: 'Total Supply', value: `${esc(auge(info.total_supply, { decimals: info.decimals, fixed: 2 }))} ${esc(info.symbol)}`, icon: 'coins' })}
      ${card({ label: 'Max Supply', value: `${esc(auge(info.max_supply, { decimals: info.decimals, fixed: 2 }))} ${esc(info.symbol)}`, icon: 'coins' })}
      ${card({ label: 'Decimals', value: esc(info.decimals), icon: 'coins' })}
      ${card({ label: 'Created', value: `#${fmtNum(meta.created_height)}`, icon: 'clock' })}`
    : `
      ${card({ label: 'Code ID', value: esc(parseInt(meta.code_id_hex, 16)), icon: 'cube' })}
      ${card({ label: 'Created', value: `#${fmtNum(meta.created_height)}`, icon: 'clock' })}`;

  const details = `
    <div class="card detail-card">
      <h3>Details</h3>
      <table class="table table-flat">
        <tbody>
          <tr><th>Contract ID</th><td class="mono">${shortHash(id, 16)} ${copyButton(id)}</td></tr>
          <tr><th>Owner</th><td><a class="link" href="account.html?id=${ownerNum}">Account #${fmtNum(ownerNum)}</a> ${copyButton(meta.owner_hex, 'sm')}</td></tr>
          <tr><th>Code ID</th><td class="mono">${esc(parseInt(meta.code_id_hex, 16))}</td></tr>
          <tr><th>AUGE20</th><td>${meta.is_auge20 ? 'Yes' : 'No'}</td></tr>
          ${isToken ? `<tr><th>Symbol</th><td><strong>${esc(info.symbol)}</strong></td></tr>` : ''}
          ${isToken ? `<tr><th>Name</th><td>${esc(info.name)}</td></tr>` : ''}
        </tbody>
      </table>
    </div>`;

  const balanceBox = isToken
    ? `
    <div class="card detail-card" id="balance-card">
      <h3>Check Balance</h3>
      <form data-balance>
        <div class="form-row">
          <input type="text" name="addr" placeholder="AUGEID (e.g. 1) or 16-hex address" class="input" />
          <button class="btn" type="submit">Lookup</button>
        </div>
        <p class="muted small">Balances are read live from the contract state.</p>
        <div id="balance-result"></div>
      </form>
    </div>`
    : '';

  document.getElementById('contract-body').innerHTML = `
    ${header}
    <div class="grid-stats">${statCards}</div>
    <div class="detail-grid">
      ${details}
      ${balanceBox}
    </div>`;

  bindCopyButtons(document.getElementById('contract-body'));

  if (isToken) bindBalanceForm(id, info);
}

function bindBalanceForm(id, info) {
  const form = document.querySelector('[data-balance]');
  if (!form) return;
  form.addEventListener('submit', async (e) => {
    e.preventDefault();
    const input = form.querySelector('input[name="addr"]');
    const out = document.getElementById('balance-result');
    out.innerHTML = skeleton(2, 1);
    try {
      const addrHex = toAddressHex(input.value);
      const res = await getContractBalance(id, addrHex);
      const amount = auge(res.balance, { decimals: info.decimals, trim: true });
      out.innerHTML = `<div class="balance-result">${esc(amount)} <strong>${esc(info.symbol)}</strong></div>`;
    } catch (err) {
      out.innerHTML = `<div class="empty-state error"><p>${esc(err.message)}</p></div>`;
    }
  });
}
