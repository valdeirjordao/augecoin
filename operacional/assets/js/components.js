// AUGECOIN Explorer — reusable UI components (header, footer, cards, badges,
// tables, toast, modal, pagination, skeletons).

import { esc, shortHash, copy, accountStateInfo } from './utils.js';
import { CONFIG } from './config.js';

const ICONS = {
  search: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="18" height="18"><circle cx="11" cy="11" r="7"/><path d="m21 21-4.3-4.3"/></svg>',
  sun: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="18" height="18"><circle cx="12" cy="12" r="4"/><path d="M12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4"/></svg>',
  moon: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="18" height="18"><path d="M21 12.8A9 9 0 1 1 11.2 3a7 7 0 0 0 9.8 9.8z"/></svg>',
  copy: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="14" height="14"><rect x="9" y="9" width="13" height="13" rx="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/></svg>',
  cube: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="20" height="20"><path d="M21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16z"/><path d="m3.3 7 8.7 5 8.7-5M12 22V12"/></svg>',
  users: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="20" height="20"><path d="M17 21v-2a4 4 0 0 0-4-4H5a4 4 0 0 0-4 4v2"/><circle cx="9" cy="7" r="4"/><path d="M23 21v-2a4 4 0 0 0-3-3.87M16 3.13a4 4 0 0 1 0 7.75"/></svg>',
  coins: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="20" height="20"><circle cx="8" cy="8" r="6"/><path d="M18.09 10.37A6 6 0 1 1 10.34 18M7 6h1v4"/></svg>',
  clock: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="20" height="20"><circle cx="12" cy="12" r="10"/><path d="M12 6v6l4 2"/></svg>',
};

function svg(name) {
  return ICONS[name] || '';
}

// ── Header ─────────────────────────────────────────────────────────────

const NAV = [
  { href: 'index.html', label: 'Home' },
  { href: 'blocks.html', label: 'Blocks' },
  { href: 'validators.html', label: 'Validators' },
  { href: 'supply.html', label: 'Supply' },
  { href: 'richlist.html', label: 'Rich List' },
];

export function renderHeader(active) {
  const host = document.getElementById('app-header');
  if (!host) return;
  const activeHref = active || (location.pathname.split('/').pop() || 'index.html');
  const links = NAV.map((n) => {
    const cls = n.href === activeHref ? 'nav-link active' : 'nav-link';
    return `<a class="${cls}" href="${n.href}">${n.label}</a>`;
  }).join('');

  host.innerHTML = `
    <header class="site-header">
      <div class="container header-inner">
        <a class="brand" href="index.html" aria-label="AUGECOIN Explorer">
          <span class="brand-logo">${svg('cube')}</span>
          <span class="brand-name">AUGECOIN<span class="brand-sub">Explorer</span></span>
        </a>
        <button class="nav-toggle" aria-label="Menu" aria-expanded="false">☰</button>
        <nav class="site-nav" id="site-nav">${links}</nav>
        <div class="header-actions">
          <button class="icon-btn" data-theme-toggle aria-label="Toggle theme">${svg('sun')}</button>
        </div>
      </div>
    </header>`;

  document.querySelector('.nav-toggle').addEventListener('click', () => {
    document.getElementById('site-nav').classList.toggle('open');
  });
}

// ── Footer ─────────────────────────────────────────────────────────────

export function renderFooter(status = null) {
  const host = document.getElementById('app-footer');
  if (!host) return;
  const h = status || {};
  host.innerHTML = `
    <footer class="site-footer">
      <div class="container footer-grid">
        <div class="footer-col">
          <div class="brand brand-footer">
            <span class="brand-logo">${svg('cube')}</span>
            <span class="brand-name">AUGECOIN<span class="brand-sub">Explorer</span></span>
          </div>
          <p class="footer-tag">Official block explorer for the AUGECOIN blockchain.</p>
        </div>
        <div class="footer-col">
          <h4>Network</h4>
          <ul class="footer-list">
            <li>Network: <strong>${esc(h.network || CONFIG.NETWORK)}</strong></li>
            <li>Chain ID: <strong>${esc(h.chain_id ?? CONFIG.CHAIN_ID)}</strong></li>
            <li>Last block: <strong>${esc(h.current_height ?? '—')}</strong></li>
            <li>Peers: <strong>${esc(h.peers_connected ?? '—')}</strong></li>
          </ul>
        </div>
        <div class="footer-col">
          <h4>Explore</h4>
          <ul class="footer-list">
            ${NAV.map((n) => `<li><a href="${n.href}">${n.label}</a></li>`).join('')}
          </ul>
        </div>
        <div class="footer-col">
          <h4>Protocol</h4>
          <ul class="footer-list">
            <li>Protocol v${esc(CONFIG.PROTOCOL_VERSION)}</li>
            <li>Block time ${esc(CONFIG.BLOCK_TIME_SECONDS)}s</li>
            <li>Reward ${esc(CONFIG.BLOCK_REWARD_AUGE)} AUGE</li>
          </ul>
        </div>
      </div>
      <div class="container footer-bottom">
        <span>© ${new Date().getFullYear()} AUGECOIN</span>
        <span class="footer-dot">•</span>
        <span>Network: Testnet</span>
      </div>
    </footer>`;
}

// ── Badge ──────────────────────────────────────────────────────────────

export function badge(kind, label, title = '') {
  const t = title ? ` title="${esc(title)}"` : '';
  return `<span class="badge badge-${esc(kind)}"${t}>${esc(label)}</span>`;
}

export function opBadge(opTypeName) {
  const n = String(opTypeName || '').toLowerCase();
  if (n === 'transaction') return badge('success', 'Transaction');
  if (n === 'validatoradmin') return badge('admin', 'ValidatorAdmin');
  if (n === 'createaccount') return badge('info', 'CreateAccount');
  if (n === 'changekey' || n === 'changekeysigned') return badge('warning', opTypeName);
  return badge('muted', opTypeName || 'Operation');
}

// ── Cards ──────────────────────────────────────────────────────────────

export function card({ label, value, icon = '', sub = '', href = '', accent = '' }) {
  const body = `
    <div class="stat-card${accent ? ' stat-accent' : ''}">
      <div class="stat-head">
        <span class="stat-label">${esc(label)}</span>
        <span class="stat-icon">${svg(icon)}</span>
      </div>
      <div class="stat-value" data-value>${value}</div>
      ${sub ? `<div class="stat-sub">${sub}</div>` : ''}
    </div>`;
  return href ? `<a class="stat-link" href="${esc(href)}">${body}</a>` : body;
}

// ── Skeleton ───────────────────────────────────────────────────────────

export function skeleton(rows = 5, cols = 4) {
  let out = '<div class="skeleton-table">';
  for (let i = 0; i < rows; i++) {
    out += '<div class="skeleton-row">';
    for (let j = 0; j < cols; j++) out += '<div class="skeleton-cell"></div>';
    out += '</div>';
  }
  return out + '</div>';
}

// ── Copy button ────────────────────────────────────────────────────────

export function copyButton(text, extraClass = '') {
  return `<button class="copy-btn ${extraClass}" data-copy="${esc(text)}" aria-label="Copy">${svg('copy')}</button>`;
}

export function bindCopyButtons(root = document) {
  root.querySelectorAll('[data-copy]').forEach((btn) => {
    btn.addEventListener('click', async () => {
      const ok = await copy(btn.getAttribute('data-copy'));
      const original = btn.innerHTML;
      btn.innerHTML = ok ? '✓' : '✗';
      btn.classList.add(ok ? 'copied' : 'failed');
      setTimeout(() => { btn.innerHTML = original; btn.classList.remove('copied', 'failed'); }, 1200);
    });
  });
}

// ── Toast ──────────────────────────────────────────────────────────────

export function toast(message, kind = 'info') {
  let host = document.getElementById('toast-host');
  if (!host) {
    host = document.createElement('div');
    host.id = 'toast-host';
    document.body.appendChild(host);
  }
  const el = document.createElement('div');
  el.className = `toast toast-${kind}`;
  el.textContent = message;
  host.appendChild(el);
  requestAnimationFrame(() => el.classList.add('show'));
  setTimeout(() => {
    el.classList.remove('show');
    setTimeout(() => el.remove(), 300);
  }, 3500);
}

// ── Modal ──────────────────────────────────────────────────────────────

export function openModal(title, bodyHTML) {
  const overlay = document.createElement('div');
  overlay.className = 'modal-overlay';
  overlay.innerHTML = `
    <div class="modal" role="dialog" aria-modal="true">
      <div class="modal-head"><h3>${esc(title)}</h3><button class="modal-close" aria-label="Close">×</button></div>
      <div class="modal-body">${bodyHTML}</div>
    </div>`;
  const close = () => overlay.remove();
  overlay.addEventListener('click', (e) => { if (e.target === overlay) close(); });
  overlay.querySelector('.modal-close').addEventListener('click', close);
  document.body.appendChild(overlay);
  return { close };
}

// ── Pagination ─────────────────────────────────────────────────────────

export function pagination({ page, pageSize, total, onChange, label = 'records' }) {
  const pages = Math.max(1, Math.ceil(total / pageSize));
  const p = Math.min(page, pages);
  const from = total === 0 ? 0 : (p - 1) * pageSize + 1;
  const to = Math.min(p * pageSize, total);

  const btn = (target, text, disabled, active) =>
    `<button class="page-btn${active ? ' active' : ''}" data-page="${target}" ${disabled ? 'disabled' : ''}>${text}</button>`;

  const maxBtns = 5;
  let start = Math.max(1, p - Math.floor(maxBtns / 2));
  let end = Math.min(pages, start + maxBtns - 1);
  start = Math.max(1, end - maxBtns + 1);
  const nums = [];
  for (let i = start; i <= end; i++) nums.push(btn(i, i, false, i === p));

  const html = `
    <div class="pagination">
      <span class="page-info">${total.toLocaleString()} ${esc(label)} · ${from}–${to}</span>
      <div class="page-controls">
        ${btn(p - 1, '‹', p === 1)}
        ${nums.join('')}
        ${btn(p + 1, '›', p === pages)}
      </div>
    </div>`;

  return {
    html,
    bind(root) {
      root.querySelectorAll('.page-btn[data-page]').forEach((b) => {
        b.addEventListener('click', () => {
          const target = parseInt(b.dataset.page, 10);
          if (!isNaN(target) && target >= 1 && target <= pages) onChange(target);
        });
      });
    },
  };
}

// ── Empty / error state ────────────────────────────────────────────────

export function emptyState(title, message = '') {
  return `<div class="empty-state"><h3>${esc(title)}</h3>${message ? `<p>${esc(message)}</p>` : ''}</div>`;
}

export function errorState(message) {
  return `<div class="empty-state error"><h3>Something went wrong</h3><p>${esc(message)}</p><button class="btn" data-retry>Retry</button></div>`;
}

// ── Status dot ─────────────────────────────────────────────────────────

export function statusDot(ok, label) {
  const kind = ok ? 'success' : 'error';
  return `<span class="status-dot status-${kind}"><span class="dot"></span>${esc(label)}</span>`;
}

export { accountStateInfo };
