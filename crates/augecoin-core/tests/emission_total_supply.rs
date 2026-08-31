use augecoin_core::constants::{CT_BLOCK_REWARD_AUGESAT, ONE_AUGE, TOTAL_EMISSION_BLOCKS};
use augecoin_core::emission::{block_reward, HARD_CAP_AUGESAT};
use proptest::prelude::*;

proptest! {
    #[test]
    fn reward_is_constant_during_emission_period(blocks in 0u64..TOTAL_EMISSION_BLOCKS - 1) {
        let r1 = block_reward(blocks);
        let r2 = block_reward(blocks + 1);
        prop_assert_eq!(r1, r2, "reward must be constant: block {} != block {}", blocks, blocks + 1);
        prop_assert_eq!(r1, CT_BLOCK_REWARD_AUGESAT);
    }

    #[test]
    fn reward_never_exceeds_block_reward(blocks in 0u64..u64::MAX) {
        let r = block_reward(blocks);
        prop_assert!(r <= CT_BLOCK_REWARD_AUGESAT);
    }

    #[test]
    fn reward_is_zero_after_emission_period(blocks in TOTAL_EMISSION_BLOCKS..u64::MAX) {
        let r = block_reward(blocks);
        prop_assert_eq!(r, 0, "reward at block {} (past emission) should be 0", blocks);
    }
}

#[test]
fn block_reward_is_7_25_auge() {
    assert_eq!(block_reward(0), CT_BLOCK_REWARD_AUGESAT);
    assert_eq!(block_reward(1), CT_BLOCK_REWARD_AUGESAT);
    assert_eq!(block_reward(210_240), CT_BLOCK_REWARD_AUGESAT);
    assert_eq!(block_reward(1_000_000), CT_BLOCK_REWARD_AUGESAT);
    assert_eq!(
        block_reward(TOTAL_EMISSION_BLOCKS - 1),
        CT_BLOCK_REWARD_AUGESAT
    );
}

#[test]
fn block_reward_zero_after_total_emission_blocks() {
    assert_eq!(block_reward(TOTAL_EMISSION_BLOCKS), 0);
    assert_eq!(block_reward(TOTAL_EMISSION_BLOCKS + 1), 0);
    assert_eq!(block_reward(TOTAL_EMISSION_BLOCKS * 2), 0);
}

#[test]
fn total_emission_matches_hard_cap() {
    // TOTAL_EMISSION_BLOCKS * CT_BLOCK_REWARD_AUGESAT must equal HARD_CAP_AUGESAT
    let total: u128 = TOTAL_EMISSION_BLOCKS as u128 * CT_BLOCK_REWARD_AUGESAT as u128;
    assert_eq!(
        total, HARD_CAP_AUGESAT as u128,
        "{} blocks × {} augesat = {} must equal {} augesat",
        TOTAL_EMISSION_BLOCKS, CT_BLOCK_REWARD_AUGESAT, total, HARD_CAP_AUGESAT
    );
}

#[test]
fn arithmetic_verification_2_50_auge_times_blocks() {
    assert_eq!(315_576_000u64 * 250_000_000, 78_894_000_000_000_000);
}

#[test]
fn one_auge_equals_100_million_augesat() {
    assert_eq!(ONE_AUGE, 100_000_000);
    assert_eq!(CT_BLOCK_REWARD_AUGESAT, 2 * ONE_AUGE + 50_000_000);
}

#[test]
fn hard_cap_in_auge_matches_declared_value() {
    assert_eq!(HARD_CAP_AUGESAT / ONE_AUGE, 788_940_000);
}
