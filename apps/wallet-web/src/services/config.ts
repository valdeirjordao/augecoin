// AUGECOIN Wallet Web — runtime configuration & chain constants.

const DEFAULT_RPC_URL = 'https://www.augeco.in/rpc';

export function getRpcUrl(): string {
  try {
    const qs = new URLSearchParams(window.location.search).get('rpc');
    if (qs) return qs;
    const ls = localStorage.getItem('auge_wallet_rpc');
    if (ls) return ls;
  } catch {
    /* non-browser env */
  }
  return DEFAULT_RPC_URL;
}

export const CONFIG = {
  get RPC_URL() {
    return getRpcUrl();
  },
  // Platform backend (identity + wallet registry). Same-origin in production
  // (nginx proxies /api) and in dev (Vite dev proxy). Override for local dev.
  API_BASE_URL: '/api',
  NETWORK: 'Testnet',
  DECIMALS: 8,
  AUGESAT_PER_AUGE: 100_000_000n,
  MIN_FEE_AUGESAT: 1_000,
  DERIVATION_INDEX: 0,
  POLL_INTERVAL_MS: 8000,
  DEFAULT_CHAIN_ID: 2n,
} as const;

/** Augesat (smallest unit) -> AUGE string. */
export function augesatToAuge(augesat: bigint | number): string {
  const n = BigInt(augesat);
  const per = CONFIG.AUGESAT_PER_AUGE;
  const integer = n / per;
  const frac = n % per;
  return `${integer}.${frac.toString().padStart(CONFIG.DECIMALS, '0')}`;
}

/** AUGE string -> augesat (number, safe for the amounts used in the UI). */
export function augeToAugesat(auge: string): number {
  const trimmed = auge.trim();
  if (!trimmed || !/^\d*\.?\d*$/.test(trimmed)) return NaN;
  const n = Number(trimmed);
  if (!Number.isFinite(n) || n < 0) return NaN;
  return Math.round(n * 1e8);
}

/** Format augesat as a trimmed, human-readable AUGE amount. */
export function formatAuge(augesat: bigint | number): string {
  return augesatToAuge(augesat).replace(/0+$/, '').replace(/\.$/, '');
}
