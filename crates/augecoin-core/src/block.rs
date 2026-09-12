use crate::operation::Operation;
use augecoin_crypto::hash::blake3_512;
use augecoin_crypto::signature::Ed25519Signature;
use augecoin_crypto::signature::HybridSignature;
use thiserror::Error;

pub const ED25519_SIG_LEN: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationBlockHeader {
    pub block_number: u64,
    pub account_key: [u8; 32],
    pub reward: u64,
    pub fee: u64,
    pub protocol_version: u16,
    pub protocol_available: u16,
    pub timestamp: u64,
    pub initial_safe_box_hash: [u8; 64],
    pub operations_hash: [u8; 64],
    pub block_payload: Vec<u8>,
    pub proof_of_work: [u8; 32],
    pub previous_proof_of_work: [u8; 32],
    pub leader_id: u64,
    /// Chain ID for replay protection (mainnet=1, testnet=2, devnet=3).
    /// Included in block hash and signature.
    pub chain_id: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationBlock {
    pub header: OperationBlockHeader,
    pub operations: Vec<Operation>,
    pub leader_signature: HybridSignature,
    pub quorum_signatures: Vec<HybridSignature>,
    pub block_hash: [u8; 64],
}

#[derive(Debug, Error)]
pub enum BlockError {
    #[error("invalid serialized data: {0}")]
    InvalidSerialization(String),
}

fn write_u64(buf: &mut Vec<u8>, v: u64) {
    buf.extend_from_slice(&v.to_be_bytes());
}
fn write_u32(buf: &mut Vec<u8>, v: u32) {
    buf.extend_from_slice(&v.to_be_bytes());
}
fn write_u16(buf: &mut Vec<u8>, v: u16) {
    buf.extend_from_slice(&v.to_be_bytes());
}
fn write_fixed(buf: &mut Vec<u8>, bytes: &[u8]) {
    buf.extend_from_slice(bytes);
}

fn read_u64(data: &[u8], pos: &mut usize) -> Result<u64, BlockError> {
    if *pos + 8 > data.len() {
        return Err(BlockError::InvalidSerialization("unexpected EOF".into()));
    }
    let bytes: [u8; 8] = data[*pos..*pos + 8].try_into().unwrap();
    *pos += 8;
    Ok(u64::from_be_bytes(bytes))
}

fn read_u32(data: &[u8], pos: &mut usize) -> Result<u32, BlockError> {
    if *pos + 4 > data.len() {
        return Err(BlockError::InvalidSerialization("unexpected EOF".into()));
    }
    let bytes: [u8; 4] = data[*pos..*pos + 4].try_into().unwrap();
    *pos += 4;
    Ok(u32::from_be_bytes(bytes))
}

fn read_u16(data: &[u8], pos: &mut usize) -> Result<u16, BlockError> {
    if *pos + 2 > data.len() {
        return Err(BlockError::InvalidSerialization("unexpected EOF".into()));
    }
    let bytes: [u8; 2] = data[*pos..*pos + 2].try_into().unwrap();
    *pos += 2;
    Ok(u16::from_be_bytes(bytes))
}

fn read_fixed<const N: usize>(data: &[u8], pos: &mut usize) -> Result<[u8; N], BlockError> {
    if *pos + N > data.len() {
        return Err(BlockError::InvalidSerialization("unexpected EOF".into()));
    }
    let mut arr = [0u8; N];
    arr.copy_from_slice(&data[*pos..*pos + N]);
    *pos += N;
    Ok(arr)
}

impl OperationBlockHeader {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        write_u64(&mut buf, self.block_number);
        write_fixed(&mut buf, &self.account_key);
        write_u64(&mut buf, self.reward);
        write_u64(&mut buf, self.fee);
        write_u16(&mut buf, self.protocol_version);
        write_u16(&mut buf, self.protocol_available);
        write_u64(&mut buf, self.timestamp);
        write_fixed(&mut buf, &self.initial_safe_box_hash);
        write_fixed(&mut buf, &self.operations_hash);
        let payload_len = self.block_payload.len() as u16;
        write_u16(&mut buf, payload_len);
        write_fixed(&mut buf, &self.block_payload);
        write_fixed(&mut buf, &self.proof_of_work);
        write_fixed(&mut buf, &self.previous_proof_of_work);
        write_u64(&mut buf, self.leader_id);
        write_u64(&mut buf, self.chain_id);
        buf
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self, BlockError> {
        let mut pos = 0;
        let block_number = read_u64(data, &mut pos)?;
        let account_key = read_fixed(data, &mut pos)?;
        let reward = read_u64(data, &mut pos)?;
        let fee = read_u64(data, &mut pos)?;
        let protocol_version = read_u16(data, &mut pos)?;
        let protocol_available = read_u16(data, &mut pos)?;
        let timestamp = read_u64(data, &mut pos)?;
        let initial_safe_box_hash = read_fixed(data, &mut pos)?;
        let operations_hash = read_fixed(data, &mut pos)?;
        let payload_len = read_u16(data, &mut pos)? as usize;
        if pos + payload_len > data.len() {
            return Err(BlockError::InvalidSerialization("unexpected EOF".into()));
        }
        let block_payload = data[pos..pos + payload_len].to_vec();
        pos += payload_len;
        let proof_of_work = read_fixed(data, &mut pos)?;
        let previous_proof_of_work = read_fixed(data, &mut pos)?;
        let leader_id = read_u64(data, &mut pos)?;
        let chain_id = read_u64(data, &mut pos).unwrap_or(1);

        Ok(OperationBlockHeader {
            block_number,
            account_key,
            reward,
            fee,
            protocol_version,
            protocol_available,
            timestamp,
            initial_safe_box_hash,
            operations_hash,
            block_payload,
            proof_of_work,
            previous_proof_of_work,
            leader_id,
            chain_id,
        })
    }

    pub fn hash(&self) -> [u8; 64] {
        blake3_512(&self.to_bytes())
    }
}

impl OperationBlock {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        let header_bytes = self.header.to_bytes();
        buf.extend_from_slice(&header_bytes);

        write_u32(&mut buf, self.operations.len() as u32);
        for op in &self.operations {
            let op_bytes = op.to_bytes();
            write_u32(&mut buf, op_bytes.len() as u32);
            buf.extend_from_slice(&op_bytes);
        }

        write_fixed(&mut buf, &self.leader_signature.bytes);

        write_u32(&mut buf, self.quorum_signatures.len() as u32);
        for sig in &self.quorum_signatures {
            write_fixed(&mut buf, &sig.bytes);
        }

        buf
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self, BlockError> {
        let header = OperationBlockHeader::from_bytes(data)?;
        let mut pos = header.to_bytes().len();

        let op_count = read_u32(data, &mut pos)? as usize;
        if op_count > 25000 {
            return Err(BlockError::InvalidSerialization(
                "op_count exceeds maximum".into(),
            ));
        }
        let mut operations = Vec::with_capacity(op_count);
        for _ in 0..op_count {
            let op_len = read_u32(data, &mut pos)? as usize;
            if pos + op_len > data.len() {
                return Err(BlockError::InvalidSerialization("unexpected EOF".into()));
            }
            let op = Operation::from_bytes(&data[pos..pos + op_len])
                .map_err(|e| BlockError::InvalidSerialization(format!("invalid op: {e}")))?;
            pos += op_len;
            operations.push(op);
        }

        let leader_sig_bytes: [u8; ED25519_SIG_LEN] = read_fixed(data, &mut pos)?;
        let leader_signature = Ed25519Signature {
            bytes: leader_sig_bytes,
        };

        let qs_count = read_u32(data, &mut pos)? as usize;
        if qs_count > 100 {
            return Err(BlockError::InvalidSerialization(
                "quorum sig count exceeds maximum".into(),
            ));
        }
        let mut quorum_signatures = Vec::with_capacity(qs_count);
        for _ in 0..qs_count {
            let sig_bytes: [u8; ED25519_SIG_LEN] = read_fixed(data, &mut pos)?;
            quorum_signatures.push(Ed25519Signature { bytes: sig_bytes });
        }

        let block_hash = header.hash();

        Ok(OperationBlock {
            header,
            operations,
            leader_signature,
            quorum_signatures,
            block_hash,
        })
    }

    pub fn hash(&self) -> [u8; 64] {
        self.header.hash()
    }

    /// Byte size of the serialized block (mirrors `to_bytes` without cloning
    /// the full block). Used by the block builder and metrics to stay within
    /// the gossip transmit ceiling.
    pub fn serialized_size(&self) -> usize {
        block_serialized_size(&self.header, &self.operations, self.quorum_signatures.len())
    }

    pub fn compute_operations_merkle_root(&self) -> [u8; 64] {
        if self.operations.is_empty() {
            return [0u8; 64];
        }
        let mut hashes: Vec<[u8; 64]> = self
            .operations
            .iter()
            .map(|op| blake3_512(&op.to_bytes()))
            .collect();
        while hashes.len() > 1 {
            let mut next = Vec::with_capacity(hashes.len().div_ceil(2));
            for chunk in hashes.chunks(2) {
                let mut combined = Vec::new();
                combined.extend_from_slice(&chunk[0]);
                if chunk.len() > 1 {
                    combined.extend_from_slice(&chunk[1]);
                } else {
                    combined.extend_from_slice(&chunk[0]);
                }
                next.push(blake3_512(&combined));
            }
            hashes = next;
        }
        hashes[0]
    }
}

/// Serialized size (in bytes) of a block assembled from `header`,
/// `operations` and `quorum_sig_count` quorum signatures — identical to what
/// [`OperationBlock::to_bytes`] would produce, computed without allocating the
/// full serialization.
pub fn block_serialized_size(
    header: &OperationBlockHeader,
    operations: &[Operation],
    quorum_sig_count: usize,
) -> usize {
    header.to_bytes().len()
        + 4 // operations count
        + operations
            .iter()
            .map(|op| 4 + op.to_bytes().len()) // u32 length prefix + op bytes
            .sum::<usize>()
        + ED25519_SIG_LEN // leader signature
        + 4 // quorum signature count
        + quorum_sig_count * ED25519_SIG_LEN
}

/// Select the maximum prefix of `pending` operations that fits within
/// `max_size` bytes once serialized into a block together with `header` and
/// `quorum_sig_count` quorum signatures.
///
/// Returns `(selected_operations, resulting_serialized_size)`. Operations
/// beyond the byte budget are left for the caller to keep in the mempool and
/// include in a later block.
pub fn select_block_operations(
    header: &OperationBlockHeader,
    pending: &[Operation],
    quorum_sig_count: usize,
    max_size: usize,
) -> (Vec<Operation>, usize) {
    let mut size = block_serialized_size(header, &[], quorum_sig_count);
    let mut selected = Vec::new();
    for op in pending {
        let contribution = 4 + op.to_bytes().len();
        if size + contribution > max_size {
            break;
        }
        size += contribution;
        selected.push(op.clone());
    }
    (selected, size)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operation::{OperationPayload, ReceiverInfo, SenderInfo};

    fn dummy_sig() -> HybridSignature {
        Ed25519Signature {
            bytes: [7u8; ED25519_SIG_LEN],
        }
    }

    fn dummy_op(account: u64) -> Operation {
        Operation {
            op_type: crate::operation::OperationType::Transaction,
            payload: OperationPayload::Transaction {
                senders: vec![SenderInfo {
                    account,
                    n_operation: 0,
                    amount: 100,
                    payload: vec![],
                }],
                receivers: vec![ReceiverInfo {
                    account: account + 1,
                    amount: 90,
                    payload: vec![],
                }],
                changers: vec![],
                fee: 10,
            },
            chain_id: 1,
            signatures: vec![dummy_sig()],
        }
    }

    fn test_block() -> OperationBlock {
        OperationBlock {
            header: OperationBlockHeader {
                block_number: 42,
                account_key: [1u8; 32],
                reward: 100_000_000,
                fee: 50,
                protocol_version: 5,
                protocol_available: 6,
                timestamp: 1700000000,
                initial_safe_box_hash: [0xAAu8; 64],
                operations_hash: [0xBBu8; 64],
                block_payload: vec![1, 2, 3],
                proof_of_work: [0u8; 32],
                previous_proof_of_work: [0u8; 32],
                leader_id: 0,
                chain_id: 1,
            },
            operations: vec![dummy_op(1), dummy_op(2)],
            leader_signature: dummy_sig(),
            quorum_signatures: vec![dummy_sig(), dummy_sig(), dummy_sig()],
            block_hash: [0u8; 64],
        }
    }

    #[test]
    fn header_roundtrip() {
        let header = test_block().header;
        let bytes = header.to_bytes();
        let restored = OperationBlockHeader::from_bytes(&bytes).unwrap();
        assert_eq!(header, restored);
    }

    #[test]
    fn block_roundtrip() {
        let block = test_block();
        let bytes = block.to_bytes();
        let restored = OperationBlock::from_bytes(&bytes).unwrap();
        assert_eq!(block.header, restored.header);
        assert_eq!(block.operations.len(), restored.operations.len());
        assert_eq!(
            block.quorum_signatures.len(),
            restored.quorum_signatures.len()
        );
    }

    #[test]
    fn block_roundtrip_empty_operations() {
        let mut block = test_block();
        block.operations = vec![];
        block.quorum_signatures = vec![];
        let bytes = block.to_bytes();
        let restored = OperationBlock::from_bytes(&bytes).unwrap();
        assert_eq!(restored.operations.len(), 0);
        assert_eq!(restored.quorum_signatures.len(), 0);
    }

    #[test]
    fn operations_merkle_root_empty() {
        let block = OperationBlock {
            operations: vec![],
            ..test_block()
        };
        assert_eq!(block.compute_operations_merkle_root(), [0u8; 64]);
    }

    #[test]
    fn operations_merkle_root_deterministic() {
        let block = test_block();
        let h1 = block.compute_operations_merkle_root();
        let h2 = block.compute_operations_merkle_root();
        assert_eq!(h1, h2);
    }

    #[test]
    fn serialized_size_matches_to_bytes() {
        let block = test_block();
        assert_eq!(block.serialized_size(), block.to_bytes().len());
    }

    #[test]
    fn select_block_operations_respects_byte_budget() {
        let block = test_block();
        let pending: Vec<Operation> = (0..100).map(dummy_op).collect();

        // Budget that fits roughly the first 3 operations plus overhead.
        let three = select_block_operations(&block.header, &pending, 3, 2000);
        assert!(three.0.len() < 100, "builder must stop early");
        assert!(
            three.1 <= 2000,
            "resulting size {} must be <= budget",
            three.1
        );
        assert!(!three.0.is_empty());

        // A generous budget admits everything.
        let all = select_block_operations(&block.header, &pending, 3, usize::MAX);
        assert_eq!(all.0.len(), 100);
    }

    #[test]
    fn select_block_operations_keeps_leftovers_in_order() {
        let block = test_block();
        let pending: Vec<Operation> = (0..10).map(dummy_op).collect();
        let per_op = block_serialized_size(&block.header, &[dummy_op(0)], 0)
            - block_serialized_size(&block.header, &[], 0);

        // Budget = header+leader overhead + exactly 4 ops worth.
        let overhead = block_serialized_size(&block.header, &[], 0);
        let (selected, _size) =
            select_block_operations(&block.header, &pending, 0, overhead + 4 * per_op);
        assert_eq!(selected.len(), 4);
        assert_eq!(selected[0], dummy_op(0));
        assert_eq!(selected[3], dummy_op(3));
    }
}
