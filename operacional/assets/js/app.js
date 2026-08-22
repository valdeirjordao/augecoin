// AUGECOIN Operacional — painel de administração de validadores.
//
// SPA vanilla (HTML5 + CSS3 + JS) sobre a API do augecoin-ops. Todas as ações
// administrativas exigem `x-api-key`; o backend é a única via de autorização,
// monitoramento, licenciamento e auditoria (o consenso segue on-chain).

import { CONFIG, getApiKey, setApiKey, clearApiKey, getConfirmKey, setConfirmKey, clearConfirmKey } from './config.js';
import { opsApi, financeApi, ApiError } from './api.js';
import { initTheme } from './theme.js';
import { badge, toast, statusDot, copyButton, bindCopyButtons, skeleton, openModal, emptyState } from './components.js';
import { esc, shortHash, fmtNum, timeAgo, fmtDuration, auge } from './utils.js';
import { barChart } from './charts.js';
// ── Ícones ─────────────────────────────────────────────────────────────

const ICONS = {
  grid: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="18" height="18"><rect x="3" y="3" width="7" height="7" rx="1"/><rect x="14" y="3" width="7" height="7" rx="1"/><rect x="3" y="14" width="7" height="7" rx="1"/><rect x="14" y="14" width="7" height="7" rx="1"/></svg>',
  users: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="18" height="18"><path d="M17 21v-2a4 4 0 0 0-4-4H5a4 4 0 0 0-4 4v2"/><circle cx="9" cy="7" r="4"/><path d="M23 21v-2a4 4 0 0 0-3-3.87M16 3.13a4 4 0 0 1 0 7.75"/></svg>',
  key: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="18" height="18"><circle cx="7.5" cy="15.5" r="5.5"/><path d="m21 2-9.6 9.6M15.5 7.5l3 3L22 7l-3-3"/></svg>',
  coins: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="18" height="18"><circle cx="8" cy="8" r="6"/><path d="M18.09 10.37A6 6 0 1 1 10.34 18M7 6h1v4"/></svg>',
  activity: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="18" height="18"><polyline points="22 12 18 12 15 21 9 3 6 12 2 12"/></svg>',
  list: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="18" height="18"><line x1="8" y1="6" x2="21" y2="6"/><line x1="8" y1="12" x2="21" y2="12"/><line x1="8" y1="18" x2="21" y2="18"/><line x1="3" y1="6" x2="3.01" y2="6"/><line x1="3" y1="12" x2="3.01" y2="12"/><line x1="3" y1="18" x2="3.01" y2="18"/></svg>',
  gear: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="18" height="18"><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"/></svg>',
  shield: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="20" height="20"><path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"/></svg>',
  cube: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="20" height="20"><path d="M21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16z"/><path d="m3.3 7 8.7 5 8.7-5M12 22V12"/></svg>',
  refresh: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="16" height="16"><polyline points="23 4 23 10 17 10"/><path d="M20.49 15a9 9 0 1 1-2.12-9.36L23 10"/></svg>',
  lock: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="16" height="16"><rect x="3" y="11" width="18" height="11" rx="2"/><path d="M7 11V7a5 5 0 0 1 10 0v4"/></svg>',
  up: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="16" height="16"><path d="M12 19V5M5 12l7-7 7 7"/></svg>',
  download: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="16" height="16"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4M7 10l5 5 5-5M12 15V3"/></svg>',
};

const icon = (n) => ICONS[n] || '';

// ── Estado ─────────────────────────────────────────────────────────────

let poller = null;
let navBadges = { pending: 0, alerts: 0 };

const NAV = [
  { href: '#/dashboard', key: 'dashboard', label: 'Dashboard', icon: 'grid' },
  { href: '#/validators', key: 'validators', label: 'Validadores', icon: 'users' },
  { href: '#/licenses', key: 'licenses', label: 'Licenças', icon: 'key' },
  { href: '#/ganhos', key: 'ganhos', label: 'Ganhos', icon: 'coins' },
  { href: '#/monitoramento', key: 'monitoramento', label: 'Monitoramento', icon: 'activity' },
  { href: '#/financeiro', key: 'financeiro', label: 'Financeiro', icon: 'coins' },
  { href: '#/auditoria', key: 'auditoria', label: 'Auditoria', icon: 'list' },
  { href: '#/configuracoes', key: 'configuracoes', label: 'Configurações', icon: 'gear' },
];

// ── Helpers de formatação ─────────────────────────────────────────────

function fmtAuge(augesat, { symbol = true, trim = true } = {}) {
  const s = auge(augesat, { trim });
  return symbol ? `${s} AUGE` : s;
}

function fmtAugeShort(augesat) {
  const n = Number(augesat) / CONFIG.AUGESAT_PER_AUGE;
  const abs = Math.abs(n);
  if (abs >= 1e9) return (n / 1e9).toFixed(2) + 'B';
  if (abs >= 1e6) return (n / 1e6).toFixed(2) + 'M';
  if (abs >= 1e3) return (n / 1e3).toFixed(2) + 'K';
  return n.toFixed(2);
}

function fmtDate(iso) {
  if (!iso) return '—';
  const d = new Date(iso);
  if (isNaN(d)) return '—';
  return d.toLocaleString('en-GB', { timeZone: 'UTC' }) + ' UTC';
}

function fmtDateShort(iso) {
  if (!iso) return '—';
  const d = new Date(iso);
  if (isNaN(d)) return '—';
  return d.toISOString().slice(0, 10);
}

function ageSeconds(iso) {
  if (!iso) return null;
  const d = new Date(iso);
  if (isNaN(d)) return null;
  return Math.floor((Date.now() - d.getTime()) / 1000);
}

// ── Badges de estado ───────────────────────────────────────────────────

function validatorStatusBadge(v) {
  if (v.status === 'pending') return badge('info', 'Pendente');
  if (v.status === 'suspended') return badge('warning', 'Suspenso');
  if (v.status === 'revoked') return badge('muted', 'Revogado');
  // active: derive online/offline
  if (v.online) return badge('success', 'Online');
  return badge('error', 'Offline');
}

function licenseStatusBadge(s) {
  switch (s) {
    case 'active': return badge('success', 'Ativa');
    case 'expired': return badge('error', 'Expirada');
    case 'suspended': return badge('warning', 'Suspensa');
    case 'revoked': return badge('muted', 'Revogada');
    default: return badge('muted', s || '—');
  }
}

function planLabel(p) {
  return { monthly: 'Mensal', semiannual: 'Semestral', annual: 'Anual' }[p] || p || '—';
}

function presenceBadge(age) {
  if (age === null) return badge('muted', '—');
  if (age <= 120) return statusDot(true, 'Online');
  if (age <= 300) return badge('warning', 'Degradado');
  if (age <= 600) return badge('error', 'Crítico');
  return badge('error', 'Remoção PoA');
}

// ── Boot ───────────────────────────────────────────────────────────────

initTheme();

const root = document.getElementById('page-root');

if (!getApiKey()) {
  renderLogin();
} else {
  verifyAndEnter();
}

async function verifyAndEnter() {
  try {
    await opsApi.get('/metrics');
    enterShell();
  } catch (err) {
    if (err instanceof ApiError && err.status === 401) {
      clearApiKey();
      renderLogin('API key inválida ou expirada.');
    } else {
      renderLogin('Falha ao contactar a API: ' + err.message);
    }
  }
}

// ── Login ──────────────────────────────────────────────────────────────

function renderLogin(message = '') {
  document.getElementById('app-header').innerHTML = headerHTML();
  root.innerHTML = `
    <main class="container page">
      <div class="page-head">
        <h1>Painel <span class="text-accent">Operacional</span></h1>
        <p class="muted">Administração de validadores — acesso restrito.</p>
      </div>
      <div class="card login-card">
        <h3 class="card-title">Acesso restrito</h3>
        ${message ? `<p class="login-error">${esc(message)}</p>` : ''}
        <form data-login>
          <label class="field">
            <span class="field-label">API key</span>
            <input type="password" name="key" autocomplete="off" placeholder="x-api-key" required>
          </label>
          <button class="btn btn-primary" type="submit">Desbloquear</button>
        </form>
        <p class="muted small">A chave fica apenas nesta aba (sessionStorage) e é enviada como header <code>x-api-key</code>.</p>
      </div>
    </main>`;
  const form = root.querySelector('[data-login]');
  form.addEventListener('submit', async (e) => {
    e.preventDefault();
    const key = form.querySelector('input[name="key"]').value.trim();
    if (!key) return;
    setApiKey(key);
    try {
      await opsApi.get('/metrics');
      enterShell();
    } catch (err) {
      clearApiKey();
      renderLogin('API key inválida — tente novamente.');
    }
  });
}

// ── Shell (sidebar + router) ───────────────────────────────────────────

function enterShell() {
  window.addEventListener('hashchange', route);
  renderShell();
  route();
}

function headerHTML() {
  return `
    <header class="site-header">
      <div class="container header-inner">
        <a class="brand" href="#/dashboard" aria-label="AUGECOIN Operacional">
          <span class="brand-logo">${icon('shield')}</span>
          <span class="brand-name">AUGECOIN<span class="brand-sub">Operacional</span></span>
        </a>
        <div class="header-actions">
          <span class="badge badge-admin" title="Painel restrito">admin</span>
          <button class="icon-btn" data-theme-toggle aria-label="Alternar tema"></button>
        </div>
      </div>
    </header>`;
}

function renderShell() {
  document.getElementById('app-header').innerHTML = headerHTML();
  const nav = NAV.map((n) => `
    <a class="nav-item" href="${n.href}" data-nav="${n.key}">
      <span class="nav-icon">${icon(n.icon)}</span>
      <span class="nav-label">${n.label}</span>
      <span class="nav-badge" data-badge="${n.key}"></span>
    </a>`).join('');

  root.innerHTML = `
    <div class="app-layout">
      <aside class="sidebar">
        <nav class="sidebar-nav">${nav}</nav>
        <div class="sidebar-foot">
          <button class="btn btn-ghost btn-block" data-lock>${icon('lock')} Bloquear</button>
        </div>
      </aside>
      <main class="app-main" id="app-main"></main>
    </div>`;

  root.querySelector('[data-lock]').addEventListener('click', () => {
    clearApiKey();
    if (poller) poller.stop();
    window.removeEventListener('hashchange', route);
    renderLogin();
  });

  // theme toggle (uses the existing delegated listener in theme.js)
  document.querySelector('[data-theme-toggle]').addEventListener('click', () => {});
}

function route() {
  if (poller) { poller.stop(); poller = null; }
  const hash = location.hash || '#/dashboard';
  const parts = hash.replace(/^#\//, '').split('/');
  const view = parts[0] || 'dashboard';
  const id = parts[1];

  setActiveNav(view);
  const main = document.getElementById('app-main');

  const handlers = {
    dashboard: renderDashboard,
    validators: id ? () => renderValidatorDetail(id) : renderValidators,
    licenses: renderLicenses,
    ganhos: renderGanhos,
    monitoramento: renderMonitoramento,
    financeiro: renderFinanceiro,
    auditoria: renderAuditoria,
    configuracoes: renderConfiguracoes,
  };

  const fn = handlers[view] || renderDashboard;
  fn(main).catch((err) => {
    main.innerHTML = `<div class="empty-state error"><h3>Erro</h3><p>${esc(err.message)}</p></div>`;
  });
}

function setActiveNav(view) {
  document.querySelectorAll('[data-nav]').forEach((el) => {
    el.classList.toggle('active', el.dataset.nav === view);
  });
}

function updateNavBadges() {
  document.querySelectorAll('[data-badge]').forEach((el) => {
    const k = el.dataset.badge;
    if (k === 'validators' && navBadges.pending > 0) el.textContent = navBadges.pending;
    else if (k === 'monitoramento' && navBadges.alerts > 0) el.textContent = navBadges.alerts;
    else el.textContent = '';
  });
}

// ── Dashboard ──────────────────────────────────────────────────────────

async function renderDashboard(main) {
  main.innerHTML = `
    <div class="page-head">
      <h1>Dashboard</h1>
      <p class="muted">Visão geral da rede e dos validadores.</p>
    </div>
    <div class="toolbar">
      <span class="muted">API: <code>${esc(CONFIG.OPS_API_URL)}</code></span>
      <div class="toolbar-actions"><button class="btn" data-refresh>${icon('refresh')} Atualizar</button></div>
    </div>
    <div id="dash-body">${skeleton(3, 4)}</div>`;

  main.querySelector('[data-refresh]').addEventListener('click', () => refreshDashboard(main));
  await refreshDashboard(main);
  poller = createPoller(() => refreshDashboard(main), CONFIG.POLL_INTERVAL_MS);
}

async function refreshDashboard(main) {
  const body = main.querySelector('#dash-body');
  if (!body) return;
  try {
    const m = await opsApi.get('/metrics');
    const r = m.rewards || {};
    const v = m.validators || {};
    const l = m.licenses || {};

    navBadges.pending = v.pending || 0;
    updateNavBadges();

    body.innerHTML = `
      <section class="grid-stats">
        ${card('Validadores ativos', fmtNum(v.active), 'users', `${v.online} online · ${v.pending} pendentes`)}
        ${card('Online agora', fmtNum(v.online), 'activity', `${fmtNum(v.suspended)} suspensos`)}
        ${card('Licenças ativas', fmtNum(l.active), 'key', `${l.expired} expiradas · ${l.suspended} suspensas`)}
        ${card('Uptime médio', fmtDuration(v.uptime_avg_seconds), 'cube', 'entre validadores')}
      </section>
      <section class="grid-stats">
        ${card('AUGE hoje', fmtAugeShort(r.day?.auge), 'coins', `${fmtNum(r.day?.blocks)} blocos hoje`)}
        ${card('AUGE no mês', fmtAugeShort(r.month?.auge), 'coins', `${fmtNum(r.month?.blocks)} blocos no mês`)}
        ${card('AUGE total distribuído', fmtAugeShort(r.total?.auge), 'coins', `${fmtNum(r.total?.augeids)} AUGEIDs`)}
        ${card('Blocos produzidos', fmtNum(r.total?.blocks), 'cube', `${fmtNum(r.total?.augeids)} AUGEIDs emitidos`)}
      </section>
      <div class="section">
        <div class="section-head"><h2>Distribuição de AUGE (augesat por período)</h2></div>
        <div class="card chart-card">
          <div style="height:220px">${barChart({
            data: [r.hour?.auge, r.day?.auge, r.week?.auge, r.month?.auge, r.year?.auge, r.total?.auge].map(Number),
            labels: ['Hora', 'Dia', 'Semana', 'Mês', 'Ano', 'Total'],
          })}</div>
        </div>
      </div>`;
  } catch (err) {
    body.innerHTML = `<div class="empty-state error"><h3>API indisponível</h3><p>${esc(err.message)}</p></div>`;
  }
}

// ── Validadores (lista) ────────────────────────────────────────────────

async function renderValidators(main) {
  main.innerHTML = `
    <div class="page-head">
      <h1>Validadores</h1>
      <p class="muted">Conjunto de validadores registrados no SaaS.</p>
    </div>
    <div class="toolbar">
      <div class="filters" id="filters"></div>
      <div class="toolbar-actions"><button class="btn" data-refresh>${icon('refresh')} Atualizar</button></div>
    </div>
    <div class="card table-card" id="validators-table">${skeleton(5, 6)}</div>`;

  main.querySelector('[data-refresh]').addEventListener('click', () => refreshValidators(main));
  await refreshValidators(main);
  poller = createPoller(() => refreshValidators(main), CONFIG.POLL_INTERVAL_MS);
}

const VALIDATOR_FILTERS = [
  { key: '', label: 'Todos' },
  { key: 'online', label: 'Online' },
  { key: 'offline', label: 'Offline' },
  { key: 'linux', label: 'Linux' },
  { key: 'windows', label: 'Windows' },
  { key: 'suspended', label: 'Suspensos' },
  { key: 'expired', label: 'Expirados' },
];

let validatorFilter = '';

async function refreshValidators(main) {
  const table = main.querySelector('#validators-table');
  const filters = main.querySelector('#filters');
  if (!filters.dataset.bound) {
    filters.dataset.bound = '1';
    filters.innerHTML = VALIDATOR_FILTERS.map((f) =>
      `<button class="chip${f.key === validatorFilter ? ' active' : ''}" data-filter="${f.key}">${f.label}</button>`).join('');
    filters.querySelectorAll('[data-filter]').forEach((b) => b.addEventListener('click', () => {
      validatorFilter = b.dataset.filter;
      filters.querySelectorAll('[data-filter]').forEach((x) => x.classList.toggle('active', x.dataset.filter === validatorFilter));
      refreshValidators(main);
    }));
  }

  try {
    const { validators } = await opsApi.get('/v1/validators');
    const list = validators.filter(matchValidatorFilter);

    navBadges.pending = validators.filter((v) => v.status === 'pending').length;
    updateNavBadges();

    if (!list.length) {
      table.innerHTML = emptyState('Nenhum validador', 'Nenhum registro corresponde ao filtro.');
      return;
    }

    table.innerHTML = `
      <div class="table-wrap">
        <table class="table">
          <thead><tr>
            <th>Nome / Chave</th><th>AUGEID</th><th>Licença</th><th>IP</th><th>Sistema</th>
            <th>Status</th><th>Uptime</th><th>Último bloco</th><th class="num">Ganhos</th><th></th>
          </tr></thead>
          <tbody>
            ${list.map((v) => `
              <tr>
                <td>
                  <a class="link" href="#/validators/${esc(v.id)}">${esc(v.augeid || 'validador')}</a>
                  <div><span class="mono hash" title="${esc(v.public_key)}">${shortHash(v.public_key, 12)}</span> ${copyButton(v.public_key)}</div>
                </td>
                <td class="mono">${esc(v.augeid || '—')}</td>
                <td class="mono">${esc(v.license_key_prefix || '—')}…</td>
                <td class="mono">${esc(v.ip || '—')}</td>
                <td>${esc(v.os || '—')}${v.version ? ' · ' + esc(v.version) : ''}</td>
                <td>${validatorStatusBadge(v)}</td>
                <td>${fmtDuration(v.uptime)}</td>
                <td class="mono">${fmtNum(v.blocks)}</td>
                <td class="num">${fmtAugeShort(v.total_rewards)}</td>
                <td class="row-actions">
                  ${v.status === 'pending' ? `<button class="btn btn-sm btn-primary" data-approve="${esc(v.id)}">Aprovar</button>` : ''}
                  ${v.status === 'active' ? `<button class="btn btn-sm btn-warning" data-suspend="${esc(v.id)}">Suspender</button>` : ''}
                  ${v.status !== 'revoked' ? `<button class="btn btn-sm btn-danger" data-revoke="${esc(v.id)}">Revogar</button>` : ''}
                </td>
              </tr>`).join('')}
          </tbody>
        </table>
      </div>`;

    bindCopyButtons();
    bindValidatorActions(main);
  } catch (err) {
    table.innerHTML = `<div class="empty-state error"><h3>API indisponível</h3><p>${esc(err.message)}</p></div>`;
  }
}

function matchValidatorFilter(v) {
  switch (validatorFilter) {
    case 'online': return v.status === 'active' && v.online;
    case 'offline': return v.status === 'active' && !v.online;
    case 'linux': return (v.os || '').toLowerCase().includes('linux');
    case 'windows': return (v.os || '').toLowerCase().includes('windows');
    case 'suspended': return v.status === 'suspended';
    case 'expired': return v.license_status === 'expired';
    default: return true;
  }
}

function bindValidatorActions(scope) {
  scope.querySelectorAll('[data-approve]').forEach((b) => b.addEventListener('click', () => validatorAction('approve', b.dataset.approve)));
  scope.querySelectorAll('[data-suspend]').forEach((b) => b.addEventListener('click', () => validatorAction('suspend', b.dataset.suspend)));
  scope.querySelectorAll('[data-revoke]').forEach((b) => b.addEventListener('click', () => validatorAction('revoke', b.dataset.revoke)));
}

async function validatorAction(action, id) {
  const labels = { approve: 'aprovar', suspend: 'suspender', revoke: 'revogar' };
  if (!confirm(`Confirmar ${labels[action]} este validador?`)) return;
  try {
    await opsApi.post(`/v1/validators/${id}/${action}`, {});
    toast(`Validador ${labels[action]}do.`, 'success');
    route();
  } catch (err) {
    toast('Falha: ' + err.message, 'error');
  }
}

// ── Validador (detalhe) ────────────────────────────────────────────────

async function renderValidatorDetail(id) {
  const main = document.getElementById('app-main');
  main.innerHTML = `<div class="page-head"><a class="link" href="#/validators">← Validadores</a></div><div id="vd-body">${skeleton(4, 4)}</div>`;
  const body = main.querySelector('#vd-body');

  try {
    const { validator: v, rewards: r } = await opsApi.get(`/v1/validators/${id}`);

    body.innerHTML = `
      <div class="page-head">
        <h1>${esc(v.augeid || 'Validador')}</h1>
        <p class="muted">${validatorStatusBadge(v)} <span class="mono">${shortHash(v.public_key, 16)}</span></p>
      </div>

      <section class="grid-2">
        <div class="card">
          <h3 class="card-title">Informações</h3>
          <div class="kv"><span class="kv-label">AUGEID</span><span class="kv-value mono">${esc(v.augeid || '—')}</span></div>
          <div class="kv"><span class="kv-label">Chave pública</span><span class="kv-value mono">${shortHash(v.public_key, 20)} ${copyButton(v.public_key)}</span></div>
          <div class="kv"><span class="kv-label">Machine hash</span><span class="kv-value mono">${v.machine_hash ? shortHash(v.machine_hash, 20) + ' ' + copyButton(v.machine_hash) : '—'}</span></div>
          <div class="kv"><span class="kv-label">IP</span><span class="kv-value mono">${esc(v.ip || '—')}</span></div>
          <div class="kv"><span class="kv-label">Sistema</span><span class="kv-value">${esc(v.os || '—')} · ${esc(v.version || '—')}</span></div>
          <div class="kv"><span class="kv-label">CPU / RAM</span><span class="kv-value">${v.cpu != null ? v.cpu + ' cores' : '—'} · ${v.ram != null ? v.ram + ' GB' : '—'}</span></div>
          <div class="kv"><span class="kv-label">ID on-chain</span><span class="kv-value mono">${v.node_validator_id != null ? 'v' + v.node_validator_id : '—'}</span></div>
          <div class="kv"><span class="kv-label">Licença</span><span class="kv-value mono">${esc(v.license_key_prefix || '—')}… · expira ${fmtDateShort(v.license_expires_at)}</span></div>
          <div class="kv"><span class="kv-label">Criado em</span><span class="kv-value">${fmtDate(v.created_at)}</span></div>
        </div>

        <div class="card">
          <h3 class="card-title">Uptime</h3>
          <div class="kv"><span class="kv-label">Agora</span><span class="kv-value">${v.online ? 'online' : 'offline'}</span></div>
          <div class="kv"><span class="kv-label">Último heartbeat</span><span class="kv-value">${v.last_seen ? timeAgo(Math.floor(new Date(v.last_seen).getTime() / 1000)) : '—'}</span></div>
          <div class="kv"><span class="kv-label">Uptime total</span><span class="kv-value">${fmtDuration(v.uptime)}</span></div>
          <div class="kv"><span class="kv-label">Blocos</span><span class="kv-value">${fmtNum(v.blocks)}</span></div>
          <div class="kv"><span class="kv-label">Perdidos</span><span class="kv-value">${fmtNum(v.blocks_lost)}</span></div>
          <div class="kv"><span class="kv-label">Liderança</span><span class="kv-value">${fmtNum(v.leadership)}</span></div>
        </div>
      </section>

      <div class="section">
        <div class="section-head"><h2>Ganhos</h2></div>
        <div class="grid-2">
          <div class="card">
            <h3 class="card-title">AUGE por período</h3>
            <div style="height:220px">${barChart({
              data: [r.hour?.auge, r.day?.auge, r.week?.auge, r.month?.auge, r.year?.auge, r.total?.auge].map(Number),
              labels: ['Hora', 'Dia', 'Semana', 'Mês', 'Ano', 'Total'],
            })}</div>
            <div class="kv"><span class="kv-label">Total AUGE</span><span class="kv-value">${fmtAuge(r.total?.auge)}</span></div>
          </div>
          <div class="card">
            <h3 class="card-title">AUGEID por período</h3>
            <div style="height:220px">${barChart({
              data: [r.hour?.augeids, r.day?.augeids, r.week?.augeids, r.month?.augeids, r.year?.augeids, r.total?.augeids].map(Number),
              labels: ['Hora', 'Dia', 'Semana', 'Mês', 'Ano', 'Total'],
            })}</div>
            <div class="kv"><span class="kv-label">Total AUGEIDs</span><span class="kv-value">${fmtNum(r.total?.augeids)}</span></div>
          </div>
        </div>
      </div>
    `;
    bindCopyButtons();
  } catch (err) {
    body.innerHTML = `<div class="empty-state error"><h3>Não encontrado</h3><p>${esc(err.message)}</p></div>`;
  }
}

// ── Licenças ───────────────────────────────────────────────────────────

async function renderLicenses(main) {
  main.innerHTML = `
    <div class="page-head">
      <h1>Licenças</h1>
      <p class="muted">Gestão de licenças de validação.</p>
    </div>
    <div class="toolbar">
      <span class="muted"></span>
      <div class="toolbar-actions">
        <button class="btn btn-primary" data-issue>${icon('key')} Emitir licença</button>
        <button class="btn" data-refresh>${icon('refresh')} Atualizar</button>
      </div>
    </div>
    <div class="card table-card" id="licenses-table">${skeleton(5, 5)}</div>`;

  main.querySelector('[data-refresh]').addEventListener('click', () => refreshLicenses(main));
  main.querySelector('[data-issue]').addEventListener('click', () => issueLicenseModal(main));
  await refreshLicenses(main);
  poller = createPoller(() => refreshLicenses(main), CONFIG.POLL_INTERVAL_MS);
}

async function refreshLicenses(main) {
  const table = main.querySelector('#licenses-table');
  try {
    const { licenses } = await opsApi.get('/v1/licenses');
    if (!licenses.length) { table.innerHTML = emptyState('Nenhuma licença'); return; }

    table.innerHTML = `
      <div class="table-wrap">
        <table class="table">
          <thead><tr>
            <th>Licença</th><th>Plano</th><th>Status</th><th>Usuário</th><th>AUGEID</th>
            <th>Expira em</th><th>Machine hash</th><th></th>
          </tr></thead>
          <tbody>
            ${licenses.map((l) => `
              <tr>
                <td class="mono">${esc(l.license_key_prefix)}…</td>
                <td>${planLabel(l.plan)}</td>
                <td>${licenseStatusBadge(l.status)}</td>
                <td class="mono">${shortHash(l.user_id, 8)}</td>
                <td class="mono">${esc(l.augeid || '—')}</td>
                <td>${fmtDateShort(l.expires_at)}</td>
                <td class="mono">${l.machine_hash ? shortHash(l.machine_hash, 8) : '—'}</td>
                <td class="row-actions">
                  ${l.status === 'active' ? `<button class="btn btn-sm btn-warning" data-suspend="${esc(l.id)}">Suspender</button>` : ''}
                  ${l.status !== 'revoked' ? `<button class="btn btn-sm btn-danger" data-revoke="${esc(l.id)}">Revogar</button>` : ''}
                </td>
              </tr>`).join('')}
          </tbody>
        </table>
      </div>`;

    table.querySelectorAll('[data-suspend]').forEach((b) => b.addEventListener('click', () => licenseAction('suspend', b.dataset.suspend, main)));
    table.querySelectorAll('[data-revoke]').forEach((b) => b.addEventListener('click', () => licenseAction('revoke', b.dataset.revoke, main)));
  } catch (err) {
    table.innerHTML = `<div class="empty-state error"><h3>API indisponível</h3><p>${esc(err.message)}</p></div>`;
  }
}

async function licenseAction(action, id, main) {
  if (!confirm(`Confirmar ${action === 'suspend' ? 'suspender' : 'revogar'} esta licença?`)) return;
  try {
    await opsApi.post(`/v1/licenses/${id}/${action}`, {});
    toast('Licença atualizada.', 'success');
    await refreshLicenses(main);
  } catch (err) {
    toast('Falha: ' + err.message, 'error');
  }
}

async function issueLicenseModal(main) {
  let members = [];
  try {
    const res = await financeApi.get('/admin/members');
    members = res.members || [];
  } catch { /* members list optional; fall back to manual UUID */ }

  const memberOptions = members.length
    ? members.map((m) => `<option value="${esc(m.id)}">${esc(m.display_name || m.email)} — ${esc(m.email)}</option>`).join('')
    : '';

  const modal = openModal('Emitir licença', `
    <form data-issue-form>
      <label class="field">
        <span class="field-label">Membro</span>
        <select name="user_id" class="field" ${members.length ? 'required' : ''}>
          <option value="">Selecione…</option>
          ${memberOptions}
        </select>
      </label>
      ${members.length === 0 ? `<label class="field"><span class="field-label">User ID (UUID)</span><input name="user_id_manual" placeholder="00000000-0000-0000-0000-000000000000"></label>` : ''}
      <label class="field">
        <span class="field-label">Plano</span>
        <select name="plan" class="field">
          <option value="monthly">Mensal — US$15</option>
          <option value="semiannual">Semestral — US$75</option>
          <option value="annual">Anual — US$120</option>
        </select>
      </label>
      <button class="btn btn-primary" type="submit">Emitir</button>
    </form>
    <div id="issue-result"></div>`);

  modal.el = document.body.lastElementChild;
  modal.el.querySelector('[data-issue-form]').addEventListener('submit', async (e) => {
    e.preventDefault();
    const sel = modal.el.querySelector('select[name="user_id"]');
    const user_id = (sel && sel.value) || (modal.el.querySelector('input[name="user_id_manual"]')?.value || '').trim();
    if (!user_id) {
      modal.el.querySelector('#issue-result').innerHTML = `<p class="login-error">Selecione um membro.</p>`;
      return;
    }
    const plan = modal.el.querySelector('select[name="plan"]').value;
    try {
      const res = await financeApi.post('/admin/licenses', { user_id, plan });
      modal.el.querySelector('#issue-result').innerHTML = `
        <div class="card" style="margin-top:16px;border-color:var(--color-success)">
          <h3 class="card-title">Chave gerada</h3>
          <div class="mono" style="font-size:1.1rem;letter-spacing:1px">${esc(res.license_key)}</div>
          <div style="margin-top:8px">${copyButton(res.license_key)}</div>
        </div>`;
      await refreshLicenses(main);
    } catch (err) {
      modal.el.querySelector('#issue-result').innerHTML = `<p class="login-error">${esc(err.message)}</p>`;
    }
  });
}

// ── Ganhos ─────────────────────────────────────────────────────────────

async function renderGanhos(main) {
  main.innerHTML = `
    <div class="page-head"><h1>Ganhos</h1><p class="muted">Dashboard financeiro da rede.</p></div>
    <div class="toolbar"><span class="muted"></span><div class="toolbar-actions"><button class="btn" data-refresh>${icon('refresh')} Atualizar</button></div></div>
    <div id="ganhos-body">${skeleton(3, 4)}</div>`;

  main.querySelector('[data-refresh]').addEventListener('click', () => refreshGanhos(main));
  await refreshGanhos(main);
  poller = createPoller(() => refreshGanhos(main), CONFIG.POLL_INTERVAL_MS);
}

async function refreshGanhos(main) {
  const body = main.querySelector('#ganhos-body');
  try {
    const [m, { validators }] = await Promise.all([
      opsApi.get('/metrics'),
      opsApi.get('/v1/validators'),
    ]);
    const r = m.rewards || {};

    body.innerHTML = `
      <section class="grid-stats">
        ${card('AUGE hoje', fmtAugeShort(r.day?.auge), 'coins', `${fmtNum(r.day?.blocks)} blocos`)}
        ${card('AUGE na semana', fmtAugeShort(r.week?.auge), 'coins', `${fmtNum(r.week?.blocks)} blocos`)}
        ${card('AUGE no mês', fmtAugeShort(r.month?.auge), 'coins', `${fmtNum(r.month?.blocks)} blocos`)}
        ${card('AUGE no ano', fmtAugeShort(r.year?.auge), 'coins', `${fmtNum(r.year?.blocks)} blocos`)}
      </section>
      <section class="grid-stats">
        ${card('AUGE total', fmtAugeShort(r.total?.auge), 'coins', 'desde o gênesis')}
        ${card('AUGEIDs emitidos', fmtNum(r.total?.augeids), 'key', 'contas reservadas')}
        ${card('Blocos produzidos', fmtNum(r.total?.blocks), 'cube', 'pela rede')}
        ${card('Taxas coletadas', fmtAugeShort(r.total?.fees), 'coins', '100% ao líder')}
      </section>
      <div class="section">
        <div class="section-head"><h2>Ganhos por validador</h2></div>
        <div class="card table-card">
          <div class="table-wrap">
            <table class="table">
              <thead><tr><th>Validador</th><th class="num">AUGE total</th><th class="num">AUGEIDs</th><th class="num">Blocos</th></tr></thead>
              <tbody>
                ${validators.map((v) => `
                  <tr>
                    <td><a class="link" href="#/validators/${esc(v.id)}">${esc(v.augeid || shortHash(v.public_key, 10))}</a></td>
                    <td class="num">${fmtAuge(v.total_rewards)}</td>
                    <td class="num">${fmtNum(v.leadership * 10)}</td>
                    <td class="num">${fmtNum(v.leadership)}</td>
                  </tr>`).join('')}
              </tbody>
            </table>
          </div>
        </div>
      </div>`;
  } catch (err) {
    body.innerHTML = `<div class="empty-state error"><h3>API indisponível</h3><p>${esc(err.message)}</p></div>`;
  }
}

// ── Monitoramento ──────────────────────────────────────────────────────

async function renderMonitoramento(main) {
  main.innerHTML = `
    <div class="page-head"><h1>Monitoramento</h1><p class="muted">Heartbeats e alertas automáticos.</p></div>
    <div class="toolbar"><span class="muted">Heartbeat a cada 30s · remoção PoA aos 10 min</span>
      <div class="toolbar-actions"><button class="btn" data-refresh>${icon('refresh')} Atualizar</button></div></div>
    <div class="section"><div class="section-head"><h2>Validadores</h2></div><div class="card table-card" id="mon-table">${skeleton(4, 4)}</div></div>
    <div class="section"><div class="section-head"><h2>Alertas</h2></div><div class="card table-card" id="alert-table">${skeleton(4, 3)}</div></div>`;

  main.querySelector('[data-refresh]').addEventListener('click', () => refreshMonitoramento(main));
  await refreshMonitoramento(main);
  poller = createPoller(() => refreshMonitoramento(main), CONFIG.POLL_INTERVAL_MS);
}

async function refreshMonitoramento(main) {
  const monTable = main.querySelector('#mon-table');
  const alertTable = main.querySelector('#alert-table');
  try {
    const [{ validators }, { alerts }] = await Promise.all([
      opsApi.get('/v1/validators'),
      opsApi.get('/alerts?limit=200'),
    ]);

    navBadges.alerts = alerts.filter((a) => !a.resolved_at).length;
    updateNavBadges();

    monTable.innerHTML = validators.length ? `
      <div class="table-wrap">
        <table class="table">
          <thead><tr><th>Validador</th><th>Estado</th><th>Último heartbeat</th><th>CPU</th><th>RAM</th><th>Bloco</th></tr></thead>
          <tbody>
            ${validators.map((v) => {
              const age = ageSeconds(v.last_seen);
              return `<tr>
                <td><a class="link" href="#/validators/${esc(v.id)}">${esc(v.augeid || shortHash(v.public_key, 10))}</a></td>
                <td>${presenceBadge(age)}</td>
                <td>${v.last_seen ? fmtDate(v.last_seen) : '—'}</td>
                <td>${v.cpu != null ? v.cpu : '—'}</td>
                <td>${v.ram != null ? v.ram + ' GB' : '—'}</td>
                <td class="mono">${fmtNum(v.blocks)}</td>
              </tr>`;
            }).join('')}
          </tbody>
        </table>
      </div>` : emptyState('Nenhum validador');

    const unresolved = alerts.filter((a) => !a.resolved_at);
    const resolved = alerts.filter((a) => a.resolved_at);
    alertTable.innerHTML = alerts.length ? `
      <div class="table-wrap">
        <table class="table">
          <thead><tr><th>Tipo</th><th>Validador</th><th>Mensagem</th><th>Estado</th><th>Desde</th></tr></thead>
          <tbody>
            ${[...unresolved, ...resolved].map((a) => `
              <tr>
                <td>${alertBadge(a.kind)}</td>
                <td class="mono">${shortHash(a.validator_id, 10)}</td>
                <td>${esc(a.message || '—')}</td>
                <td>${a.resolved_at ? badge('muted', 'Resolvido') : statusDot(true, 'Aberto')}</td>
                <td>${fmtDate(a.created_at)}</td>
              </tr>`).join('')}
          </tbody>
        </table>
      </div>` : emptyState('Nenhum alerta', 'Nenhum alerta registrado.');
  } catch (err) {
    monTable.innerHTML = `<div class="empty-state error"><h3>API indisponível</h3><p>${esc(err.message)}</p></div>`;
  }
}

function alertBadge(kind) {
  switch (kind) {
    case 'offline_warning': return badge('warning', 'Aviso');
    case 'offline_critical': return badge('error', 'Crítico');
    case 'poa_removal': return badge('error', 'Remoção PoA');
    default: return badge('muted', kind);
  }
}

// ── Auditoria ──────────────────────────────────────────────────────────

let auditPage = 1;
const AUDIT_PAGE_SIZE = 50;

async function renderAuditoria(main) {
  main.innerHTML = `
    <div class="page-head"><h1>Auditoria</h1><p class="muted">Trilha de eventos imutável (licenças, validadores, alertas).</p></div>
    <div class="toolbar"><span class="muted"></span><div class="toolbar-actions"><button class="btn" data-refresh>${icon('refresh')} Atualizar</button></div></div>
    <div class="card table-card" id="audit-table">${skeleton(5, 4)}</div>
    <div id="audit-pagination"></div>`;

  main.querySelector('[data-refresh]').addEventListener('click', () => refreshAuditoria(main));
  await refreshAuditoria(main);
}

async function refreshAuditoria(main) {
  const table = main.querySelector('#audit-table');
  const pag = main.querySelector('#audit-pagination');
  try {
    const { entries } = await opsApi.get(`/audit?limit=${AUDIT_PAGE_SIZE}&offset=${(auditPage - 1) * AUDIT_PAGE_SIZE}`);
    if (!entries.length) {
      table.innerHTML = emptyState('Nenhum evento');
      pag.innerHTML = '';
      return;
    }
    table.innerHTML = `
      <div class="table-wrap">
        <table class="table">
          <thead><tr><th>Quando</th><th>Entidade</th><th>Evento</th><th>Ator</th><th>Detalhes</th></tr></thead>
          <tbody>
            ${entries.map((e) => `
              <tr>
                <td>${fmtDate(e.created_at)}</td>
                <td><span class="badge badge-muted">${esc(e.entity_type)}</span> <span class="mono">${shortHash(e.entity_id, 6)}</span></td>
                <td><span class="badge badge-accent">${esc(e.event)}</span></td>
                <td>${esc(e.actor)}</td>
                <td class="mono" style="white-space:normal">${esc(JSON.stringify(e.data))}</td>
              </tr>`).join('')}
          </tbody>
        </table>
      </div>`;

    pag.innerHTML = `
      <div class="pagination">
        <button class="page-btn" data-prev ${auditPage <= 1 ? 'disabled' : ''}>‹ Anterior</button>
        <span class="page-info">página ${auditPage}</span>
        <button class="page-btn" data-next ${entries.length < AUDIT_PAGE_SIZE ? 'disabled' : ''}>Próxima ›</button>
      </div>`;
    pag.querySelector('[data-prev]').addEventListener('click', () => { if (auditPage > 1) { auditPage--; refreshAuditoria(main); } });
    pag.querySelector('[data-next]').addEventListener('click', () => { auditPage++; refreshAuditoria(main); });
  } catch (err) {
    table.innerHTML = `<div class="empty-state error"><h3>API indisponível</h3><p>${esc(err.message)}</p></div>`;
  }
}

// ── Configurações ──────────────────────────────────────────────────────

async function renderFinanceiro(main) {
  if (!getConfirmKey()) {
    main.innerHTML = `
      <div class="page-head"><h1>Financeiro</h1><p class="muted">Faturas e configuração de pagamento.</p></div>
      <div class="card login-card">
        <h3 class="card-title">Chave financeira</h3>
        <p class="muted small">Informe a chave financeira para aprovar pagamentos. Fica apenas nesta aba.</p>
        <form data-fin-key>
          <label class="field"><span class="field-label">Chave</span><input type="password" name="key" autocomplete="off" required></label>
          <button class="btn btn-primary" type="submit">Desbloquear</button>
        </form>
      </div>`;
    const form = main.querySelector('[data-fin-key]');
    form.addEventListener('submit', async (e) => {
      e.preventDefault();
      const key = form.querySelector('input[name="key"]').value.trim();
      if (!key) return;
      setConfirmKey(key);
      try {
        await financeApi.get('/admin/orders');
        renderFinanceiro(main);
      } catch {
        clearConfirmKey();
        toast('Chave financeira inválida.', 'error');
        form.querySelector('input[name="key"]').value = '';
      }
    });
    return;
  }

  main.innerHTML = `
    <div class="page-head"><h1>Financeiro</h1><p class="muted">Configuração de pagamento e aprovação manual de faturas.</p></div>
    <section class="grid-2">
      <div class="card">
        <h3 class="card-title">Configuração de pagamento</h3>
        <form data-payment-config>
          <label class="field"><span class="field-label">Chave PIX</span><input name="pix_key" placeholder="e-mail, CPF/CNPJ, telefone ou chave aleatória"></label>
          <label class="field"><span class="field-label">Endereço USDT</span><input name="usdt_address" placeholder="0x… ou T…"></label>
          <label class="field"><span class="field-label">Rede USDT</span><input name="usdt_network" placeholder="TRC20 / ERC20 / BEP20"></label>
          <label class="field"><span class="field-label">Endereço AUGE</span><input name="auge_address" placeholder="Endereço Base58"></label>
          <label class="field"><span class="field-label">Rede AUGE</span><input name="auge_network" placeholder="AUGECOIN"></label>
          <button class="btn btn-primary" type="submit">Salvar</button>
        </form>
      </div>
      <div class="card">
        <h3 class="card-title">Como funciona</h3>
        <p class="muted small">Ao comprar, o membro vê o dado do método escolhido (PIX / USDT / AUGE) com a rede, quando preenchida.</p>
        <p class="muted small">Ao aprovar o pagamento, a licença é emitida automaticamente e aparece na aba Licenças; o membro recebe a chave de ativação na carteira.</p>
      </div>
    </section>
    <div class="section">
      <div class="section-head"><h2>Faturas / pedidos</h2></div>
      <div class="card table-card" id="orders-table">${skeleton(5, 6)}</div>
    </div>`;

  main.querySelector('[data-payment-config]').addEventListener('submit', async (e) => {
    e.preventDefault();
    const f = e.target;
    try {
      await financeApi.put('/admin/payment-config', {
        pix_key: f.pix_key.value,
        usdt_address: f.usdt_address.value,
        usdt_network: f.usdt_network.value,
        auge_address: f.auge_address.value,
        auge_network: f.auge_network.value,
      });
      toast('Configuração salva.', 'success');
    } catch (err) {
      toast('Falha: ' + err.message, 'error');
    }
  });

  await loadFinance(main);
}

async function loadFinance(main) {
  const table = main.querySelector('#orders-table');
  const PLAN_L = { monthly: 'Mensal', semiannual: 'Semestral', annual: 'Anual' };
  const METHOD_L = { pix: 'PIX', usdt: 'USDT', auge: 'AUGE' };
  const orderBadge = (s) => {
    switch (s) {
      case 'pending': return badge('warning', 'Aguardando');
      case 'paid': return badge('success', 'Pago');
      case 'issued': return badge('info', 'Emitida');
      case 'cancelled': return badge('muted', 'Cancelado');
      default: return badge('muted', s || '—');
    }
  };

  try {
    const [cfgRes, ordersRes] = await Promise.all([
      financeApi.get('/admin/payment-config'),
      financeApi.get('/admin/orders'),
    ]);
    const cfg = cfgRes.payment || {};
    const form = main.querySelector('[data-payment-config]');
    if (form) {
      form.pix_key.value = cfg.pix_key || '';
      form.usdt_address.value = cfg.usdt_address || '';
      form.usdt_network.value = cfg.usdt_network || '';
      form.auge_address.value = cfg.auge_address || '';
      form.auge_network.value = cfg.auge_network || '';
    }
    const orders = ordersRes.orders || [];
    if (!orders.length) { table.innerHTML = emptyState('Nenhum pedido'); return; }
    table.innerHTML = `
      <div class="table-wrap">
        <table class="table">
          <thead><tr><th>Membro</th><th>Plano</th><th>Método</th><th>Valor</th><th>Status</th><th>Criado</th><th>Ações</th></tr></thead>
          <tbody>
            ${orders.map((o) => `
              <tr>
                <td><div>${esc(o.display_name || '—')}</div><div class="mono small muted">${esc(o.email)}</div></td>
                <td>${esc(PLAN_L[o.plan] || o.plan)}</td>
                <td>${esc(METHOD_L[o.method] || o.method)}</td>
                <td>US$ ${fmtNum(o.amount_usd)}</td>
                <td>${orderBadge(o.status)}</td>
                <td class="muted small">${fmtDate(o.created_at)}</td>
                <td>
                  ${o.status === 'pending' ? `
                    <div class="actions">
                      <button class="btn" data-approve="${esc(o.id)}">Aprovar</button>
                      <button class="btn btn-danger" data-cancel="${esc(o.id)}">Cancelar</button>
                    </div>` : '<span class="muted small">—</span>'}
                </td>
              </tr>`).join('')}
          </tbody>
        </table>
      </div>`;

    table.querySelectorAll('[data-approve]').forEach((b) => b.addEventListener('click', async () => {
      try {
        const res = await financeApi.post(`/validator/orders/${b.getAttribute('data-approve')}/confirm`, {});
        if (res && res.license_issue_error) {
          toast('Pagamento confirmado, mas a emissão da licença falhou: ' + res.license_issue_error, 'warning');
        } else {
          toast('Pagamento confirmado e licença emitida.', 'success');
        }
        await loadFinance(main);
      } catch (err) {
        toast('Falha: ' + err.message, 'error');
      }
    }));
    table.querySelectorAll('[data-cancel]').forEach((b) => b.addEventListener('click', async () => {
      try {
        await financeApi.post(`/validator/orders/${b.getAttribute('data-cancel')}/cancel`, {});
        toast('Pedido cancelado.', 'success');
        await loadFinance(main);
      } catch (err) {
        toast('Falha: ' + err.message, 'error');
      }
    }));
  } catch (err) {
    table.innerHTML = `<div class="empty-state error"><h3>API indisponível</h3><p>${esc(err.message)}</p></div>`;
  }
}

async function renderConfiguracoes(main) {
  main.innerHTML = `
    <div class="page-head"><h1>Configurações</h1><p class="muted">Endpoint, chave e releases (OTA).</p></div>
    <section class="grid-2">
      <div class="card">
        <h3 class="card-title">Conexão</h3>
        <div class="kv"><span class="kv-label">API</span><span class="kv-value mono">${esc(CONFIG.OPS_API_URL)}</span></div>
        <div class="kv"><span class="kv-label">Rede</span><span class="kv-value">${esc(CONFIG.NETWORK)}</span></div>
        <div class="kv"><span class="kv-label">Decimais</span><span class="kv-value">${CONFIG.DECIMALS}</span></div>
        <div class="kv"><span class="kv-label">Recompensa</span><span class="kv-value">${CONFIG.BLOCK_REWARD_AUGE} AUGE / bloco</span></div>
      </div>
      <div class="card">
        <h3 class="card-title">Releases (OTA)</h3>
        <form data-release>
          <label class="field"><span class="field-label">Versão</span><input name="version" placeholder="1.2.3" required></label>
          <label class="field">
            <span class="field-label">Plataforma</span>
            <select name="platform" class="field"><option value="windows">Windows</option><option value="linux">Linux</option></select>
          </label>
          <label class="field"><span class="field-label">URL do artefato</span><input name="artifact_url" placeholder="https://…"></label>
          <label class="field"><span class="field-label">Hash BLAKE3 (64 hex)</span><input name="artifact_hash" required></label>
          <label class="field"><span class="field-label">Assinatura Ed25519 (128 hex)</span><input name="signature" required></label>
          <button class="btn btn-primary" type="submit">Publicar release</button>
        </form>
        <p class="muted small" style="margin-top:10px">A assinatura é verificada no servidor contra a chave pública configurada.</p>
      </div>
    </section>
    <div class="section">
      <div class="section-head"><h2>Releases publicados</h2></div>
      <div class="card table-card" id="releases-table">${skeleton(3, 4)}</div>
    </div>`;

  main.querySelector('[data-release]').addEventListener('submit', async (e) => {
    e.preventDefault();
    const f = e.target;
    try {
      await opsApi.post('/releases', {
        version: f.version.value.trim(),
        platform: f.platform.value,
        artifact_url: f.artifact_url.value.trim() || null,
        artifact_hash: f.artifact_hash.value.trim(),
        signature: f.signature.value.trim(),
      });
      toast('Release publicado.', 'success');
      f.reset();
      await refreshReleases(main);
    } catch (err) {
      toast('Falha: ' + err.message, 'error');
    }
  });

  await refreshReleases(main);
}

async function refreshReleases(main) {
  const table = main.querySelector('#releases-table');
  try {
    const { releases } = await opsApi.get('/releases');
    table.innerHTML = releases.length ? `
      <div class="table-wrap">
        <table class="table">
          <thead><tr><th>Versão</th><th>Plataforma</th><th>Hash (BLAKE3)</th><th>Assinatura</th><th>Publicado</th></tr></thead>
          <tbody>
            ${releases.map((r) => `
              <tr>
                <td class="mono">${esc(r.version)}</td>
                <td>${esc(r.platform)}</td>
                <td class="mono">${shortHash(r.artifact_hash, 10)}</td>
                <td class="mono">${shortHash(r.signature, 10)}</td>
                <td>${fmtDate(r.published_at)}</td>
              </tr>`).join('')}
          </tbody>
        </table>
      </div>` : emptyState('Nenhum release');
  } catch (err) {
    table.innerHTML = `<div class="empty-state error"><h3>API indisponível</h3><p>${esc(err.message)}</p></div>`;
  }
}

// ── Componentes locais ─────────────────────────────────────────────────

function card(label, value, iconName, sub = '') {
  return `
    <div class="stat-card">
      <div class="stat-head"><span class="stat-label">${esc(label)}</span><span class="stat-icon">${icon(iconName)}</span></div>
      <div class="stat-value">${value}</div>
      ${sub ? `<div class="stat-sub">${esc(sub)}</div>` : ''}
    </div>`;
}

// ── Poller ─────────────────────────────────────────────────────────────

function createPoller(fn, intervalMs) {
  let stopped = false;
  const tick = async () => {
    if (stopped) return;
    try { await fn(); } catch { /* handled in fn */ }
  };
  const timer = setInterval(tick, intervalMs);
  return { stop() { stopped = true; clearInterval(timer); } };
}
