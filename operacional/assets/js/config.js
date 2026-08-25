// AUGECOIN Operacional — runtime configuration & constants.
//
// The ops API endpoint can be overridden at runtime with:
//   ?ops=https://other.host/api          (query string)
//   localStorage.setItem('auge_ops_api', ...) (persistent)
// Defaults to same-origin `/api`, proxied by nginx to the augecoin-ops service.

export const CONFIG = Object.freeze({
  get OPS_API_URL() {
    const qs = new URLSearchParams(location.search).get('ops');
    const ls = (() => { try { return localStorage.getItem('auge_ops_api'); } catch { return null; } })();
    return qs || ls || '/api';
  },

  NETWORK: 'Testnet',
  DECIMALS: 8,
  AUGESAT_PER_AUGE: 100_000_000,
  BLOCK_REWARD_AUGE: 7.25,
  BLOCK_TIME_SECONDS: 15,
  POLL_INTERVAL_MS: 15000,
  HEARTBEAT_WINDOW_SECS: 120,

  // Backend financeiro (faturas + configuração de pagamento), servido pelo
  // wallet-web (8787) e exposto pelo nginx como /wallet-api/api.
  WALLET_API_URL: '/wallet-api/api',
});

const KEY = 'auge_admin_key';

export function getApiKey() {
  try { return sessionStorage.getItem(KEY) || ''; } catch { return ''; }
}

export function setApiKey(key) {
  try { sessionStorage.setItem(KEY, key); } catch { /* ignore */ }
}

export function clearApiKey() {
  try { sessionStorage.removeItem(KEY); } catch { /* ignore */ }
}

const CONFIRM_KEY = 'auge_finance_key';

export function getConfirmKey() {
  try { return sessionStorage.getItem(CONFIRM_KEY) || ''; } catch { return ''; }
}

export function setConfirmKey(key) {
  try { sessionStorage.setItem(CONFIRM_KEY, key); } catch { /* ignore */ }
}

export function clearConfirmKey() {
  try { sessionStorage.removeItem(CONFIRM_KEY); } catch { /* ignore */ }
}
