// AUGECOIN Wallet — runtime configuration & chain constants.

export const CONFIG = Object.freeze({
  get RPC_URL() {
    const qs = new URLSearchParams(location.search).get('rpc');
    const ls = (() => { try { return localStorage.getItem('auge_wallet_rpc'); } catch { return null; } })();
    return qs || ls || 'https://www.augeco.in/rpc';
  },

  // Platform backend (identity + wallet registry). Same-origin in production
  // (nginx proxies /api) and overridable for local dev.
  get API_URL() {
    const qs = new URLSearchParams(location.search).get('api');
    const ls = (() => { try { return localStorage.getItem('auge_wallet_api'); } catch { return null; } })();
    return qs || ls || '/api';
  },

  NETWORK: 'Testnet',
  CHAIN_ID: 2n,
  DECIMALS: 8,
  AUGESAT_PER_AUGE: 100_000_000n,
  MIN_FEE_AUGESAT: 1_000,
  DERIVATION_INDEX: 0,
  POLL_INTERVAL_MS: 8000,
});
