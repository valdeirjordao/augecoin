// AUGECOIN Wallet — platform backend client (Conta da Plataforma + Wallet Registry).
//
// Talks to the wallet-web server over `/api`. Uses HttpOnly cookies for the
// access/refresh tokens and a double-submit CSRF token echoed in the
// `X-CSRF-Token` header. This layer holds NO keys and NO balances.

import { CONFIG } from './config.js';

export class ApiError extends Error {
  constructor(message, status) {
    super(message);
    this.name = 'ApiError';
    this.status = status;
  }
}

function readCsrf() {
  const m = document.cookie.match(/(?:^|;\s*)auge_csrf=([^;]+)/);
  return m ? decodeURIComponent(m[1]) : '';
}

async function apiFetch(path, { method = 'GET', body, csrf = false } = {}) {
  const headers = {};
  if (body !== undefined) headers['Content-Type'] = 'application/json';
  if (csrf) {
    const token = readCsrf();
    if (token) headers['X-CSRF-Token'] = token;
  }

  let res = await fetch(CONFIG.API_URL + path, {
    method,
    headers,
    credentials: 'include',
    body: body !== undefined ? JSON.stringify(body) : undefined,
  });

  // Transparent single refresh on expired access token.
  if (res.status === 401 && path !== '/auth/refresh') {
    const refreshed = await fetch(CONFIG.API_URL + '/auth/refresh', { method: 'POST', credentials: 'include' });
    if (refreshed.ok) {
      res = await fetch(CONFIG.API_URL + path, {
        method,
        headers,
        credentials: 'include',
        body: body !== undefined ? JSON.stringify(body) : undefined,
      });
    }
  }

  if (res.status === 204) return undefined;

  const data = await res.json().catch(() => null);
  if (!res.ok) throw new ApiError((data && data.error) || `HTTP ${res.status}`, res.status);
  return data;
}

// ── Auth / session ─────────────────────────────────────────────────────

export const register = (email, username, password, publicKeyHex) =>
  apiFetch('/auth/register', {
    method: 'POST',
    body: { email, password, display_name: username, public_key_hex: publicKeyHex },
  });

export const login = (email, password) =>
  apiFetch('/auth/login', { method: 'POST', body: { email, password } });

export const logout = () => apiFetch('/auth/logout', { method: 'POST' });

export const me = () => apiFetch('/me');

// ── Profile / preferences ──────────────────────────────────────────────

export const updateProfile = (displayName) =>
  apiFetch('/profile', { method: 'PATCH', body: { display_name: displayName }, csrf: true });

export const updateKey = (publicKeyHex) =>
  apiFetch('/me/key', { method: 'PATCH', body: { public_key_hex: publicKeyHex }, csrf: true });

export const updatePreferences = (prefs) =>
  apiFetch('/preferences', { method: 'PATCH', body: prefs, csrf: true });

export const directory = async () => (await apiFetch('/directory')).members;

export const directoryByKey = async (pubkey) =>
  (await apiFetch('/directory/by-key/' + encodeURIComponent(pubkey))).display_name;

// ── Wallet registry (bridge user <-> AUGEID) ───────────────────────────

export const listLinkedWallets = async () => (await apiFetch('/linked-wallets')).wallets;

export const linkWallet = (accountNumber) =>
  apiFetch('/linked-wallets', { method: 'POST', body: { account_number: accountNumber }, csrf: true });

export const unlinkWallet = (accountNumber) =>
  apiFetch('/linked-wallets/' + accountNumber, { method: 'DELETE', csrf: true });

// ── Validador (SaaS de licenças de validador) ──────────────────────────

export const listPlans = () => apiFetch('/validator/plans');

export const listOrders = async () => (await apiFetch('/validator/orders')).orders;

export const createOrder = (args) =>
  apiFetch('/validator/orders', { method: 'POST', body: args, csrf: true });

export const issueLicense = (orderId) =>
  apiFetch(`/validator/orders/${orderId}/issue`, { method: 'POST', body: {}, csrf: true });

export const getOverview = () => apiFetch('/validator/overview');

export const getDownloads = () => apiFetch('/validator/downloads');

// ── Faturas & Assinaturas (serviços recorrentes) ──────────────────────

export const getBilling = () => apiFetch('/billing');

export const getPaymentInfo = () => apiFetch('/payment-info');
