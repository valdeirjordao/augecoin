use crate::validator::ValidatorSet;
use augecoin_core::block::OperationBlock;
use augecoin_crypto::signature::HybridSignature;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoundPhase {
    Propose,
    Verify,
    Sign,
    Commit,
}

#[derive(Debug, Clone)]
pub struct RoundState {
    pub height: u64,
    pub phase: RoundPhase,
    pub leader_id: u64,
    pub proposed_block: Option<OperationBlock>,
    pub collected_signatures: Vec<HybridSignature>,
    pub timeout_seconds: u64,
    pub phase_entered_at: u64,
}

#[derive(Debug, Error)]
pub enum RoundError {
    #[error("invalid phase transition from {from:?} to {to:?}")]
    InvalidTransition { from: RoundPhase, to: RoundPhase },
    #[error("no block proposed")]
    NoBlockProposed,
}

impl RoundState {
    pub fn new(height: u64, leader_id: u64, timeout_seconds: u64, now: u64) -> Self {
        RoundState {
            height,
            phase: RoundPhase::Propose,
            leader_id,
            proposed_block: None,
            collected_signatures: Vec::new(),
            timeout_seconds,
            phase_entered_at: now,
        }
    }

    pub fn propose_block(&mut self, block: OperationBlock) -> Result<(), RoundError> {
        if self.phase != RoundPhase::Propose {
            return Err(RoundError::InvalidTransition {
                from: self.phase,
                to: RoundPhase::Verify,
            });
        }
        self.proposed_block = Some(block);
        self.phase = RoundPhase::Verify;
        Ok(())
    }

    pub fn verify_and_advance(&mut self, is_valid: bool) -> Result<(), RoundError> {
        if self.phase != RoundPhase::Verify {
            return Err(RoundError::InvalidTransition {
                from: self.phase,
                to: RoundPhase::Sign,
            });
        }
        if !is_valid {
            return Ok(());
        }
        self.phase = RoundPhase::Sign;
        Ok(())
    }

    pub fn add_signature(&mut self, signature: HybridSignature) {
        self.collected_signatures.push(signature);
    }

    pub fn try_commit(&mut self, validator_set: &ValidatorSet) -> Result<bool, RoundError> {
        if self.phase != RoundPhase::Sign {
            return Err(RoundError::InvalidTransition {
                from: self.phase,
                to: RoundPhase::Commit,
            });
        }

        let active_count = validator_set.active_validators().len() as u64;
        if active_count == 0 {
            return Ok(false);
        }

        let quorum_threshold = crate::quorum::quorum_threshold(active_count);
        if self.collected_signatures.len() as u64 >= quorum_threshold {
            self.phase = RoundPhase::Commit;
            return Ok(true);
        }

        Ok(false)
    }

    pub fn check_timeout(&self, now: u64) -> bool {
        now >= self.phase_entered_at + self.timeout_seconds
    }

    pub fn view_change(&mut self, next_leader_id: u64, now: u64) {
        self.leader_id = next_leader_id;
        self.phase = RoundPhase::Propose;
        self.proposed_block = None;
        self.collected_signatures.clear();
        self.phase_entered_at = now;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use augecoin_core::block::OperationBlockHeader;
    use augecoin_core::operation::{
        Operation, OperationPayload, OperationType, ReceiverInfo, SenderInfo,
    };

    fn dummy_sig() -> HybridSignature {
        HybridSignature { bytes: [7u8; 64] }
    }

    fn dummy_block(leader_id: u64) -> OperationBlock {
        OperationBlock {
            header: OperationBlockHeader {
                block_number: 1,
                account_key: [0u8; 32],
                reward: 100,
                fee: 0,
                protocol_version: 5,
                protocol_available: 6,
                timestamp: 1000,
                initial_safe_box_hash: [0u8; 64],
                operations_hash: [0u8; 64],
                block_payload: vec![],
                proof_of_work: [0u8; 32],
                previous_proof_of_work: [0u8; 32],
                leader_id,
                chain_id: 1,
            },
            operations: vec![Operation {
                op_type: OperationType::Transaction,
                chain_id: 1,
                payload: OperationPayload::Transaction {
                    senders: vec![SenderInfo {
                        account: 1,
                        n_operation: 0,
                        amount: 100,
                        payload: vec![],
                    }],
                    receivers: vec![ReceiverInfo {
                        account: 2,
                        amount: 90,
                        payload: vec![],
                    }],
                    changers: vec![],
                    fee: 10,
                },
                signatures: vec![dummy_sig()],
            }],
            leader_signature: dummy_sig(),
            quorum_signatures: vec![],
            block_hash: [0u8; 64],
        }
    }

    #[test]
    fn normal_transition_propose_to_commit() {
        let mut state = RoundState::new(1, 0, 120, 0);
        assert_eq!(state.phase, RoundPhase::Propose);

        state.propose_block(dummy_block(0)).unwrap();
        assert_eq!(state.phase, RoundPhase::Verify);

        state.verify_and_advance(true).unwrap();
        assert_eq!(state.phase, RoundPhase::Sign);
    }

    #[test]
    fn timeout_triggers_view_change() {
        let state = RoundState::new(1, 0, 120, 0);
        assert!(state.check_timeout(121));
        assert!(!state.check_timeout(100));
    }

    #[test]
    fn view_change_resets_state() {
        let mut state = RoundState::new(1, 0, 120, 0);
        state.view_change(1, 500);
        assert_eq!(state.leader_id, 1);
        assert_eq!(state.phase, RoundPhase::Propose);
        assert!(state.proposed_block.is_none());
        assert!(state.collected_signatures.is_empty());
    }

    #[test]
    fn invalid_proposal_does_not_advance() {
        let mut state = RoundState::new(1, 0, 120, 0);
        state.propose_block(dummy_block(0)).unwrap();
        state.verify_and_advance(false).unwrap();
        assert_eq!(state.phase, RoundPhase::Verify);
    }

    #[test]
    fn cannot_propose_in_verify_phase() {
        let mut state = RoundState::new(1, 0, 120, 0);
        state.propose_block(dummy_block(0)).unwrap();
        let result = state.propose_block(dummy_block(0));
        assert!(result.is_err());
    }
}
