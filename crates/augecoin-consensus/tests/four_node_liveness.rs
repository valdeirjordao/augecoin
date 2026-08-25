use augecoin_consensus::equivocation::detect_equivocation;
use augecoin_consensus::quorum::{quorum_threshold, verify_quorum};
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
fn four_validators_finalize_consecutive_blocks() {
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

    let mut prev_hash = [0u8; 64];
    let mut blocks_finalized = 0u64;

    for height in 1..=5 {
        let leader = validator_set.leader_for_height(height).unwrap();
        let leader_id = leader.id;
        let kp = keypairs[leader_id as usize];
        let block = make_block(height, leader_id, prev_hash, kp);

        let mut sigs: Vec<HybridSignature> = keypairs
            .iter()
            .enumerate()
            .filter(|(i, _)| *i as u64 != leader_id)
            .map(|(_, kp)| sign_block(kp, &block))
            .collect();
        sigs.push(sign_block(kp, &block));

        let threshold = quorum_threshold(validator_set.active_validators().len() as u64);
        assert!(
            sigs.len() as u64 >= threshold,
            "not enough signatures for quorum"
        );

        let block_hash = block.hash();
        assert!(
            verify_quorum(&sigs, &validator_set, &block_hash),
            "quorum verification failed"
        );

        prev_hash = block_hash;
        blocks_finalized += 1;
    }

    assert_eq!(blocks_finalized, 5);
}

#[test]
fn one_validator_offline_network_continues() {
    let admin = HybridKeyPair::generate();
    let (v0, kp0) = make_validator(0);
    let (v1, kp1) = make_validator(1);
    let (v2, kp2) = make_validator(2);
    let (v3, kp3) = make_validator(3);

    let validator_set =
        ValidatorSet::new(admin.verifying_key(), vec![v0, v1, v2.clone(), v3.clone()]);

    let active_count = validator_set.active_validators().len();
    let threshold = quorum_threshold(active_count as u64);
    assert_eq!(threshold, 3, "quorum should be 3 of 4");

    let keypairs = [&kp0, &kp1, &kp2, &kp3];
    let online_validators: Vec<usize> = vec![0, 1, 2];

    let prev_hash = [0u8; 64];
    let block = make_block(1, 0, prev_hash, keypairs[0]);

    let sigs: Vec<HybridSignature> = online_validators
        .iter()
        .map(|&i| sign_block(keypairs[i], &block))
        .collect();

    let block_hash = block.hash();
    assert!(
        verify_quorum(&sigs, &validator_set, &block_hash),
        "network should finalize with 3 of 4 validators"
    );
}

#[test]
fn two_validators_offline_network_does_not_finalize() {
    let admin = HybridKeyPair::generate();
    let (v0, kp0) = make_validator(0);
    let (v1, kp1) = make_validator(1);
    let (v2, _kp2) = make_validator(2);
    let (v3, _kp3) = make_validator(3);

    let validator_set =
        ValidatorSet::new(admin.verifying_key(), vec![v0, v1, v2.clone(), v3.clone()]);

    let keypairs = [&kp0, &kp1];

    let block = make_block(1, 0, [0u8; 64], keypairs[0]);
    let sigs: Vec<HybridSignature> = keypairs.iter().map(|kp| sign_block(kp, &block)).collect();

    let block_hash = block.hash();
    assert!(
        !verify_quorum(&sigs, &validator_set, &block_hash),
        "network should NOT finalize with only 2 of 4 validators"
    );
}

#[test]
fn equivocation_is_detected() {
    let kp = HdWallet::from_seed(&[1u8; 64]).derive_keypair(0);
    let b1 = make_block(10, 1, [0u8; 64], &kp);
    let b2 = make_block(10, 1, [1u8; 64], &kp);

    let proofs = detect_equivocation(&[b1.clone(), b2.clone()]);
    assert_eq!(proofs.len(), 1);
    assert_eq!(proofs[0].validator_id, 1);
    assert_eq!(proofs[0].height, 10);
}
