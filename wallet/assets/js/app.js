// AUGECOIN Wallet — application logic (vanilla JS, no build step).
//
// Two strictly separated layers:
//   Camada 1 — Conta da Plataforma (login/cadastro/perfil/sessão)  → `/api`
//   Camada 2 — Carteira Blockchain (saldo/transferências/compra/venda)
// A Wallet Registry (linked_wallets) é a ponte entre as duas.

import { CONFIG } from './config.js';
import { initTheme } from './theme.js';
import { initHomepageBg } from './homepage-bg.js';
import { esc, auge, fmtNum, fmtTime, shortHash, copy, hexToBytes, bytesToHex, augesatToAuge, deriveShortAddress, deriveAddress, deriveEmbeddedAddress, embeddedPublicKeyHex, isValidAugeAddress } from './utils.js';
import { generateMnemonic, validateMnemonic } from './bip39.js';
import { derivePublicKeyHex } from './crypto.js';
import * as api from './api.js';
import * as rpc from './rpc.js';
import * as ops from './ops.js';
import * as store from './store.js';
import * as ui from './components.js';
import { qrMatrix, qrToCanvas } from './qr.js';

const state = {
  user: null,
  preferences: null,
  mnemonic: null,
  pendingRecovery: null,
  wallets: [],
  chainId: CONFIG.CHAIN_ID,
  poller: null,
  historyScrollY: null,
};

// ── Helpers ────────────────────────────────────────────────────────────

const root = document.getElementById('page-root');

function session() {
  if (!state.mnemonic || !state.user) return null;
  return { mnemonic: state.mnemonic, publicKeyHex: state.user.public_key_hex, chainId: state.chainId };
}

const isOwned = (a) => a && (a.state === 'Owned' || a.state === 'Normal');
const ownedWallets = () => state.wallets.filter((w) => isOwned(w.account));
const hasWallet = () => state.wallets.length > 0;
const userIdentity = () => state.user?.email || state.user?.username || state.user?.display_name;

function augesatFromAuge(v) {
  const n = Number(v);
  if (!isFinite(n) || n < 0) return 0n;
  return BigInt(Math.round(n * 1e8));
}

async function loadWallets() {
  if (!state.user) { state.wallets = []; return; }
  try {
    const linked = await api.listLinkedWallets();
    state.wallets = await Promise.all(linked.map(async (w) => ({
      ...w,
      account: await rpc.getAccountByNumber(w.account_number).catch(() => null),
    })));
  } catch {
    state.wallets = [];
  }
}

async function afterWalletsChanged() {
  await loadWallets();
  ui.renderHeader({ user: state.user, hasWallet: hasWallet() });
}

// ── Boot / router ──────────────────────────────────────────────────────

function currentRoute() {
  const h = location.hash || '#/';
  return h.replace(/^#/, '') || '/';
}

async function boot() {
  initTheme();
  bindGlobalHandlers();
  window.addEventListener('hashchange', router);

  rpc.getNodeStatus().then((s) => { state.chainId = BigInt(s.chain_id); }).catch(() => {});
  rpc.getNodeStatus().then((s) => ui.renderFooter(s)).catch(() => ui.renderFooter());

  // Optimistic first paint: render the public landing immediately so the
  // hero is not blocked on the /api/me round-trip; authenticated sessions
  // are upgraded to the dashboard right after.
  renderVisitor();

  try {
    const res = await api.me();
    if (res && res.user) {
      state.user = res.user;
      state.preferences = res.preferences || null;
      // Always land on the dashboard root after auth, never restore an
      // internal route (e.g. #/augeid) that the browser "back" replayed.
      if (location.hash && location.hash !== '#/') {
        history.replaceState(null, '', location.pathname + location.search);
      }
      await refreshSession();
    }
  } catch {
    // No authenticated session — keep the already-rendered landing.
  }
}

async function refreshSession() {
  await loadWallets();
  ui.renderHeader({ user: state.user, hasWallet: hasWallet() });
  router();
}

async function router() {
  const route = currentRoute();
  if (!state.user) {
    if (route === '/login' || route === '/login/register') { renderAuth(route === '/login/register' ? 'register' : 'login'); return; }
    if (route === '/recover') { renderRecover(); return; }
    if (route === '/mnemonic-setup') { renderMnemonicSetup(); return; }
    renderVisitor();
    return;
  }
  ui.markActiveNav('#' + route);
  root.innerHTML = ui.skeleton();
  try {
    switch (route) {
      case '/marketplace': await renderMarketplace(); break;
      case '/validator': await renderValidator(); break;
      case '/validator/plans': await renderValidatorPlans(); break;
      case '/validator/license': await renderValidatorLicense(); break;
      case '/validator/dashboard': await renderValidatorDashboard(); break;
      case '/billing': await renderBilling(); break;
      case '/augeid': await renderAugeId(); break;
      case '/my-wallets': await renderMyWallets(); break;
      case '/profile': await renderProfile(); break;
      case '/send': await renderSend(); break;
      case '/receive': await renderReceive(); break;
      case '/history': await renderHistory(); break;
      default: if (route.startsWith('/history/')) await renderTransactionDetail(route.slice('/history/'.length)); else await renderDashboard();
    }
  } catch (err) {
    console.error('[wallet] route failed:', err);
    root.innerHTML = ui.errorState(err.message);
  }
}

function bindGlobalHandlers() {
  document.addEventListener('click', async (e) => {
    if (e.target.closest('[data-logout]')) {
      e.preventDefault();
      await api.logout().catch(() => {});
      state.user = null; state.mnemonic = null; state.wallets = [];
      // Replace the current (internal) history entry with the root so the
      // browser "back" never re-plays an internal route like #/augeid.
      history.replaceState(null, '', location.pathname + location.search);
      location.hash = '#/';
      renderVisitor();
    }
    if (e.target.closest('[data-retry]')) { location.reload(); }
    if (e.target.closest('[data-goto]')) {
      e.preventDefault();
      location.hash = e.target.closest('[data-goto]').getAttribute('data-goto');
    }
  });
}

// ── Visitante (landing) ────────────────────────────────────────────────

const TRUST_ITEMS = [
  { icon: 'shield', title: 'Dual-sig Ed25519+Dilithium', desc: 'Assinatura híbrida clássica + pós-quântica desde o gênesis.' },
  { icon: 'hex', title: 'PoA com quórum 2/3+1', desc: 'Finalidade determinística por consenso Proof-of-Authority.' },
  { icon: 'clock', title: 'Blocos de 1 minuto', desc: 'Confirmações rápidas com baixa latência de rede.' },
  { icon: 'coins', title: 'Supply fixo: 750M AUGE', desc: 'Emissão determinística em 50 anos, sem pré-mineração.' },
];

const HOW_STEPS = [
  { n: '1', title: 'Crie sua conta', desc: 'Cadastre-se com usuário e senha. Leva menos de um minuto.' },
  { n: '2', title: 'Guarde seu mnemonic', desc: 'Sua frase de recuperação de 16 palavras é a chave da sua custódia.' },
  { n: '3', title: 'Receba e envie AUGEID', desc: 'Compartilhe seu endereço para receber AUGE e transacione AUGEIDs.' },
];

const FAQ_ITEMS = [
  { q: 'A wallet é custodial?', a: 'Não. Suas chaves são derivadas e armazenadas localmente, criptografadas no seu dispositivo, e nunca saem dele.' },
  { q: 'O que é o AUGEID?', a: 'É um ativo on-chain (conta numerada) emitido pela blockchain. Você pode comprá-lo, recebê-lo ou transferi-lo.' },
  { q: 'Esqueci meu mnemonic. E agora?', a: 'Sem o mnemonic e sem a senha, o acesso à conta é perdido permanentemente. Guarde ambos em local seguro e offline.' },
];

function renderVisitor() {
  ui.renderLandingHeader();
  root.innerHTML = `
    <main class="landing">
      <section class="hero landing-hero">
        <canvas class="hero-canvas" aria-hidden="true"></canvas>
        <div class="hero-content container">
          <span class="hero-eyebrow">Carteira oficial da blockchain AUGECOIN</span>
          <h1>Seus AUGE sob <span class="text-accent">custódia própria</span>.</h1>
          <p class="hero-sub">Identidade on-chain, segurança pós-quântica e baixa latência — sem intermediários entre você e seus ativos.</p>
        </div>
      </section>

      <section class="trust-strip" aria-label="Credenciais da rede">
        <div class="container trust-strip-inner">
          ${TRUST_ITEMS.map((t) => `
            <div class="trust-item">
              <span class="trust-item-icon">${ui.svg(t.icon)}</span>
              <div><div class="trust-item-title">${esc(t.title)}</div><div class="trust-item-desc">${esc(t.desc)}</div></div>
            </div>`).join('')}
        </div>
      </section>

      <section class="container landing-section" id="sobre">
        <div class="landing-section-head">
          <h2>Como funciona</h2>
          <p class="muted">Três passos para entrar na rede.</p>
        </div>
        <div class="how-grid">
          ${HOW_STEPS.map((s) => `
            <div class="how-step card">
              <span class="how-step-num">${esc(s.n)}</span>
              <div class="how-step-title">${esc(s.title)}</div>
              <div class="how-step-desc">${esc(s.desc)}</div>
            </div>`).join('')}
        </div>
      </section>

      <section class="container landing-section" id="seguranca">
        <div class="landing-section-head">
          <h2>Segurança</h2>
          <p class="muted">Custódia própria, sempre.</p>
        </div>
        <div class="security-grid">
          <div class="security-card card">
            <span class="trust-item-icon">${ui.svg('shield')}</span>
            <div class="how-step-title">Chave nunca sai do dispositivo</div>
            <p class="muted">As chaves são derivadas localmente e assinam localmente. O servidor nunca vê sua chave privada.</p>
          </div>
          <div class="security-card card">
            <span class="trust-item-icon">${ui.svg('hex')}</span>
            <div class="how-step-title">Mnemonic de 16 palavras</div>
            <p class="muted">Sua frase de recuperação é a única forma de restaurar o acesso em outro dispositivo.</p>
          </div>
          <div class="security-card card">
            <span class="trust-item-icon">${ui.svg('clock')}</span>
            <div class="how-step-title">Sem custódia de terceiros</div>
            <p class="muted">Você detém as chaves. Perdeu o mnemonic e a senha? O acesso é permanentemente irreversível.</p>
          </div>
        </div>
      </section>

      <section class="container landing-section" id="faq">
        <div class="landing-section-head">
          <h2>Perguntas frequentes</h2>
        </div>
        <div class="faq-list">
          ${FAQ_ITEMS.map((f) => `
            <details class="faq-item card">
              <summary>${esc(f.q)}</summary>
              <p class="muted">${esc(f.a)}</p>
            </details>`).join('')}
        </div>
      </section>
    </main>`;

  initHomepageBg(root.querySelector('.hero-canvas'));

  document.querySelectorAll('[data-goto-auth]').forEach((b) =>
    b.addEventListener('click', () => {
      const m = b.getAttribute('data-goto-auth');
      location.hash = m === 'register' ? '#/login/register' : '#/login';
    }),
  );
  document.querySelectorAll('[data-scroll]').forEach((a) =>
    a.addEventListener('click', (e) => {
      e.preventDefault();
      const target = document.querySelector(a.getAttribute('data-scroll'));
      target?.scrollIntoView({ behavior: 'smooth', block: 'start' });
      document.getElementById('site-nav')?.classList.remove('open');
    }),
  );
}

// ── Auth ───────────────────────────────────────────────────────────────

function renderAuth(mode = 'login', message = '') {
  ui.renderHeader({ user: null, hasWallet: false });
  root.innerHTML = `
    <main class="container page">
      <div class="login-card card">
         <div class="brand brand-footer" style="justify-content:center;margin-bottom:4px">
           <span class="brand-name">AUGECOIN<span class="brand-sub">WALLET</span></span>
        </div>
        <p class="muted center">Identidade da plataforma. Seu AUGEID é um ativo emitido pela blockchain — comprado ou recebido de outro membro.</p>
        ${message ? `<p class="login-error">${esc(message)}</p>` : ''}
        <a class="link small" data-visitor style="display:block;text-align:center;margin-top:10px;cursor:pointer">← Voltar ao início</a>
        <form data-auth style="margin-top:14px">
           ${mode === 'register' ? '<label class="field"><span class="field-label">Nome de usuário</span><input name="username" type="text" placeholder="seu_nome" required autocomplete="username"></label>' : ''}
           <label class="field"><span class="field-label">E-mail</span><input name="email" type="email" placeholder="voce@exemplo.com" required autocomplete="email"></label>
           <label class="field"><span class="field-label">Senha</span><input name="password" type="password" placeholder="••••••••" required></label>
           ${mode === 'register' ? '<label class="field"><span class="field-label">Repetir senha</span><input name="repeat" type="password" required autocomplete="new-password"></label>' : ''}
           <button class="btn btn-primary" type="submit" style="width:100%">${mode === 'register' ? 'Criar conta' : 'Entrar'}</button>
        </form>
         ${mode === 'login' ? '<a class="link small" href="#/recover" style="display:block;text-align:center;margin-top:14px">Recuperar acesso via Mnemônicos</a>' : '<p class="muted small center">Sua frase de recuperação será exibida após o cadastro.</p>'}
      </div>
    </main>`;

  root.querySelector('[data-visitor]')?.addEventListener('click', () => renderVisitor());
  root.querySelector('form[data-auth]').addEventListener('submit', async (e) => {
    e.preventDefault();
    const f = e.target;
     const email = f.email.value.trim();
     const password = f.password.value;
     if (mode === 'register' && password !== f.repeat.value) { renderAuth(mode, 'As senhas não coincidem.'); return; }
     try {
       if (mode === 'register') {
         await doRegister(email, f.username.value.trim(), password);
       } else {
         await doLogin(email, password);
       }
    } catch (err) {
      renderAuth(mode, err.message);
    }
  });
}

function renderMnemonicSetup() {
  const pending = state.pendingRecovery;
  if (!pending) { renderVisitor(); return; }
  ui.renderHeader({ user: null, hasWallet: false });
  const words = pending.mnemonic.split(/\s+/);
  root.innerHTML = `<main class="container page"><div class="card mnemonic-card">
    <div class="page-head center"><h1>Sua Frase de Recuperação</h1><p class="muted">Estas 16 palavras são a única forma de recuperar sua carteira.</p></div>
    <div class="mnemonic-alert mnemonic-alert-error"><strong>ATENÇÃO</strong>Se você esquecer seu usuário e senha e perder este mnemonic, você perde o acesso à sua conta permanentemente. Para acessar esta conta em outro dispositivo, você precisará deste mnemonic.</div>
    <div class="mnemonic-grid">${words.map((word, i) => `<span class="mnemonic-word"><span class="muted">${i + 1}</span>${esc(word)}</span>`).join('')}</div>
    <div class="actions"><button class="btn" data-copy-phrase>Copiar frase</button></div>
    <label class="checkbox-field mnemonic-confirm"><input type="checkbox" data-confirm-phrase> <span>Confirmo que salvei minha frase</span></label>
    <button class="btn btn-primary" data-finish-registration disabled style="width:100%">Continuar</button>
  </div></main>`;
  root.querySelector('[data-copy-phrase]').addEventListener('click', async () => {
    await copy(pending.mnemonic);
    ui.toast('Frase copiada.', 'success');
  });
  root.querySelector('[data-confirm-phrase]').addEventListener('change', (e) => {
    root.querySelector('[data-finish-registration]').disabled = !e.target.checked;
  });
  root.querySelector('[data-finish-registration]').addEventListener('click', finishRegistration);
}

function renderRecover(message = '') {
  ui.renderHeader({ user: null, hasWallet: false });
  root.innerHTML = `<main class="container page"><div class="card login-card"><h1>Recuperar acesso</h1><p class="muted">Informe seus 16 mnemônicos e crie uma nova senha local.</p>${message ? `<p class="login-error">${esc(message)}</p>` : ''}<form data-recover>
    <label class="field"><span class="field-label">Mnemônicos</span><textarea name="phrase" rows="5" required placeholder="16 palavras separadas por espaço"></textarea></label>
    <label class="field"><span class="field-label">E-mail</span><input name="email" type="email" required></label>
    <label class="field"><span class="field-label">Nova senha local</span><input name="password" type="password" required></label>
    <button class="btn btn-primary" type="submit">Recuperar carteira</button></form></div></main>`;
  root.querySelector('form').addEventListener('submit', async (e) => {
    e.preventDefault(); const f = e.target; const phrase = f.phrase.value.trim();
    if (!await validateMnemonic(phrase)) { renderRecover('A frase precisa conter 16 palavras válidas.'); return; }
    try {
      const pub = await derivePublicKeyHex(phrase, CONFIG.DERIVATION_INDEX);
      const { user } = await api.login(f.email.value.trim(), f.password.value);
      await api.updateKey(pub).catch(() => {});
      await store.saveMnemonic(userIdentity() || f.email.value.trim(), phrase, f.password.value);
      state.user = { ...user, public_key_hex: pub }; state.mnemonic = phrase; location.hash = '#/'; await refreshSession();
    } catch (err) { renderRecover(err.message); }
  });
}

async function doRegister(email, username, password) {
  const m = await generateMnemonic();
  const pub = await derivePublicKeyHex(m, CONFIG.DERIVATION_INDEX);
  const { user } = await api.register(email, username, password, pub);
  state.pendingRecovery = { username, password, mnemonic: m, user };
  location.hash = '#/mnemonic-setup';
}

async function finishRegistration() {
  const pending = state.pendingRecovery;
  if (!pending) return;
  const identity = pending.user.email || pending.user.username || pending.username;
  await store.saveMnemonic(identity, pending.mnemonic, pending.password);
  state.user = pending.user;
  state.mnemonic = pending.mnemonic;
  state.pendingRecovery = null;
  state.preferences = { theme: 'dark', language: 'pt-BR', notifications: true };
  renderAddressSetup();
}

/** Post-mnemonic screen: short Base58Check address + QR + explanation. */
async function renderAddressSetup() {
  ui.renderHeader({ user: state.user, hasWallet: hasWallet() });
  const pub = state.user?.public_key_hex;
  const short = pub ? deriveEmbeddedAddress(pub) : '';
  root.innerHTML = `<main class="container page"><div class="card confirm-card">
    <div class="page-head center"><h1>Seu endereço de recebimento</h1><p class="muted">Este endereço recebe AUGE (moeda) e AUGEID.</p></div>
    ${short ? `<div class="receive-address mono">${esc(short)}</div><div class="qr-wrap"><canvas data-qr-address="${esc(short)}"></canvas></div><div class="actions" style="justify-content:center"><button class="btn" data-copy-addr>Copiar endereço</button></div>` : '<p class="muted">Endereço não disponível.</p>'}
    <div data-activate-status class="muted small center" style="margin-top:12px"></div>
    <p class="muted small center">Abrimos uma conta AUGEID para você. Assim que for confirmada, este endereço já recebe AUGE e AUGEID.</p>
    <div class="actions" style="justify-content:center;margin-top:16px"><button class="btn btn-primary" data-finish>Concluir</button></div>
  </div></main>`;
  const c = root.querySelector('[data-qr-address]');
  if (c && short) qrToCanvas(qrMatrix(short), c, 4, 2);
  root.querySelector('[data-copy-addr]')?.addEventListener('click', async () => { await copy(short); ui.toast('Endereço copiado.', 'success'); });
  root.querySelector('[data-finish]').addEventListener('click', async () => { location.hash = '#/'; await refreshSession(); });
  if (pub && short) {
    const status = root.querySelector('[data-activate-status]');
    if (status) status.textContent = 'Endereço pronto. Um AUGEID Reserved será ativado somente quando você receber AUGE.';
  }
}

async function doLogin(email, password) {
  const { user } = await api.login(email, password);
  const identity = user.email || user.username || email;
  const m = await store.loadMnemonic(identity, password).catch(() => null);
  state.user = user;
  state.mnemonic = m;
  if (!m) {
     const has = await store.hasMnemonic(identity).catch(() => false);
    if (has) {
      ui.toast('Chave local encontrada, mas não desbloqueou com essa senha.', 'error');
    } else {
      ui.toast('Nenhuma chave local para esta conta neste dispositivo (modo somente leitura).', 'info');
    }
  }
  location.hash = '#/';
  await refreshSession();
}

// ── Unlock ─────────────────────────────────────────────────────────────

const walletLocked = () => state.user && !state.mnemonic;

/** Full-page lock card shown when the platform session exists but the local
 *  blockchain key is not unlocked on this device (Fase 3). */
function renderLockScreen() {
  root.innerHTML = `<main class="container page">
    <div class="page-head"><h1>Dashboard</h1></div>
    <div class="card lock-card">
      <div class="lock-icon">${ui.svg('shield')}</div>
      <h2>Sua Carteira Blockchain está bloqueada neste dispositivo.</h2>
      <p class="muted">Desbloqueie com sua senha — ou recupere/regene uma nova chave se este dispositivo não tiver a sua chave.</p>
      <div class="actions" style="justify-content:center"><button class="btn btn-primary" data-unlock-wallet>Desbloquear / recuperar carteira</button></div>
    </div>
  </main>`;
  root.querySelector('[data-unlock-wallet]').addEventListener('click', openUnlockModal);
}

/** Modal with the two unlock options: password (if a local key exists) or
 *  rebuild from the 16-word mnemonic. */
async function openUnlockModal() {
  const identity = userIdentity();
  const hasLocal = await store.hasMnemonic(identity).catch(() => false);

  const { close, el } = ui.openModal('Desbloquear carteira', `
    <div class="tabs" style="margin-bottom:16px">
      <button class="${hasLocal ? 'active' : ''}" data-unlock-tab="unlock">Desbloquear</button>
      <button class="${hasLocal ? '' : 'active'}" data-unlock-tab="recover">Recuperar mnemonic</button>
    </div>
    <div data-unlock-panel="unlock" ${hasLocal ? '' : 'style="display:none"'}>
      ${hasLocal ? `<form data-unlock-form>
        <label class="field"><span class="field-label">Senha</span><input type="password" name="pw" required autofocus placeholder="Sua senha"></label>
        <div class="actions"><button class="btn btn-primary" type="submit">Desbloquear</button><button class="btn" type="button" data-cancel>Cancelar</button></div>
      </form>` : '<p class="muted">Nenhuma chave local neste dispositivo — use a opção "Recuperar mnemonic".</p>'}
    </div>
    <div data-unlock-panel="recover" ${hasLocal ? 'style="display:none"' : ''}>
      <p class="muted small">Informe suas 16 palavras para reconstruir a chave neste dispositivo.</p>
      <form data-recover-form>
        <label class="field"><span class="field-label">Mnemonic (16 palavras)</span><textarea name="phrase" rows="4" required placeholder="16 palavras separadas por espaço"></textarea></label>
        <label class="field"><span class="field-label">Senha local</span><input type="password" name="pw" required></label>
        <div class="actions"><button class="btn btn-primary" type="submit">Recuperar</button><button class="btn" type="button" data-cancel>Cancelar</button></div>
      </form>
    </div>`);

  const tabs = el.querySelectorAll('[data-unlock-tab]');
  tabs.forEach((t) => t.addEventListener('click', () => {
    tabs.forEach((x) => x.classList.toggle('active', x === t));
    el.querySelector('[data-unlock-panel="unlock"]').style.display = t.dataset.unlockTab === 'unlock' ? '' : 'none';
    el.querySelector('[data-unlock-panel="recover"]').style.display = t.dataset.unlockTab === 'recover' ? '' : 'none';
  }));

  el.querySelectorAll('[data-cancel]').forEach((b) => b.addEventListener('click', close));

  const unlockForm = el.querySelector('[data-unlock-form]');
  if (unlockForm) {
    unlockForm.addEventListener('submit', async (e) => {
      e.preventDefault();
      const pw = unlockForm.pw.value;
      const m = await store.loadMnemonic(identity, pw).catch(() => null);
      if (!m) {
        unlockForm.insertAdjacentHTML('beforeend', '<p class="login-error">Senha incorreta.</p>');
        return;
      }
      state.mnemonic = m;
      close();
      ui.toast('Carteira desbloqueada.', 'success');
      router();
    });
  }

  el.querySelector('[data-recover-form]').addEventListener('submit', async (e) => {
    e.preventDefault();
    const f = e.target;
    const phrase = f.phrase.value.trim();
    if (!await validateMnemonic(phrase)) {
      f.insertAdjacentHTML('beforeend', '<p class="login-error">A frase precisa conter 16 palavras válidas.</p>');
      return;
    }
    try {
      const pub = await derivePublicKeyHex(phrase, CONFIG.DERIVATION_INDEX);
      await api.updateKey(pub).catch(() => {});
      await store.saveMnemonic(identity, phrase, f.pw.value);
      state.user = { ...state.user, public_key_hex: pub };
      state.mnemonic = phrase;
      close();
      ui.toast('Chave recuperada neste dispositivo.', 'success');
      router();
    } catch (err) {
      f.insertAdjacentHTML('beforeend', `<p class="login-error">${esc(err.message)}</p>`);
    }
  });
}

function promptPassword(message) {
  return new Promise((resolve) => {
    const { close, el } = ui.openModal('Desbloquear carteira', `
      <p class="muted">${esc(message)}</p>
      <form>
        <label class="field"><span class="field-label">Senha</span><input type="password" name="pw" required autofocus></label>
        <div class="actions">
          <button class="btn btn-primary" type="submit">Desbloquear</button>
          <button class="btn" type="button" data-cancel>Cancelar</button>
        </div>
      </form>`);
    el.querySelector('form').addEventListener('submit', (e) => { e.preventDefault(); const pw = el.querySelector('[name="pw"]').value; close(); resolve(pw); });
    el.querySelector('[data-cancel]').addEventListener('click', () => { close(); resolve(null); });
  });
}

async function requireSession() {
  const s = session();
  if (s) return s;
  if (!state.user) return null;

  const hasLocal = await store.hasMnemonic(userIdentity()).catch(() => false);
  if (!hasLocal) {
    return recoverKey();
  }

  const pw = await promptPassword('Para assinar o aceite, sua chave local precisa ser desbloqueada. Digite a senha da sua conta (a mesma do cadastro/login).');
  if (!pw) return null;
  const m = await store.loadMnemonic(userIdentity(), pw).catch(() => null);
  if (!m) { ui.toast('Senha incorreta.', 'error'); return null; }
  state.mnemonic = m;
  return session();
}

/** Key recovery: generate a fresh key, update it on the backend, store locally. */
async function recoverKey() {
  if (!state.user) return null;

  const { close, el } = ui.openModal('Recuperar chave', `
    <p class="muted">Nenhuma chave local foi encontrada para esta conta. Você pode <strong>gerar uma nova chave</strong> e atualizá-la na sua conta da plataforma.</p>
    <p class="muted small">Atenção: a chave antiga será substituída — AUGEIDs vinculados à chave antiga ficarão inacessíveis. Após recuperar, você precisará receber um novo AUGEID.</p>
    <form>
      <label class="field"><span class="field-label">Senha da conta (para criptografar a nova chave)</span><input type="password" name="pw" required autofocus></label>
      <div class="actions">
        <button class="btn btn-primary" type="submit">Gerar nova chave</button>
        <button class="btn" type="button" data-cancel>Cancelar</button>
      </div>
    </form>`);

  const pw = await new Promise((resolve) => {
    el.querySelector('form').addEventListener('submit', (e) => { e.preventDefault(); const v = el.querySelector('[name="pw"]').value; close(); resolve(v); });
    el.querySelector('[data-cancel]').addEventListener('click', () => { close(); resolve(null); });
  });
  if (!pw) return null;

  try {
    // Verify the password against the backend (also refreshes the session).
    await api.login(userIdentity(), pw);
  } catch {
    ui.toast('Senha incorreta.', 'error');
    return null;
  }

  try {
    const m = await generateMnemonic();
    const pub = await derivePublicKeyHex(m, CONFIG.DERIVATION_INDEX);
    await api.updateKey(pub);
    await store.saveMnemonic(userIdentity(), m, pw);
    state.user = { ...state.user, public_key_hex: pub };
    state.mnemonic = m;
    ui.toast('Nova chave gerada e salva. Você precisa receber um novo AUGEID para ativar a carteira.', 'success');
    ui.renderHeader({ user: state.user, hasWallet: hasWallet() });
    return session();
  } catch (err) {
    ui.toast(err.message, 'error');
    return null;
  }
}

function promptText(title, label, { placeholder = '', required = true } = {}) {
  return new Promise((resolve) => {
    const { close, el } = ui.openModal(title, `
      <form>
        <label class="field"><span class="field-label">${esc(label)}</span><input name="value" placeholder="${esc(placeholder)}" ${required ? 'required' : ''}></label>
        <div class="actions">
          <button class="btn btn-primary" type="submit">Confirmar</button>
          <button class="btn" type="button" data-cancel>Cancelar</button>
        </div>
      </form>`);
    el.querySelector('form').addEventListener('submit', (e) => { e.preventDefault(); const v = el.querySelector('[name="value"]').value; close(); resolve(v); });
    el.querySelector('[data-cancel]').addEventListener('click', () => { close(); resolve(null); });
  });
}

async function selectMember(title) {
  let members = [];
  try { members = await api.directory(); } catch { /* ignore */ }
  return new Promise((resolve) => {
    const opts = members.map((m) => `<option value="${esc(m.public_key_hex)}">${esc(m.display_name)} (${esc(m.email)})</option>`).join('');
    const { close, el } = ui.openModal(title, `
      <form>
        <label class="field"><span class="field-label">Destinatário</span>
          <select name="member" required><option value="">Selecione um membro…</option>${opts}</select></label>
        <div class="actions">
          <button class="btn btn-primary" type="submit">Confirmar</button>
          <button class="btn" type="button" data-cancel>Cancelar</button>
        </div>
      </form>`);
    el.querySelector('form').addEventListener('submit', (e) => { e.preventDefault(); const v = el.querySelector('[name="member"]').value; close(); resolve(v || null); });
    el.querySelector('[data-cancel]').addEventListener('click', () => { close(); resolve(null); });
  });
}

// ── Cards ──────────────────────────────────────────────────────────────

function platformAccountCard() {
  const u = state.user;
  return `
    <div class="card platform-account-card">
      ${ui.platformChip()}
      <div class="platform-account-name">${esc(u.display_name)}</div>
      <div class="muted mono">${esc(u.email)}</div>
      <div class="muted small">Identidade da plataforma · sem saldo</div>
    </div>`;
}

function walletCard(w) {
  const a = w.account;
  return `
    <div class="card blockchain-wallet-card">
      <div class="wallet-card-head">
        <div>
          ${ui.walletChip()}
          <div class="account-card-number">AUGEID #${fmtNum(w.account_number)}</div>
          <div class="account-card-name">${esc(a?.name || 'Sem nome')}</div>
        </div>
        ${a ? ui.statusBadge(a.state) : ''}
      </div>
      <div class="account-card-balance">
        <span class="muted">Saldo</span>
        <strong>${a ? auge(a.balance) : '—'} AUGE</strong>
      </div>
      <div class="qr-wrap"><canvas data-qr="${w.account_number}"></canvas></div>
    </div>`;
}

function emptyWalletCard() {
  return `
    <div class="card empty-wallet-card">
      <span class="layer-chip layer-wallet">Carteira Blockchain ainda não ativada</span>
      <p class="muted">Você ainda não possui um AUGEID. Para movimentar AUGE, compre um AUGEID no marketplace ou receba um de outro membro.</p>
      <div class="actions">
        <a class="btn btn-primary" href="#/marketplace">Comprar AUGEID</a>
        <a class="btn" href="#/receive">Receber AUGEID</a>
      </div>
    </div>`;
}

function drawQr(host) {
  host.querySelectorAll('canvas[data-qr]').forEach((c) => {
    qrToCanvas(qrMatrix('augeid:' + c.getAttribute('data-qr')), c, 4, 2);
  });
}

function walletAddress(wallet) {
  const account = wallet?.account;
  if (account?.address) return account.address;
  if (account?.account_key_ed_hex) return deriveAddress(account.account_key_ed_hex);
  return account?.account_address || '';
}

/** Base58 payment address derived from the account's Ed25519 key. */
function walletShortAddress(wallet) {
  const account = wallet?.account;
  if (account?.address) return account.address;
  if (account?.account_key_ed_hex) return deriveShortAddress(account.account_key_ed_hex);
  return '';
}

/** Truncated address: `3Fb2...HzpM`. */
function shortAddr(address) {
  const a = String(address || '');
  return a.length > 12 ? `${a.slice(0, 5)}...${a.slice(-4)}` : a;
}

async function renderAugeId() {
  const owned = ownedWallets();
  root.innerHTML = `
    <main class="container page">
      <div class="page-head"><h1>Enviar AUGEID</h1><p class="muted">Transfira um AUGEID sob sua custódia para outro membro.</p></div>
      <div class="card login-card">
        ${owned.length === 0 ? '<p class="muted">Nenhum AUGEID sob custódia.</p>' : `
        <form data-augeid-transfer>
          <label class="field"><span class="field-label">De qual conta enviar</span>
            <select name="account" required>${owned.map((w) => `<option value="${w.account_number}">AUGEID #${fmtNum(w.account_number)}${w.account?.name ? ` — ${esc(w.account.name)}` : ''}</option>`).join('')}</select></label>
          <label class="field"><span class="field-label">Endereço de destino</span><input name="destination" placeholder="Chave pública (64 hex), nome ou endereço Base58" required></label>
          <label class="field"><span class="field-label">Senha</span><input name="password" type="password" placeholder="Sua senha para assinar" required></label>
          <div data-send-status></div>
          <button class="btn btn-primary" type="submit" data-send-btn>Enviar</button>
        </form>`}
      </div>
    </main>`;

  const form = root.querySelector('form'); if (!form) return;
  const statusEl = root.querySelector('[data-send-status]');
  const sendBtn = root.querySelector('[data-send-btn]');
  const setStatus = (kind, text) => {
    const icon = kind === 'success'
      ? '<span class="status-dot status-success"><span class="dot"></span></span>'
      : kind === 'error'
        ? '<span class="status-dot status-error"><span class="dot"></span></span>'
        : '<span class="spinner"></span>';
    statusEl.innerHTML = `<div class="send-status send-status-${kind}">${icon}<span>${esc(text)}</span></div>`;
  };

  form.addEventListener('submit', async (e) => {
    e.preventDefault();
    const wallet = owned.find((item) => item.account_number === Number(form.account.value));
    const destination = form.destination.value.trim();
    const password = form.password.value;

    if (!password) { setStatus('error', 'Informe sua senha.'); return; }
    if (BigInt(wallet.account?.balance || 0) < BigInt(CONFIG.MIN_FEE_AUGESAT)) { setStatus('error', 'Saldo insuficiente para pagar a taxa de rede.'); return; }

    // Validate password locally (decrypt the key) before signing.
    const m = state.mnemonic || await store.loadMnemonic(userIdentity(), password).catch(() => null);
    if (!m) { setStatus('error', 'Senha incorreta.'); return; }
    state.mnemonic = m;

    let key = null;
    try { key = await resolveDestinationKey(destination); } catch { key = null; }
    if (!key) { setStatus('error', 'Endereço de destino inválido ou não encontrado.'); return; }

    setStatus('sending', 'Enviando para a rede…');
    sendBtn.disabled = true;
    try {
      const res = await ops.submitChangeKey(session(), { account: wallet.account_number, nOperation: wallet.account.n_operation, fee: Number(CONFIG.MIN_FEE_AUGESAT), newPublicKeyHex: key });
      if (!res.accepted) { setStatus('error', res.error || 'Transferência rejeitada.'); return; }
      setStatus('waiting', 'Aguardando confirmação de bloco…');
      const confirmed = await confirmOperation(res.op_hash_hex);
      if (confirmed) {
        setStatus('success', `Confirmado — bloco #${fmtNum(confirmed.block_number)}`);
        await afterWalletsChanged();
      } else {
        setStatus('waiting', `Enviado, ainda aguardando confirmação (${shortHash(res.op_hash_hex, 8)})`);
      }
    } catch (err) {
      setStatus('error', err.message);
    } finally {
      sendBtn.disabled = false;
    }
  });
}

/** Resolve a destination to an Ed25519 public key (hex), or null. */
async function resolveDestinationKey(destination) {
  const d = String(destination || '').trim();
  if (!d) return null;
  if (/^[0-9a-f]{64}$/i.test(d)) return d.toLowerCase();
  const embedded = embeddedPublicKeyHex(d);
  if (embedded) return embedded;
  // Canonical Base58 address — the node resolves it by scanning accounts.
  // Falls back to name resolution below.
  try {
    const acc = await rpc.getAccount({ address: d });
    if (acc && acc.account_key_ed_hex) return acc.account_key_ed_hex;
  } catch { /* not a resolvable address — try name */ }
  return (await rpc.resolveName(d))?.account_key_ed_hex || null;
}

/** Poll getoperationbyhash until the operation lands in a block (or timeout). */
async function confirmOperation(opHash, tries = 40, intervalMs = 8000) {
  for (let i = 0; i < tries; i++) {
    try {
      const res = await rpc.getOperationByHash(opHash);
      if (res && res.block_number != null) return res;
    } catch { /* not yet confirmed */ }
    await new Promise((r) => setTimeout(r, intervalMs));
  }
  return null;
}

// ── Dashboard ──────────────────────────────────────────────────────────

async function renderDashboard() {
  if (walletLocked()) { renderLockScreen(); return; }
  const owned = ownedWallets();
  const totalBalance = owned.reduce((s, w) => s + BigInt(w.account?.balance || 0), 0n);
  const name = state.user?.display_name || state.user?.email || '';

  root.innerHTML = `
    <main class="container page">
      <div class="dashboard-greeting">
        <h1>Olá, <span class="greeting-name">${esc(name)}</span></h1>
        <p class="muted">Visão geral da sua carteira.</p>
      </div>

      <div class="balance-card">
        <div class="balance-label">Saldo total</div>
        <div class="balance-value">${auge(totalBalance)}<span class="balance-unit">AUGE</span></div>
        <div class="quick-actions">
          <a class="quick-action" href="#/send"><span class="quick-action-icon send">${ui.svg('send')}</span><span class="quick-action-label">Enviar</span></a>
          <a class="quick-action" href="#/receive"><span class="quick-action-icon receive">${ui.svg('receive')}</span><span class="quick-action-label">Receber</span></a>
          <a class="quick-action" href="#/augeid"><span class="quick-action-icon augeid">${ui.svg('hex')}</span><span class="quick-action-label">AUGEID</span></a>
          <a class="quick-action" href="#/history"><span class="quick-action-icon history">${ui.svg('history')}</span><span class="quick-action-label">Histórico</span></a>
        </div>
      </div>

      <section class="grid-stats">
        ${ui.statCard('Saldo total', auge(totalBalance), 'coins')}
        ${ui.statCard('Carteiras vinculadas', fmtNum(state.wallets.length), 'users')}
        ${ui.statCard('Contas ativas', fmtNum(owned.length), 'wallet')}
      </section>

      ${!hasWallet() ? `<div class="section">${emptyWalletCard()}</div>` : ''}

      <div class="section">
        <div class="section-head"><h2>Evolução do saldo</h2><span class="muted small">últimas operações confirmadas</span></div>
        <div class="card" id="chart-box"></div>
      </div>

      <div class="section">
        <div class="section-head"><h2>Últimas operações</h2><a class="link small" href="#/history">ver todas</a></div>
        <div id="recent-tx"></div>
      </div>
    </main>`;

  const items = await loadHistoryItems();
  renderChart(root.querySelector('#chart-box'), items, totalBalance);
  renderRecentTransactions(root.querySelector('#recent-tx'), items);
}

async function loadHistoryItems(force = false) {
  if (!force && state.transactions) return state.transactions;
  const myAccounts = new Set(state.wallets.map((w) => Number(w.account_number)));
  if (myAccounts.size === 0) return [];
  let height = 0;
  try { height = Number((await rpc.getNodeStatus()).current_height); } catch { /* ignore */ }
  if (!height) return [];
  const items = await fetchHistory(height, myAccounts);
  state.transactions = items;
  return items;
}

/** Reconstruct a balance-over-time series from real operations, anchored at
 *  the current total balance. */
function computeBalanceSeries(items, currentBalance) {
  const sorted = [...items].sort((a, b) => b.block - a.block);
  let balance = BigInt(currentBalance || 0);
  const series = [];
  for (const it of sorted) {
    const amt = BigInt(it.amount ?? 0);
    if (it.direction === 'send') balance += amt;
    else if (it.direction === 'receive') balance -= amt;
    series.push({ block: it.block, balance });
  }
  return series.reverse();
}

function renderChart(box, items, totalBalance) {
  if (!box) return;
  const series = computeBalanceSeries(items, totalBalance);
  if (series.length < 2) {
    box.innerHTML = `<div class="empty-state"><h3>Sem dados suficientes</h3><p>O gráfico aparece após as primeiras operações confirmadas.</p></div>`;
    return;
  }
  box.innerHTML = `<canvas data-balance-chart style="width:100%;height:220px;display:block"></canvas>`;
  const canvas = box.querySelector('[data-balance-chart]');
  drawBalanceChart(canvas, series);
}

function drawBalanceChart(canvas, series) {
  if (!canvas) return;
  const dpr = Math.min(window.devicePixelRatio || 1, 2);
  const w = canvas.clientWidth || canvas.parentElement.clientWidth || 300;
  const h = canvas.clientHeight || 220;
  canvas.width = Math.floor(w * dpr);
  canvas.height = Math.floor(h * dpr);
  const ctx = canvas.getContext('2d');
  if (!ctx) return;
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.clearRect(0, 0, w, h);

  const css = getComputedStyle(document.documentElement);
  const gridColor = css.getPropertyValue('--chart-grid').trim() || 'rgba(148,163,184,0.12)';
  const lineColor = css.getPropertyValue('--color-primary').trim() || '#F97316';
  const fillColor = css.getPropertyValue('--chart-fill').trim() || 'rgba(249,115,22,0.16)';

  const balances = series.map((s) => Number(s.balance));
  const min = Math.min(...balances);
  const max = Math.max(...balances);
  const range = max - min || 1;
  const pad = 24;
  const plotW = w - pad * 2;
  const plotH = h - pad * 2;

  ctx.strokeStyle = gridColor;
  ctx.lineWidth = 1;
  for (let i = 0; i <= 4; i++) {
    const gy = pad + (plotH / 4) * i;
    ctx.beginPath();
    ctx.moveTo(pad, gy);
    ctx.lineTo(w - pad, gy);
    ctx.stroke();
  }

  const px = (i) => pad + (series.length === 1 ? plotW / 2 : (plotW * i) / (series.length - 1));
  const py = (b) => pad + plotH - ((b - min) / range) * plotH;

  ctx.beginPath();
  ctx.moveTo(px(0), py(balances[0]));
  series.forEach((s, i) => ctx.lineTo(px(i), py(Number(s.balance))));
  ctx.lineTo(px(series.length - 1), h - pad);
  ctx.lineTo(px(0), h - pad);
  ctx.closePath();
  ctx.fillStyle = fillColor;
  ctx.fill();

  ctx.beginPath();
  series.forEach((s, i) => { const x = px(i); const y = py(Number(s.balance)); i === 0 ? ctx.moveTo(x, y) : ctx.lineTo(x, y); });
  ctx.strokeStyle = lineColor;
  ctx.lineWidth = 2;
  ctx.stroke();
}

function renderRecentTransactions(host, items) {
  if (!host) return;
  if (!items || items.length === 0) {
    host.innerHTML = `<div class="card">${ui.emptyState('Nenhuma operação recente.')}</div>`;
    return;
  }
  const recent = items.slice(0, 5);
  host.innerHTML = `<div class="card" style="padding:0">${recent.map((tx) => {
    const marker = tx.direction === 'send' ? 'tx-send' : 'tx-receive';
    const label = tx.direction === 'send' ? 'Enviado' : 'Recebido';
    const counterparty = tx.counterparty != null ? `#${fmtNum(tx.counterparty)}` : '—';
    const value = tx.amount != null ? `${auge(tx.amount)} AUGE` : '—';
    return `<a class="recent-tx" href="#/history/${encodeURIComponent(tx.opHash)}"><span class="tx-marker ${marker}"></span><span class="recent-tx-label">${label}</span><span class="recent-tx-counter mono">${counterparty}</span><span class="recent-tx-value price">${value}</span><span class="recent-tx-time muted small">${esc(fmtTime(tx.timestamp))}</span></a>`;
  }).join('')}</div>`;
}

// ── Marketplace ────────────────────────────────────────────────────────

async function renderMarketplace() {
  root.innerHTML = `<main class="container page">
    <div class="page-head"><h1>Marketplace</h1><p class="muted">AUGEIDs disponíveis para compra.</p></div>
    <div class="toolbar">
      <input id="mp-search" placeholder="Buscar por nome ou número">
      <select id="mp-sort">
        <option value="number">Número do AUGEID</option>
        <option value="price-asc">Menor preço</option>
        <option value="price-desc">Maior preço</option>
        <option value="recent">Mais recente</option>
      </select>
    </div>
    <div id="mp-body">${ui.skeleton()}</div></main>`;
  const host = root.querySelector('#mp-body');
  const searchEl = root.querySelector('#mp-search');
  const sortEl = root.querySelector('#mp-sort');
  try {
    const res = await rpc.listAccountsForSale();
    const items = await Promise.all(res.entries.map(async (e) => {
      const info = await rpc.getAccountByNumber(e.account_number).catch(() => null);
      const seller = (await api.directoryByKey(e.seller_public_key_hex).catch(() => null)) || 'Validador';
      return { ...e, name: info?.name ?? null, account_to_pay: info?.account_to_pay ?? 0, sellerName: seller };
    }));

    const render = () => {
      const term = (searchEl.value || '').trim().toLowerCase();
      let list = items.filter((i) => {
        if (!term) return true;
        const num = String(i.account_number);
        const name = (i.name || '').toLowerCase();
        return num.includes(term) || name.includes(term);
      });
      const sort = sortEl.value;
      list = [...list].sort((a, b) => {
        if (sort === 'price-asc') return a.price - b.price;
        if (sort === 'price-desc') return b.price - a.price;
        if (sort === 'recent') return b.listed_at_block - a.listed_at_block;
        return a.account_number - b.account_number;
      });

      if (list.length === 0) { host.innerHTML = ui.emptyState('Nenhum AUGEID à venda.'); return; }

      host.innerHTML = `<div class="grid-2">${list.map((it) => `
        <div class="card">
          <div class="card-head-row">
            <span class="account-card-number">AUGEID #${fmtNum(it.account_number)}</span>
            ${ui.badge('warning', 'ForSale')}
          </div>
          <div class="marketplace-card-name">${esc(it.name || 'Sem nome')}</div>
          <dl class="marketplace-card-fields">
            <div><dt>Preço</dt><dd class="price">${auge(it.price)} AUGE</dd></div>
            <div><dt>Vendedor</dt><dd>${esc(it.sellerName)}</dd></div>
          </dl>
          <button class="btn btn-primary" data-buy="${it.account_number}">Comprar</button>
        </div>`).join('')}</div>`;

      host.querySelectorAll('[data-buy]').forEach((btn) => btn.addEventListener('click', async () => {
        const num = Number(btn.getAttribute('data-buy'));
        const item = items.find((i) => i.account_number === num);
        await buyFlow(item);
      }));
    };

    searchEl.addEventListener('input', render);
    sortEl.addEventListener('change', render);
    render();
  } catch (err) {
    host.innerHTML = ui.errorState(err.message);
  }
}

async function buyFlow(item) {
  const s = await requireSession();
  if (!s) return;
  const buyer = ownedWallets()[0];
  if (!buyer) { ui.toast('Você precisa de um AUGEID com saldo para financiar a compra.', 'error'); return; }

  const { close } = ui.openModal('Confirmar compra', `
    <p>Você está comprando o <strong>AUGEID #${fmtNum(item.account_number)}</strong>${item.name ? ` (${esc(item.name)})` : ''}.</p>
    <p>Preço: <strong class="price">${auge(item.price)} AUGE</strong></p>
    <p class="muted">Vendedor: ${esc(item.sellerName)}</p>
    <div class="actions">
      <button class="btn btn-primary" data-confirm>Confirmar compra</button>
      <button class="btn" data-cancel>Cancelar</button>
    </div>`);
  document.querySelector('.modal [data-confirm]').addEventListener('click', async () => {
    close();
    try {
      const res = await ops.submitBuy(s, {
        buyerAccount: buyer.account.account_number,
        nOperation: buyer.account.n_operation,
        accountToPurchase: item.account_number,
        amount: item.price,
        fee: Number(CONFIG.MIN_FEE_AUGESAT),
        newPublicKeyHex: s.publicKeyHex,
        sellerAccount: item.account_to_pay,
      });
      if (res.accepted) {
        await api.linkWallet(item.account_number).catch(() => {});
        await afterWalletsChanged();
        ui.toast(`AUGEID #${fmtNum(item.account_number)} comprado!`, 'success');
        router();
      } else {
        ui.toast(res.error || 'Compra rejeitada.', 'error');
      }
    } catch (err) { ui.toast(err.message, 'error'); }
  });
  document.querySelector('.modal [data-cancel]').addEventListener('click', close);
}

// ── Validador ──────────────────────────────────────────────────────────

function fmtAuge(s) {
  if (s === undefined || s === null) return '0 AUGE';
  try {
    const raw = auge(s);
    const [int, frac] = raw.split('.');
    const trimmed = (frac || '').replace(/0+$/, '');
    return trimmed ? `${int}.${trimmed} AUGE` : `${int} AUGE`;
  } catch {
    return '0 AUGE';
  }
}

function fmtUptime(s) {
  const n = Number(s || 0);
  if (!n) return '—';
  const d = Math.floor(n / 86400);
  const h = Math.floor((n % 86400) / 3600);
  return `${d}d ${h}h`;
}

const PLAN_LABELS = { monthly: 'Mensal', semiannual: 'Semestral', annual: 'Anual' };
const METHOD_LABELS = { pix: 'PIX', usdt: 'USDT', auge: 'AUGE' };
const PLAN_BENEFITS = {
  monthly: ['Validação de blocos', 'Ganhos em AUGE e AUGEIDs', 'Suporte por e-mail'],
  semiannual: ['Tudo do plano mensal', '2 meses grátis', 'Prioridade no suporte'],
  annual: ['Tudo do plano semestral', '4 meses grátis', 'Suporte prioritário 24/7'],
};

/** Instruções de pagamento conforme o método escolhido no pedido. */
function paymentInstruction(order, payment) {
  const p = payment || {};
  if (order.method === 'pix') {
    if (!p.pix_key) return '<p class="muted">A chave PIX será informada em breve.</p>';
    return `<div class="kv"><span class="kv-label">Chave PIX</span><span class="kv-value mono">${esc(p.pix_key)} ${ui.copyButton(p.pix_key)}</span></div>`;
  }
  if (order.method === 'usdt') {
    if (!p.usdt_address) return '<p class="muted">O endereço USDT será informado em breve.</p>';
    return `
      <div class="kv"><span class="kv-label">Rede</span><span class="kv-value">${esc(p.usdt_network || '—')}</span></div>
      <div class="kv"><span class="kv-label">Endereço</span><span class="kv-value mono">${esc(p.usdt_address)} ${ui.copyButton(p.usdt_address)}</span></div>`;
  }
  if (order.method === 'auge') {
    if (!p.auge_address) return '<p class="muted">O endereço AUGE será informado em breve.</p>';
    return `
      <div class="kv"><span class="kv-label">Rede</span><span class="kv-value">${esc(p.auge_network || '—')}</span></div>
      <div class="kv"><span class="kv-label">Endereço</span><span class="kv-value mono">${esc(p.auge_address)} ${ui.copyButton(p.auge_address)}</span></div>`;
  }
  return '';
}

function licenseStatusBadge(s) {
  switch (s) {
    case 'active': return ui.badge('success', 'Ativa');
    case 'expired': return ui.badge('warning', 'Expirada');
    case 'suspended': return ui.badge('admin', 'Suspensa');
    default: return ui.badge('muted', s || 'Desconhecida');
  }
}

function orderStatusBadge(o) {
  switch (o.status) {
    case 'pending': return ui.badge('warning', 'Aguardando pagamento');
    case 'paid': return ui.badge('success', 'Pago');
    case 'issued': return ui.badge('info', 'Emitida');
    default: return ui.badge('muted', o.status || 'Desconhecido');
  }
}

function validatorStatusBadge(v) {
  if (v.status === 'pending') return ui.badge('muted', 'Pendente');
  if (v.status === 'suspended') return ui.badge('admin', 'Suspenso');
  if (v.status === 'revoked') return ui.badge('muted', 'Revogado');
  return v.online ? ui.badge('success', 'Online') : ui.badge('warning', 'Offline');
}

async function renderValidator() {
  root.innerHTML = `
    <main class="container page">
      <div class="page-head"><h1>Seja um Validador</h1><p class="muted">Execute um nó, produza blocos e receba recompensas em AUGE e AUGEIDs.</p></div>

      <div class="card">
        <div class="section-head"><h2>Como funciona</h2></div>
        <ol class="how-it-works">
          <li>Escolha um plano e compre uma licença.</li>
          <li>Baixe e instale o software Validador (Windows ou Linux).</li>
          <li>Ative com sua licença — o software gera a chave Ed25519 localmente.</li>
          <li>O servidor valida a licença e registra a chave pública.</li>
          <li>A blockchain autoriza o validador e ele começa a produzir blocos.</li>
          <li>Acompanhe os ganhos em tempo real.</li>
        </ol>
      </div>

      <section class="grid-stats">
        ${ui.statCard('Recompensa por bloco', '7.25 AUGE', 'coins')}
        ${ui.statCard('AUGEIDs por bloco', '3', 'hex')}
        ${ui.statCard('Tempo online', '24/7', 'clock')}
      </section>

      <div class="card">
        <div class="section-head"><h2>Requisitos</h2></div>
        <ul class="requirements">
          <li>Sistema Windows ou Linux (64-bit).</li>
          <li>Conexão estável com a internet.</li>
          <li>Mínimo 4 GB de RAM e 20 GB de disco.</li>
          <li>A chave privada fica sempre na sua máquina — nunca sai dela.</li>
        </ul>
      </div>

      <div class="actions" style="margin-top:20px">
        <a class="btn btn-primary" href="#/validator/plans">Quero ser Validador</a>
      </div>
    </main>`;
}

async function renderValidatorPlans() {
  root.innerHTML = `<main class="container page"><div class="page-head"><h1>Planos</h1><p class="muted">Escolha o plano de validação e comece a produzir blocos.</p></div><div id="vp-body">${ui.skeleton()}</div></main>`;
  const host = root.querySelector('#vp-body');

  let plans = [];
  try {
    const res = await api.listPlans();
    plans = res.plans || [];
  } catch (err) {
    host.innerHTML = ui.errorState(err.message);
    return;
  }

  let selectedPlan = null;
  let method = 'pix';
  let creating = false;

  const render = (order = null, payment = null) => {
    if (order) {
      host.innerHTML = `
        <div class="page-head"><h2>Pedido de licença</h2><p class="muted">Aguardando confirmação do pagamento.</p></div>
        <div class="card">
          <div class="section-head"><h2>Resumo</h2></div>
          <div class="kv"><span class="kv-label">Plano</span><span class="kv-value">${esc(PLAN_LABELS[order.plan] || order.plan)} — US$${esc(order.amount_usd)}</span></div>
          <div class="kv"><span class="kv-label">Método</span><span class="kv-value">${esc(METHOD_LABELS[order.method] || order.method)}</span></div>
          <div class="kv"><span class="kv-label">Referência</span><span class="kv-value mono">${esc(order.id)}</span></div>
          <div class="kv"><span class="kv-label">Status</span><span class="kv-value">${ui.badge('warning', 'Aguardando pagamento')}</span></div>
        </div>
        <div class="card">
          <div class="section-head"><h2>Como pagar (${esc(METHOD_LABELS[order.method] || order.method)})</h2></div>
          ${paymentInstruction(order, payment)}
          <p class="muted" style="margin-top:8px">Use a referência <strong class="mono">${esc(order.id)}</strong> ao pagar. Assim que o pagamento for confirmado, emita sua licença na página <a href="#/validator/license">Minha Licença</a>.</p>
        </div>
        <div class="actions" style="margin-top:16px">
          <a class="btn btn-primary" href="#/validator/license">Ver minha licença</a>
          <button class="btn" data-new-order>Novo pedido</button>
        </div>`;
      ui.bindCopyButtons(host);
      host.querySelector('[data-new-order]').addEventListener('click', () => { selectedPlan = null; render(); });
      return;
    }

    const plansHtml = plans.map((p) => `
      <div class="card plan-card">
        <h3 class="plan-name">${esc(p.label)}</h3>
        <div class="plan-price">US$${esc(p.usd)}<span class="plan-period">/plano</span></div>
        ${p.discount ? ui.badge('accent', p.discount) : ''}
        <ul class="plan-benefits">${(PLAN_BENEFITS[p.id] || []).map((b) => `<li>${esc(b)}</li>`).join('')}</ul>
        <button class="btn btn-primary btn-block" data-plan="${esc(p.id)}">Comprar</button>
      </div>`).join('');

    host.innerHTML = `
      <div class="grid-3">${plansHtml}</div>
      ${selectedPlan ? `
        <div class="card" style="margin-top:16px">
          <div class="section-head"><h2>Método de pagamento — ${esc(selectedPlan.label)}</h2></div>
          <div class="method-options">
            ${['pix', 'usdt', 'auge'].map((m) => `<label class="method-option"><input type="radio" name="method" value="${m}" ${method === m ? 'checked' : ''}> <span>${esc(METHOD_LABELS[m])}</span></label>`).join('')}
          </div>
          <div class="actions">
            <button class="btn btn-primary" data-continue ${creating ? 'disabled' : ''}>${creating ? 'Criando pedido…' : 'Continuar'}</button>
            <button class="btn btn-ghost" data-cancel-plan>Cancelar</button>
          </div>
        </div>` : ''}`;

    host.querySelectorAll('[data-plan]').forEach((b) => b.addEventListener('click', () => {
      selectedPlan = plans.find((p) => p.id === b.getAttribute('data-plan')) || null;
      render();
    }));

    host.querySelectorAll('input[name="method"]').forEach((r) => r.addEventListener('change', () => { method = r.value; }));

    host.querySelector('[data-cancel-plan]')?.addEventListener('click', () => { selectedPlan = null; render(); });

    host.querySelector('[data-continue]')?.addEventListener('click', async () => {
      if (!selectedPlan) return;
      creating = true; render();
      try {
        const [orderRes, payRes] = await Promise.all([
          api.createOrder({ plan: selectedPlan.id, method }),
          api.getPaymentInfo().catch(() => null),
        ]);
        render(orderRes.order, payRes ? payRes.payment : null);
      } catch (err) {
        ui.toast(err.message, 'error');
        creating = false; render();
      }
    });
  };

  render();
}

async function renderValidatorLicense() {
  root.innerHTML = `<main class="container page"><div class="page-head"><h1>Minha Licença</h1><p class="muted">Sua licença de validação e o download do software.</p></div><div id="vl-body">${ui.skeleton()}</div></main>`;
  const host = root.querySelector('#vl-body');

  let orders = [], licenses = [], downloads = {};
  try {
    const [o, ov, d] = await Promise.all([api.listOrders(), api.getOverview(), api.getDownloads()]);
    orders = o; licenses = ov.licenses || []; downloads = d.downloads || {};
  } catch (err) {
    host.innerHTML = ui.errorState(err.message);
    return;
  }

  const dl = (platform, label) => {
    const d = downloads[platform];
    if (d && d.artifact_url) return `<a class="btn" href="${esc(d.artifact_url)}" target="_blank" rel="noreferrer">${label} · v${esc(d.version)}</a>`;
    return `<button class="btn" disabled title="Release ainda não publicado">${label}</button>`;
  };

  const renderLicenses = () => licenses.length === 0
    ? '<div class="card"><p class="muted">Você ainda não possui uma licença. <a href="#/validator/plans">Comprar plano</a></p></div>'
    : `<div class="grid-2">${licenses.map((l) => `
        <div class="card">
          <div class="kv"><span class="kv-label">Plano</span><span class="kv-value">${esc(PLAN_LABELS[l.plan] || l.plan)}</span></div>
          <div class="kv"><span class="kv-label">Status</span><span class="kv-value">${licenseStatusBadge(l.status)}</span></div>
          <div class="kv"><span class="kv-label">AUGEID</span><span class="kv-value mono">${l.augeid ? `#${fmtNum(l.augeid)}` : '—'}</span></div>
          <div class="kv"><span class="kv-label">Expira em</span><span class="kv-value">${esc((l.expires_at || '').slice(0, 10))}</span></div>
        </div>`).join('')}</div>`;

  const activeWithAugeid = licenses.find((l) => l.status === 'active' && l.augeid);

  const renderOrders = () => orders.length === 0
    ? '<div class="card"><p class="muted">Nenhum pedido ainda.</p></div>'
    : `<div class="grid-2">${orders.map((o) => `
        <div class="card">
          <div class="kv"><span class="kv-label">Plano</span><span class="kv-value">${esc(PLAN_LABELS[o.plan] || o.plan)} — US$${esc(o.amount_usd)}</span></div>
          <div class="kv"><span class="kv-label">Status</span><span class="kv-value">${orderStatusBadge(o)}</span></div>
          <div class="kv"><span class="kv-label">Referência</span><span class="kv-value mono">${esc((o.id || '').slice(0, 13))}…</span></div>
          ${o.status === 'paid' ? `<div class="actions" style="margin-top:8px"><button class="btn btn-primary" data-emit="${esc(o.id)}">Emitir licença</button></div>` : ''}
        </div>`).join('')}</div>`;

  const activationBlock = activeWithAugeid ? `
    <div class="card" style="border-color:var(--color-success);margin-bottom:16px">
      <div class="section-head"><h2>Ativação do software</h2></div>
      <p class="muted small">Baixe o software abaixo e informe <strong>apenas o seu AUGEID</strong> para ativar o validador na sua máquina.</p>
      <div class="receive-address mono" style="margin-top:10px">AUGEID #${fmtNum(activeWithAugeid.augeid)}</div>
      <div class="actions" style="margin-top:10px">${ui.copyButton(activeWithAugeid.augeid)}</div>
    </div>` : '';

  host.innerHTML = `
    ${activationBlock}
    <div class="section">
      <div class="section-head"><h2>Licenças</h2></div>
      <div id="licenses-box">${renderLicenses()}</div>
    </div>
    <div class="section">
      <div class="section-head"><h2>Download do software</h2></div>
      <div class="actions">${dl('windows', 'Windows (MSI)')}${dl('linux', 'Linux (DEB/AppImage)')}</div>
    </div>
    <div class="section">
      <div class="section-head"><h2>Pedidos</h2></div>
      <div id="orders-box">${renderOrders()}</div>
    </div>`;

  ui.bindCopyButtons(host);

  host.querySelectorAll('[data-emit]').forEach((btn) => btn.addEventListener('click', async () => {
    const id = btn.getAttribute('data-emit');
    btn.disabled = true; btn.textContent = 'Emitindo…';
    try {
      const res = await api.issueLicense(id);
      if (res && res.license_id) {
        orders = await api.listOrders();
        const ov = await api.getOverview();
        licenses = ov.licenses || [];
        host.querySelector('#orders-box').innerHTML = renderOrders();
        host.querySelector('#licenses-box').innerHTML = renderLicenses();
        ui.toast('Licença emitida! Ative com seu AUGEID.', 'success');
      }
    } catch (err) {
      ui.toast(err.message, 'error');
    }
  }));
}

function validatorCard(d) {
  const v = d.validator || {};
  const r = d.rewards || {};
  const buckets = [['Hora', r.hour], ['Dia', r.day], ['Semana', r.week], ['Mês', r.month], ['Ano', r.year]];
  const total = r.total || {};
  const bars = buckets.map(([label, b]) => ({ label, n: b ? Number(b.auge || 0) : 0 }));
  const max = Math.max(1, ...bars.map((x) => x.n));
  const barsHtml = bars.map((x) => `
    <div class="mini-bar" title="${esc(x.label)}: ${fmtAuge(String(x.n))}">
      <div class="mini-bar-fill" style="height:${(x.n / max) * 100}%"></div>
      <span class="mini-bar-label">${esc(x.label)}</span>
    </div>`).join('');

  return `
    <div class="card">
      <div class="kv"><span class="kv-label">Validador</span><span class="kv-value">${esc(v.augeid || '—')} <span class="mono">${esc((v.public_key || '').slice(0, 10))}…</span></span></div>
      <div class="kv"><span class="kv-label">Status</span><span class="kv-value">${validatorStatusBadge(v)}</span></div>
      <div class="kv"><span class="kv-label">Uptime</span><span class="kv-value">${fmtUptime(v.uptime)}</span></div>
      <div class="kv"><span class="kv-label">Blocos produzidos</span><span class="kv-value">${fmtNum(v.leadership)}</span></div>
      <div class="section-head" style="margin-top:12px"><h2>AUGE por período</h2></div>
      <div class="mini-bars">${barsHtml}</div>
      <div class="kv"><span class="kv-label">AUGE total</span><span class="kv-value">${fmtAuge(total.auge)}</span></div>
      <div class="kv"><span class="kv-label">AUGEIDs total</span><span class="kv-value">${fmtNum(total.augeids)}</span></div>
    </div>`;
}

async function renderValidatorDashboard() {
  root.innerHTML = `<main class="container page"><div class="page-head"><h1>Painel do Validador</h1><p class="muted">Status e ganhos em tempo real.</p></div><div id="vd-body">${ui.skeleton()}</div></main>`;
  const host = root.querySelector('#vd-body');

  try {
    const ov = await api.getOverview();
    const validators = ov.validators || [];
    if (validators.length === 0) {
      host.innerHTML = `<div class="card"><p class="muted">Nenhum validador ativado ainda. Baixe o software e ative sua licença. <a href="#/validator/license">Ver licença</a></p></div>`;
      return;
    }
    host.innerHTML = `<div class="grid-2">${validators.map(validatorCard).join('')}</div>`;
  } catch (err) {
    host.innerHTML = ui.errorState(err.message);
  }
}

// ── Assinaturas (Faturas & Licenças / serviços recorrentes) ───────────

const PLAN_DAYS = { monthly: 30, semiannual: 180, annual: 365 };

function daysRemainingLabel(days) {
  if (days === null || days === undefined) return '—';
  if (days <= 0) return 'Expirado';
  if (days === 1) return 'Vence hoje';
  return `${days} dia${days > 1 ? 's' : ''} restante${days > 1 ? 's' : ''}`;
}

function expiryProgress(l) {
  const total = PLAN_DAYS[l.plan] || 30;
  const remaining = l.days_remaining;
  if (remaining === null || remaining === undefined || remaining <= 0) return 0;
  return Math.max(0, Math.min(100, Math.round((remaining / total) * 100)));
}

function subscriptionCard(l) {
  const active = l.status === 'active';
  const pct = expiryProgress(l);
  const fillColor = l.expiring_soon ? 'var(--color-warning)' : l.status === 'expired' ? 'var(--color-error)' : 'var(--color-success)';
  return `
    <div class="card">
      <div class="card-head-row">
        <span class="account-card-number">${esc(PLAN_LABELS[l.plan] || l.plan)}</span>
        ${licenseStatusBadge(l.status)}
      </div>
      <div class="kv"><span class="kv-label">Expira em</span><span class="kv-value">${esc((l.expires_at || '').slice(0, 10))}</span></div>
      <div class="kv"><span class="kv-label">Tempo restante</span><span class="kv-value">${esc(daysRemainingLabel(l.days_remaining))}</span></div>
      <div class="expiry-track" title="${active ? `${pct}% do período restante` : ''}">
        <div class="expiry-fill" style="width:${pct}%;background:${fillColor}"></div>
      </div>
    </div>`;
}

function invoiceStatusBadge(i) {
  switch (i.status) {
    case 'pending': return ui.badge('warning', 'Aguardando pagamento');
    case 'paid': return ui.badge('success', 'Paga');
    case 'issued': return ui.badge('info', 'Emitida');
    case 'cancelled': return ui.badge('muted', 'Cancelada');
    default: return ui.badge('muted', i.status || 'Desconhecida');
  }
}

async function renderBilling() {
  root.innerHTML = `<main class="container page"><div class="page-head"><h1>Assinaturas</h1><p class="muted">Faturas, licenças ativas e serviços recorrentes.</p></div><div id="billing-body">${ui.skeleton()}</div></main>`;
  const host = root.querySelector('#billing-body');

  let data;
  try {
    data = await api.getBilling();
  } catch (err) {
    host.innerHTML = ui.errorState(err.message);
    return;
  }

  const { subscriptions = [], invoices = [], summary = {} } = data;
  const s = summary || {};

  const subscriptionsHtml = subscriptions.length === 0
    ? `<div class="card"><p class="muted">Nenhum serviço recorrente ativo. <a href="#/validator/plans">Ver planos</a></p></div>`
    : `<div class="grid-2">${subscriptions.map(subscriptionCard).join('')}</div>`;

  const invoicesHtml = invoices.length === 0
    ? `<div class="card"><p class="muted">Nenhuma fatura ainda.</p></div>`
    : `<div class="table-wrap"><table class="table">
        <thead><tr><th>Referência</th><th>Plano</th><th>Valor</th><th>Método</th><th>Data</th><th>Status</th></tr></thead>
        <tbody>${invoices.map((i) => `
          <tr>
            <td class="mono">${esc((i.id || '').slice(0, 13))}…</td>
            <td>${esc(PLAN_LABELS[i.plan] || i.plan)}</td>
            <td class="price">US$${esc(i.amount_usd ?? '—')}</td>
            <td>${esc(METHOD_LABELS[i.method] || i.method)}</td>
            <td class="muted small">${esc((i.created_at || '').slice(0, 10))}</td>
            <td>${invoiceStatusBadge(i)}</td>
          </tr>`).join('')}</tbody>
      </table></div>`;

  host.innerHTML = `
    <section class="grid-stats">
      ${ui.statCard('Serviços ativos', fmtNum(s.active ?? 0), 'server')}
      ${ui.statCard('A vencer em breve', fmtNum(s.expiring_soon ?? 0), 'clock')}
      ${ui.statCard('Faturas pagas', `${fmtNum(s.paid_invoices ?? 0)}`, 'check', s.total_paid_usd ? `US$${fmtNum(s.total_paid_usd)} pagos` : '')}
      ${ui.statCard('Faturas pendentes', fmtNum(s.pending_invoices ?? 0), 'alert')}
    </section>

    <div class="section">
      <div class="section-head"><h2>Serviços recorrentes</h2><span class="muted small">licenças ativas e vencimento</span></div>
      ${subscriptionsHtml}
    </div>

    <div class="section">
      <div class="section-head"><h2>Faturas</h2><a class="link small" href="#/validator/plans">novo plano</a></div>
      ${invoicesHtml}
    </div>`;
}

// ── My Wallets ─────────────────────────────────────────────────────────

async function renderMyWallets() {
  root.innerHTML = `<main class="container page"><div class="page-head"><h1>Minhas Carteiras</h1><p class="muted">Todas as carteiras blockchain vinculadas à sua conta.</p></div><div id="mw-body">${ui.skeleton()}</div></main>`;
  const host = root.querySelector('#mw-body');

  if (state.wallets.length === 0) { host.innerHTML = ui.emptyState('Nenhuma carteira vinculada.'); return; }

  host.innerHTML = `
    <div class="table-wrap"><table class="table">
      <thead><tr><th>AUGEID</th><th>Nome</th><th>Saldo</th><th>Status</th><th>Ações</th></tr></thead>
      <tbody>${state.wallets.map((w) => {
        const a = w.account;
        const actions = [];
        if (a?.state === 'ForSale') actions.push(`<button class="btn btn-danger" data-cancel="${w.account_number}">Cancelar venda</button>`);
        if (a && (a.state === 'Reserved' || a.state === 'Owned' || a.state === 'Normal')) {
          actions.push(`<button class="btn" data-sell="${w.account_number}">Vender</button>`);
          actions.push(`<button class="btn" data-transfer="${w.account_number}">Transferir</button>`);
          actions.push(`<button class="btn" data-rename="${w.account_number}">Alterar Nome</button>`);
        }
        return `<tr>
          <td class="mono">#${fmtNum(w.account_number)}</td>
          <td>${esc(a?.name || 'Sem nome')}</td>
          <td class="price">${a ? auge(a.balance) : '—'}</td>
          <td>${a ? ui.statusBadge(a.state) : '—'}</td>
          <td><div class="actions">${actions.join('')}</div></td>
        </tr>`;
      }).join('')}</tbody>
    </table></div>`;

  bindWalletActions(host);
}

function bindWalletActions(host) {
  const find = (n) => state.wallets.find((w) => w.account_number === n);

  host.querySelectorAll('[data-cancel]').forEach((b) => b.addEventListener('click', async () => {
    const n = Number(b.getAttribute('data-cancel'));
    const w = find(n);
    const s = await requireSession(); if (!s || !w?.account) return;
    const res = await ops.submitCancelSale(s, { account: n, nOperation: w.account.n_operation, fee: Number(CONFIG.MIN_FEE_AUGESAT) });
    if (res.accepted) { ui.toast('Venda cancelada.', 'success'); await afterWalletsChanged(); router(); }
    else ui.toast(res.error || 'Cancelamento rejeitado.', 'error');
  }));

  host.querySelectorAll('[data-sell]').forEach((b) => b.addEventListener('click', async () => {
    const n = Number(b.getAttribute('data-sell'));
    const w = find(n);
    const s = await requireSession(); if (!s || !w?.account) return;
    const price = await promptText('Vender AUGEID #' + fmtNum(n), 'Preço (AUGE)', { placeholder: '0.00' });
    if (!price) return;
    const res = await ops.submitSell(s, {
      account: n, nOperation: w.account.n_operation, salePrice: augesatFromAuge(price),
      accountToPay: n, newPublicKeyHex: w.account.account_key_ed_hex, lockedUntilBlock: 0, fee: Number(CONFIG.MIN_FEE_AUGESAT),
    });
    if (res.accepted) { ui.toast('AUGEID colocado à venda.', 'success'); await afterWalletsChanged(); router(); }
    else ui.toast(res.error || 'Venda rejeitada.', 'error');
  }));

  host.querySelectorAll('[data-transfer]').forEach((b) => b.addEventListener('click', async () => {
    const n = Number(b.getAttribute('data-transfer'));
    const w = find(n);
    const s = await requireSession(); if (!s || !w?.account) return;
    const recipient = await selectMember('Transferir AUGEID #' + fmtNum(n));
    if (!recipient) return;
    const res = await ops.submitChangeKey(s, { account: n, nOperation: w.account.n_operation, fee: Number(CONFIG.MIN_FEE_AUGESAT), newPublicKeyHex: recipient });
    if (res.accepted) { ui.toast('Transferência enviada.', 'success'); await afterWalletsChanged(); router(); }
    else ui.toast(res.error || 'Transferência rejeitada.', 'error');
  }));

  host.querySelectorAll('[data-rename]').forEach((b) => b.addEventListener('click', async () => {
    const n = Number(b.getAttribute('data-rename'));
    const w = find(n);
    const s = await requireSession(); if (!s || !w?.account) return;
    const name = await promptText('Alterar nome do AUGEID #' + fmtNum(n), 'Novo nome', { placeholder: 'ex.: CarlosPay' });
    if (!name) return;
    const available = await rpc.isNameAvailable(name);
    if (!available) { ui.toast('Nome já em uso.', 'error'); return; }
    const current = await rpc.getAccountByNumber(n);
    const res = await ops.submitChangeAccountInfo(s, {
      account: n, nOperation: current.n_operation, fee: Number(CONFIG.MIN_FEE_AUGESAT), newPublicKeyHex: current.account_key_ed_hex,
      newName: name.trim() || null, newType: current.account_type,
      newAccountDataHex: current.account_data_hex || '', newAccountSealHex: current.account_seal_hex || '',
    });
    if (res.accepted) { ui.toast('Nome atualizado.', 'success'); await afterWalletsChanged(); router(); }
    else ui.toast(res.error || 'Falha ao alterar nome.', 'error');
  }));
}

// ── Profile ────────────────────────────────────────────────────────────

async function renderProfile() {
  const u = state.user;
  root.innerHTML = `
    <main class="container page">
      <div class="page-head"><h1>Perfil</h1><p class="muted">Suas informações da plataforma — separadas das carteiras blockchain.</p></div>

      <section class="profile-section">
        <div class="section-head"><h2>${ui.platformChip()}</h2></div>
        ${platformAccountCard()}
        <div class="card">
          <form data-profile>
            <label class="field"><span class="field-label">Nome de exibição</span><input name="name" value="${esc(u.display_name)}"></label>
            <label class="field"><span class="field-label">E-mail</span><input name="email" value="${esc(u.email)}" disabled></label>
            <div class="field"><span class="field-label">Chave pública</span><div class="mono small" style="word-break:break-all">${esc(u.public_key_hex || 'não registrada')} ${u.public_key_hex ? ui.copyButton(u.public_key_hex) : ''}</div><span class="muted small">Use esta chave para receber AUGEIDs. A chave privada nunca é exibida.</span></div>
            <label class="field checkbox-field"><input type="checkbox" name="notifications" ${state.preferences?.notifications !== false ? 'checked' : ''}><span>Receber notificações</span></label>
            <button class="btn btn-primary" type="submit">Salvar</button>
            <button class="btn btn-ghost" type="button" data-recover-key>Recuperar / regenerar chave</button>
          </form>
        </div>
      </section>

      <section class="profile-section">
        <div class="section-head"><h2>${ui.walletChip()}</h2></div>
        ${hasWallet() ? `<div class="grid-2">${state.wallets.map(walletCard).join('')}</div>` : emptyWalletCard()}
      </section>
    </main>`;

  drawQr(root);
  ui.bindCopyButtons(root);
  root.querySelector('form[data-profile]').addEventListener('submit', async (e) => {
    e.preventDefault();
    const f = e.target;
    try {
      await api.updateProfile(f.name.value.trim());
      await api.updatePreferences({ notifications: f.notifications.checked });
      state.user = { ...state.user, display_name: f.name.value.trim() };
      state.preferences = { ...state.preferences, notifications: f.notifications.checked };
      ui.renderHeader({ user: state.user, hasWallet: hasWallet() });
      ui.toast('Perfil atualizado.', 'success');
    } catch (err) { ui.toast(err.message, 'error'); }
  });

  root.querySelector('[data-recover-key]').addEventListener('click', async () => {
    const s = await recoverKey();
    if (s) {
      ui.toast('Chave recuperada. Receba um novo AUGEID para ativar a carteira.', 'success');
      router();
    }
  });
}

// ── Send ───────────────────────────────────────────────────────────────

async function renderSend() {
  const owned = ownedWallets();
  root.innerHTML = `
    <main class="container page">
      <div class="page-head"><h1>Enviar AUGE</h1><p class="muted">Informe um endereço ou AUGEID. A carteira detecta automaticamente.</p></div>
      <div class="card login-card">
        ${owned.length === 0 ? '<p class="muted">Nenhum AUGEID com saldo vinculado.</p>' : `
        <form data-send>
          <label class="field"><span class="field-label">AUGEID de origem</span>
            <select name="from" required><option value="">Selecione…</option>
              ${owned.map((w) => `<option value="${w.account_number}">AUGEID #${fmtNum(w.account_number)} — ${auge(w.account.balance)} AUGE</option>`).join('')}
            </select></label>
          <label class="field"><span class="field-label">Destino</span><input name="to" placeholder="AUGE-845621 ou JBa4YQ5Q8YwXFNxFnCxLhsMREv5TYhPC9vsAA2" required></label>
          <div data-resolved class="muted small"></div>
          <button class="btn" type="button" data-resolve>Resolver destino</button>
          <label class="field"><span class="field-label">Quantidade (AUGE)</span><input name="amount" type="number" step="any" min="0" required></label>
          <button class="btn btn-primary" type="submit">Enviar</button>
        </form>`}
      </div>
    </main>`;

  if (owned.length === 0) return;

  let resolved = null;
  const form = root.querySelector('form[data-send]');
  const resolvedEl = root.querySelector('[data-resolved]');

  root.querySelector('[data-resolve]').addEventListener('click', async () => {
    const to = form.to.value.trim();
    if (!to) { resolvedEl.textContent = 'Informe um AUGEID ou nome.'; return; }
    try { resolved = await rpc.resolveDestination(to); } catch { resolved = null; }
    resolvedEl.textContent = resolved?.account_number ? `Destino: AUGEID #${fmtNum(resolved.account_number)}` : (resolved?.exists ? 'Endereço resolvido.' : 'Destino não encontrado.');
  });

  form.addEventListener('submit', async (e) => {
    e.preventDefault();
    const s = await requireSession(); if (!s) return;
    if (!resolved) { ui.toast('Resolva o destino primeiro.', 'error'); return; }
    const sender = owned.find((w) => w.account_number === Number(form.from.value));
    const amount = augesatFromAuge(form.amount.value);
    if (amount <= 0n) { ui.toast('Quantidade inválida.', 'error'); return; }
    try {
      const res = await ops.submitTransfer(s, {
        sender: sender.account.account_number, nOperation: sender.account.n_operation,
        to: resolved.account_number, amount, fee: Number(CONFIG.MIN_FEE_AUGESAT),
      });
      if (res.accepted) { ui.toast(`Enviado ${augesatToAuge(amount)} AUGE para AUGEID #${fmtNum(resolved.account_number)}.`, 'success'); await afterWalletsChanged(); }
      else ui.toast(res.error || 'Transação rejeitada.', 'error');
    } catch (err) { ui.toast(err.message, 'error'); }
  });
}

// ── Receive ────────────────────────────────────────────────────────────

async function renderReceive() {
  const owned = ownedWallets();
  if (owned.length === 0) { renderReceiveFirstAugeid(); return; }

  root.innerHTML = `
    <main class="container page">
      <div class="page-head"><h1>Receber AUGE</h1><p class="muted">Escolha a conta para receber e compartilhe o endereço ou o QR code.</p></div>
      <div class="card login-card">
        ${owned.length > 1 ? '<label class="field"><span class="field-label">Conta para receber</span>' : ''}<select id="account-selector" name="sel" ${owned.length <= 1 ? 'hidden' : ''}>
             ${owned.map((w) => `<option value="${w.account_number}">${esc(w.account?.name || `AUGEID #${fmtNum(w.account_number)}`)} — ${esc(shortAddr(walletShortAddress(w)))}</option>`).join('')}
           </select>${owned.length > 1 ? '</label>' : ''}
        <div data-receive></div>
      </div>
    </main>`;

  const sel = root.querySelector('select[name="sel"]');
  if (!sel) return;
  const host = root.querySelector('[data-receive]');
  sel.addEventListener('change', () => {
    const n = Number(sel.value);
    const w = owned.find((x) => x.account_number === n);
    if (!w) { host.innerHTML = ''; return; }
    const address = walletShortAddress(w);
    host.innerHTML = `
      <div class="receive-info">
         <div><span class="field-label">Endereço curto</span><div class="receive-address mono">${esc(address)} ${ui.copyButton(address)}</div></div>
        <div><span class="field-label">Nome</span><div class="receive-name">${esc(w.account.name || 'Sem nome')}</div></div>
      </div>
      <div class="qr-wrap"><canvas data-qr="${n}"></canvas></div>
      <div class="receive-augeid center"><span class="field-label">Número da conta</span><div class="receive-augeid mono">#${fmtNum(n)} ${ui.copyButton(`augeid:${fmtNum(n)}`)}</div></div>`;
    ui.bindCopyButtons(host);
    drawQr(host);
  });
  if (owned.length >= 1) { sel.value = String(owned[0].account_number); sel.dispatchEvent(new Event('change')); }
}

/** Receive screen for a member that has no AUGEID yet: show their address to
 *  receive the first AUGEID (from another member), a link to buy one, and any
 *  pending AUGEIDs awaiting acceptance. */
async function renderReceiveFirstAugeid() {
  const pub = state.user?.public_key_hex || '';
  const hasKey = /^[0-9a-f]{64}$/i.test(pub);
   const short = hasKey ? deriveEmbeddedAddress(pub) : '';

  root.innerHTML = `
    <main class="container page">
      <div class="page-head"><h1>Receber</h1><p class="muted">Ative sua carteira recebendo seu primeiro AUGEID.</p></div>
      <div class="card login-card">
        <div class="empty-wallet-card" style="border-left:none;padding:0">
          <span class="layer-chip layer-wallet">Você ainda não possui um AUGEID</span>
          <p class="muted center">Seu primeiro recebimento ativará automaticamente um AUGEID Reserved já emitido pela rede.</p>
          ${hasKey ? `
            <div style="width:100%;text-align:left">
              <div class="field"><span class="field-label">Endereço</span><div class="receive-address mono">${esc(short)} ${ui.copyButton(short)}</div></div>
              <div class="qr-wrap"><canvas data-qr-address="${esc(short)}"></canvas></div>
            </div>
            <p class="muted small center">Compartilhe este endereço para receber AUGE. A ativação ocorre no mesmo bloco do primeiro recebimento.</p>
          ` : '<p class="muted">Sua conta ainda não possui chave pública. Abra o Perfil e recupere a chave.</p>'}
          <div class="actions" style="justify-content:center">
            <button class="btn" type="button" disabled>Um AUGEID Reserved será ativado no primeiro recebimento</button>
          </div>
        </div>
      </div>
      <div class="section">
        <div class="section-head"><h2>AUGEIDs recebidos</h2></div>
        <div id="gift-body">${ui.skeleton()}</div>
      </div>
    </main>`;

  ui.bindCopyButtons(root);
  const qr = root.querySelector('[data-qr-address]');
  if (qr && short) qrToCanvas(qrMatrix(short), qr, 4, 2);
  await renderPendingGifts(root.querySelector('#gift-body'), pub);
}

/** List + accept AUGEIDs that were transferred to the member's key. */
async function renderPendingGifts(host, publicKeyHex) {
  if (!host) return;
  if (!/^[0-9a-f]{64}$/i.test(publicKeyHex || '')) {
    host.innerHTML = ui.emptyState('Nenhum AUGEID pendente.');
    return;
  }
  try {
    const res = await rpc.listPendingGifts(publicKeyHex);
    if (!res.entries || res.entries.length === 0) {
      host.innerHTML = ui.emptyState('Nenhum AUGEID recebido no momento.', 'Quando alguém transferir um AUGEID para o seu endereço, ele aparecerá aqui para aceite.');
      return;
    }
    host.innerHTML = `<div class="grid-2">${res.entries.map((g) => `
      <div class="card">
        <div class="card-head-row">
          <span class="account-card-number">AUGEID #${fmtNum(g.account_number)}</span>
          ${ui.badge('admin', 'Pendente')}
        </div>
        <dl class="gift-card-fields">
          <div><dt>Remetente</dt><dd class="mono">${esc(shortHash(g.from_public_key_hex, 8))}</dd></div>
          <div><dt>Bloco</dt><dd>#${fmtNum(g.gifted_at_block)}</dd></div>
        </dl>
        ${g.name ? `<div class="muted">Nome atual: ${esc(g.name)}</div>` : ''}
        <button class="btn btn-primary" data-accept="${g.account_number}">Aceitar AUGEID</button>
      </div>`).join('')}</div>`;

    host.querySelectorAll('[data-accept]').forEach((btn) => btn.addEventListener('click', async () => {
      const num = Number(btn.getAttribute('data-accept'));
      const s = await requireSession();
      if (!s) return;
      try {
        const acc = await rpc.getAccountByNumber(num);
        const res = await ops.submitAcceptGift(s, { account: num, nOperation: acc.n_operation, fee: Number(CONFIG.MIN_FEE_AUGESAT) });
        if (res.accepted) {
          await api.linkWallet(num).catch(() => {});
          await afterWalletsChanged();
          ui.toast(`AUGEID #${fmtNum(num)} aceito. Carteira ativada!`, 'success');
          router();
        } else {
          ui.toast(res.error || 'Aceite rejeitado.', 'error');
        }
      } catch (err) { ui.toast(err.message, 'error'); }
    }));
  } catch (err) {
    host.innerHTML = ui.errorState(err.message);
  }
}

// ── History ────────────────────────────────────────────────────────────

/** Classify an on-chain operation against the user's accounts. Returns a
 *  history item, or null when the operation does not involve the user. */
function classifyOperation(op, block, myAccounts) {
  const p = op.payload || {};
  const t = op.op_type_name;
  const base = {
    opHash: op.op_hash_hex,
    block: Number(block.block_number),
    timestamp: block.timestamp,
    opType: t,
    fee: p.fee != null ? p.fee : null,
  };
  const mine = (n) => myAccounts.has(Number(n));

  if (t === 'Transaction' || t === 'MultiOperation') {
    const senders = Array.isArray(p.senders) ? p.senders : [];
    const receivers = Array.isArray(p.receivers) ? p.receivers : [];
    const s = senders.find((x) => mine(x.account));
    const r = receivers.find((x) => mine(x.account));
    if (s) return { ...base, direction: 'send', amount: s.amount, from: s.account, to: receivers[0]?.account ?? null, counterparty: receivers[0]?.account ?? null, counterpartyKey: null };
    if (r) return { ...base, direction: 'receive', amount: r.amount, from: senders[0]?.account ?? null, to: r.account, counterparty: senders[0]?.account ?? null, counterpartyKey: null };
    return null;
  }
  if (t === 'ChangeKey' || t === 'ChangeKeySigned' || t === 'ChangeAccountInfo' || t === 'ListAccountForSale' || t === 'DelistAccount' || t === 'GiftAccount' || t === 'AcceptGift') {
    if (mine(p.account)) return { ...base, direction: 'send', amount: null, from: p.account, to: null, counterparty: null, counterpartyKey: p.new_ed25519_public_key_hex || null };
    return null;
  }
  if (t === 'BuyAccount') {
    if (mine(p.buyer_account)) return { ...base, direction: 'send', amount: p.amount, from: p.buyer_account, to: p.account_to_purchase ?? null, counterparty: p.account_to_purchase ?? null, counterpartyKey: null };
    if (mine(p.seller_account)) return { ...base, direction: 'receive', amount: p.amount, from: p.buyer_account ?? null, to: p.seller_account, counterparty: p.buyer_account ?? null, counterpartyKey: null };
    return null;
  }
  if (t === 'CreateAccount') {
    if (mine(p.account_number)) return { ...base, direction: 'receive', amount: 0, from: null, to: p.account_number, counterparty: null, counterpartyKey: null };
    return null;
  }
  return null;
}

/** Scan the last `windowBlocks` blocks for operations involving `myAccounts`. */
async function fetchHistory(height, myAccounts, windowBlocks = 120) {
  const items = [];
  const from = Math.max(1, height - windowBlocks + 1);
  const batchSize = 20;
  for (let start = height; start >= from; start -= batchSize) {
    const batch = [];
    for (let b = start; b > start - batchSize && b >= from; b--) batch.push(b);
    const results = await Promise.all(batch.map(async (b) => {
      try { return await rpc.getBlockOperations(b); } catch { return null; }
    }));
    for (const res of results) {
      if (!res || !Array.isArray(res.operations)) continue;
      for (const op of res.operations) {
        const item = classifyOperation(op, res.block, myAccounts);
        if (item) items.push(item);
      }
    }
  }
  return items;
}

async function renderHistory() {
  root.innerHTML = `<main class="container page"><div class="page-head"><h1>Histórico</h1><p class="muted">Operações on-chain confirmadas das suas carteiras.</p></div><div id="hist-body">${ui.skeleton()}</div></main>`;
  const host = root.querySelector('#hist-body');
  const myAccounts = new Set(state.wallets.map((w) => Number(w.account_number)));
  if (myAccounts.size === 0) { host.innerHTML = ui.emptyState('Nenhuma carteira vinculada.'); return; }

  let node = null;
  try { node = await rpc.getNodeStatus(); } catch { /* ignore */ }
  const height = node ? Number(node.current_height) : 0;
  if (!height) { host.innerHTML = ui.errorState('Não foi possível consultar a rede.'); return; }

  const items = await fetchHistory(height, myAccounts);
  state.transactions = items;

  if (items.length === 0) {
    host.innerHTML = ui.emptyState('Nenhuma operação recente.', 'As operações aparecem aqui após serem confirmadas em bloco.');
    return;
  }

  host.innerHTML = `
    <div class="table-wrap"><table class="table">
      <thead><tr><th>Tipo</th><th>Contraparte</th><th>Valor</th><th>Data / hora</th><th>Status</th></tr></thead>
      <tbody>${items.map((tx) => {
        const dir = tx.direction;
        const marker = dir === 'send' ? 'tx-send' : 'tx-receive';
        const label = dir === 'send' ? 'Enviado' : 'Recebido';
        const counterparty = tx.counterparty != null ? `#${fmtNum(tx.counterparty)}` : '—';
        const value = tx.amount != null ? `${auge(tx.amount)} AUGE` : '—';
        return `<tr class="tx-row" data-tx="${esc(tx.opHash)}"><td><span class="tx-marker ${marker}"></span>${label}</td><td class="mono">${counterparty}</td><td class="price">${value}</td><td class="muted small">${esc(fmtTime(tx.timestamp))}</td><td>${ui.badge('success', `Confirmado · #${fmtNum(tx.block)}`)}</td></tr>`;
      }).join('')}</tbody>
    </table></div>
    <p class="muted small">Altura atual: #${fmtNum(height)}</p>`;

  host.querySelectorAll('[data-tx]').forEach((row) => row.addEventListener('click', () => {
    state.historyScrollY = window.scrollY;
    location.hash = `#/history/${encodeURIComponent(row.dataset.tx)}`;
  }));

  if (state.historyScrollY != null) {
    window.scrollTo(0, state.historyScrollY);
    state.historyScrollY = null;
  }
}

async function renderTransactionDetail(txid) {
  const hash = decodeURIComponent(txid);
  root.innerHTML = `<main class="container page"><div class="page-head"><a href="#/history">← Voltar ao Histórico</a><h1>Detalhes da operação</h1></div><div class="card tx-detail">${ui.skeletonCard()}</div></main>`;

  const myAccounts = new Set(state.wallets.map((w) => Number(w.account_number)));
  let tx = (state.transactions || []).find((item) => item.opHash === hash) || null;

  if (!tx) {
    try {
      const res = await rpc.getOperationByHash(hash);
      let timestamp = null;
      try { timestamp = (await rpc.getBlock(res.block_number)).timestamp; } catch { /* ignore */ }
      tx = classifyOperation(res.operation, { block_number: res.block_number, timestamp }, myAccounts);
    } catch {
      tx = null;
    }
  }

  if (!tx) {
    root.innerHTML = `<main class="container page">${ui.emptyState('Operação não encontrada.', 'Volte ao histórico e tente novamente.')}</main>`;
    return;
  }

  let height = 0;
  try { height = Number((await rpc.getNodeStatus()).current_height); } catch { /* ignore */ }

  const dirLabel = tx.direction === 'send' ? 'Enviado' : 'Recebido';
  const marker = tx.direction === 'send' ? 'tx-send' : 'tx-receive';
  const amount = tx.amount != null ? `${auge(tx.amount)} AUGE` : '—';
  const confirmations = height && tx.block ? Math.max(0, height - tx.block + 1) : null;

  root.innerHTML = `<main class="container page"><div class="page-head"><a href="#/history">← Voltar ao Histórico</a><h1>Detalhes da operação</h1></div><div class="card tx-detail">
    <div class="tx-status"><span class="tx-marker ${marker}"></span><strong>${dirLabel}</strong>${ui.badge('success', `Confirmado · bloco #${fmtNum(tx.block)}`)}</div>
    <div class="kv"><span class="kv-label">Valor</span><span class="kv-value">${amount}</span></div>
    <div class="kv"><span class="kv-label">Tipo</span><span class="kv-value">${esc(tx.opType)}</span></div>
    <div class="kv"><span class="kv-label">De</span><span class="kv-value mono">${tx.from != null ? `#${fmtNum(tx.from)} ${ui.copyButton(`augeid:${tx.from}`)}` : '—'}</span></div>
    <div class="kv"><span class="kv-label">Para</span><span class="kv-value mono">${tx.to != null ? `#${fmtNum(tx.to)} ${ui.copyButton(`augeid:${tx.to}`)}` : '—'}</span></div>
    <div class="kv"><span class="kv-label">Hash da operação</span><span class="kv-value mono">${esc(tx.opHash)} ${ui.copyButton(tx.opHash)}</span></div>
    <div class="kv"><span class="kv-label">Data / hora</span><span class="kv-value">${esc(fmtTime(tx.timestamp))}</span></div>
    <div class="kv"><span class="kv-label">Taxa de rede</span><span class="kv-value">${tx.fee != null ? `${fmtNum(tx.fee)} augesat` : '—'}</span></div>
    <div class="kv"><span class="kv-label">Confirmações</span><span class="kv-value">${confirmations != null ? fmtNum(confirmations) : '—'}</span></div>
  </div></main>`;
  ui.bindCopyButtons(root);
}

// ── Start ──────────────────────────────────────────────────────────────

boot();
