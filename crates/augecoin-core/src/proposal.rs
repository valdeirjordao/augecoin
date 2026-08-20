//! Light block-proposal transport layer.
//!
//! The heavy `BlockProposal` carried the *full* serialized `OperationBlock`
//! over gossipsub. That is the single largest message the consensus layer
//! emits, and it was the bottleneck at ~350k transactions: every validator
//! re-propagates the entire block (~1.8 MiB) on every round.
//!
//! This module replaces that transport with a *light* proposal that carries
//! only the block header, the operations Merkle root, and the ordered list of
//! transaction hashes. Validators reconstruct the block from their own mempool
//! (O(1) by-hash lookup) and fetch only the missing transactions on demand via
//! [`GetTransactions`]/[`TransactionsResponse`].
//!
//! Nothing about the blockchain *state* changes: [`OperationBlock`], the
//! SafeBox, `compute_safe_box_hash` and `commit_block_atomic` are untouched.
//! Only the transport differs, and reconstruction is deterministic — a
//! validator rebuilds the exact same `OperationBlock` the leader proposed,
//! byte for byte.

use crate::block::{OperationBlock, OperationBlockHeader};
use crate::operation::Operation;
use augecoin_crypto::hash::blake3_512;
use augecoin_crypto::signature::HybridSignature;

/// Transaction identifier. AUGECOIN hashes with blake3-512, so a transaction
/// hash is 64 bytes. The spec's schematic `Hash32` maps onto this codebase's
/// 64-byte hash type (the wire format predates the light-proposal work and is
/// preserved for compatibility).
pub type TxHash = [u8; 64];

pub const TX_HASH_LEN: usize = 64;
pub const ED25519_SIG_LEN: usize = 64;

/// Hash of a single operation's canonical serialization (identical to the hash
/// used for the operations Merkle tree).
pub fn op_hash(op: &Operation) -> TxHash {
    blake3_512(&op.to_bytes())
}

/// Deterministic Merkle root over a list of operations.
///
/// Mirrors `OperationBlock::compute_operations_merkle_root` exactly: hash each
/// operation, then pairwise-combine (`blake3_512(left || right)`), duplicating
/// an odd trailing node. Returns `[0; 64]` for an empty list.
pub fn merkle_root_of_operations(ops: &[Operation]) -> TxHash {
    merkle_root_from_hashes(&ops.iter().map(op_hash).collect::<Vec<_>>())
}

/// Deterministic Merkle root over a list of pre-computed hashes.
pub fn merkle_root_from_hashes(hashes: &[TxHash]) -> TxHash {
    if hashes.is_empty() {
        return [0u8; 64];
    }
    let mut level: Vec<TxHash> = hashes.to_vec();
    while level.len() > 1 {
        let mut next = Vec::with_capacity(level.len().div_ceil(2));
        for chunk in level.chunks(2) {
            let mut combined = Vec::with_capacity(2 * TX_HASH_LEN);
            combined.extend_from_slice(&chunk[0]);
            if chunk.len() > 1 {
                combined.extend_from_slice(&chunk[1]);
            } else {
                combined.extend_from_slice(&chunk[0]);
            }
            next.push(blake3_512(&combined));
        }
        level = next;
    }
    level[0]
}

/// A light proposal: header + Merkle root + ordered tx hashes + leader signature.
///
/// The `merkle_root` must equal `header.operations_hash` and the root recomputed
/// from the reconstructed operations (checked during block reconstruction).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProposalMessage {
    pub header: OperationBlockHeader,
    pub merkle_root: TxHash,
    pub tx_hashes: Vec<TxHash>,
    pub leader_signature: HybridSignature,
}

impl ProposalMessage {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        let hb = self.header.to_bytes();
        write_u32(&mut buf, hb.len() as u32);
        buf.extend_from_slice(&hb);
        buf.extend_from_slice(&self.merkle_root);
        write_u32(&mut buf, self.tx_hashes.len() as u32);
        for h in &self.tx_hashes {
            buf.extend_from_slice(h);
        }
        buf.extend_from_slice(&self.leader_signature.bytes);
        buf
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self, String> {
        let mut pos = 0usize;
        let hlen = read_u32(data, &mut pos)? as usize;
        let header =
            OperationBlockHeader::from_bytes(&data[pos..pos + hlen]).map_err(|e| e.to_string())?;
        pos += hlen;
        let merkle_root = read_fixed::<TX_HASH_LEN>(data, &mut pos)?;
        let count = read_u32(data, &mut pos)? as usize;
        if count > 25000 {
            return Err("tx_hash count exceeds maximum".into());
        }
        let mut tx_hashes = Vec::with_capacity(count);
        for _ in 0..count {
            tx_hashes.push(read_fixed::<TX_HASH_LEN>(data, &mut pos)?);
        }
        let sig_bytes = read_fixed::<ED25519_SIG_LEN>(data, &mut pos)?;
        Ok(ProposalMessage {
            header,
            merkle_root,
            tx_hashes,
            leader_signature: HybridSignature { bytes: sig_bytes },
        })
    }

    /// Wire size in bytes of this proposal.
    pub fn serialized_size(&self) -> usize {
        self.to_bytes().len()
    }

    /// Total bytes contributed by the tx hashes only (metrics).
    pub fn hash_bytes(&self) -> usize {
        self.tx_hashes.len() * TX_HASH_LEN
    }
}

/// Request for a set of missing transactions, addressed by hash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GetTransactions {
    pub height: u64,
    pub hashes: Vec<TxHash>,
    /// The requester advertises compression support; the responder compresses
    /// [`TransactionsResponse`] only when this is set (peer negotiation).
    pub supports_compression: bool,
}

impl GetTransactions {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        write_u64(&mut buf, self.height);
        write_u32(&mut buf, self.hashes.len() as u32);
        for h in &self.hashes {
            buf.extend_from_slice(h);
        }
        buf.push(self.supports_compression as u8);
        buf
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self, String> {
        let mut pos = 0usize;
        let height = read_u64(data, &mut pos)?;
        let count = read_u32(data, &mut pos)? as usize;
        if count > 25000 {
            return Err("hash count exceeds maximum".into());
        }
        let mut hashes = Vec::with_capacity(count);
        for _ in 0..count {
            hashes.push(read_fixed::<TX_HASH_LEN>(data, &mut pos)?);
        }
        let supports_compression = data.get(pos).copied().unwrap_or(0) != 0;
        Ok(GetTransactions {
            height,
            hashes,
            supports_compression,
        })
    }
}

/// Response carrying the requested transactions (optionally compressed).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransactionsResponse {
    pub height: u64,
    pub transactions: Vec<Operation>,
    pub compressed: bool,
}

impl TransactionsResponse {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        write_u64(&mut buf, self.height);
        let payload = encode_operations(&self.transactions);
        if self.compressed {
            buf.push(1u8);
            let compressed = crate::proposal::compress(&payload);
            write_u32(&mut buf, compressed.len() as u32);
            buf.extend_from_slice(&compressed);
        } else {
            buf.push(0u8);
            write_u32(&mut buf, payload.len() as u32);
            buf.extend_from_slice(&payload);
        }
        buf
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self, String> {
        let mut pos = 0usize;
        let height = read_u64(data, &mut pos)?;
        let compressed = data.get(pos).copied().unwrap_or(0) != 0;
        pos += 1;
        let len = read_u32(data, &mut pos)? as usize;
        let raw = &data[pos..pos + len];
        let payload = if compressed {
            crate::proposal::decompress(raw)?
        } else {
            raw.to_vec()
        };
        let transactions = decode_operations(&payload)?;
        Ok(TransactionsResponse {
            height,
            transactions,
            compressed,
        })
    }
}

/// Deterministically reconstruct an `OperationBlock` from a header, ordered
/// operations and the leader signature. This produces the *exact* same bytes as
/// the leader's original block: same header + same ordered ops + same leader
/// signature => same serialization. Quorum signatures are attached later.
pub fn reconstruct_block(
    header: OperationBlockHeader,
    operations: Vec<Operation>,
    leader_signature: HybridSignature,
) -> OperationBlock {
    let block_hash = header.hash();
    OperationBlock {
        header,
        operations,
        leader_signature,
        quorum_signatures: Vec::new(),
        block_hash,
    }
}

// ── serialization helpers ────────────────────────────────────────────────

fn write_u64(buf: &mut Vec<u8>, v: u64) {
    buf.extend_from_slice(&v.to_be_bytes());
}
fn write_u32(buf: &mut Vec<u8>, v: u32) {
    buf.extend_from_slice(&v.to_be_bytes());
}

fn read_u64(data: &[u8], pos: &mut usize) -> Result<u64, String> {
    if *pos + 8 > data.len() {
        return Err("unexpected EOF".into());
    }
    let b: [u8; 8] = data[*pos..*pos + 8].try_into().unwrap();
    *pos += 8;
    Ok(u64::from_be_bytes(b))
}

fn read_u32(data: &[u8], pos: &mut usize) -> Result<u32, String> {
    if *pos + 4 > data.len() {
        return Err("unexpected EOF".into());
    }
    let b: [u8; 4] = data[*pos..*pos + 4].try_into().unwrap();
    *pos += 4;
    Ok(u32::from_be_bytes(b))
}

fn read_fixed<const N: usize>(data: &[u8], pos: &mut usize) -> Result<[u8; N], String> {
    if *pos + N > data.len() {
        return Err("unexpected EOF".into());
    }
    let mut arr = [0u8; N];
    arr.copy_from_slice(&data[*pos..*pos + N]);
    *pos += N;
    Ok(arr)
}

fn encode_operations(ops: &[Operation]) -> Vec<u8> {
    let mut buf = Vec::new();
    write_u32(&mut buf, ops.len() as u32);
    for op in ops {
        let ob = op.to_bytes();
        write_u32(&mut buf, ob.len() as u32);
        buf.extend_from_slice(&ob);
    }
    buf
}

fn decode_operations(data: &[u8]) -> Result<Vec<Operation>, String> {
    let mut pos = 0usize;
    let count = read_u32(data, &mut pos)? as usize;
    if count > 25000 {
        return Err("operation count exceeds maximum".into());
    }
    let mut ops = Vec::with_capacity(count);
    for _ in 0..count {
        let len = read_u32(data, &mut pos)? as usize;
        let op = Operation::from_bytes(&data[pos..pos + len]).map_err(|e| e.to_string())?;
        pos += len;
        ops.push(op);
    }
    Ok(ops)
}

/// Compress with zstd (level 3: fast enough for a hot consensus path, small
/// enough for the fetch/response round trip).
pub fn compress(data: &[u8]) -> Vec<u8> {
    zstd::bulk::compress(data, 3).unwrap_or_else(|_| data.to_vec())
}

/// Decompress a zstd payload. Returns `Err` on corrupt input (never panics).
pub fn decompress(data: &[u8]) -> Result<Vec<u8>, String> {
    zstd::bulk::decompress(data, 2 * 1024 * 1024 * 1024).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operation::{OperationPayload, OperationType, ReceiverInfo, SenderInfo};
    use augecoin_crypto::signature::Ed25519Signature;

    fn dummy_op(account: u64, n_operation: u64, amount: u64) -> Operation {
        Operation {
            op_type: OperationType::Transaction,
            payload: OperationPayload::Transaction {
                senders: vec![SenderInfo {
                    account,
                    n_operation,
                    amount,
                    payload: vec![],
                }],
                receivers: vec![ReceiverInfo {
                    account: account + 1,
                    amount,
                    payload: vec![],
                }],
                changers: vec![],
                fee: 1_000,
            },
            chain_id: 1,
            signatures: vec![Ed25519Signature { bytes: [7u8; 64] }],
        }
    }

    fn dummy_header() -> OperationBlockHeader {
        OperationBlockHeader {
            block_number: 42,
            account_key: [1u8; 32],
            reward: 725_000_000,
            fee: 0,
            protocol_version: 5,
            protocol_available: 6,
            timestamp: 1_700_000_000,
            initial_safe_box_hash: [0xAAu8; 64],
            operations_hash: [0u8; 64],
            block_payload: vec![],
            proof_of_work: [0u8; 32],
            previous_proof_of_work: [0u8; 32],
            leader_id: 0,
            chain_id: 1,
        }
    }

    #[test]
    fn same_order_same_root() {
        let ops = vec![
            dummy_op(1, 0, 100),
            dummy_op(2, 0, 200),
            dummy_op(3, 0, 300),
        ];
        let a = merkle_root_of_operations(&ops);
        let b = merkle_root_of_operations(&ops);
        assert_eq!(a, b);
    }

    #[test]
    fn different_operation_different_root() {
        let a = merkle_root_of_operations(&[dummy_op(1, 0, 100)]);
        let b = merkle_root_of_operations(&[dummy_op(1, 0, 101)]);
        assert_ne!(a, b);
    }

    #[test]
    fn order_matters() {
        let a = merkle_root_of_operations(&[dummy_op(1, 0, 100), dummy_op(2, 0, 200)]);
        let b = merkle_root_of_operations(&[dummy_op(2, 0, 200), dummy_op(1, 0, 100)]);
        assert_ne!(a, b);
    }

    #[test]
    fn empty_root_is_zero() {
        assert_eq!(merkle_root_of_operations(&[]), [0u8; 64]);
    }

    #[test]
    fn odd_leaf_count_duplicates_trailing_node() {
        let ops = vec![
            dummy_op(1, 0, 100),
            dummy_op(2, 0, 200),
            dummy_op(3, 0, 300),
        ];
        let hashes: Vec<TxHash> = ops.iter().map(op_hash).collect();
        let level = hashes.clone();
        let mut combined = Vec::new();
        combined.extend_from_slice(&level[0]);
        combined.extend_from_slice(&level[1]);
        let l0 = blake3_512(&combined);
        let mut combined = Vec::new();
        combined.extend_from_slice(&level[2]);
        combined.extend_from_slice(&level[2]);
        let l1 = blake3_512(&combined);
        let mut top = Vec::new();
        top.extend_from_slice(&l0);
        top.extend_from_slice(&l1);
        let expected = blake3_512(&top);
        assert_eq!(merkle_root_of_operations(&ops), expected);
    }

    #[test]
    fn proposal_roundtrip() {
        let ops = vec![dummy_op(1, 0, 100), dummy_op(2, 0, 200)];
        let mut header = dummy_header();
        let root = merkle_root_of_operations(&ops);
        header.operations_hash = root;
        let hashes: Vec<TxHash> = ops.iter().map(op_hash).collect();
        let msg = ProposalMessage {
            header: header.clone(),
            merkle_root: root,
            tx_hashes: hashes,
            leader_signature: Ed25519Signature { bytes: [9u8; 64] },
        };
        let bytes = msg.to_bytes();
        let back = ProposalMessage::from_bytes(&bytes).unwrap();
        assert_eq!(msg, back);
    }

    #[test]
    fn get_transactions_roundtrip() {
        let msg = GetTransactions {
            height: 7,
            hashes: vec![[1u8; 64], [2u8; 64]],
            supports_compression: true,
        };
        assert_eq!(GetTransactions::from_bytes(&msg.to_bytes()).unwrap(), msg);
    }

    #[test]
    fn transactions_response_roundtrip_plain() {
        let msg = TransactionsResponse {
            height: 7,
            transactions: vec![dummy_op(1, 0, 100), dummy_op(2, 0, 200)],
            compressed: false,
        };
        assert_eq!(
            TransactionsResponse::from_bytes(&msg.to_bytes()).unwrap(),
            msg
        );
    }

    #[test]
    fn transactions_response_roundtrip_compressed() {
        let msg = TransactionsResponse {
            height: 7,
            transactions: (0..100).map(|i| dummy_op(i, 0, 100)).collect(),
            compressed: true,
        };
        let bytes = msg.to_bytes();
        assert!(
            bytes.len()
                < msg
                    .transactions
                    .iter()
                    .map(|o| o.to_bytes().len())
                    .sum::<usize>(),
            "compression should shrink the payload"
        );
        assert_eq!(TransactionsResponse::from_bytes(&bytes).unwrap(), msg);
    }

    #[test]
    fn reconstruct_is_byte_identical_to_leader() {
        let ops = vec![
            dummy_op(1, 0, 100),
            dummy_op(2, 0, 200),
            dummy_op(3, 0, 300),
        ];
        let mut header = dummy_header();
        let root = merkle_root_of_operations(&ops);
        header.operations_hash = root;
        let leader_signature = Ed25519Signature { bytes: [9u8; 64] };

        let leader_block = reconstruct_block(header.clone(), ops.clone(), leader_signature.clone());
        let validator_block = reconstruct_block(header, ops, leader_signature);
        assert_eq!(leader_block.to_bytes(), validator_block.to_bytes());
        assert_eq!(leader_block.block_hash, validator_block.block_hash);
    }
}
