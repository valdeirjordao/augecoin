//! Shared protocol size limits — single source of truth for the gossip
//! transmission ceiling and the block builder's derived target.
//!
//! Both the network layer (gossipsub `max_transmit_size`) and the block
//! builder read from here so the two numbers can never drift apart: raising
//! the transmit ceiling automatically raises the block builder's target
//! without anyone having to recompute a magic `max_operations`.

/// Default maximum size of a single gossipsub message (RPC payload), in bytes.
/// 2 MiB: with a 15s block time this yields ~13 386 transfer ops/block at a
/// 90% usable budget (~1.8 MiB) — roughly 750–900 TPS sustained.
pub const DEFAULT_MAX_TRANSMIT_SIZE: usize = 2_097_152; // 2 MiB

/// Fraction of [`max_transmit_size`] that the block builder targets. Never aim
/// at the exact limit: leave headroom for consensus-message framing, the
/// quorum signatures carried by a commit notification, and any compression
/// variance.
pub const BLOCK_SIZE_SAFETY_MARGIN: f64 = 0.9;

/// Runtime override for the transmit ceiling. Read from
/// `AUGECOIN_MAX_TRANSMIT_SIZE` (bytes); falls back to
/// [`DEFAULT_MAX_TRANSMIT_SIZE`]. A single place so the gossip layer and the
/// block builder always agree.
pub fn max_transmit_size() -> usize {
    std::env::var("AUGECOIN_MAX_TRANSMIT_SIZE")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|&v| v > 0)
        .unwrap_or(DEFAULT_MAX_TRANSMIT_SIZE)
}

/// The serialized-size ceiling the block builder targets for a full block.
/// This is the value the builder compares against, not the raw gossip limit.
pub fn max_block_serialized_size() -> usize {
    ((max_transmit_size() as f64) * BLOCK_SIZE_SAFETY_MARGIN).floor() as usize
}

/// Fixed safety reserve the block builder leaves *on top of* the 90% transmit
/// budget. It absorbs consensus-message framing (`MessageTooLarge` headroom),
/// commit-notification quorum signatures and compression variance. The full
/// committed block (the largest wire message for a block) must still fit in
/// `max_transmit_size`.
pub const PROPOSAL_SAFETY_MARGIN: usize = 16 * 1024; // 16 KiB

/// The byte budget the block builder actually targets. It is always smaller
/// than [`max_block_serialized_size`] so that a committed block plus its
/// quorum signatures and envelope can never trip `MessageTooLarge`.
pub fn max_block_builder_size() -> usize {
    max_block_serialized_size().saturating_sub(PROPOSAL_SAFETY_MARGIN)
}
