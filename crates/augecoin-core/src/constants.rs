pub const DECIMALS: u8 = 8;
pub const ONE_AUGE: u64 = 100_000_000;
/// Minimum fee per operation: 0.00001 AUGE, expressed in augesat.
pub const MIN_FEE_AUGESAT: u64 = 1_000;

pub const MINER_FEE_PER_OPERATION: u64 = MIN_FEE_AUGESAT;

/// Chain ID for replay protection across networks.
/// mainnet=1, testnet=2, devnet=3. Set via AUGECOIN_CHAIN_ID env var.
pub const CT_CHAIN_ID_MAINNET: u64 = 1;
pub const CT_CHAIN_ID_TESTNET: u64 = 2;
pub const CT_CHAIN_ID_DEVNET: u64 = 3;

pub const CT_ACCOUNTS_PER_BLOCK: u64 = 10;
pub const CT_AUGEIDS_PER_BLOCK: u64 = CT_ACCOUNTS_PER_BLOCK;
pub const CT_MAX_ACCOUNT_DATA: usize = 32;

pub const CT_MAX_BLOCK_PAYLOAD: usize = 255;
pub const CT_MAX_OPERATION_PAYLOAD: usize = 255;
pub const CT_MAX_MULTI_OPERATION_SENDERS: usize = 100;
pub const CT_MAX_MULTI_OPERATION_RECEIVERS: usize = 1000;
pub const CT_MAX_MULTI_OPERATION_CHANGERS: usize = 100;

// ── AUGECOIN Monetary Policy ────────────────────────────────────────────
// Block interval:       15 seconds
// Block reward:         7.25 AUGE
// Emission model:       Linear (no halving)
// Premine:              None
// Developer reward:     0%
// Validator reward:     100%
// Fee distribution:     100% validator
// Economic period:      50 years (block-count based at 15s interval)
// Blocks per year:      2 102 400 (at 15s)
// Total blocks:         105 120 000
// Maximum emission:     762 120 000 AUGE
// ────────────────────────────────────────────────────────────────────────

/// Block reward in augesat (authoritative value used by consensus).
/// 7.25 AUGE = 725 000 000 augesat.
pub const CT_BLOCK_REWARD_AUGESAT: u64 = 725_000_000; // 7.25 AUGE in augesat
pub const TOTAL_SUPPLY_AUGE: u64 = 762_120_000;
pub const TOTAL_SUPPLY_AUGESAT: u64 = 76_212_000_000_000_000;
pub const TOTAL_EMISSION_BLOCKS: u64 = 105_120_000;

// Legacy constants — retained for protocol compatibility but unused in emission logic
pub const CT_FIRST_REWARD_AUGESAT: u64 = 100_000_000;
pub const CT_MIN_REWARD_AUGESAT: u64 = 10_000_000;
pub const CT_NEW_LINE_REWARD_DECREASE: u64 = 210_240;
pub const CT_DEVELOPER_ACCOUNT_START: u64 = 0;
pub const CT_DEVELOPER_ACCOUNT_COUNT: u64 = 5;
pub const CT_DEVELOPER_REWARD_PERCENT: u64 = 20;

pub const CT_BUILD_PROTOCOL: u16 = 5;
pub const CT_MAX_PROTOCOL: u16 = 6;

pub const CT_PROTOCOL_2_ACTIVATION: u64 = 0;
pub const CT_PROTOCOL_3_ACTIVATION: u64 = 0;
pub const CT_PROTOCOL_4_ACTIVATION: u64 = 0;
pub const CT_PROTOCOL_5_ACTIVATION: u64 = 0;
pub const CT_PROTOCOL_6_ACTIVATION: u64 = u64::MAX;

pub const CT_NET_PROTOCOL_VERSION: u16 = 14;
pub const CT_NET_PROTOCOL_AVAILABLE: u16 = 15;

pub const CT_DEFAULT_P2P_PORT: u16 = 4004;
pub const CT_DEFAULT_JSON_RPC_PORT: u16 = 4003;
pub const CT_DEFAULT_MINER_RPC_PORT: u16 = 4009;

pub const CT_BLOCK_TIME_SECONDS: u64 = 15;
pub const CT_MAX_FUTURE_BLOCK_TIMESTAMP_SECONDS: u64 = 30;
pub const CT_NEW_LINE_SECONDS_AVG: u64 = 15;

pub const CT_MAX_SENDERS_PER_OPERATION: usize = 100;
pub const CT_MAX_RECEIVERS_PER_OPERATION: usize = 1000;
pub const CT_MAX_CHANGERS_PER_OPERATION: usize = 100;

/// Number of recent blocks to keep after pruning.
/// Older blocks are deleted; the SafeBox retains all account state.
/// Must be >= 1. Value of 100 matches the PascalCoin checkpoint depth.
pub const PRUNE_KEEP_BLOCKS: u64 = 100;

/// SafeBox full-snapshot interval (in blocks). The SafeBox is persisted to
/// RocksDB only every N blocks; between snapshots only modified accounts are
/// written. This is a local persistence optimization and does not affect the
/// consensus SafeBox hash.
pub const SAFEBOX_SNAPSHOT_INTERVAL_DEV: u64 = 100;
pub const SAFEBOX_SNAPSHOT_INTERVAL_MAINNET: u64 = 1000;
