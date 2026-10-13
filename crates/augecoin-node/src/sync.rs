//! Checkpoint-based sync integration for the runtime.
//!
//! When a node detects it is more than ~100 blocks behind peers, the standard
//! block-by-block catch-up fails because peers have pruned old blocks. This
//! module switches to a **checkpoint + replay** protocol:
//!
//!   1. Every validator periodically produces an `AccountSnapshot` checkpoint
//!      and broadcasts it.
//!   2. When a syncing node detects a large gap (> 100 blocks), it uses the
//!      checkpoint to jump to an intermediate state, then replays remaining
//!      blocks.
//!   3. After replay, the node resumes normal catch-up for the last few blocks.
//!
//! Uses the existing `SyncManager` from `augecoin_network::sync` for the
//! actual checkpoint application and block replay logic.


use augecoin_network::sync::SyncManager;
use augecoin_network::transport::ConsensusTransport;
use augecoin_storage::Storage;

/// Phases of the checkpoint-based sync flow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckpointSyncPhase {
    /// Not using checkpoint sync.
    Idle,
    /// Waiting for a peer to send a checkpoint snapshot.
    WaitingForCheckpoint,
    /// Applying the received checkpoint into storage.
    RestoringCheckpoint,
    /// Replaying blocks from checkpoint height toward target.
    ReplayingBlocks,
    /// Checkpoint sync finished; resume normal catch-up.
    Complete,
}

pub struct CheckpointSync {
    /// Whether the checkpoint path may ever run. Off unless explicitly enabled
    /// via `AUGECOIN_ENABLE_CHECKPOINT_SYNC`, because the current checkpoint /
    /// block-replay wire format carries no quorum signature or execution proof
    /// and therefore must not mutate production state.
    enabled: bool,
    phase: CheckpointSyncPhase,
    sync_manager: SyncManager,
    target_height: u64,
    wait_counter: u64,
    blocks_replayed: u64,
    last_checkpoint_height: u64,
}

/// Read the opt-in flag that guards the checkpoint-sync path.
fn checkpoint_sync_enabled() -> bool {
    std::env::var("AUGECOIN_ENABLE_CHECKPOINT_SYNC")
        .map(|v| {
            let v = v.trim();
            v == "1" || v.eq_ignore_ascii_case("true") || v.eq_ignore_ascii_case("yes")
        })
        .unwrap_or(false)
}

impl CheckpointSync {
    pub fn new() -> Self {
        Self::with_enabled(checkpoint_sync_enabled())
    }

    /// Construct with an explicit enabled flag (tests / callers that bypass env).
    pub fn with_enabled(enabled: bool) -> Self {
        CheckpointSync {
            enabled,
            phase: CheckpointSyncPhase::Idle,
            sync_manager: SyncManager::new(),
            target_height: 0,
            wait_counter: 0,
            blocks_replayed: 0,
            last_checkpoint_height: 0,
        }
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn is_active(&self) -> bool {
        self.enabled && self.phase != CheckpointSyncPhase::Idle
    }

    pub fn phase(&self) -> CheckpointSyncPhase {
        self.phase
    }

    /// Start checkpoint-based sync targeting a specific height.
    pub fn start(&mut self, target: u64) {
        if !self.enabled {
            return;
        }
        if self.sync_manager.start_sync(target).is_ok() {
            self.phase = CheckpointSyncPhase::WaitingForCheckpoint;
            self.target_height = target;
            self.wait_counter = 0;
            self.blocks_replayed = 0;
            self.last_checkpoint_height = 0;
            println!("[sync-checkpoint] starting sync to height {target}");
        }
    }

    /// Process an inbound sync message (checkpoint bytes or block bytes).
    /// Returns `true` if the message was consumed.
    pub fn handle_sync_message(&mut self, data: Vec<u8>, storage: &Storage) -> bool {
        if !self.enabled || self.phase == CheckpointSyncPhase::Idle {
            return false;
        }

        // Try as a checkpoint first (AccountSnapshot::to_bytes starts with version byte 0x01).
        if !data.is_empty() && data[0] == 1 {
            if self.sync_manager.receive_checkpoint(&data).is_ok() {
                let cp_h = self.sync_manager.state().current_height;
                println!("[sync-checkpoint] received checkpoint at height {cp_h}");
                if self.sync_manager.apply_checkpoint(storage).is_ok() {
                    self.phase = CheckpointSyncPhase::RestoringCheckpoint;
                    if let Ok(replay_h) = self.sync_manager.begin_replay() {
                        self.phase = CheckpointSyncPhase::ReplayingBlocks;
                        self.last_checkpoint_height = replay_h;
                        println!("[sync-checkpoint] checkpoint applied, replaying from height {replay_h}");
                    } else {
                        println!("[sync-checkpoint] no blocks to replay, sync complete");
                        self.phase = CheckpointSyncPhase::Complete;
                    }
                }
            }
            return true;
        }

        // Not a checkpoint — treat as raw block bytes for replay.
        if matches!(self.phase, CheckpointSyncPhase::ReplayingBlocks | CheckpointSyncPhase::RestoringCheckpoint) {
            if self.sync_manager.receive_block(&data, storage).is_ok() {
                self.blocks_replayed += 1;
                let st = self.sync_manager.state();
                if self.sync_manager.is_complete() {
                    println!(
                        "[sync-checkpoint] replay complete: {} blocks replayed to height {}",
                        self.blocks_replayed,
                        st.current_height
                    );
                    self.phase = CheckpointSyncPhase::Complete;
                }
            }
        }

        false
    }

    /// Periodic tick: produce checkpoints, drain sync messages, handle timeouts.
    /// Returns `true` when checkpoint sync has just completed so the caller can
    /// clean the mempool and invalidate the read cache.
    pub fn tick(
        &mut self,
        _now: u64,
        height: u64,
        storage: &Storage,
        network: &dyn ConsensusTransport,
    ) -> bool {
        if !self.enabled {
            return false;
        }
        if self.phase == CheckpointSyncPhase::Idle {
            self.produce_checkpoint_if_due(height, storage, network);
            return false;
        }

        let mut completed = false;

        // Drain inbound sync messages.
        while let Some(data) = network.try_recv_sync() {
            self.handle_sync_message(data, storage);
            if self.phase == CheckpointSyncPhase::Complete {
                completed = true;
            }
        }

        // Timeout waiting for checkpoint.
        if self.phase == CheckpointSyncPhase::WaitingForCheckpoint {
            self.wait_counter += 1;
            if self.wait_counter >= 60 {
                println!("[sync-checkpoint] timeout waiting for checkpoint, falling back to block sync");
                self.phase = CheckpointSyncPhase::Idle;
            }
        }

        completed
    }

    /// Produce a checkpoint snapshot and broadcast it if enough blocks have passed.
    fn produce_checkpoint_if_due(&mut self, height: u64, storage: &Storage, network: &dyn ConsensusTransport) {
        if height == 0 || height <= self.last_checkpoint_height {
            return;
        }
        // Only produce snapshots every N blocks.
        if height % 500 != 0 && height - self.last_checkpoint_height < 500 {
            return;
        }

        let snapshot = storage.create_checkpoint(height);
        let checkpoint_bytes = snapshot.to_bytes();
        // Never spam the gossip layer with a snapshot it cannot carry: a
        // ~60 MB SafeBox broadcast is rejected every tick (MessageTooLarge)
        // and burns a full serialization per iteration. Record the height
        // regardless so we do not retry in a hot loop.
        self.last_checkpoint_height = height;
        if checkpoint_bytes.len() > augecoin_core::limits::max_transmit_size() {
            eprintln!(
                "[sync-checkpoint] skipping checkpoint broadcast at height {height}: {} bytes exceeds transmit ceiling {}",
                checkpoint_bytes.len(),
                augecoin_core::limits::max_transmit_size()
            );
            return;
        }
        let _ = network.broadcast(checkpoint_bytes);
    }
}

impl Default for CheckpointSync {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod gate_tests {
    use super::*;

    #[test]
    fn disabled_checkpoint_sync_never_activates() {
        let mut cs = CheckpointSync::with_enabled(false);
        assert!(!cs.enabled());
        cs.start(1_000);
        // A disabled instance must stay inert: no wait phase, no state change.
        assert!(!cs.is_active());
        assert_eq!(cs.phase(), CheckpointSyncPhase::Idle);
    }

    #[test]
    fn enabled_checkpoint_sync_starts_waiting() {
        let mut cs = CheckpointSync::with_enabled(true);
        cs.start(1_000);
        assert!(cs.is_active());
        assert_eq!(cs.phase(), CheckpointSyncPhase::WaitingForCheckpoint);
    }
}

