use augecoin_core::block::OperationBlock;
use augecoin_crypto::signature::HybridSignature;

#[derive(Debug, Clone)]
pub struct EquivocationProof {
    pub validator_id: u64,
    pub height: u64,
    pub block1_hash: [u8; 64],
    pub block1_leader_sig: HybridSignature,
    pub block2_hash: [u8; 64],
    pub block2_leader_sig: HybridSignature,
}

impl serde::Serialize for EquivocationProof {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("EquivocationProof", 6)?;
        s.serialize_field("validator_id", &self.validator_id)?;
        s.serialize_field("height", &self.height)?;
        s.serialize_field("block1_hash", &hex::encode(self.block1_hash))?;
        s.serialize_field(
            "block1_leader_sig",
            &hex::encode(self.block1_leader_sig.bytes),
        )?;
        s.serialize_field("block2_hash", &hex::encode(self.block2_hash))?;
        s.serialize_field(
            "block2_leader_sig",
            &hex::encode(self.block2_leader_sig.bytes),
        )?;
        s.end()
    }
}

pub fn detect_equivocation(blocks: &[OperationBlock]) -> Vec<EquivocationProof> {
    let mut proofs = Vec::new();

    for i in 0..blocks.len() {
        for j in (i + 1)..blocks.len() {
            let b1 = &blocks[i];
            let b2 = &blocks[j];

            if b1.header.block_number == b2.header.block_number
                && b1.header.leader_id == b2.header.leader_id
            {
                let h1 = b1.hash();
                let h2 = b2.hash();

                if h1 != h2 {
                    proofs.push(EquivocationProof {
                        validator_id: b1.header.leader_id,
                        height: b1.header.block_number,
                        block1_hash: h1,
                        block1_leader_sig: b1.leader_signature.clone(),
                        block2_hash: h2,
                        block2_leader_sig: b2.leader_signature.clone(),
                    });
                }
            }
        }
    }

    proofs
}

#[cfg(test)]
mod tests {
    use super::*;
    use augecoin_core::block::OperationBlockHeader;

    fn make_block(height: u64, leader_id: u64, extra: u64) -> OperationBlock {
        OperationBlock {
            header: OperationBlockHeader {
                block_number: height,
                account_key: [0u8; 32],
                reward: 100,
                fee: 0,
                protocol_version: 5,
                protocol_available: 6,
                timestamp: 1000 + extra,
                initial_safe_box_hash: [0u8; 64],
                operations_hash: [0u8; 64],
                block_payload: vec![extra as u8],
                proof_of_work: [0u8; 32],
                previous_proof_of_work: [0u8; 32],
                leader_id,
                chain_id: 1,
            },
            operations: vec![],
            leader_signature: HybridSignature {
                bytes: [extra as u8; 64],
            },
            quorum_signatures: vec![],
            block_hash: [0u8; 64],
        }
    }

    #[test]
    fn conflicting_blocks_generate_proof() {
        let b1 = make_block(10, 2, 1);
        let b2 = make_block(10, 2, 2);

        let proofs = detect_equivocation(&[b1, b2]);
        assert_eq!(proofs.len(), 1);
        assert_eq!(proofs[0].validator_id, 2);
        assert_eq!(proofs[0].height, 10);
    }

    #[test]
    fn different_heights_no_conflict() {
        let b1 = make_block(10, 2, 1);
        let b2 = make_block(11, 2, 1);

        let proofs = detect_equivocation(&[b1, b2]);
        assert!(proofs.is_empty());
    }

    #[test]
    fn different_leaders_no_conflict() {
        let b1 = make_block(10, 2, 1);
        let b2 = make_block(10, 3, 1);

        let proofs = detect_equivocation(&[b1, b2]);
        assert!(proofs.is_empty());
    }

    #[test]
    fn same_block_duplicate_no_false_positive() {
        let b1 = make_block(10, 2, 1);
        let b2 = make_block(10, 2, 1);

        let proofs = detect_equivocation(&[b1, b2]);
        assert!(proofs.is_empty());
    }
}
