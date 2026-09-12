use augecoin_consensus::equivocation::detect_equivocation;
use augecoin_consensus::quorum::{quorum_threshold, verify_quorum};
use augecoin_consensus::round::{RoundPhase, RoundState};
use augecoin_consensus::validator::{ValidatorInfo, ValidatorSet};
use augecoin_core::block::{OperationBlock, OperationBlockHeader};
use augecoin_crypto::hdkeys::HdWallet;
use augecoin_crypto::signature::{HybridKeyPair, HybridSignature};

fn make_validator(index: u64) -> (ValidatorInfo, HybridKeyPair) {
    let seed = [index as u8; 64];
    let kp = HdWallet::from_seed(&seed).derive_keypair(0);
    let vk = kp.verifying_key();
    let mut ed = [0u8; 32];
    ed.copy_from_slice(&vk.to_bytes());
    (ValidatorInfo::new_active(index, ed), kp)
}

fn make_block(
    height: u64,
    leader_id: u64,
    prev_hash: [u8; 64],
    keypair: &HybridKeyPair,
) -> OperationBlock {
    let header = OperationBlockHeader {
        block_number: height,
        account_key: prev_hash[0..32].try_into().unwrap(),
        reward: 100,
        fee: 0,
        protocol_version: 1,
        protocol_available: 1,
        timestamp: height * 60,
        initial_safe_box_hash: [0u8; 64],
        operations_hash: [0u8; 64],
        block_payload: vec![],
        proof_of_work: [0u8; 32],
        previous_proof_of_work: [0u8; 32],
        leader_id,
        chain_id: 1,
    };
    let block_hash = header.hash();
    let leader_signature = keypair.sign(&block_hash);

    OperationBlock {
        header,
        operations: vec![],
        leader_signature,
        quorum_signatures: vec![],
        block_hash,
    }
}

fn sign_block(keypair: &HybridKeyPair, block: &OperationBlock) -> HybridSignature {
    let hash = block.hash();
    keypair.sign(&hash)
}

#[test]
fn leader_equivocation_detected() {
    let kp = HdWallet::from_seed(&[1u8; 64]).derive_keypair(0);
    let b1 = make_block(10, 1, [0u8; 64], &kp);
    let b2 = make_block(10, 1, [1u8; 64], &kp);

    let proofs = detect_equivocation(&[b1, b2]);
    assert_eq!(proofs.len(), 1);
    assert_eq!(proofs[0].validator_id, 1);
    assert_eq!(proofs[0].height, 10);
}

#[test]
fn non_leader_equivocation_by_leader() {
    let kp = HdWallet::from_seed(&[2u8; 64]).derive_keypair(0);
    let b1 = make_block(5, 0, [0u8; 64], &kp);
    let b2 = make_block(5, 0, [42u8; 64], &kp);

    let proofs = detect_equivocation(&[b1, b2]);
    assert_eq!(proofs.len(), 1);
    assert_eq!(proofs[0].validator_id, 0);
    assert_eq!(proofs[0].height, 5);
}

#[test]
fn duplicate_vote_same_validator_same_block() {
    let admin = HybridKeyPair::generate();
    let (v0, kp0) = make_validator(0);
    let (v1, kp1) = make_validator(1);
    let (v2, kp2) = make_validator(2);
    let (v3, _kp3) = make_validator(3);

    let validator_set = ValidatorSet::new(
        admin.verifying_key(),
        vec![v0, v1.clone(), v2.clone(), v3.clone()],
    );

    let block = make_block(1, 0, [0u8; 64], &kp0);
    let block_hash = block.hash();

    let sig0 = sign_block(&kp0, &block);
    let sig1 = sign_block(&kp1, &block);
    let sig2 = sign_block(&kp2, &block);

    let mut sigs = vec![sig0.clone(), sig1.clone(), sig2.clone()];
    sigs.push(sig0.clone());

    assert!(verify_quorum(&sigs, &validator_set, &block_hash));

    use std::collections::HashSet;
    let mut seen = HashSet::new();
    let deduped: Vec<_> = sigs.into_iter().filter(|s| seen.insert(s.bytes)).collect();
    assert_eq!(deduped.len(), 3);
    assert!(verify_quorum(&deduped, &validator_set, &block_hash));
}

#[test]
fn quorum_exceeded_still_finalizes() {
    let admin = HybridKeyPair::generate();
    let (v0, kp0) = make_validator(0);
    let (v1, kp1) = make_validator(1);
    let (v2, kp2) = make_validator(2);
    let (v3, kp3) = make_validator(3);
    let keypairs = [&kp0, &kp1, &kp2, &kp3];

    let validator_set = ValidatorSet::new(
        admin.verifying_key(),
        vec![v0, v1.clone(), v2.clone(), v3.clone()],
    );

    let threshold = quorum_threshold(validator_set.active_validators().len() as u64);
    assert_eq!(
        threshold, 3,
        "quorum threshold for 4 validators should be 3"
    );

    let block = make_block(1, 0, [0u8; 64], &kp0);
    let block_hash = block.hash();

    let sigs: Vec<HybridSignature> = keypairs.iter().map(|kp| sign_block(kp, &block)).collect();
    assert_eq!(sigs.len(), 4);
    assert!(
        verify_quorum(&sigs, &validator_set, &block_hash),
        "4 of 4 signatures should finalize even though quorum is 3"
    );
}

#[test]
fn leader_timeout_no_progress() {
    let state = RoundState::new(1, 0, 120, 0);
    assert!(!state.check_timeout(100));
    assert!(state.check_timeout(120));
    assert!(state.check_timeout(200));
    assert_eq!(state.phase, RoundPhase::Propose);
}

#[test]
fn round_state_propose_in_wrong_phase() {
    let mut state = RoundState::new(1, 0, 120, 0);

    state
        .propose_block(make_block(1, 0, [0u8; 64], &HybridKeyPair::generate()))
        .unwrap();
    assert_eq!(state.phase, RoundPhase::Verify);

    let result = state.propose_block(make_block(1, 0, [0u8; 64], &HybridKeyPair::generate()));
    assert!(result.is_err());

    state.verify_and_advance(true).unwrap();
    assert_eq!(state.phase, RoundPhase::Sign);

    let result = state.propose_block(make_block(1, 0, [0u8; 64], &HybridKeyPair::generate()));
    assert!(result.is_err());

    state.view_change(1, 500);
    assert_eq!(state.phase, RoundPhase::Propose);
    state
        .propose_block(make_block(1, 1, [0u8; 64], &HybridKeyPair::generate()))
        .unwrap();
    assert_eq!(state.phase, RoundPhase::Verify);
}

#[test]
fn quorum_verified_against_correct_block_hash() {
    let admin = HybridKeyPair::generate();
    let (v0, kp0) = make_validator(0);
    let (v1, kp1) = make_validator(1);
    let (v2, kp2) = make_validator(2);
    let (v3, kp3) = make_validator(3);
    let keypairs = [&kp0, &kp1, &kp2, &kp3];

    let validator_set = ValidatorSet::new(
        admin.verifying_key(),
        vec![v0, v1.clone(), v2.clone(), v3.clone()],
    );

    let block_a = make_block(1, 0, [0u8; 64], &kp0);
    let block_b = make_block(1, 0, [1u8; 64], &kp0);

    let hash_a = block_a.hash();
    let hash_b = block_b.hash();
    assert_ne!(
        hash_a, hash_b,
        "different prev_hashes should produce different block hashes"
    );

    let sigs_for_a: Vec<HybridSignature> =
        keypairs.iter().map(|kp| sign_block(kp, &block_a)).collect();

    assert!(verify_quorum(&sigs_for_a, &validator_set, &hash_a));

    assert!(
        !verify_quorum(&sigs_for_a, &validator_set, &hash_b),
        "signatures collected for block A should not verify against block B's hash"
    );
}
