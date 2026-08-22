// AUGECOIN Wallet — reusable UI components.

import { esc, shortHash, copy } from './utils.js';
import { CONFIG } from './config.js';

const ICONS = {
  sun: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="18" height="18"><circle cx="12" cy="12" r="4"/><path d="M12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4"/></svg>',
  moon: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="18" height="18"><path d="M21 12.8A9 9 0 1 1 11.2 3a7 7 0 0 0 9.8 9.8z"/></svg>',
  cube: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="20" height="20"><path d="M21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16z"/><path d="m3.3 7 8.7 5 8.7-5M12 22V12"/></svg>',
  coins: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="20" height="20"><circle cx="8" cy="8" r="6"/><path d="M18.09 10.37A6 6 0 1 1 10.34 18M7 6h1v4"/></svg>',
  users: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="20" height="20"><path d="M17 21v-2a4 4 0 0 0-4-4H5a4 4 0 0 0-4 4v2"/><circle cx="9" cy="7" r="4"/><path d="M23 21v-2a4 4 0 0 0-3-3.87M16 3.13a4 4 0 0 1 0 7.75"/></svg>',
  clock: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="20" height="20"><circle cx="12" cy="12" r="10"/><path d="M12 6v6l4 2"/></svg>',
  user: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="18" height="18"><circle cx="12" cy="8" r="4"/><path d="M4 21c0-4 3.6-6 8-6s8 2 8 6"/></svg>',
  hex: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="18" height="18"><polygon points="12 2 20 7 20 17 12 22 4 17 4 7"/><circle cx="12" cy="12" r="3.5"/></svg>',
  copy: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="14" height="14"><rect x="9" y="9" width="13" height="13" rx="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/></svg>',
  send: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="20" height="20"><path d="M22 2L11 13"/><path d="M22 2l-7 20-4-9-9-4 20-7z"/></svg>',
  receive: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="20" height="20"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><polyline points="7 10 12 15 17 10"/><line x1="12" y1="15" x2="12" y2="3"/></svg>',
  history: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="20" height="20"><circle cx="12" cy="12" r="10"/><polyline points="12 6 12 12 16 14"/></svg>',
  home: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="20" height="20"><path d="M3 9l9-7 9 7v11a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/><polyline points="9 22 9 12 15 12 15 22"/></svg>',
  wallet: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="20" height="20"><rect x="2" y="4" width="20" height="16" rx="2"/><path d="M2 10h20"/></svg>',
  server: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="20" height="20"><rect x="2" y="3" width="20" height="7" rx="2"/><rect x="2" y="14" width="20" height="7" rx="2"/><line x1="6" y1="6.5" x2="6.01" y2="6.5"/><line x1="6" y1="17.5" x2="6.01" y2="17.5"/></svg>',
  more: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="20" height="20"><circle cx="12" cy="12" r="1"/><circle cx="12" cy="5" r="1"/><circle cx="12" cy="19" r="1"/></svg>',
  shield: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="20" height="20"><path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"/></svg>',
  check: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="16" height="16"><polyline points="20 6 9 17 4 12"/></svg>',
  alert: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="16" height="16"><circle cx="12" cy="12" r="10"/><line x1="12" y1="8" x2="12" y2="12"/><line x1="12" y1="16" x2="12.01" y2="16"/></svg>',
  chevronDown: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="16" height="16"><polyline points="6 9 12 15 18 9"/></svg>',
  receipt: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="20" height="20"><path d="M4 2v20l2-1 2 1 2-1 2 1 2-1 2 1 2-1 2 1V2l-2 1-2-1-2 1-2-1-2 1-2-1-2 1z"/><path d="M8 7h8M8 11h8M8 15h5"/></svg>',
};

function svg(name) { return ICONS[name] || ''; }

// ── Header ─────────────────────────────────────────────────────────────

const SIDEBAR_ITEMS = [
  { hash: '#/', label: 'Dashboard', icon: 'home' },
  { hash: '#/receive', label: 'Receber', icon: 'receive' },
  { hash: '#/augeid', label: 'AUGEID', icon: 'hex' },
  { hash: '#/history', label: 'Histórico', icon: 'history' },
  { hash: '#/marketplace', label: 'Marketplace', icon: 'coins' },
  { hash: '#/validator', label: 'Validador', icon: 'server' },
  { hash: '#/billing', label: 'Assinaturas', icon: 'receipt' },
  { hash: '#/my-wallets', label: 'Contas', icon: 'wallet' },
  { hash: '#/profile', label: 'Segurança', icon: 'shield' },
  { hash: '#/profile', label: 'Config', icon: 'user' },
];

export function renderSidebar(user) {
  const host = document.getElementById('app-sidebar');
  if (!host) return;
  if (!user) { host.innerHTML = ''; host.classList.remove('open'); return; }
  host.innerHTML = `<nav class="sidebar-nav">
    ${SIDEBAR_ITEMS.map((n) => `<a class="sidebar-link" href="${n.hash}" data-sidebar="${n.hash}">${svg(n.icon)}<span>${esc(n.label)}</span></a>`).join('')}
  </nav>`;
}

export function renderHeader({ user, hasWallet } = {}) {
  const host = document.getElementById('app-header');
  if (!host) return;

  renderSidebar(user);
  renderBottomNav(user);

  const session = user
    ? `<span class="header-user"><span class="layer-chip layer-platform">${svg('user')} ${esc(user.display_name || user.username || user.email)}</span></span>
       <button class="btn btn-ghost" data-logout>Sair</button>`
    : '';

  host.innerHTML = `
    <header class="site-header">
      <div class="container header-inner">
        <a class="brand" href="#/" aria-label="AUGECOIN Wallet">
          <span class="brand-logo">${svg('cube')}</span>
          <span class="brand-name">AUGECOIN<span class="brand-sub">WALLET</span></span>
        </a>
        <div class="header-actions">
          ${session}
          <button class="icon-btn" data-theme-toggle aria-label="Alternar tema">${svg('sun')}</button>
        </div>
      </div>
    </header>`;
}

// ── Landing header (pre-login public page) ─────────────────────────────

export function renderLandingHeader() {
  const host = document.getElementById('app-header');
  if (!host) return;

  const sidebar = document.getElementById('app-sidebar');
  if (sidebar) sidebar.innerHTML = '';
  renderBottomNav(null);

  host.innerHTML = `
    <header class="site-header">
      <div class="container header-inner">
        <a class="brand" href="#/" aria-label="AUGECOIN Wallet">
          <span class="brand-logo">${svg('cube')}</span>
          <span class="brand-name">AUGECOIN<span class="brand-sub">WALLET</span></span>
        </a>
        <button class="nav-toggle" aria-label="Menu" aria-expanded="false">&#9776;</button>
        <nav class="site-nav" id="site-nav">
          <a class="nav-link" href="#sobre" data-scroll="#sobre">Sobre</a>
          <a class="nav-link" href="#seguranca" data-scroll="#seguranca">Segurança</a>
          <a class="nav-link" href="#faq" data-scroll="#faq">FAQ</a>
        </nav>
        <div class="header-actions">
          <button class="btn" data-goto-auth="login">Entrar</button>
          <button class="btn btn-primary" data-goto-auth="register">Criar conta</button>
          <button class="icon-btn" data-theme-toggle aria-label="Alternar tema">${svg('sun')}</button>
        </div>
      </div>
    </header>`;

  document.querySelector('.nav-toggle')?.addEventListener('click', () => {
    const nav = document.getElementById('site-nav');
    nav?.classList.toggle('open');
    const btn = document.querySelector('.nav-toggle');
    btn?.setAttribute('aria-expanded', nav?.classList.contains('open'));
  });
}

export function markActiveNav(hash) {
  document.querySelectorAll('[data-nav]').forEach((a) => {
    a.classList.toggle('active', a.getAttribute('data-nav') === hash);
  });
  document.querySelectorAll('[data-sidebar]').forEach((a) => {
    const target = a.getAttribute('data-sidebar');
    const active = target === hash || (target !== '#/' && hash.startsWith(target) && hash !== '#/');
    a.classList.toggle('active', active);
  });
  document.querySelectorAll('[data-bottom-nav]').forEach((a) => {
    a.classList.toggle('active', a.getAttribute('data-bottom-nav') === hash);
  });
}

// ── Bottom navigation (mobile) ─────────────────────────────────────────

export function renderBottomNav(user) {
  let host = document.getElementById('bottom-nav');
  if (!host) {
    host = document.createElement('div');
    host.id = 'bottom-nav';
    host.className = 'bottom-nav';
    document.body.appendChild(host);
  }
  if (!user) { host.innerHTML = ''; return; }

  const items = [
    { hash: '#/', label: 'Início', icon: 'home' },
    { hash: '#/send', label: 'Enviar', icon: 'send' },
    { hash: '#/receive', label: 'Receber', icon: 'receive' },
    { hash: '#/history', label: 'Histórico', icon: 'history' },
    { hash: '#/augeid', label: 'AUGEID', icon: 'hex' },
  ];

  host.innerHTML = `<nav class="bottom-nav-inner">
    ${items.map((i) => `<a class="bottom-nav-item" href="${i.hash}" data-bottom-nav="${i.hash}">${svg(i.icon)}${esc(i.label)}</a>`).join('')}
  </nav>`;
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
            <span class="brand-name">AUGECOIN<span class="brand-sub">WALLET</span></span>
          </div>
          <span class="brand-word" aria-hidden="true">wallet</span>
          <p class="footer-tag">Carteira oficial da blockchain AUGECOIN.</p>
        </div>
        <div class="footer-col">
          <h4>Rede</h4>
          <ul class="footer-list">
            <li>Rede: <strong>${esc(h.network || CONFIG.NETWORK)}</strong></li>
            <li>Chain ID: <strong>${esc(h.chain_id ?? CONFIG.CHAIN_ID)}</strong></li>
            <li>Bloco: <strong>${esc(h.current_height ?? '—')}</strong></li>
            <li>Peers: <strong>${esc(h.peers_connected ?? '—')}</strong></li>
          </ul>
        </div>
        <div class="footer-col">
          <h4>Plataforma</h4>
          <ul class="footer-list">
            <li>Login · Cadastro · Perfil</li>
            <li>Sessão JWT segura</li>
          </ul>
        </div>
        <div class="footer-col">
          <h4>Blockchain</h4>
          <ul class="footer-list">
            <li>Saldo · Transferências</li>
            <li>AUGEID · Compra · Venda</li>
          </ul>
        </div>
      </div>
      <div class="container footer-bottom">
        <span>&copy; ${new Date().getFullYear()} AUGECOIN</span>
        <span class="footer-dot">&bull;</span>
        <span>Rede: ${esc(CONFIG.NETWORK)}</span>
      </div>
    </footer>`;
}

// ── Badges ─────────────────────────────────────────────────────────────

export function badge(kind, label, title = '') {
  const t = title ? ` title="${esc(title)}"` : '';
  return `<span class="badge badge-${esc(kind)}"${t}>${esc(label)}</span>`;
}

export function statusBadge(state) {
  switch (String(state || 'Unknown')) {
    case 'Reserved': return badge('muted', 'Reservado');
    case 'Owned': return badge('info', 'Proprietário');
    case 'Normal': return badge('success', 'Normal');
    case 'ForSale': return badge('warning', 'À venda');
    case 'GiftPending': return badge('admin', 'Presente pendente');
    case 'ForAtomicAccountSwap': return badge('muted', 'Swap atômico');
    case 'ForAtomicCoinSwap': return badge('muted', 'Swap de moedas');
    default: return badge('muted', String(state || 'Desconhecido'));
  }
}

// ── Layer chips ─────────────────────────────────────────────────────────

export function platformChip(label = 'Conta da Plataforma') {
  return `<span class="layer-chip layer-platform">${svg('user')} ${esc(label)}</span>`;
}

export function walletChip(label = 'Carteira Blockchain') {
  return `<span class="layer-chip layer-wallet">${svg('hex')} ${esc(label)}</span>`;
}

// ── Stat card ──────────────────────────────────────────────────────────

export function statCard(label, value, icon = '', sub = '') {
  return `<div class="stat-card"><div class="stat-head"><span class="stat-label">${esc(label)}</span><span class="stat-icon">${svg(icon)}</span></div><div class="stat-value">${value}</div>${sub ? `<div class="stat-sub">${esc(sub)}</div>` : ''}</div>`;
}

// ── Skeleton ───────────────────────────────────────────────────────────

export function skeleton(rows = 4, cols = 4) {
  let out = '<div class="skeleton-table">';
  for (let i = 0; i < rows; i++) {
    out += '<div class="skeleton-row">';
    for (let j = 0; j < cols; j++) out += '<div class="skeleton-cell"></div>';
    out += '</div>';
  }
  return out + '</div>';
}

export function skeletonCard() {
  return `<div class="skeleton-card"><div class="skeleton-line w-60"></div><div class="skeleton-line h-32"></div><div class="skeleton-line w-80"></div><div class="skeleton-line w-40"></div></div>`;
}

// ── Copy ───────────────────────────────────────────────────────────────

export function copyButton(text) {
  return `<button class="copy-btn" data-copy="${esc(text)}" aria-label="Copiar">${svg('copy')}</button>`;
}

export function bindCopyButtons(root = document) {
  root.querySelectorAll('[data-copy]').forEach((btn) => {
    btn.addEventListener('click', async () => {
      const ok = await copy(btn.getAttribute('data-copy'));
      const original = btn.innerHTML;
      btn.innerHTML = ok ? svg('check') : '✗';
      btn.classList.add(ok ? 'copied' : 'failed');
      setTimeout(() => { btn.innerHTML = original; btn.classList.remove('copied', 'failed'); }, 1500);
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
  }, 4000);
}

// ── Modal ──────────────────────────────────────────────────────────────

export function openModal(title, bodyHTML) {
  const overlay = document.createElement('div');
  overlay.className = 'modal-overlay';
  overlay.innerHTML = `
    <div class="modal" role="dialog" aria-modal="true">
      <div class="modal-head"><h3>${esc(title)}</h3><button class="modal-close" aria-label="Fechar">&times;</button></div>
      <div class="modal-body">${bodyHTML}</div>
    </div>`;
  const close = () => overlay.remove();
  overlay.addEventListener('click', (e) => { if (e.target === overlay) close(); });
  overlay.querySelector('.modal-close').addEventListener('click', close);
  document.body.appendChild(overlay);
  return { close, el: overlay };
}

// ── Empty / error states ───────────────────────────────────────────────

export function emptyState(title, message = '') {
  return `<div class="empty-state"><h3>${esc(title)}</h3>${message ? `<p>${esc(message)}</p>` : ''}</div>`;
}

export function errorState(message) {
  return `<div class="empty-state error"><h3>Algo deu errado</h3><p>${esc(message)}</p><button class="btn" data-retry>Recarregar</button></div>`;
}

// ── Loading overlay ────────────────────────────────────────────────────

export function showLoading(message = 'Carregando...') {
  const existing = document.getElementById('loading-overlay');
  if (existing) return;
  const el = document.createElement('div');
  el.id = 'loading-overlay';
  el.className = 'loading-overlay';
  el.innerHTML = `<div class="spinner"></div><p class="muted">${esc(message)}</p>`;
  document.body.appendChild(el);
}

export function hideLoading() {
  document.getElementById('loading-overlay')?.remove();
}

// ── SVG icon export ────────────────────────────────────────────────────

export { svg };
