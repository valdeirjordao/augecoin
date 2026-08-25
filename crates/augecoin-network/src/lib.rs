pub mod sync;
pub mod transport;

pub use libp2p::Multiaddr;
pub use libp2p::PeerId;

use libp2p::gossipsub;
use libp2p::identify;
use libp2p::kad;
use libp2p::multiaddr::Protocol;
use libp2p::noise;
use libp2p::ping;
use libp2p::swarm::NetworkBehaviour;
use libp2p::{identity, SwarmBuilder};
use libp2p_connection_limits as connection_limits;
use std::collections::HashSet;
use std::time::{Duration, Instant};
use thiserror::Error;

pub const OPS_TOPIC: &str = "augecoin/ops";
pub const BLOCK_TOPIC: &str = "augecoin/blocks";
pub const CONSENSUS_TOPIC: &str = "augecoin/consensus";
pub const PEERS_TOPIC: &str = "augecoin/peers";

/// Maximum size of a single gossipsub message (RPC payload), in bytes.
///
/// This bound must comfortably carry the largest legitimate consensus message
/// — a `BlockResponse` carrying a full block (Proposal/Prepare/Commit/Status
/// and gossiped operations are all strictly smaller). The value is read from
/// [`augecoin_core::limits::max_transmit_size`] so the gossip layer and the
/// block builder always agree on the same number; override it at runtime with
/// `AUGECOIN_MAX_TRANSMIT_SIZE`.
pub fn max_transmit_size() -> usize {
    augecoin_core::limits::max_transmit_size()
}

/// Build the hardened gossipsub configuration used by every node.
///
/// Every DoS-relevant knob is set explicitly rather than inherited from the
/// library default:
/// - the transmit ceiling (`max_transmit_size`) caps the wire message size;
/// - a deterministic `message_id_fn` (source + sequence + topic) keeps the
///   default dedup semantics but makes them explicit and collision-free across
///   topics;
/// - `duplicate_cache_time`, `heartbeat_interval`, `history_length`,
///   `history_gossip` and the mesh parameters are pinned to values known to
///   form a stable full mesh on the 4–5 validator testnet.
pub fn gossipsub_config() -> Result<gossipsub::Config, NetworkError> {
    gossipsub::ConfigBuilder::default()
        .max_transmit_size(max_transmit_size())
        .message_id_fn(|message: &gossipsub::Message| {
            let mut id = String::with_capacity(128);
            if let Some(peer) = message.source.as_ref() {
                id.push_str(&peer.to_base58());
            }
            id.push(':');
            id.push_str(&message.sequence_number.unwrap_or_default().to_string());
            id.push(':');
            id.push_str(&message.topic.to_string());
            gossipsub::MessageId::from(id)
        })
        .duplicate_cache_time(Duration::from_secs(60))
        .heartbeat_interval(Duration::from_secs(1))
        .history_length(5)
        .history_gossip(3)
        .mesh_n(6)
        .mesh_n_low(5)
        .mesh_n_high(12)
        .build()
        .map_err(|e| NetworkError::Gossipsub(e.to_string()))
}

/// Parse and validate an `AUGECOIN_EXTERNAL_ADDRESS` value.
///
/// Returns the parsed [`Multiaddr`] on success, or a human-readable error.
/// The address must contain an explicit IP (v4 or v6) and a TCP port, and must
/// never be an unspecified address (`0.0.0.0` / `::`), so a node can never
/// advertise a non-dialable listen address.
pub fn parse_external_address(value: &str) -> Result<Multiaddr, String> {
    let addr: Multiaddr = value
        .trim()
        .parse()
        .map_err(|e| format!("invalid multiaddr: {e}"))?;

    let mut has_ip = false;
    let mut has_tcp = false;
    for proto in addr.iter() {
        match proto {
            Protocol::Ip4(ip) if ip.is_unspecified() => {
                return Err("IP must not be 0.0.0.0".to_string())
            }
            Protocol::Ip6(ip) if ip.is_unspecified() => return Err("IP must not be ::".to_string()),
            Protocol::Ip4(_) | Protocol::Ip6(_) => has_ip = true,
            Protocol::Tcp(_) => has_tcp = true,
            _ => {}
        }
    }
    if !has_ip {
        return Err("external address must include /ip4 or /ip6".to_string());
    }
    if !has_tcp {
        return Err("external address must include /tcp/<port>".to_string());
    }
    Ok(addr)
}

#[derive(Debug, Error)]
pub enum NetworkError {
    #[error("transport error: {0}")]
    Transport(String),
    #[error("gossipsub error: {0}")]
    Gossipsub(String),
}

#[derive(NetworkBehaviour)]
pub struct AugecoinBehaviour {
    ping: ping::Behaviour,
    identify: identify::Behaviour,
    gossipsub: gossipsub::Behaviour,
    kademlia: kad::Behaviour<kad::store::MemoryStore>,
    connection_limits: connection_limits::Behaviour,
}

impl AugecoinBehaviour {
    pub fn new(keypair: &identity::Keypair, is_validator: bool) -> Result<Self, NetworkError> {
        let gossipsub_config = gossipsub_config()?;
        let mut gossipsub = gossipsub::Behaviour::new(
            gossipsub::MessageAuthenticity::Signed(keypair.clone()),
            gossipsub_config,
        )
        .map_err(|e| NetworkError::Gossipsub(e.to_string()))?;
        println!(
            "[network] gossipsub max_transmit_size={}",
            max_transmit_size()
        );

        if is_validator {
            for topic in [OPS_TOPIC, BLOCK_TOPIC, CONSENSUS_TOPIC, PEERS_TOPIC] {
                gossipsub
                    .subscribe(&gossipsub::IdentTopic::new(topic))
                    .map_err(|e| NetworkError::Gossipsub(e.to_string()))?;
            }
        }

        let peer_id = keypair.public().to_peer_id();
        // Subscribe to our own unicast topic so peers can address us directly.
        gossipsub
            .subscribe(&gossipsub::IdentTopic::new(unicast_topic(&peer_id)))
            .map_err(|e| NetworkError::Gossipsub(e.to_string()))?;

        let kademlia = kad::Behaviour::new(peer_id, kad::store::MemoryStore::new(peer_id));

        // Hard cap of ONE established connection per peer. Without this the
        // periodic bootnode re-dial opens a brand-new TCP connection on every
        // tick while the old ones stay alive, accumulating hundreds of
        // duplicate sockets on the bootnode (observed as the V0 peers=240
        // anomaly). Duplicates beyond the first are closed by libp2p.
        let limits =
            connection_limits::ConnectionLimits::default().with_max_established_per_peer(Some(1));
        let connection_limits = connection_limits::Behaviour::new(limits);

        Ok(AugecoinBehaviour {
            ping: ping::Behaviour::new(ping::Config::new()),
            identify: identify::Behaviour::new(identify::Config::new(
                "/augecoin/1.0.0".into(),
                keypair.public(),
            )),
            gossipsub,
            kademlia,
            connection_limits,
        })
    }

    /// Number of unique peers currently in the gossipsub mesh of the
    /// consensus topics (the peers that actually exchange consensus traffic).
    pub fn gossipsub_consensus_peers(&self) -> usize {
        let mut set = HashSet::new();
        for topic in [OPS_TOPIC, BLOCK_TOPIC, CONSENSUS_TOPIC] {
            let hash = gossipsub::IdentTopic::new(topic).hash();
            for peer in self.gossipsub.mesh_peers(&hash) {
                set.insert(*peer);
            }
        }
        set.len()
    }

    /// Number of peers known through the Kademlia DHT routing table. These
    /// are *discovered* peers and are not necessarily connected.
    pub fn kademlia_known_peers(&mut self) -> usize {
        self.kademlia
            .kbuckets()
            .map(|bucket| bucket.num_entries())
            .sum()
    }

    /// Return every peer currently present in the Kademlia routing table
    /// together with its known addresses. Used to proactively establish
    /// connections to discovered validators (Kademlia discovery does not
    /// dial on its own).
    pub fn kademlia_peer_addresses(&mut self) -> Vec<(PeerId, Vec<Multiaddr>)> {
        let mut out = Vec::new();
        for bucket in self.kademlia.kbuckets() {
            for entry in bucket.iter() {
                let peer = *entry.node.key.preimage();
                let addrs: Vec<Multiaddr> = entry.node.value.iter().cloned().collect();
                if !addrs.is_empty() {
                    out.push((peer, addrs));
                }
            }
        }
        out
    }

    /// Shared publish path with a size guard. Rejects (with an explicit log)
    /// any message that exceeds the transmit ceiling before handing it to
    /// gossipsub, and surfaces the same rejection if gossipsub reports
    /// `MessageTooLarge` after compression/transform.
    fn publish_to_topic(
        &mut self,
        topic: &str,
        data: Vec<u8>,
    ) -> Result<gossipsub::MessageId, NetworkError> {
        let max = max_transmit_size();
        if data.len() > max {
            eprintln!(
                "[network] oversized message rejected: {} bytes (max {})",
                data.len(),
                max
            );
            return Err(NetworkError::Gossipsub("MessageTooLarge".into()));
        }
        self.gossipsub
            .publish(gossipsub::IdentTopic::new(topic), data)
            .map_err(|e| {
                if matches!(e, gossipsub::PublishError::MessageTooLarge) {
                    eprintln!("[network] oversized message rejected");
                }
                NetworkError::Gossipsub(e.to_string())
            })
    }

    pub fn publish_operation(
        &mut self,
        data: Vec<u8>,
    ) -> Result<gossipsub::MessageId, NetworkError> {
        self.publish_to_topic(OPS_TOPIC, data)
    }

    pub fn publish_block(&mut self, data: Vec<u8>) -> Result<gossipsub::MessageId, NetworkError> {
        self.publish_to_topic(BLOCK_TOPIC, data)
    }

    pub fn publish_consensus(
        &mut self,
        data: Vec<u8>,
    ) -> Result<gossipsub::MessageId, NetworkError> {
        self.publish_to_topic(CONSENSUS_TOPIC, data)
    }

    pub fn publish_peers(&mut self, data: Vec<u8>) -> Result<gossipsub::MessageId, NetworkError> {
        self.publish_to_topic(PEERS_TOPIC, data)
    }

    pub fn publish_unicast(
        &mut self,
        peer: &PeerId,
        data: Vec<u8>,
    ) -> Result<gossipsub::MessageId, NetworkError> {
        self.publish_to_topic(&unicast_topic(peer), data)
    }

    /// Hash of the peer-exchange topic, used to tell peer announcements apart
    /// from consensus traffic on the shared inbound pipe.
    pub fn peers_topic_hash() -> gossipsub::TopicHash {
        gossipsub::IdentTopic::new(PEERS_TOPIC).hash()
    }

    /// Hash of the operations topic, used to route gossiped operations into
    /// the mempool instead of the consensus engine.
    pub fn ops_topic_hash() -> gossipsub::TopicHash {
        gossipsub::IdentTopic::new(OPS_TOPIC).hash()
    }
}

pub struct Bootnodes {
    pub addrs: Vec<Multiaddr>,
}

impl Bootnodes {
    pub fn new(addrs: Vec<Multiaddr>) -> Self {
        Bootnodes { addrs }
    }

    pub fn empty() -> Self {
        Bootnodes { addrs: vec![] }
    }
}

pub fn build_validator_swarm(
    keypair: identity::Keypair,
    _listen_port: u16,
) -> Result<libp2p::Swarm<AugecoinBehaviour>, NetworkError> {
    let behaviour_keypair = keypair.clone();

    let swarm = SwarmBuilder::with_existing_identity(keypair)
        .with_tokio()
        .with_tcp(
            Default::default(),
            noise::Config::new,
            libp2p::yamux::Config::default,
        )
        .map_err(|e| NetworkError::Transport(e.to_string()))?
        .with_behaviour(move |_key| AugecoinBehaviour::new(&behaviour_keypair, true).unwrap())
        .map_err(|e| NetworkError::Transport(e.to_string()))?
        .with_swarm_config(|cfg| cfg.with_idle_connection_timeout(Duration::from_secs(300)))
        .build();

    Ok(swarm)
}

pub fn build_observer_swarm(
    keypair: identity::Keypair,
    _listen_port: u16,
) -> Result<libp2p::Swarm<AugecoinBehaviour>, NetworkError> {
    let behaviour_keypair = keypair.clone();

    let swarm = SwarmBuilder::with_existing_identity(keypair)
        .with_tokio()
        .with_tcp(
            Default::default(),
            noise::Config::new,
            libp2p::yamux::Config::default,
        )
        .map_err(|e| NetworkError::Transport(e.to_string()))?
        .with_behaviour(move |_key| AugecoinBehaviour::new(&behaviour_keypair, false).unwrap())
        .map_err(|e| NetworkError::Transport(e.to_string()))?
        .with_swarm_config(|cfg| cfg.with_idle_connection_timeout(Duration::from_secs(300)))
        .build();

    Ok(swarm)
}

pub fn generate_identity() -> identity::Keypair {
    identity::Keypair::generate_ed25519()
}

/// Deterministically derive the libp2p `PeerId` of a validator from its
/// Ed25519 public key (the same 32-byte key stored in the validator set).
/// This is what lets the consensus layer address a validator by its integer
/// id over LibP2P without ever touching sockets or TCP ports.
pub fn peer_id_from_ed25519(ed25519_public_key: &[u8; 32]) -> Option<PeerId> {
    let ed = identity::ed25519::PublicKey::try_from_bytes(ed25519_public_key).ok()?;
    Some(identity::PublicKey::from(ed).to_peer_id())
}

/// Per-peer unicast topic. Each validator subscribes to its own topic and
/// publishes to a peer's topic to send it a point-to-point message.
pub fn unicast_topic(peer: &PeerId) -> String {
    format!("augecoin/unicast/{peer}")
}

pub fn identity_from_ed25519_seed(seed: &[u8; 32]) -> identity::Keypair {
    identity::Keypair::ed25519_from_bytes(seed.to_vec()).expect("valid ed25519 seed")
}

pub async fn dial_bootnodes(swarm: &mut libp2p::Swarm<AugecoinBehaviour>, bootnodes: &Bootnodes) {
    for addr in &bootnodes.addrs {
        let _ = swarm.dial(addr.clone());
    }
}

/// A peer-exchange announcement: a flat list of `(peer_id, dialable multiaddr)`
/// pairs. Encoded as `[u32 len][peer_id utf8][u32 len][multiaddr utf8]` repeated.
///
/// Peer IDs are base58 strings and multiaddrs are `/ip4/.../tcp/...` strings;
/// neither can contain NUL or newline bytes, so the length-prefixed encoding is
/// unambiguous and safe to parse from untrusted network data.
pub fn encode_peer_announcement(peers: &[(PeerId, Multiaddr)]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(peers.len() * 64);
    for (peer, addr) in peers {
        let pid = peer.to_string();
        let am = addr.to_string();
        buf.extend_from_slice(&(pid.len() as u32).to_be_bytes());
        buf.extend_from_slice(pid.as_bytes());
        buf.extend_from_slice(&(am.len() as u32).to_be_bytes());
        buf.extend_from_slice(am.as_bytes());
    }
    buf
}

/// Reverse of [`encode_peer_announcement`]. Malformed entries are skipped, never
/// fatal; the caller re-validates peer ids and multiaddrs before dialing.
pub fn decode_peer_announcement(data: &[u8]) -> Vec<(PeerId, Multiaddr)> {
    let mut out = Vec::new();
    let mut pos = 0usize;
    while pos + 4 <= data.len() {
        let pid_len =
            u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
        pos += 4;
        if pos + pid_len > data.len() {
            break;
        }
        let pid = match std::str::from_utf8(&data[pos..pos + pid_len]) {
            Ok(s) => s,
            Err(_) => break,
        }
        .parse::<PeerId>();
        pos += pid_len;
        if pos + 4 > data.len() {
            break;
        }
        let am_len =
            u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
        pos += 4;
        if pos + am_len > data.len() {
            break;
        }
        let am = match std::str::from_utf8(&data[pos..pos + am_len]) {
            Ok(s) => s,
            Err(_) => break,
        }
        .parse::<Multiaddr>();
        pos += am_len;
        if let (Ok(peer), Ok(addr)) = (pid, am) {
            out.push((peer, addr));
        }
    }
    out
}

#[derive(Default)]
pub struct RateLimiter {
    peer_violations: std::collections::HashMap<PeerId, (u32, Instant)>,
    banned_peers: HashSet<PeerId>,
    ban_duration: Duration,
    max_violations: u32,
}

impl RateLimiter {
    pub fn new(ban_duration: Duration, max_violations: u32) -> Self {
        RateLimiter {
            peer_violations: std::collections::HashMap::new(),
            banned_peers: HashSet::new(),
            ban_duration,
            max_violations,
        }
    }

    pub fn record_violation(&mut self, peer: PeerId) -> bool {
        if self.banned_peers.contains(&peer) {
            return false;
        }

        let entry = self
            .peer_violations
            .entry(peer)
            .or_insert((0, Instant::now()));
        entry.0 += 1;

        if entry.0 >= self.max_violations {
            self.banned_peers.insert(peer);
            true
        } else {
            false
        }
    }

    pub fn is_banned(&self, peer: &PeerId) -> bool {
        self.banned_peers.contains(peer)
    }

    pub fn check_expired_bans(&mut self) {
        let now = Instant::now();
        self.peer_violations.retain(|peer, (count, time)| {
            if self.banned_peers.contains(peer) && now.duration_since(*time) >= self.ban_duration {
                self.banned_peers.remove(peer);
                false
            } else {
                *count > 0 || self.banned_peers.contains(peer)
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;
    use std::time::Duration;

    async fn connect_peers(
        swarm1: &mut libp2p::Swarm<AugecoinBehaviour>,
        swarm2: &mut libp2p::Swarm<AugecoinBehaviour>,
    ) {
        let peer2_id = *swarm2.local_peer_id();
        let addr: libp2p::Multiaddr = "/ip4/127.0.0.1/tcp/0".parse().unwrap();
        swarm2.listen_on(addr).unwrap();

        let mut swarm2_addr = None;
        let timeout_addr = tokio::time::sleep(Duration::from_secs(2));
        tokio::pin!(timeout_addr);

        loop {
            tokio::select! {
                event = swarm2.select_next_some() => {
                    if let libp2p::swarm::SwarmEvent::NewListenAddr { address, .. } = event {
                        swarm2_addr = Some(address);
                        break;
                    }
                }
                _ = &mut timeout_addr => { break; }
            }
        }

        let addr = swarm2_addr.unwrap();
        let multiaddr = addr.with(libp2p::multiaddr::Protocol::P2p(peer2_id));
        swarm1.dial(multiaddr).unwrap();

        let timeout_conn = tokio::time::sleep(Duration::from_secs(5));
        tokio::pin!(timeout_conn);

        loop {
            tokio::select! {
                event = swarm1.select_next_some() => {
                    if matches!(event, libp2p::swarm::SwarmEvent::ConnectionEstablished { .. }) {
                        break;
                    }
                }
                _ = &mut timeout_conn => { break; }
                _ = swarm2.select_next_some() => {}
            }
        }
    }

    #[tokio::test]
    async fn validator_connects_to_bootnode() {
        let id1 = generate_identity();
        let id2 = generate_identity();

        let mut bootnode = build_validator_swarm(id1, 0).unwrap();
        let addr: libp2p::Multiaddr = "/ip4/127.0.0.1/tcp/0".parse().unwrap();
        bootnode.listen_on(addr).unwrap();

        let mut boot_addr = None;
        let timeout = tokio::time::sleep(Duration::from_secs(2));
        tokio::pin!(timeout);
        loop {
            tokio::select! {
                event = bootnode.select_next_some() => {
                    if let libp2p::swarm::SwarmEvent::NewListenAddr { address, .. } = event {
                        boot_addr = Some(address);
                        break;
                    }
                }
                _ = &mut timeout => { break; }
            }
        }

        let peer_id = bootnode.local_peer_id();
        let boot_multiaddr = boot_addr
            .unwrap()
            .with(libp2p::multiaddr::Protocol::P2p(*peer_id));
        let bootnodes = Bootnodes::new(vec![boot_multiaddr]);

        let mut validator = build_validator_swarm(id2, 0).unwrap();
        dial_bootnodes(&mut validator, &bootnodes).await;

        let mut connected = false;
        let timeout2 = tokio::time::sleep(Duration::from_secs(5));
        tokio::pin!(timeout2);
        loop {
            tokio::select! {
                event = validator.select_next_some() => {
                    if matches!(event, libp2p::swarm::SwarmEvent::ConnectionEstablished { .. }) {
                        connected = true;
                        break;
                    }
                }
                _ = &mut timeout2 => { break; }
                _ = bootnode.select_next_some() => {}
            }
        }

        assert!(connected, "validator should connect to bootnode");
    }

    #[test]
    fn rate_limiter_bans_after_max_violations() {
        let mut rl = RateLimiter::new(Duration::from_secs(60), 3);
        let peer = PeerId::random();

        assert!(!rl.record_violation(peer));
        assert!(!rl.record_violation(peer));
        assert!(rl.record_violation(peer));
        assert!(rl.is_banned(&peer));
    }

    #[test]
    fn well_behaved_peer_never_banned() {
        let mut rl = RateLimiter::new(Duration::from_secs(60), 5);
        let peer = PeerId::random();

        assert!(!rl.record_violation(peer));
        assert!(!rl.record_violation(peer));
        assert!(!rl.is_banned(&peer));
    }

    #[tokio::test]
    async fn gossipsub_consensus_between_validators() {
        let mut swarm1 = build_validator_swarm(generate_identity(), 0).unwrap();
        let mut swarm2 = build_validator_swarm(generate_identity(), 0).unwrap();

        connect_peers(&mut swarm1, &mut swarm2).await;

        let deadline = tokio::time::Instant::now() + Duration::from_secs(8);
        loop {
            tokio::select! {
                _ = swarm1.select_next_some() => {}
                _ = swarm2.select_next_some() => {}
                _ = tokio::time::sleep_until(deadline) => { break; }
            }
        }

        let consensus_data = b"consensus msg test".to_vec();
        swarm1
            .behaviour_mut()
            .publish_consensus(consensus_data.clone())
            .unwrap();

        let deadline2 = tokio::time::Instant::now() + Duration::from_secs(5);
        let mut received = false;
        loop {
            tokio::select! {
                event = swarm2.select_next_some() => {
                    if let libp2p::swarm::SwarmEvent::Behaviour(
                        AugecoinBehaviourEvent::Gossipsub(gossipsub::Event::Message { message, .. })
                    ) = event {
                        if message.data == consensus_data { received = true; break; }
                    }
                }
                _ = tokio::time::sleep_until(deadline2) => { break; }
            }
        }

        assert!(
            received,
            "validator should receive consensus msg via gossipsub"
        );
    }

    #[test]
    fn external_address_parses_valid_ipv4() {
        let addr = parse_external_address("/ip4/203.0.113.10/tcp/9200").unwrap();
        assert_eq!(addr.to_string(), "/ip4/203.0.113.10/tcp/9200");
    }

    #[test]
    fn external_address_parses_valid_ipv6() {
        let addr = parse_external_address("/ip6/2001:db8::1/tcp/9200").unwrap();
        assert_eq!(addr.to_string(), "/ip6/2001:db8::1/tcp/9200");
    }

    #[test]
    fn external_address_rejects_unspecified_v4() {
        let err = parse_external_address("/ip4/0.0.0.0/tcp/9200").unwrap_err();
        assert!(err.contains("0.0.0.0"), "unexpected error: {err}");
    }

    #[test]
    fn external_address_rejects_unspecified_v6() {
        let err = parse_external_address("/ip6/::/tcp/9200").unwrap_err();
        assert!(err.contains("::"), "unexpected error: {err}");
    }

    #[test]
    fn external_address_rejects_missing_tcp() {
        let err = parse_external_address("/ip4/203.0.113.10").unwrap_err();
        assert!(err.contains("/tcp"), "unexpected error: {err}");
    }

    #[test]
    fn external_address_rejects_missing_ip() {
        let err = parse_external_address("/tcp/9200").unwrap_err();
        assert!(err.contains("/ip4 or /ip6"), "unexpected error: {err}");
    }

    #[test]
    fn external_address_rejects_garbage() {
        assert!(parse_external_address("not a multiaddr").is_err());
    }

    #[test]
    fn gossipsub_size_guard_rejects_oversized() {
        let mut behaviour = AugecoinBehaviour::new(&generate_identity(), true).unwrap();
        let max = max_transmit_size();

        // Small message: must NOT be rejected for size (may be rejected for
        // "no peers subscribed", which is expected without any connections).
        let small = vec![0xABu8; 128];
        let res = behaviour.publish_consensus(small);
        assert!(
            !matches!(
                &res,
                Err(NetworkError::Gossipsub(e)) if e == "MessageTooLarge"
            ),
            "small message must not be rejected as oversized: {res:?}"
        );

        // Medium message.
        let medium = vec![0xCDu8; 64 * 1024];
        let res = behaviour.publish_consensus(medium);
        assert!(
            !matches!(
                &res,
                Err(NetworkError::Gossipsub(e)) if e == "MessageTooLarge"
            ),
            "medium message must not be rejected as oversized: {res:?}"
        );

        // At the exact limit.
        let at_limit = vec![0xEFu8; max];
        let res = behaviour.publish_consensus(at_limit);
        assert!(
            !matches!(
                &res,
                Err(NetworkError::Gossipsub(e)) if e == "MessageTooLarge"
            ),
            "at-limit message must not be rejected as oversized: {res:?}"
        );

        // One byte over the limit: must be rejected as oversized.
        let oversized = vec![0x11u8; max + 1];
        let res = behaviour.publish_consensus(oversized);
        match res {
            Err(NetworkError::Gossipsub(e)) => {
                assert_eq!(e, "MessageTooLarge", "expected MessageTooLarge, got {e}");
            }
            other => panic!("oversized message must be rejected, got {other:?}"),
        }
    }
}
