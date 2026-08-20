use crate::validator::{ValidatorInfo, ValidatorSet};
use augecoin_crypto::signature::HybridSignature;

pub fn quorum_threshold(validator_count: u64) -> u64 {
    if validator_count == 0 {
        return 0;
    }
    (validator_count * 2) / 3 + 1
}

pub fn verify_quorum(
    signatures: &[HybridSignature],
    validator_set: &ValidatorSet,
    block_hash: &[u8; 64],
) -> bool {
    let active = validator_set.active_validators();
    let threshold = quorum_threshold(active.len() as u64);

    let mut valid_count: u64 = 0;

    for sig in signatures {
        for validator in &active {
            if sig.verify_validator(validator, block_hash) {
                valid_count += 1;
                break;
            }
        }
    }

    valid_count >= threshold
}

trait SignatureExt {
    fn verify_validator(&self, validator: &ValidatorInfo, message: &[u8]) -> bool;
}

impl SignatureExt for HybridSignature {
    fn verify_validator(&self, validator: &ValidatorInfo, message: &[u8]) -> bool {
        let pk = ed25519_dalek::VerifyingKey::from_bytes(&validator.ed25519_public_key).unwrap();
        self.verify(&pk, message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validator::ValidatorInfo;
    use augecoin_crypto::hdkeys::HdWallet;
    use augecoin_crypto::signature::HybridKeyPair;

    #[test]
    fn quorum_3_of_4() {
        assert_eq!(quorum_threshold(4), 3);
    }

    #[test]
    fn quorum_2_of_2() {
        assert_eq!(quorum_threshold(2), 2);
    }

    #[test]
    fn quorum_5_of_7() {
        assert_eq!(quorum_threshold(7), 5);
    }

    #[test]
    fn quorum_1_of_1() {
        assert_eq!(quorum_threshold(1), 1);
    }

    #[test]
    fn exactly_quorum_finalizes() {
        let admin = HybridKeyPair::generate();
        let validators: Vec<ValidatorInfo> = (0..4).map(make_test_validator).collect();
        let set = ValidatorSet::new(admin.verifying_key(), validators);

        let block_hash = [1u8; 64];
        let sigs: Vec<HybridSignature> = (0..3)
            .map(|i| {
                let kp = make_test_keypair(i);
                kp.sign(&block_hash)
            })
            .collect();

        assert!(verify_quorum(&sigs, &set, &block_hash));
    }

    #[test]
    fn less_than_quorum_does_not_finalize() {
        let admin = HybridKeyPair::generate();
        let validators: Vec<ValidatorInfo> = (0..4).map(make_test_validator).collect();
        let set = ValidatorSet::new(admin.verifying_key(), validators);

        let block_hash = [2u8; 64];
        let sigs: Vec<HybridSignature> = (0..2)
            .map(|i| {
                let kp = make_test_keypair(i);
                kp.sign(&block_hash)
            })
            .collect();

        assert!(!verify_quorum(&sigs, &set, &block_hash));
    }

    fn make_test_keypair(index: u64) -> HybridKeyPair {
        let seed = [index as u8; 64];
        HdWallet::from_seed(&seed).derive_keypair(0)
    }

    fn make_test_validator(index: u64) -> ValidatorInfo {
        let kp = make_test_keypair(index);
        let vk = kp.verifying_key();
        let mut ed = [0u8; 32];
        ed.copy_from_slice(&vk.to_bytes());
        ValidatorInfo::new_active(index, ed)
    }
}
