// AUGECOIN Wallet Web — platform backend.
//
// Responsibilities (and ONLY these):
//   - platform identity (register/login/profile/preferences/session)
//   - wallet registry (linked_wallets: user_id <-> account_number)
//   - directory (display_name/public_key lookup for gift/transfer UX)
//
// NOT here: balances, ownership authority, transfers, consensus. The frontend
// talks to the node RPC for all on-chain truth. This service holds no keys and
// no balances — it is a bridge between web auth and on-chain identity.

import express from 'express';
import cookieParser from 'cookie-parser';
import { randomUUID } from 'node:crypto';
import { getDb } from './db.js';
import { hashPassword, verifyPassword } from './password.js';
import {
  COOKIE_NAMES,
  cookieOpts,
  signAccessToken,
  verifyAccessToken,
  newCsrfToken,
  newRefreshToken,
  consumeRefreshToken,
  revokeAllRefreshTokens,
} from './tokens.js';
import { check, reset } from './ratelimit.js';
import { ops, PLANS } from './ops.js';

const PORT = Number(process.env.AUGECOIN_WALLET_PORT || 8787);
const app = express();

app.disable('x-powered-by');
// Behind nginx on the same host: derive the real client IP from X-Forwarded-For
// so rate limiting keys per-client rather than per-proxy.
app.set('trust proxy', 'loopback');
app.use(express.json({ limit: '64kb' }));
app.use(cookieParser());

const TRUSTED_ORIGINS = (process.env.AUGECOIN_WALLET_ORIGINS || '')
  .split(',')
  .map((s) => s.trim())
  .filter(Boolean);

function originAllowed(req) {
  const origin = req.headers.origin;
  if (!origin) return true; // same-origin / non-browser clients
  if (TRUSTED_ORIGINS.length === 0) return true;
  return TRUSTED_ORIGINS.includes(origin);
}

function setCookies(res, accessToken, refreshToken, csrfToken) {
  res.cookie(COOKIE_NAMES.access, accessToken, { ...cookieOpts(), maxAge: 15 * 60 * 1000 });
  res.cookie(COOKIE_NAMES.refresh, refreshToken, { ...cookieOpts(), maxAge: 7 * 24 * 60 * 60 * 1000 });
  res.cookie(COOKIE_NAMES.csrf, csrfToken, { ...cookieOpts(), httpOnly: false });
}

function clearCookies(res) {
  res.clearCookie(COOKIE_NAMES.access, cookieOpts());
  res.clearCookie(COOKIE_NAMES.refresh, cookieOpts());
  res.clearCookie(COOKIE_NAMES.csrf, { ...cookieOpts(), httpOnly: false });
}

/** Attach `req.user` (full platform_users row) when the access token is valid. */
function requireAuth(req, res, next) {
  const token = req.cookies[COOKIE_NAMES.access];
  const payload = token ? verifyAccessToken(token) : null;
  if (!payload) return res.status(401).json({ error: 'unauthorized' });
  const user = getDb().prepare('SELECT * FROM platform_users WHERE id = ?').get(payload.sub);
  if (!user) return res.status(401).json({ error: 'unauthorized' });
  req.user = user;
  next();
}

/** CSRF double-submit + Origin check for mutating requests. */
function requireCsrf(req, res, next) {
  if (!originAllowed(req)) return res.status(403).json({ error: 'forbidden origin' });
  const header = req.headers['x-csrf-token'];
  const cookie = req.cookies[COOKIE_NAMES.csrf];
  if (!header || !cookie || header !== cookie) {
    return res.status(403).json({ error: 'invalid csrf token' });
  }
  next();
}

function publicUser(row) {
  return {
    id: row.id,
    email: row.email,
    display_name: row.display_name,
    public_key_hex: row.public_key_hex,
    created_at: row.created_at,
    last_login: row.last_login,
  };
}

function preferencesFor(userId) {
  const p = getDb().prepare('SELECT * FROM user_preferences WHERE user_id = ?').get(userId);
  if (!p) return { theme: 'dark', language: 'pt-BR', notifications: true };
  return { theme: p.theme, language: p.language, notifications: !!p.notifications };
}

function issueSession(res, user) {
  const accessToken = signAccessToken(user);
  const refreshToken = newRefreshToken(user.id);
  const csrfToken = newCsrfToken();
  setCookies(res, accessToken, refreshToken, csrfToken);
  return csrfToken;
}

// ── Health ────────────────────────────────────────────────────────────
app.get('/health', (_req, res) => res.json({ ok: true }));

// ── Auth ──────────────────────────────────────────────────────────────
app.post('/api/auth/register', (req, res) => {
  const ip = req.ip || 'unknown';
  const rl = check(`reg:${ip}`, { limit: 10, windowMs: 60_000 });
  if (!rl.ok) return res.status(429).json({ error: 'too many attempts' });

  const { email, password, display_name: displayName, public_key_hex: publicKeyHex } = req.body || {};
  const cleanEmail = String(email || '').trim().toLowerCase();
  const name = String(displayName || '').trim();
  const key = String(publicKeyHex || '').trim().toLowerCase();

  if (!/^[^@\s]+@[^@\s]+\.[^@\s]+$/.test(cleanEmail)) {
    return res.status(400).json({ error: 'email inválido' });
  }
  if (!password || String(password).length < 8) {
    return res.status(400).json({ error: 'senha deve ter ao menos 8 caracteres' });
  }
  if (!name) return res.status(400).json({ error: 'nome é obrigatório' });
  if (!/^[0-9a-f]{64}$/.test(key)) {
    return res.status(400).json({ error: 'chave pública Ed25519 é obrigatória (64 caracteres hex)' });
  }

  const db = getDb();
  const existing = db.prepare('SELECT id FROM platform_users WHERE email = ?').get(cleanEmail);
  if (existing) return res.status(409).json({ error: 'este e-mail já está cadastrado' });

  const now = new Date().toISOString();
  const id = randomUUID();
  db.prepare(
    'INSERT INTO platform_users (id, email, password_hash, display_name, public_key_hex, created_at) VALUES (?, ?, ?, ?, ?, ?)',
  ).run(id, cleanEmail, hashPassword(String(password)), name, key, now);
  db.prepare(
    'INSERT INTO user_preferences (user_id, theme, language, notifications) VALUES (?, ?, ?, ?)',
  ).run(id, 'dark', 'pt-BR', 1);

  const user = db.prepare('SELECT * FROM platform_users WHERE id = ?').get(id);
  const csrfToken = issueSession(res, user);
  // Registration creates ONLY a platform account — never an AUGEID.
  res.status(201).json({ user: publicUser(user), csrf_token: csrfToken });
});

app.post('/api/auth/login', (req, res) => {
  const ip = req.ip || 'unknown';
  const { email, password } = req.body || {};
  const cleanEmail = String(email || '').trim().toLowerCase();

  const rl = check(`login:${ip}`, { limit: 5, windowMs: 60_000 });
  const emailRl = check(`login-email:${cleanEmail}`, { limit: 5, windowMs: 60_000 });
  if (!rl.ok || !emailRl.ok) {
    return res.status(429).json({ error: 'muitas tentativas, aguarde um momento' });
  }

  const db = getDb();
  const user = db.prepare('SELECT * FROM platform_users WHERE email = ?').get(cleanEmail);
  if (!user || !verifyPassword(String(password || ''), user.password_hash)) {
    return res.status(401).json({ error: 'credenciais inválidas' });
  }

  db.prepare('UPDATE platform_users SET last_login = ? WHERE id = ?').run(new Date().toISOString(), user.id);
  reset(`login-email:${cleanEmail}`);
  const fresh = db.prepare('SELECT * FROM platform_users WHERE id = ?').get(user.id);
  const csrfToken = issueSession(res, fresh);
  res.json({ user: publicUser(fresh), csrf_token: csrfToken });
});

app.post('/api/auth/refresh', (req, res) => {
  const token = req.cookies[COOKIE_NAMES.refresh];
  const userId = consumeRefreshToken(token);
  if (!userId) {
    clearCookies(res);
    return res.status(401).json({ error: 'invalid refresh token' });
  }
  const user = getDb().prepare('SELECT * FROM platform_users WHERE id = ?').get(userId);
  if (!user) {
    clearCookies(res);
    return res.status(401).json({ error: 'unauthorized' });
  }
  const csrfToken = issueSession(res, user);
  res.json({ user: publicUser(user), csrf_token: csrfToken });
});

app.post('/api/auth/logout', (req, res) => {
  const token = req.cookies[COOKIE_NAMES.refresh];
  const userId = token ? consumeRefreshToken(token) : null;
  if (userId) revokeAllRefreshTokens(userId); // global logout
  clearCookies(res);
  res.status(204).end();
});

// ── Session / me ──────────────────────────────────────────────────────
app.get('/api/me', requireAuth, (req, res) => {
  res.json({ user: publicUser(req.user), preferences: preferencesFor(req.user.id) });
});

app.patch('/api/me/key', requireAuth, requireCsrf, (req, res) => {
  const key = String(req.body?.public_key_hex || '').trim();
  if (!/^[0-9a-fA-F]{64}$/.test(key)) {
    return res.status(400).json({ error: 'chave pública inválida' });
  }
  getDb().prepare('UPDATE platform_users SET public_key_hex = ? WHERE id = ?').run(key, req.user.id);
  const user = getDb().prepare('SELECT * FROM platform_users WHERE id = ?').get(req.user.id);
  res.json({ user: publicUser(user) });
});

// ── Profile / preferences ─────────────────────────────────────────────
app.patch('/api/profile', requireAuth, requireCsrf, (req, res) => {
  const name = String(req.body?.display_name || '').trim();
  if (!name) return res.status(400).json({ error: 'nome é obrigatório' });
  getDb().prepare('UPDATE platform_users SET display_name = ? WHERE id = ?').run(name, req.user.id);
  const user = getDb().prepare('SELECT * FROM platform_users WHERE id = ?').get(req.user.id);
  res.json({ user: publicUser(user) });
});

app.get('/api/preferences', requireAuth, (req, res) => {
  res.json({ preferences: preferencesFor(req.user.id) });
});

app.patch('/api/preferences', requireAuth, requireCsrf, (req, res) => {
  const { theme, language, notifications } = req.body || {};
  const db = getDb();
  const existing = db.prepare('SELECT user_id FROM user_preferences WHERE user_id = ?').get(req.user.id);
  if (existing) {
    db.prepare(
      'UPDATE user_preferences SET theme = COALESCE(?, theme), language = COALESCE(?, language), notifications = COALESCE(?, notifications) WHERE user_id = ?',
    ).run(
      theme ?? null,
      language ?? null,
      typeof notifications === 'boolean' ? (notifications ? 1 : 0) : null,
      req.user.id,
    );
  } else {
    db.prepare(
      'INSERT INTO user_preferences (user_id, theme, language, notifications) VALUES (?, ?, ?, ?)',
    ).run(req.user.id, theme || 'dark', language || 'pt-BR', notifications === false ? 0 : 1);
  }
  res.json({ preferences: preferencesFor(req.user.id) });
});

// ── Directory (gift/transfer recipient resolution) ────────────────────
app.get('/api/directory', requireAuth, (_req, res) => {
  const rows = getDb()
    .prepare('SELECT id, email, display_name, public_key_hex FROM platform_users ORDER BY display_name')
    .all();
  res.json({
    members: rows.map((r) => ({
      id: r.id,
      email: r.email,
      display_name: r.display_name,
      public_key_hex: r.public_key_hex,
    })),
  });
});

app.get('/api/directory/by-key/:pubkey', (_req, res) => {
  const pubkey = String(_req.params.pubkey || '').trim().toLowerCase();
  const row = getDb()
    .prepare('SELECT display_name FROM platform_users WHERE lower(public_key_hex) = ?')
    .get(pubkey);
  res.json({ display_name: row ? row.display_name : null });
});

// ── Wallet registry (bridge user <-> AUGEID) ──────────────────────────
app.get('/api/linked-wallets', requireAuth, (req, res) => {
  const rows = getDb()
    .prepare('SELECT user_id, account_number, linked_at FROM linked_wallets WHERE user_id = ? ORDER BY account_number')
    .all(req.user.id);
  res.json({ wallets: rows });
});

app.post('/api/linked-wallets', requireAuth, requireCsrf, (req, res) => {
  const accountNumber = Number(req.body?.account_number);
  if (!Number.isInteger(accountNumber) || accountNumber <= 0) {
    return res.status(400).json({ error: 'account_number inválido' });
  }
  const db = getDb();
  db.prepare(
    'INSERT INTO linked_wallets (user_id, account_number, linked_at) VALUES (?, ?, ?) ON CONFLICT(user_id, account_number) DO NOTHING',
  ).run(req.user.id, accountNumber, new Date().toISOString());
  const row = db
    .prepare('SELECT user_id, account_number, linked_at FROM linked_wallets WHERE user_id = ? AND account_number = ?')
    .get(req.user.id, accountNumber);
  res.status(201).json({ wallet: row });
});

app.delete('/api/linked-wallets/:accountNumber', requireAuth, requireCsrf, (req, res) => {
  const accountNumber = Number(req.params.accountNumber);
  getDb()
    .prepare('DELETE FROM linked_wallets WHERE user_id = ? AND account_number = ?')
    .run(req.user.id, accountNumber);
  res.status(204).end();
});

// ── Validador (SaaS de validação) ─────────────────────────────────────
//
// Purchase orders bridge the wallet user to the ops backend: the wallet server
// issues the license on the user's behalf (server-side admin key) after payment
// is confirmed by an operator. The plaintext license key is returned to the
// owner exactly once, at issue time — it is never stored.

function orderView(row) {
  return {
    id: row.id,
    user_id: row.user_id,
    plan: row.plan,
    method: row.method,
    amount_usd: row.amount_usd,
    status: row.status,
    license_id: row.license_id,
    license_key: row.license_key,
    created_at: row.created_at,
    paid_at: row.paid_at,
    issued_at: row.issued_at,
  };
}

/** First linked AUGEID (account_number) of a member, or null. The license is
 *  bound to this number so the member activates with the AUGEID only. */
function augeidFor(userId) {
  const row = getDb()
    .prepare('SELECT account_number FROM linked_wallets WHERE user_id = ? ORDER BY linked_at LIMIT 1')
    .get(userId);
  return row ? String(row.account_number) : null;
}

const CONFIRM_KEY = process.env.AUGECOIN_VALIDATOR_CONFIRM_KEY || '';

function requireConfirmKey(req, res, next) {
  const key = req.headers['x-confirm-key'];
  if (!CONFIRM_KEY || !key || key !== CONFIRM_KEY) {
    return res.status(401).json({ error: 'unauthorized' });
  }
  next();
}

app.get('/api/validator/plans', (_req, res) => {
  res.json({ plans: Object.values(PLANS) });
});

app.get('/api/validator/orders', requireAuth, (req, res) => {
  const rows = getDb()
    .prepare('SELECT * FROM validator_orders WHERE user_id = ? ORDER BY created_at DESC')
    .all(req.user.id);
  res.json({ orders: rows.map(orderView) });
});

app.post('/api/validator/orders', requireAuth, requireCsrf, (req, res) => {
  const { plan, method } = req.body || {};
  if (!PLANS[plan]) return res.status(400).json({ error: 'plano inválido' });
  if (!['pix', 'usdt', 'auge'].includes(method)) {
    return res.status(400).json({ error: 'método inválido' });
  }

  const id = randomUUID();
  const now = new Date().toISOString();
  getDb()
    .prepare(
      'INSERT INTO validator_orders (id, user_id, plan, method, amount_usd, status, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)',
    )
    .run(id, req.user.id, plan, method, PLANS[plan].usd, 'pending', now);
  const row = getDb().prepare('SELECT * FROM validator_orders WHERE id = ?').get(id);
  res.status(201).json({ order: orderView(row) });
});

// Operator-only: mark an order as paid (manual payment review). Once payment is
// confirmed the license is issued automatically (server-side admin key), so the
// member gets it immediately without a second manual step. If issuing fails the
// order stays `paid` and the member can still emit it from the wallet.
app.post('/api/validator/orders/:id/confirm', requireConfirmKey, async (req, res) => {
  const row = getDb().prepare('SELECT * FROM validator_orders WHERE id = ?').get(req.params.id);
  if (!row) return res.status(404).json({ error: 'pedido não encontrado' });
  if (row.status !== 'pending') return res.status(409).json({ error: 'pedido não está pendente' });

  getDb()
    .prepare('UPDATE validator_orders SET status = ?, paid_at = ? WHERE id = ?')
    .run('paid', new Date().toISOString(), row.id);

  let issueError = null;
  try {
    const issued = await ops.post('/v1/licenses', { user_id: row.user_id, plan: row.plan, augeid: augeidFor(row.user_id) });
    getDb()
      .prepare('UPDATE validator_orders SET status = ?, license_id = ?, license_key = ?, issued_at = ? WHERE id = ?')
      .run('issued', issued.license.id, issued.license_key, new Date().toISOString(), row.id);
  } catch (e) {
    console.error(e);
    issueError = e;
  }

  const fresh = getDb().prepare('SELECT * FROM validator_orders WHERE id = ?').get(row.id);
  res.json({ order: orderView(fresh), license_issue_error: issueError ? String(issueError.message || issueError) : null });
});

// Owner issues the license once payment is confirmed. The plaintext key is
// returned exactly once; re-issuing is rejected.
app.post('/api/validator/orders/:id/issue', requireAuth, requireCsrf, async (req, res) => {
  const row = getDb().prepare('SELECT * FROM validator_orders WHERE id = ?').get(req.params.id);
  if (!row) return res.status(404).json({ error: 'pedido não encontrado' });
  if (row.user_id !== req.user.id) return res.status(403).json({ error: 'forbidden' });
  if (row.status === 'cancelled') return res.status(409).json({ error: 'pedido cancelado' });
  if (row.status === 'issued' && row.license_id) {
    return res.status(409).json({ error: 'licença já emitida', license_id: row.license_id });
  }
  if (row.status !== 'paid') return res.status(409).json({ error: 'pagamento ainda não confirmado' });

  try {
    const issued = await ops.post('/v1/licenses', { user_id: req.user.id, plan: row.plan, augeid: augeidFor(req.user.id) });
    getDb()
      .prepare('UPDATE validator_orders SET status = ?, license_id = ?, license_key = ?, issued_at = ? WHERE id = ?')
      .run('issued', issued.license.id, issued.license_key, new Date().toISOString(), row.id);
    res.status(201).json({ license: issued.license, license_key: issued.license_key });
  } catch (e) {
    console.error(e);
    res.status(502).json({ error: 'falha ao emitir licença' });
  }
});

// Combined view for the validator dashboard (licenses + validators + rewards).
app.get('/api/validator/overview', requireAuth, async (req, res) => {
  try {
    const { licenses } = await ops.get(`/v1/licenses?user_id=${encodeURIComponent(req.user.id)}`);
    const validators = [];
    for (const lic of licenses) {
      const vs = await ops.get(`/v1/validators?license_id=${encodeURIComponent(lic.id)}`);
      for (const v of vs.validators) {
        const detail = await ops.get(`/v1/validators/${encodeURIComponent(v.id)}`);
        validators.push(detail);
      }
    }
    res.json({ licenses, validators });
  } catch (e) {
    console.error(e);
    res.status(502).json({ error: 'falha ao consultar o backend operacional' });
  }
});

// ── Faturas & Assinaturas (serviços recorrentes) ──────────────────────
//
// Unified billing view for the member: recurring services (licenses) with
// their expiry, plus the invoices (orders) with paid/pending status. Derived
// from the existing validator SaaS data — no separate schema.

const BILLING_EXPIRY_SOON_DAYS = 7;

function subscriptionView(license, now) {
  const expiresAt = license.expires_at ? new Date(license.expires_at) : null;
  const daysRemaining = expiresAt ? Math.ceil((expiresAt - now) / 86_400_000) : null;
  let status = license.status || 'unknown';
  if (status === 'active' && daysRemaining !== null && daysRemaining <= 0) status = 'expired';
  return {
    id: license.id,
    plan: license.plan,
    status,
    expires_at: license.expires_at || null,
    created_at: license.created_at || null,
    days_remaining: daysRemaining,
    expiring_soon:
      status === 'active' &&
      daysRemaining !== null &&
      daysRemaining > 0 &&
      daysRemaining <= BILLING_EXPIRY_SOON_DAYS,
  };
}

app.get('/api/billing', requireAuth, async (req, res) => {
  const now = new Date();

  const invoices = getDb()
    .prepare('SELECT * FROM validator_orders WHERE user_id = ? ORDER BY created_at DESC')
    .all(req.user.id)
    .map(orderView);

  let subscriptions = [];
  try {
    const { licenses } = await ops.get(`/v1/licenses?user_id=${encodeURIComponent(req.user.id)}`);
    subscriptions = (licenses || []).map((l) => subscriptionView(l, now));
  } catch (e) {
    console.error(e);
    return res.status(502).json({ error: 'falha ao consultar o backend operacional' });
  }

  const paidStatuses = new Set(['paid', 'issued']);
  const paidInvoices = invoices.filter((i) => paidStatuses.has(i.status));
  const pendingInvoices = invoices.filter((i) => i.status === 'pending');

  res.json({
    subscriptions,
    invoices,
    summary: {
      active: subscriptions.filter((s) => s.status === 'active').length,
      expiring_soon: subscriptions.filter((s) => s.expiring_soon).length,
      expired: subscriptions.filter((s) => s.status === 'expired').length,
      paid_invoices: paidInvoices.length,
      pending_invoices: pendingInvoices.length,
      total_paid_usd: paidInvoices.reduce((s, i) => s + (Number(i.amount_usd) || 0), 0),
    },
  });
});

// Desktop installer download links (latest release per platform).
app.get('/api/validator/downloads', requireAuth, async (_req, res) => {
  try {
    const { releases } = await ops.get('/releases');
    const downloads = {};
    for (const r of releases) {
      downloads[r.platform] = { version: r.version, artifact_url: r.artifact_url };
    }
    res.json({ downloads });
  } catch (e) {
    console.error(e);
    res.json({ downloads: {} });
  }
});

// ── Financial configuration + admin order management ──────────────────
//
// The operator configures the payment instructions (PIX / USDT / AUGE) once;
// the member sees them during purchase. Orders (invoices) are listed and
// manually approved/cancelled here. All admin routes use the confirm key.

function paymentConfigView(row) {
  return {
    pix_key: row.pix_key,
    usdt_address: row.usdt_address,
    usdt_network: row.usdt_network,
    auge_address: row.auge_address,
    auge_network: row.auge_network,
  };
}

function getPaymentConfigRow() {
  let row = getDb().prepare('SELECT * FROM payment_config WHERE id = 1').get();
  if (!row) {
    getDb().prepare('INSERT INTO payment_config (id) VALUES (1)').run();
    row = getDb().prepare('SELECT * FROM payment_config WHERE id = 1').get();
  }
  return row;
}

// Public: payment instructions shown to the member during purchase.
app.get('/api/payment-info', (_req, res) => {
  res.json({ payment: paymentConfigView(getPaymentConfigRow()) });
});

// Admin: read the financial configuration.
app.get('/api/admin/payment-config', requireConfirmKey, (_req, res) => {
  res.json({ payment: paymentConfigView(getPaymentConfigRow()) });
});

// Admin: upsert the financial configuration.
app.put('/api/admin/payment-config', requireConfirmKey, (req, res) => {
  const b = req.body || {};
  const row = getPaymentConfigRow();
  const pick = (v) => (typeof v === 'string' ? v.trim() : '');
  getDb()
    .prepare(
      `UPDATE payment_config SET pix_key = ?, usdt_address = ?, usdt_network = ?, auge_address = ?, auge_network = ? WHERE id = 1`,
    )
    .run(
      pick(b.pix_key ?? row.pix_key),
      pick(b.usdt_address ?? row.usdt_address),
      pick(b.usdt_network ?? row.usdt_network),
      pick(b.auge_address ?? row.auge_address),
      pick(b.auge_network ?? row.auge_network),
    );
  res.json({ payment: paymentConfigView(getPaymentConfigRow()) });
});

// Admin: list every purchase order with the member identity.
app.get('/api/admin/orders', requireConfirmKey, (_req, res) => {
  const rows = getDb()
    .prepare(
      `SELECT o.*, u.email, u.display_name
         FROM validator_orders o
         JOIN platform_users u ON u.id = o.user_id
        ORDER BY o.created_at DESC`,
    )
    .all();
  res.json({
    orders: rows.map((r) => ({
      ...orderView(r),
      email: r.email,
      display_name: r.display_name,
    })),
  });
});

// Admin: cancel a pending order.
app.post('/api/validator/orders/:id/cancel', requireConfirmKey, (req, res) => {
  const row = getDb().prepare('SELECT * FROM validator_orders WHERE id = ?').get(req.params.id);
  if (!row) return res.status(404).json({ error: 'pedido não encontrado' });
  if (row.status !== 'pending') return res.status(409).json({ error: 'pedido não está pendente' });
  getDb().prepare('UPDATE validator_orders SET status = ? WHERE id = ?').run('cancelled', row.id);
  res.json({ order: orderView({ ...row, status: 'cancelled' }) });
});

// Admin: list platform members (id + email + display name) for the manual
// license-issue picker — the admin selects a member by name instead of typing
// a raw UUID.
app.get('/api/admin/members', requireConfirmKey, (_req, res) => {
  const rows = getDb()
    .prepare('SELECT id, email, display_name, public_key_hex FROM platform_users ORDER BY display_name')
    .all();
  res.json({
    members: rows.map((r) => ({
      id: r.id,
      email: r.email,
      display_name: r.display_name,
      public_key_hex: r.public_key_hex,
    })),
  });
});

// Admin: issue a license directly for a member (manual sale / off-platform
// payment). Issues on the ops backend and records a `validator_orders` row so
// the member sees the license and its activation key in the wallet.
app.post('/api/admin/licenses', requireConfirmKey, async (req, res) => {
  const { user_id: userId, plan } = req.body || {};
  if (!userId || typeof userId !== 'string') return res.status(400).json({ error: 'user_id inválido' });
  if (!PLANS[plan]) return res.status(400).json({ error: 'plano inválido' });

  const member = getDb().prepare('SELECT id FROM platform_users WHERE id = ?').get(userId);
  if (!member) return res.status(404).json({ error: 'membro não encontrado' });

  try {
    const issued = await ops.post('/v1/licenses', { user_id: userId, plan, augeid: augeidFor(userId) });
    const id = randomUUID();
    const now = new Date().toISOString();
    getDb()
      .prepare(
        `INSERT INTO validator_orders (id, user_id, plan, method, amount_usd, status, license_id, license_key, created_at, paid_at, issued_at)
         VALUES (?, ?, ?, 'auge', ?, 'issued', ?, ?, ?, ?, ?)`,
      )
      .run(id, userId, plan, PLANS[plan].usd, issued.license.id, issued.license_key, now, now, now);
    res.status(201).json({ license: issued.license, license_key: issued.license_key });
  } catch (e) {
    console.error(e);
    res.status(502).json({ error: 'falha ao emitir licença' });
  }
});

// ── Error handler ─────────────────────────────────────────────────────
app.use((err, _req, res, _next) => {
  console.error(err);
  res.status(500).json({ error: 'internal error' });
});

app.listen(PORT, () => {
  console.log(`[wallet-web-server] listening on :${PORT}`);
});
