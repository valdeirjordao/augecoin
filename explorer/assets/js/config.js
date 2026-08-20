// AUGECOIN Explorer — runtime configuration & chain constants.
//
// The RPC endpoint can be overridden at runtime with:
//   ?rpc=https://other.host/rpc          (query string)
//   localStorage.setItem('auge_rpc', ...) (persistent)
export const CONFIG = Object.freeze({
  get RPC_URL() {
    const qs = new URLSearchParams(location.search).get('rpc');
    const ls = (() => { try { return localStorage.getItem('auge_rpc'); } catch { return null; } })();
    return qs || ls || 'https://www.augeco.in/rpc';
  },

  NETWORK: 'Testnet',
  CHAIN_ID: 2,
  PROTOCOL_VERSION: 5,
  PROTOCOL_AVAILABLE: 6,

  // Monetary policy (crates/augecoin-core/src/emission.rs)
  DECIMALS: 8,
  AUGESAT_PER_AUGE: 100_000_000,
  TOTAL_SUPPLY_AUGE: 762_120_000,
  BLOCK_REWARD_AUGE: 7.25,
  BLOCK_REWARD_AUGESAT: 725_000_000,
  TOTAL_EMISSION_BLOCKS: 105_120_000,
  BLOCK_TIME_SECONDS: 15,
  BLOCKS_PER_DAY: 5760,

  // UI tuning
  RECENT_BLOCKS: 12,
  RECENT_TRANSACTIONS: 10,
  HISTORY_SCAN_BLOCKS: 40,
  POLL_INTERVAL_MS: 6000,
  RICH_LIST_MAX: 100,
  FIND_ACCOUNTS_PAGE_SIZE: 100,
  RPC_TIMEOUT_MS: 15000,
  RPC_RETRIES: 3,
});
