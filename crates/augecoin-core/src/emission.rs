use crate::constants::{
    CT_BLOCK_REWARD_AUGESAT, TOTAL_EMISSION_BLOCKS, TOTAL_SUPPLY_AUGE, TOTAL_SUPPLY_AUGESAT,
};

// ── AUGECOIN Monetary Policy ────────────────────────────────────────────
// Block interval:       15 seconds
// Block reward:         7.25 AUGE
// Emission model:       Linear (no halving)
// Premine:              None
// Developer reward:     0%
// Validator reward:     100%
// Fee distribution:     100% validator
// Economic period:      50 years
// Blocks:               105 120 000
// Maximum emission:     762 120 000 AUGE
//
// 105 120 000 blocks × 7.25 AUGE = 762 120 000 AUGE
// ────────────────────────────────────────────────────────────────────────

pub const TOTAL_EMISSION_BLOCKS_CONST: u64 = TOTAL_EMISSION_BLOCKS;

/// Returns the block reward in augesat (smallest monetary unit).
///
/// Every block in the emission period (blocks 0 through 105 119 999)
/// receives exactly 7.25 AUGE = 725 000 000 augesat.
/// After the emission period, the reward is 0.
///
/// No halving, no variable reward, no genesis special case.
pub fn block_reward(block_number: u64) -> u64 {
    if block_number < TOTAL_EMISSION_BLOCKS {
        CT_BLOCK_REWARD_AUGESAT
    } else {
        0
    }
}

pub const HARD_CAP_AUGE: u64 = TOTAL_SUPPLY_AUGE;
pub const HARD_CAP_AUGESAT: u64 = TOTAL_SUPPLY_AUGESAT;

// Developer reward functions — retained as no-ops for API compatibility.
// AUGECOIN has no automatic developer allocation.

pub fn developer_reward(_reward: u64) -> u64 {
    0
}

pub fn developer_leader_reward(reward: u64) -> u64 {
    reward
}

pub fn developer_account_for_block(_block_number: u64) -> u64 {
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::ONE_AUGE;

    #[test]
    fn block_reward_is_always_7_25_auge() {
        let expected = CT_BLOCK_REWARD_AUGESAT;
        assert_eq!(block_reward(0), expected);
        assert_eq!(block_reward(1), expected);
        assert_eq!(block_reward(100), expected);
        assert_eq!(block_reward(210_240), expected);
        assert_eq!(block_reward(1_000_000), expected);
        assert_eq!(block_reward(TOTAL_EMISSION_BLOCKS - 1), expected);
    }

    #[test]
    fn block_reward_zero_after_emission_period() {
        assert_eq!(block_reward(TOTAL_EMISSION_BLOCKS), 0);
        assert_eq!(block_reward(TOTAL_EMISSION_BLOCKS + 1), 0);
        assert_eq!(block_reward(TOTAL_EMISSION_BLOCKS + 1_000_000), 0);
    }

    #[test]
    fn total_emission_equals_hard_cap() {
        let total: u128 = TOTAL_EMISSION_BLOCKS as u128 * CT_BLOCK_REWARD_AUGESAT as u128;
        assert_eq!(total, HARD_CAP_AUGESAT as u128);
    }

    #[test]
    fn total_emission_in_auge_equals_hard_cap() {
        let total_augesat: u128 = TOTAL_EMISSION_BLOCKS as u128 * CT_BLOCK_REWARD_AUGESAT as u128;
        assert_eq!(total_augesat / ONE_AUGE as u128, HARD_CAP_AUGE as u128);
    }

    #[test]
    fn seven_point_twenty_five_auge_times_blocks() {
        assert_eq!(105_120_000u64 * 725_000_000, 76_212_000_000_000_000);
    }

    #[test]
    fn developer_reward_is_zero() {
        for r in [0, 1, 100_000, 1_000_000_000] {
            assert_eq!(
                developer_reward(r),
                0,
                "developer_reward({}) should be 0",
                r
            );
        }
    }

    #[test]
    fn developer_leader_reward_returns_full_reward() {
        for r in [0, 1, 100_000, 1_000_000_000] {
            assert_eq!(developer_leader_reward(r), r);
        }
    }

    #[test]
    fn developer_account_for_block_always_zero() {
        for b in [0u64, 1, 100, 1_000_000, 26_280_000] {
            assert_eq!(developer_account_for_block(b), 0);
        }
    }

    #[test]
    fn no_premine_genesis_has_same_reward() {
        assert_eq!(block_reward(0), block_reward(1));
        assert_eq!(block_reward(0), CT_BLOCK_REWARD_AUGESAT);
    }
}
