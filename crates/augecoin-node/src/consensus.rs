use augecoin_consensus::quorum::quorum_threshold;
use augecoin_consensus::validator::{ValidatorInfo, ValidatorSet};
use augecoin_core::block::{OperationBlock, OperationBlockHeader};
use augecoin_core::constants::{CT_BUILD_PROTOCOL, CT_MAX_PROTOCOL};
use augecoin_core::emission::block_reward;
use augecoin_core::mempool::Mempool;
use augecoin_core::operation::Operation;
use augecoin_core::proposal::{
    merkle_root_of_operations, op_hash, reconstruct_block, GetTransactions, ProposalMessage,
    TransactionsResponse,
};
use augecoin_crypto::hdkeys::HdWallet;
use augecoin_crypto::signature::{Ed25519Signature, HybridKeyPair, HybridSignature};
use augecoin_network::transport::ConsensusTransport;
use augecoin_network::{peer_id_from_ed25519, PeerId};
use augecoin_storage::Storage;
use crossbeam_channel::{Receiver, Sender};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

const ED25519_SIG_LEN: usize = 64;
const BLOCK_HASH_LEN: usize = 64;

#[derive(Debug, Clone)]
pub enum ConsensusEvent {
    BlockProposal {
        height: u64,
        block_bytes: Vec<u8>,
        from_id: u64,
    },
    LightProposal {
        height: u64,
        proposal: Box<ProposalMessage>,
        from_id: u64,
    },
    GetTransactions {
        height: u64,
        request: Box<GetTransactions>,
        requester_id: u64,
    },
    TransactionsResponse {
        height: u64,
        response: Box<TransactionsResponse>,
        from_id: u64,
    },
    SignVote {
        height: u64,
        block_hash: [u8; 64],
        signature: HybridSignature,
        voter_id: u64,
    },
    CommitNotification {
        height: u64,
        block_bytes: Vec<u8>,
        quorum_sigs: Vec<HybridSignature>,
        from_id: u64,
    },
    BlockRequest {
        height: u64,
        requester_id: u64,
    },
    BlockResponse {
        height: u64,
        block_bytes: Vec<u8>,
        quorum_sigs: Vec<HybridSignature>,
        from_id: u64,
    },
    RoundChange {
        height: u64,
        new_round: u64,
        from_id: u64,
        signature: HybridSignature,
    },
    StatusRequest {
        requester_id: u64,
    },
    StatusResponse {
        height: u64,
        from_id: u64,
    },
}

pub struct ConsensusEngine {
    pub validator_id: u64,
    pub validator_count: u64,
    pub keypair: HybridKeyPair,
    pub validator_set: ValidatorSet,
    pub storage: Arc<Storage>,
    pub mempool: Arc<Mutex<Mempool>>,
    pub event_tx: Sender<ConsensusEvent>,
    pub event_rx: Receiver<ConsensusEvent>,
    pub block_commit_tx: Sender<OperationBlock>,
    pub block_commit_rx: Receiver<OperationBlock>,
    pub block_time: u64,
    running: Arc<AtomicBool>,
    network: Option<Arc<dyn ConsensusTransport>>,
}

fn read_u64_be(data: &[u8], pos: &mut usize) -> Result<u64, String> {
    if *pos + 8 > data.len() {
        return Err("unexpected EOF".to_string());
    }
    let b: [u8; 8] = data[*pos..*pos + 8].try_into().unwrap();
    *pos += 8;
    Ok(u64::from_be_bytes(b))
}

fn read_u32_be(data: &[u8], pos: &mut usize) -> Result<u32, String> {
    if *pos + 4 > data.len() {
        return Err("unexpected EOF".to_string());
    }
    let b: [u8; 4] = data[*pos..*pos + 4].try_into().unwrap();
    *pos += 4;
    Ok(u32::from_be_bytes(b))
}

fn write_u64_be(buf: &mut Vec<u8>, val: u64) {
    buf.extend_from_slice(&val.to_be_bytes());
}

fn write_u32_be(buf: &mut Vec<u8>, val: u32) {
    buf.extend_from_slice(&val.to_be_bytes());
}

pub fn serialize_consensus_msg(msg: &ConsensusEvent) -> Vec<u8> {
    match msg {
        ConsensusEvent::BlockProposal {
            height,
            block_bytes,
            from_id,
        } => {
            let mut payload = Vec::new();
            write_u64_be(&mut payload, *height);
            write_u64_be(&mut payload, *from_id);
            write_u32_be(&mut payload, block_bytes.len() as u32);
            payload.extend_from_slice(block_bytes);
            let mut buf = vec![0u8];
            write_u32_be(&mut buf, payload.len() as u32);
            buf.extend_from_slice(&payload);
            buf
        }
        ConsensusEvent::LightProposal {
            height,
            proposal,
            from_id,
        } => {
            let mut payload = Vec::new();
            write_u64_be(&mut payload, *height);
            write_u64_be(&mut payload, *from_id);
            let pbytes = proposal.to_bytes();
            write_u32_be(&mut payload, pbytes.len() as u32);
            payload.extend_from_slice(&pbytes);
            let mut buf = vec![8u8];
            write_u32_be(&mut buf, payload.len() as u32);
            buf.extend_from_slice(&payload);
            buf
        }
        ConsensusEvent::GetTransactions {
            height,
            request,
            requester_id,
        } => {
            let mut payload = Vec::new();
            write_u64_be(&mut payload, *height);
            write_u64_be(&mut payload, *requester_id);
            let rbytes = request.to_bytes();
            write_u32_be(&mut payload, rbytes.len() as u32);
            payload.extend_from_slice(&rbytes);
            let mut buf = vec![9u8];
            write_u32_be(&mut buf, payload.len() as u32);
            buf.extend_from_slice(&payload);
            buf
        }
        ConsensusEvent::TransactionsResponse {
            height,
            response,
            from_id,
        } => {
            let mut payload = Vec::new();
            write_u64_be(&mut payload, *height);
            write_u64_be(&mut payload, *from_id);
            let rbytes = response.to_bytes();
            write_u32_be(&mut payload, rbytes.len() as u32);
            payload.extend_from_slice(&rbytes);
            let mut buf = vec![10u8];
            write_u32_be(&mut buf, payload.len() as u32);
            buf.extend_from_slice(&payload);
            buf
        }
        ConsensusEvent::SignVote {
            height,
            block_hash,
            signature,
            voter_id,
        } => {
            let mut payload = Vec::new();
            write_u64_be(&mut payload, *height);
            payload.extend_from_slice(block_hash);
            write_u64_be(&mut payload, *voter_id);
            payload.extend_from_slice(&signature.bytes);
            let mut buf = vec![1u8];
            write_u32_be(&mut buf, payload.len() as u32);
            buf.extend_from_slice(&payload);
            buf
        }
        ConsensusEvent::CommitNotification {
            height,
            block_bytes,
            quorum_sigs,
            from_id,
        } => {
            let mut payload = Vec::new();
            write_u64_be(&mut payload, *height);
            write_u64_be(&mut payload, *from_id);
            write_u32_be(&mut payload, quorum_sigs.len() as u32);
            for sig in quorum_sigs {
                payload.extend_from_slice(&sig.bytes);
            }
            write_u32_be(&mut payload, block_bytes.len() as u32);
            payload.extend_from_slice(block_bytes);
            let mut buf = vec![2u8];
            write_u32_be(&mut buf, payload.len() as u32);
            buf.extend_from_slice(&payload);
            buf
        }
        ConsensusEvent::BlockRequest {
            height,
            requester_id,
        } => {
            let mut payload = Vec::new();
            write_u64_be(&mut payload, *height);
            write_u64_be(&mut payload, *requester_id);
            let mut buf = vec![3u8];
            write_u32_be(&mut buf, payload.len() as u32);
            buf.extend_from_slice(&payload);
            buf
        }
        ConsensusEvent::BlockResponse {
            height,
            block_bytes,
            quorum_sigs,
            from_id,
        } => {
            let mut payload = Vec::new();
            write_u64_be(&mut payload, *height);
            write_u64_be(&mut payload, *from_id);
            write_u32_be(&mut payload, quorum_sigs.len() as u32);
            for sig in quorum_sigs {
                payload.extend_from_slice(&sig.bytes);
            }
            write_u32_be(&mut payload, block_bytes.len() as u32);
            payload.extend_from_slice(block_bytes);
            let mut buf = vec![4u8];
            write_u32_be(&mut buf, payload.len() as u32);
            buf.extend_from_slice(&payload);
            buf
        }
        ConsensusEvent::RoundChange {
            height,
            new_round,
            from_id,
            signature,
        } => {
            let mut payload = Vec::new();
            write_u64_be(&mut payload, *height);
            write_u64_be(&mut payload, *new_round);
            write_u64_be(&mut payload, *from_id);
            payload.extend_from_slice(&signature.bytes);
            let mut buf = vec![5u8];
            write_u32_be(&mut buf, payload.len() as u32);
            buf.extend_from_slice(&payload);
            buf
        }
        ConsensusEvent::StatusRequest { requester_id } => {
            let mut payload = Vec::new();
            write_u64_be(&mut payload, *requester_id);
            let mut buf = vec![6u8];
            write_u32_be(&mut buf, payload.len() as u32);
            buf.extend_from_slice(&payload);
            buf
        }
        ConsensusEvent::StatusResponse { height, from_id } => {
            let mut payload = Vec::new();
            write_u64_be(&mut payload, *height);
            write_u64_be(&mut payload, *from_id);
            let mut buf = vec![7u8];
            write_u32_be(&mut buf, payload.len() as u32);
            buf.extend_from_slice(&payload);
            buf
        }
    }
}

pub fn deserialize_consensus_msg(data: &[u8]) -> Result<ConsensusEvent, String> {
    if data.is_empty() {
        return Err("empty data".to_string());
    }
    let msg_type = data[0];
    let mut pos = 1;
    let _payload_len = read_u32_be(data, &mut pos)?;

    match msg_type {
        0 => {
            let height = read_u64_be(data, &mut pos)?;
            let from_id = read_u64_be(data, &mut pos)?;
            let block_len = read_u32_be(data, &mut pos)? as usize;
            if pos + block_len > data.len() {
                return Err("block data too short".to_string());
            }
            let block_bytes = data[pos..pos + block_len].to_vec();
            Ok(ConsensusEvent::BlockProposal {
                height,
                block_bytes,
                from_id,
            })
        }
        8 => {
            let height = read_u64_be(data, &mut pos)?;
            let from_id = read_u64_be(data, &mut pos)?;
            let p_len = read_u32_be(data, &mut pos)? as usize;
            if pos + p_len > data.len() {
                return Err("proposal data too short".to_string());
            }
            let proposal = Box::new(ProposalMessage::from_bytes(&data[pos..pos + p_len])?);
            Ok(ConsensusEvent::LightProposal {
                height,
                proposal,
                from_id,
            })
        }
        9 => {
            let height = read_u64_be(data, &mut pos)?;
            let requester_id = read_u64_be(data, &mut pos)?;
            let r_len = read_u32_be(data, &mut pos)? as usize;
            if pos + r_len > data.len() {
                return Err("get-transactions data too short".to_string());
            }
            let request = Box::new(GetTransactions::from_bytes(&data[pos..pos + r_len])?);
            Ok(ConsensusEvent::GetTransactions {
                height,
                request,
                requester_id,
            })
        }
        10 => {
            let height = read_u64_be(data, &mut pos)?;
            let from_id = read_u64_be(data, &mut pos)?;
            let r_len = read_u32_be(data, &mut pos)? as usize;
            if pos + r_len > data.len() {
                return Err("transactions-response data too short".to_string());
            }
            let response = Box::new(TransactionsResponse::from_bytes(&data[pos..pos + r_len])?);
            Ok(ConsensusEvent::TransactionsResponse {
                height,
                response,
                from_id,
            })
        }
        1 => {
            let height = read_u64_be(data, &mut pos)?;
            if pos + BLOCK_HASH_LEN > data.len() {
                return Err("too short for block hash".to_string());
            }
            let mut block_hash = [0u8; BLOCK_HASH_LEN];
            block_hash.copy_from_slice(&data[pos..pos + BLOCK_HASH_LEN]);
            pos += BLOCK_HASH_LEN;
            let voter_id = read_u64_be(data, &mut pos)?;
            if pos + ED25519_SIG_LEN > data.len() {
                return Err("too short for signature".to_string());
            }
            let mut sig_bytes = [0u8; ED25519_SIG_LEN];
            sig_bytes.copy_from_slice(&data[pos..pos + ED25519_SIG_LEN]);
            Ok(ConsensusEvent::SignVote {
                height,
                block_hash,
                signature: Ed25519Signature { bytes: sig_bytes },
                voter_id,
            })
        }
        2 => {
            let height = read_u64_be(data, &mut pos)?;
            let from_id = read_u64_be(data, &mut pos)?;
            let qs_count = read_u32_be(data, &mut pos)? as usize;
            let mut quorum_sigs = Vec::with_capacity(qs_count);
            for _ in 0..qs_count {
                if pos + ED25519_SIG_LEN > data.len() {
                    return Err("too short for quorum sig".to_string());
                }
                let mut sig_bytes = [0u8; ED25519_SIG_LEN];
                sig_bytes.copy_from_slice(&data[pos..pos + ED25519_SIG_LEN]);
                pos += ED25519_SIG_LEN;
                quorum_sigs.push(Ed25519Signature { bytes: sig_bytes });
            }
            let block_len = read_u32_be(data, &mut pos)? as usize;
            if pos + block_len > data.len() {
                return Err("block data too short for commit".to_string());
            }
            let block_bytes = data[pos..pos + block_len].to_vec();
            Ok(ConsensusEvent::CommitNotification {
                height,
                block_bytes,
                quorum_sigs,
                from_id,
            })
        }
        3 => {
            let height = read_u64_be(data, &mut pos)?;
            let requester_id = read_u64_be(data, &mut pos)?;
            Ok(ConsensusEvent::BlockRequest {
                height,
                requester_id,
            })
        }
        4 => {
            let height = read_u64_be(data, &mut pos)?;
            let from_id = read_u64_be(data, &mut pos)?;
            let qs_count = read_u32_be(data, &mut pos)? as usize;
            let mut quorum_sigs = Vec::with_capacity(qs_count);
            for _ in 0..qs_count {
                if pos + ED25519_SIG_LEN > data.len() {
                    return Err("too short for quorum sig".to_string());
                }
                let mut sig_bytes = [0u8; ED25519_SIG_LEN];
                sig_bytes.copy_from_slice(&data[pos..pos + ED25519_SIG_LEN]);
                pos += ED25519_SIG_LEN;
                quorum_sigs.push(Ed25519Signature { bytes: sig_bytes });
            }
            let block_len = read_u32_be(data, &mut pos)? as usize;
            if pos + block_len > data.len() {
                return Err("block data too short for response".to_string());
            }
            let block_bytes = data[pos..pos + block_len].to_vec();
            Ok(ConsensusEvent::BlockResponse {
                height,
                block_bytes,
                quorum_sigs,
                from_id,
            })
        }
        5 => {
            let height = read_u64_be(data, &mut pos)?;
            let new_round = read_u64_be(data, &mut pos)?;
            let from_id = read_u64_be(data, &mut pos)?;
            if pos + ED25519_SIG_LEN > data.len() {
                return Err("too short for round-change sig".to_string());
            }
            let mut sig_bytes = [0u8; ED25519_SIG_LEN];
            sig_bytes.copy_from_slice(&data[pos..pos + ED25519_SIG_LEN]);
            Ok(ConsensusEvent::RoundChange {
                height,
                new_round,
                from_id,
                signature: Ed25519Signature { bytes: sig_bytes },
            })
        }
        6 => {
            let requester_id = read_u64_be(data, &mut pos)?;
            Ok(ConsensusEvent::StatusRequest { requester_id })
        }
        7 => {
            let height = read_u64_be(data, &mut pos)?;
            let from_id = read_u64_be(data, &mut pos)?;
            Ok(ConsensusEvent::StatusResponse { height, from_id })
        }
        _ => Err(format!("unknown message type: {msg_type}")),
    }
}

/// Deterministically reconstruct an `OperationBlock` from a light proposal and
/// the resolved operations (in proposal order), enforcing every transport
/// security invariant (PART 13):
///
///   * `proposal.merkle_root` must equal `header.operations_hash`;
///   * the operation count must equal the tx-hash count;
///   * the recomputed Merkle root must equal `proposal.merkle_root`;
///   * each operation's hash must match its slot in `tx_hashes` (order + content);
///   * the reconstructed block hash must equal `header.hash()`.
///
/// Returns the exact byte-for-byte `OperationBlock` the leader proposed, or a
/// descriptive error. Callers attach quorum signatures separately.
pub fn reconstruct_from_proposal(
    proposal: &ProposalMessage,
    ops: Vec<Operation>,
) -> Result<OperationBlock, String> {
    if proposal.merkle_root != proposal.header.operations_hash {
        return Err("merkle root does not match header.operations_hash".into());
    }
    if ops.len() != proposal.tx_hashes.len() {
        return Err(format!(
            "transaction count mismatch: got {}, expected {}",
            ops.len(),
            proposal.tx_hashes.len()
        ));
    }
    let actual_root = merkle_root_of_operations(&ops);
    if actual_root != proposal.merkle_root {
        return Err("recomputed merkle root does not match proposal".into());
    }
    for (i, op) in ops.iter().enumerate() {
        if op_hash(op) != proposal.tx_hashes[i] {
            return Err(format!("transaction hash mismatch at index {i}"));
        }
    }
    let block = reconstruct_block(
        proposal.header.clone(),
        ops,
        proposal.leader_signature.clone(),
    );
    if block.hash() != proposal.header.hash() {
        return Err("reconstructed block hash mismatch".into());
    }
    Ok(block)
}

// DEV-ONLY: Uses deterministic seeds. Never use in production.
#[doc(hidden)]
pub fn create_dev_validator_set_and_keys(
    validator_count: u64,
) -> (
    ValidatorSet,
    Vec<(ValidatorInfo, HybridKeyPair)>,
    HybridKeyPair,
) {
    let base_seed: [u8; 64] = [0xABu8; 64];
    let admin_wallet = HdWallet::from_seed(&base_seed);
    let admin_kp = admin_wallet.derive_keypair(999);
    let admin_pk = admin_kp.verifying_key();

    let mut validators = Vec::new();
    let mut validator_keys = Vec::new();

    for id in 0..validator_count {
        let mut seed = [0u8; 64];
        seed[0] = 0xAB;
        seed[1..9].copy_from_slice(&id.to_be_bytes());
        let wallet = HdWallet::from_seed(&seed);
        let kp = wallet.derive_keypair(0);
        let vk = kp.verifying_key();
        let ed = vk.to_bytes();

        let vi = ValidatorInfo::new_active(id, ed);
        validators.push(vi);
        validator_keys.push((ValidatorInfo::new_active(id, ed), kp));
    }

    let validator_set = ValidatorSet::new(admin_pk, validators);
    (validator_set, validator_keys, admin_kp)
}

// DEV-ONLY: Derives the deterministic keypair for a single validator id using
// the same scheme as [`create_dev_validator_set_and_keys`]. Lets a validator
// whose id is outside the genesis set (e.g. a 5th validator joining a 4-node
// dev network) derive its key without indexing past the genesis array.
#[doc(hidden)]
pub fn create_dev_validator_key(id: u64) -> HybridKeyPair {
    let mut seed = [0u8; 64];
    seed[0] = 0xAB;
    seed[1..9].copy_from_slice(&id.to_be_bytes());
    let wallet = HdWallet::from_seed(&seed);
    wallet.derive_keypair(0)
}

impl ConsensusEngine {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        validator_id: u64,
        validator_count: u64,
        keypair: HybridKeyPair,
        validator_set: ValidatorSet,
        storage: Arc<Storage>,
        mempool: Arc<Mutex<Mempool>>,
        block_time: u64,
    ) -> Self {
        let (event_tx, event_rx) = crossbeam_channel::unbounded();
        let (block_commit_tx, block_commit_rx) = crossbeam_channel::unbounded();
        ConsensusEngine {
            validator_id,
            validator_count,
            keypair,
            validator_set,
            storage,
            mempool,
            event_tx: event_tx.clone(),
            event_rx,
            block_commit_tx,
            block_commit_rx,
            block_time,
            running: Arc::new(AtomicBool::new(true)),
            network: None,
        }
    }

    pub fn attach_network(&mut self, transport: Arc<dyn ConsensusTransport>) {
        let event_tx = self.event_tx.clone();
        let running = self.running.clone();
        let transport_clone = transport.clone();

        std::thread::spawn(move || {
            while running.load(Ordering::SeqCst) {
                match transport_clone.recv() {
                    Ok(inbound) => match deserialize_consensus_msg(&inbound.data) {
                        Ok(msg) => {
                            let _ = event_tx.send(msg);
                        }
                        Err(e) => {
                            eprintln!(
                                "[consensus] deserialize error from {:?}: {e}",
                                inbound.source
                            );
                        }
                    },
                    Err(crossbeam_channel::RecvError) => break,
                }
            }
        });

        self.network = Some(transport);
    }

    pub fn broadcast_to_others(&self, msg: &ConsensusEvent) {
        if let Some(ref net) = self.network {
            let data = serialize_consensus_msg(msg);
            net.broadcast(data);
        }
    }

    pub fn send_to(&self, validator_id: u64, msg: &ConsensusEvent) {
        if let Some(ref net) = self.network {
            let data = serialize_consensus_msg(msg);
            // Resolve the target validator's PeerId from the validator set and
            // unicast over LibP2P. Fall back to broadcast if the id is unknown
            // (e.g. a validator that is not yet in our set).
            let peer: Option<PeerId> = self
                .validator_set
                .validators()
                .iter()
                .find(|v| v.id == validator_id)
                .and_then(|v| peer_id_from_ed25519(&v.ed25519_public_key));
            match peer {
                Some(p) => net.send_to(p, data),
                None => net.broadcast(data),
            }
        }
    }

    pub fn peer_count(&self) -> u32 {
        self.network.as_ref().map(|n| n.peer_count()).unwrap_or(0)
    }

    /// Drain a gossiped operation (received on the operations topic) for
    /// mempool admission by the caller. Non-blocking.
    pub fn try_recv_operation(&self) -> Option<Vec<u8>> {
        self.network.as_ref().and_then(|n| n.try_recv_operation())
    }

    pub fn gossipsub_peer_count(&self) -> u32 {
        self.network
            .as_ref()
            .map(|n| n.gossipsub_peer_count())
            .unwrap_or(0)
    }

    pub fn kademlia_peer_count(&self) -> u32 {
        self.network
            .as_ref()
            .map(|n| n.kademlia_peer_count())
            .unwrap_or(0)
    }

    pub fn round_change_message(height: u64, new_round: u64) -> Vec<u8> {
        let mut msg = Vec::with_capacity(16);
        msg.extend_from_slice(&height.to_be_bytes());
        msg.extend_from_slice(&new_round.to_be_bytes());
        msg
    }

    pub fn sign_round_change(&self, height: u64, new_round: u64) -> HybridSignature {
        self.keypair
            .sign(&Self::round_change_message(height, new_round))
    }

    pub fn verify_round_change(
        &self,
        height: u64,
        new_round: u64,
        from_id: u64,
        sig: &HybridSignature,
    ) -> bool {
        let msg = Self::round_change_message(height, new_round);
        for vi in self.validator_set.validators() {
            if vi.id == from_id {
                let pk = match ed25519_dalek::VerifyingKey::from_bytes(&vi.ed25519_public_key) {
                    Ok(k) => k,
                    Err(_) => return false,
                };
                return sig.verify(&pk, &msg);
            }
        }
        false
    }

    pub fn build_block(
        &self,
        block_number: u64,
        prev_hash: [u8; 64],
        prev_safe_box_hash: [u8; 64],
    ) -> Result<OperationBlock, String> {
        let reward = block_reward(block_number);
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let header = OperationBlockHeader {
            block_number,
            account_key: self.keypair.verifying_key().to_bytes(),
            reward,
            fee: 0,
            protocol_version: CT_BUILD_PROTOCOL,
            protocol_available: CT_MAX_PROTOCOL,
            timestamp,
            initial_safe_box_hash: prev_safe_box_hash,
            operations_hash: [0u8; 64],
            block_payload: vec![],
            proof_of_work: [0u8; 32],
            previous_proof_of_work: prev_hash[..32].try_into().unwrap_or([0u8; 32]),
            leader_id: self.validator_id,
            chain_id: augecoin_core::constants::CT_CHAIN_ID_MAINNET,
        };

        // Byte-aware block assembly: cap the block by serialized size, not by
        // operation count. The gossipsub transmit ceiling (and its safety
        // margin) is read from a single shared place so the block builder can
        // never again produce a block the gossip layer refuses to carry.
        // `max_block_builder_size` further reserves a fixed 16 KiB on top of
        // the 90% budget for envelope/compression variance (PART 7).
        let max_size = augecoin_core::limits::max_block_builder_size();
        // Reserve room for the quorum signatures the committed block will
        // carry. Using the full active-validator count is a conservative upper
        // bound on quorum size.
        let quorum_sig_count = self.validator_set.active_validators().len();

        let (ops, serialized_size) = {
            let mp = self.mempool.lock().unwrap();
            let pending = mp.pending_cloned();
            eprintln!(
                "[debug-build_block] block={block_number} pending={} mempool_len={}",
                pending.len(),
                mp.len()
            );
            augecoin_core::block::select_block_operations(
                &header,
                &pending,
                quorum_sig_count,
                max_size,
            )
        };

        if serialized_size > max_size {
            return Err(format!(
                "block serialized size {serialized_size} exceeds limit {max_size}"
            ));
        }

        let block_hash = header.hash();
        let leader_signature = self.keypair.sign(&block_hash);

        let block = OperationBlock {
            header,
            operations: ops,
            leader_signature,
            quorum_signatures: vec![],
            block_hash,
        };

        let ops_hash = block.compute_operations_merkle_root();
        let mut fixed_block = block;
        fixed_block.header.operations_hash = ops_hash;
        fixed_block.block_hash = fixed_block.header.hash();
        let fixed_leader_sig = self.keypair.sign(&fixed_block.block_hash);
        fixed_block.leader_signature = fixed_leader_sig;

        Ok(fixed_block)
    }

    pub fn sign_block(&self, block: &OperationBlock) -> HybridSignature {
        let hash = block.hash();
        self.keypair.sign(&hash)
    }

    /// Build the *light* proposal for an already-assembled block: header,
    /// Merkle root and ordered transaction hashes only (no operation bodies).
    pub fn build_light_proposal(&self, block: &OperationBlock) -> ProposalMessage {
        ProposalMessage {
            header: block.header.clone(),
            merkle_root: block.header.operations_hash,
            tx_hashes: block.operations.iter().map(op_hash).collect(),
            leader_signature: block.leader_signature.clone(),
        }
    }

    /// Verify a light proposal's leader signature (signed over the header hash)
    /// against the proposer's public key from the validator set.
    pub fn verify_proposal_signature(&self, proposal: &ProposalMessage) -> bool {
        let header_hash = proposal.header.hash();
        for vi in self.validator_set.validators() {
            if vi.id == proposal.header.leader_id {
                let pk = match ed25519_dalek::VerifyingKey::from_bytes(&vi.ed25519_public_key) {
                    Ok(k) => k,
                    Err(_) => return false,
                };
                return proposal.leader_signature.verify(&pk, &header_hash);
            }
        }
        false
    }

    pub fn verify_block_sig(
        &self,
        block: &OperationBlock,
        voter_id: u64,
        signature: &HybridSignature,
    ) -> bool {
        let block_hash = block.hash();
        for vi in self.validator_set.validators() {
            if vi.id == voter_id {
                let pk = match ed25519_dalek::VerifyingKey::from_bytes(&vi.ed25519_public_key) {
                    Ok(k) => k,
                    Err(_) => return false,
                };
                return signature.verify(&pk, &block_hash);
            }
        }
        false
    }

    pub fn quorum_threshold(&self) -> u64 {
        let active = self.validator_set.active_validators().len() as u64;
        quorum_threshold(active)
    }

    pub fn update_validator_set(&mut self, new_set: ValidatorSet) {
        self.validator_set = new_set;
    }

    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
        if let Some(ref net) = self.network {
            net.shutdown();
        }
    }
}
