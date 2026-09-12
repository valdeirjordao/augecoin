use crate::{
    decode_peer_announcement, encode_peer_announcement, AugecoinBehaviour, AugecoinBehaviourEvent,
    Bootnodes,
};
use crossbeam_channel::{Receiver, Sender};
use futures::StreamExt;
use libp2p::gossipsub;
use libp2p::multiaddr::Protocol;
use libp2p::swarm::SwarmEvent;
use libp2p::{Multiaddr, PeerId, Swarm};
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

const MAX_CONSENSUS_MSG_SIZE: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone)]
pub enum OutboundConsensus {
    Broadcast(Vec<u8>),
    Unicast { target: PeerId, data: Vec<u8> },
}

#[derive(Debug, Clone)]
pub struct InboundConsensus {
    pub data: Vec<u8>,
    pub source: Option<PeerId>,
}

pub struct ConsensusNetworkHandle {
    pub outbound_tx: Sender<OutboundConsensus>,
    pub inbound_rx: Receiver<InboundConsensus>,
    pub op_inbound_rx: Receiver<Vec<u8>>,
    pub peers_connected: Arc<AtomicU32>,
    pub peers_gossipsub: Arc<AtomicU32>,
    pub peers_kademlia: Arc<AtomicU32>,
    pub shutdown_tx: Sender<()>,
}

impl ConsensusNetworkHandle {
    pub fn broadcast(&self, data: Vec<u8>) {
        let _ = self.outbound_tx.send(OutboundConsensus::Broadcast(data));
    }

    pub fn send_to(&self, peer: PeerId, data: Vec<u8>) {
        let _ = self
            .outbound_tx
            .send(OutboundConsensus::Unicast { target: peer, data });
    }

    pub fn recv_inbound(&self) -> Result<InboundConsensus, crossbeam_channel::RecvError> {
        self.inbound_rx.recv()
    }

    pub fn try_recv_inbound(&self) -> Option<InboundConsensus> {
        self.inbound_rx.try_recv().ok()
    }

    /// Unique connected peers (deduplicated by peer id).
    pub fn peer_count(&self) -> u32 {
        self.peers_connected.load(Ordering::SeqCst)
    }

    /// Peers in the gossipsub consensus mesh.
    pub fn gossipsub_peer_count(&self) -> u32 {
        self.peers_gossipsub.load(Ordering::SeqCst)
    }

    /// Peers known via Kademlia DHT discovery (not necessarily connected).
    pub fn kademlia_peer_count(&self) -> u32 {
        self.peers_kademlia.load(Ordering::SeqCst)
    }

    /// Drain gossiped operations (received on the operations topic) into the
    /// caller for mempool admission. Non-blocking.
    pub fn try_recv_operation(&self) -> Option<Vec<u8>> {
        self.op_inbound_rx.try_recv().ok()
    }

    pub fn shutdown(&self) {
        let _ = self.shutdown_tx.send(());
    }
}

/// Single abstraction over the P2P transport consumed by the consensus engine.
///
/// The engine never touches sockets, TCP ports or `TcpStream`; it only speaks
/// in terms of broadcast / unicast / receive. The only implementation is the
/// LibP2P transport (`ConsensusNetworkHandle`), which routes broadcast through
/// gossipsub and unicast through a per-peer gossipsub topic.
pub trait ConsensusTransport: Send + Sync {
    fn broadcast(&self, data: Vec<u8>);
    fn send_to(&self, peer: PeerId, data: Vec<u8>);
    fn recv(&self) -> Result<InboundConsensus, crossbeam_channel::RecvError>;
    fn try_recv(&self) -> Option<InboundConsensus>;
    fn peer_count(&self) -> u32;
    fn gossipsub_peer_count(&self) -> u32;
    fn kademlia_peer_count(&self) -> u32;
    fn try_recv_operation(&self) -> Option<Vec<u8>>;
    fn shutdown(&self);
}

impl ConsensusTransport for ConsensusNetworkHandle {
    fn broadcast(&self, data: Vec<u8>) {
        ConsensusNetworkHandle::broadcast(self, data);
    }

    fn send_to(&self, peer: PeerId, data: Vec<u8>) {
        ConsensusNetworkHandle::send_to(self, peer, data);
    }

    fn recv(&self) -> Result<InboundConsensus, crossbeam_channel::RecvError> {
        self.recv_inbound()
    }

    fn try_recv(&self) -> Option<InboundConsensus> {
        self.try_recv_inbound()
    }

    fn peer_count(&self) -> u32 {
        ConsensusNetworkHandle::peer_count(self)
    }

    fn gossipsub_peer_count(&self) -> u32 {
        ConsensusNetworkHandle::gossipsub_peer_count(self)
    }

    fn kademlia_peer_count(&self) -> u32 {
        ConsensusNetworkHandle::kademlia_peer_count(self)
    }

    fn try_recv_operation(&self) -> Option<Vec<u8>> {
        ConsensusNetworkHandle::try_recv_operation(self)
    }

    fn shutdown(&self) {
        ConsensusNetworkHandle::shutdown(self);
    }
}

/// Strip a trailing `/p2p/...` segment so addresses can be compared
/// regardless of whether the peer id is embedded.
fn strip_p2p(addr: &Multiaddr) -> Multiaddr {
    addr.iter()
        .filter(|proto| !matches!(proto, libp2p::multiaddr::Protocol::P2p(_)))
        .collect()
}

/// Extract the first IP (v4 or v6) seen in a multiaddr.
fn extract_ip(addr: &Multiaddr) -> Option<IpAddr> {
    for proto in addr.iter() {
        match proto {
            Protocol::Ip4(ip) => return Some(IpAddr::V4(ip)),
            Protocol::Ip6(ip) => return Some(IpAddr::V6(ip)),
            _ => {}
        }
    }
    None
}

/// Build an `/ip{4,6}/<ip>/tcp/<port>` multiaddr.
fn ip_tcp_addr(ip: IpAddr, port: u16) -> Option<Multiaddr> {
    match ip {
        IpAddr::V4(v4) => format!("/ip4/{v4}/tcp/{port}").parse().ok(),
        IpAddr::V6(v6) => format!("/ip6/{v6}/tcp/{port}").parse().ok(),
    }
}

/// Replace an unspecified IP (`0.0.0.0` or `::`) with the observed peer IP.
///
/// Nodes bind their listener to `0.0.0.0`, so `identify` reports the raw
/// unspecified address. Dialing `0.0.0.0` always fails, which is the root
/// cause of the star topology (everyone only reachable via the bootnode). We
/// rewrite the IP to the one we actually observed on the connection, making
/// the address dialable by every other validator.
fn translate_addr(addr: &Multiaddr, observed_ip: Option<IpAddr>) -> Multiaddr {
    let Some(observed) = observed_ip else {
        return addr.clone();
    };
    let mut out = Multiaddr::empty();
    for proto in addr.iter() {
        match proto {
            Protocol::Ip4(ip) if ip.is_unspecified() => match observed {
                IpAddr::V4(v4) => out.push(Protocol::Ip4(v4)),
                _ => out.push(Protocol::Ip4(ip)),
            },
            Protocol::Ip6(ip) if ip.is_unspecified() => match observed {
                IpAddr::V6(v6) => out.push(Protocol::Ip6(v6)),
                _ => out.push(Protocol::Ip6(ip)),
            },
            other => out.push(other),
        }
    }
    out
}

pub fn start_consensus_network(
    mut swarm: Swarm<AugecoinBehaviour>,
    listen_port: u16,
    bootnodes: Bootnodes,
    op_broadcast_rx: Receiver<Vec<u8>>,
    external_address: Option<Multiaddr>,
) -> ConsensusNetworkHandle {
    let (outbound_tx, outbound_rx) = crossbeam_channel::unbounded::<OutboundConsensus>();
    let (inbound_tx, inbound_rx) = crossbeam_channel::unbounded::<InboundConsensus>();
    let (op_inbound_tx, op_inbound_rx) = crossbeam_channel::unbounded::<Vec<u8>>();
    let (shutdown_tx, shutdown_rx) = crossbeam_channel::unbounded::<()>();
    let peers_connected = Arc::new(AtomicU32::new(0));
    let peers_connected_clone = peers_connected.clone();
    let peers_gossipsub = Arc::new(AtomicU32::new(0));
    let peers_gossipsub_clone = peers_gossipsub.clone();
    let peers_kademlia = Arc::new(AtomicU32::new(0));
    let peers_kademlia_clone = peers_kademlia.clone();

    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("failed to create tokio runtime for consensus network");

        rt.block_on(async move {
            let listen_addr: Multiaddr = format!("/ip4/0.0.0.0/tcp/{listen_port}")
                .parse()
                .expect("valid listen addr");

            if let Err(e) = swarm.listen_on(listen_addr) {
                eprintln!("[network] failed to listen on port {listen_port}: {e}");
                return;
            }

            println!("[network] LibP2P consensus network listening on port {listen_port}");
            println!("[network] local peer_id: {}", swarm.local_peer_id());

            let bootnode_addrs: Vec<Multiaddr> = bootnodes.addrs.clone();
            let bootnode_stripped: Vec<Multiaddr> =
                bootnode_addrs.iter().map(strip_p2p).collect();
            // Learned mapping bootnode address -> peer id, used to skip
            // re-dialing an already-connected bootnode.
            let mut bootnode_peers: HashMap<Multiaddr, PeerId> = HashMap::new();
            // Connection count per peer so the gauge reflects *unique* peers.
            let mut conn_per_peer: HashMap<PeerId, u32> = HashMap::new();
            // Observed IP per peer (from the established connection). Used to
            // translate unspecified listen addresses into dialable ones.
            let mut peer_observed_ip: HashMap<PeerId, IpAddr> = HashMap::new();
            // Our own externally-reachable address. Priority order:
            //   1. explicit `AUGECOIN_EXTERNAL_ADDRESS` (config)
            //   2. `identify.observed_addr` learned from a remote peer
            //   3. the local listen address
            // Broadcast in peer-exchange announcements. Never 0.0.0.0.
            let mut self_external_addr: Option<Multiaddr> = external_address.clone();
            // Register an explicitly configured external address with libp2p so
            // `identify` advertises it to peers immediately (before the first
            // identify exchange would otherwise learn an observed address).
            if let Some(addr) = &external_address {
                swarm.add_external_address(addr.clone());
                println!("[network] external address configured: {addr}");
            }
            // Peers whose last connection dropped; removed from the Kademlia
            // table after a grace period so dead peers do not linger forever.
            let mut dead_peer_since: HashMap<PeerId, Instant> = HashMap::new();

            for addr in &bootnode_addrs {
                match swarm.dial(addr.clone()) {
                    Ok(()) => println!("[network] dialing bootnode {addr}"),
                    Err(e) => eprintln!("[network] failed to dial bootnode {addr}: {e}"),
                }
            }

            let mut reconnect_tick = tokio::time::interval(Duration::from_secs(10));
            let mut kad_tick = tokio::time::interval(Duration::from_secs(30));
            let mut peers_tick = tokio::time::interval(Duration::from_secs(8));
            let mut dead_peer_tick = tokio::time::interval(Duration::from_secs(15));
            let mut stats_tick = tokio::time::interval(Duration::from_secs(5));
            let mut shutdown_check = tokio::time::interval(Duration::from_millis(100));

            loop {
                tokio::select! {
                    event = swarm.select_next_some() => {
                        match event {
                            SwarmEvent::Behaviour(AugecoinBehaviourEvent::Gossipsub(
                                gossipsub::Event::Message {
                                    message,
                                    ..
                                },
                            )) => {
                                if message.topic == AugecoinBehaviour::peers_topic_hash() {
                                    // Peer-exchange announcement: dial every
                                    // peer we do not already have a connection
                                    // to. This makes the mesh converge
                                    // deterministically and lets a newly joined
                                    // validator reach all existing ones.
                                    for (peer, addr) in decode_peer_announcement(&message.data) {
                                        if peer == *swarm.local_peer_id() {
                                            continue;
                                        }
                                        if conn_per_peer.contains_key(&peer) {
                                            continue;
                                        }
                                        match swarm.dial(addr.clone()) {
                                            Ok(()) => println!(
                                                "[network] dialing exchanged peer {peer} at {addr}"
                                            ),
                                            Err(e) => eprintln!(
                                                "[network] failed to dial exchanged peer {peer} at {addr}: {e}"
                                            ),
                                        }
                                    }
                                } else if message.topic == AugecoinBehaviour::ops_topic_hash() {
                                    // Gossiped operation: forward to the caller
                                    // for mempool admission (dedup happens in the
                                    // mempool via sender/n_operation).
                                    if message.data.len() <= MAX_CONSENSUS_MSG_SIZE {
                                        let _ = op_inbound_tx.send(message.data);
                                    }
                                } else if message.topic
                                    == gossipsub::IdentTopic::new(crate::unicast_topic(
                                        swarm.local_peer_id(),
                                    ))
                                    .hash()
                                {
                                    // Unicast message addressed to us.
                                    if message.data.len() <= MAX_CONSENSUS_MSG_SIZE {
                                        let _ = inbound_tx.send(InboundConsensus {
                                            data: message.data,
                                            source: message.source,
                                        });
                                    }
                                } else if message.data.len() <= MAX_CONSENSUS_MSG_SIZE {
                                    let _ = inbound_tx.send(InboundConsensus {
                                        data: message.data,
                                        source: message.source,
                                    });
                                } else {
                                    eprintln!(
                                        "[network] dropping oversized consensus message: {} bytes",
                                        message.data.len()
                                    );
                                }
                            }
                            SwarmEvent::Behaviour(AugecoinBehaviourEvent::Identify(
                                libp2p::identify::Event::Received { peer_id, info, .. }
                            )) => {
                                println!(
                                    "[network] peer identified: {} protocol={} agent={}",
                                    peer_id, info.protocol_version, info.agent_version
                                );
                                // Learn our own observed address for peer exchange.
                                if self_external_addr.is_none() {
                                    if let Some(own_ip) = extract_ip(&info.observed_addr) {
                                        if let Some(own) = ip_tcp_addr(own_ip, listen_port) {
                                            println!("[network] self external address: {own}");
                                            swarm.add_external_address(own.clone());
                                            self_external_addr = Some(own);
                                        }
                                    }
                                }
                                // Translate unspecified listen addresses (0.0.0.0)
                                // to the observed peer IP before storing them in
                                // the Kademlia table, otherwise peers would try
                                // to dial a non-routable address.
                                let observed_ip = peer_observed_ip.get(&peer_id).copied();
                                for addr in info.listen_addrs {
                                    let translated = translate_addr(&addr, observed_ip);
                                    swarm.behaviour_mut().kademlia.add_address(&peer_id, translated);
                                }
                            }
                            SwarmEvent::Behaviour(AugecoinBehaviourEvent::Kademlia(
                                libp2p::kad::Event::UnroutablePeer { peer },
                            )) => {
                                eprintln!("[network] kademlia: peer {peer} unroutable");
                            }
                            SwarmEvent::Behaviour(AugecoinBehaviourEvent::Kademlia(
                                libp2p::kad::Event::RoutingUpdated {
                                    peer,
                                    addresses,
                                    ..
                                },
                            )) => {
                                // A new peer was learned via the DHT. Kademlia
                                // does not dial discovered peers on its own in
                                // this setup, so establish the connection
                                // explicitly (dedup: one connection per peer).
                                if !conn_per_peer.contains_key(&peer) {
                                    let addr = addresses.first().clone();
                                    match swarm.dial(addr.clone()) {
                                        Ok(()) => println!(
                                            "[network] dialing discovered peer {peer} at {addr}"
                                        ),
                                        Err(e) => eprintln!(
                                            "[network] failed to dial discovered peer {peer}: {e}"
                                        ),
                                    }
                                }
                            }
                            SwarmEvent::Behaviour(AugecoinBehaviourEvent::Kademlia(
                                libp2p::kad::Event::OutboundQueryProgressed {
                                    result: libp2p::kad::QueryResult::Bootstrap(Ok(ok)),
                                    ..
                                },
                            )) => {
                                println!(
                                    "[network] kademlia bootstrap ok: peer={} remaining={}",
                                    ok.peer, ok.num_remaining
                                );
                            }
                            SwarmEvent::Behaviour(AugecoinBehaviourEvent::Kademlia(
                                libp2p::kad::Event::OutboundQueryProgressed {
                                    result: libp2p::kad::QueryResult::Bootstrap(Err(e)),
                                    ..
                                },
                            )) => {
                                eprintln!("[network] kademlia bootstrap failed: {e:?}");
                            }
                            SwarmEvent::ConnectionEstablished { peer_id, endpoint, .. } => {
                                let remote = strip_p2p(&endpoint.get_remote_address().to_owned());
                                if let Some(ip) = extract_ip(&remote) {
                                    peer_observed_ip.insert(peer_id, ip);
                                }
                                dead_peer_since.remove(&peer_id);
                                for (i, stripped) in bootnode_stripped.iter().enumerate() {
                                    if &remote == stripped {
                                        bootnode_peers.insert(bootnode_addrs[i].clone(), peer_id);
                                    }
                                }
                                let entry = conn_per_peer.entry(peer_id).or_insert(0);
                                *entry += 1;
                                if *entry == 1 {
                                    let count = peers_connected_clone.fetch_add(1, Ordering::SeqCst) + 1;
                                    println!("[network] peer connected: {} (unique peers: {})", peer_id, count);
                                }
                            }
                            SwarmEvent::ConnectionClosed { peer_id, cause, .. } => {
                                if let Some(entry) = conn_per_peer.get_mut(&peer_id) {
                                    *entry = entry.saturating_sub(1);
                                    if *entry == 0 {
                                        conn_per_peer.remove(&peer_id);
                                        dead_peer_since.entry(peer_id).or_insert(Instant::now());
                                        let count = peers_connected_clone.fetch_sub(1, Ordering::SeqCst) - 1;
                                        println!(
                                            "[network] peer disconnected: {} (unique peers: {}, cause: {:?})",
                                            peer_id, count, cause
                                        );
                                    }
                                }
                            }
                            SwarmEvent::NewListenAddr { address, .. } => {
                                println!("[network] listening on {address}");
                            }
                            SwarmEvent::OutgoingConnectionError { peer_id: Some(peer_id), error, .. } => {
                                eprintln!("[network] dial to {peer_id} failed: {error}");
                            }
                            _ => {}
                        }
                    }
                    _ = reconnect_tick.tick() => {
                        for addr in &bootnode_addrs {
                            // Skip re-dialing a bootnode we are already
                            // connected to; dialing again would open a
                            // duplicate connection instead of reusing the
                            // existing one.
                            if let Some(pid) = bootnode_peers.get(addr) {
                                if conn_per_peer.contains_key(pid) {
                                    continue;
                                }
                            }
                            let _ = swarm.dial(addr.clone());
                        }
                    }
                    _ = kad_tick.tick() => {
                        match swarm.behaviour_mut().kademlia.bootstrap() {
                            Ok(_) => {}
                            Err(e) => eprintln!("[network] kademlia bootstrap skipped: {e:?}"),
                        }
                        // Kademlia discovers peers but does not dial them in
                        // this setup; connect to every discovered peer we are
                        // not already connected to (dedup keeps exactly one
                        // connection per peer via connection-limits).
                        let discovered = swarm.behaviour_mut().kademlia_peer_addresses();
                        for (peer, addrs) in discovered {
                            if peer == *swarm.local_peer_id() {
                                continue;
                            }
                            if conn_per_peer.contains_key(&peer) {
                                continue;
                            }
                            for addr in addrs {
                                match swarm.dial(addr.clone()) {
                                    Ok(()) => {
                                        println!("[network] dialing discovered peer {peer} at {addr}");
                                        break;
                                    }
                                    Err(e) => eprintln!(
                                        "[network] failed to dial discovered peer {peer} at {addr}: {e}"
                                    ),
                                }
                            }
                        }
                    }
                    _ = peers_tick.tick() => {
                        // Periodically broadcast our own address and the peers
                        // we know about so the full mesh converges even before
                        // Kademlia has fully propagated every address.
                        let mut announce: Vec<(PeerId, Multiaddr)> = Vec::new();
                        if let Some(ref own) = self_external_addr {
                            announce.push((*swarm.local_peer_id(), own.clone()));
                        }
                        for (peer, addrs) in swarm.behaviour_mut().kademlia_peer_addresses() {
                            if peer == *swarm.local_peer_id() {
                                continue;
                            }
                            if announce.iter().any(|(p, _)| *p == peer) {
                                continue;
                            }
                            if let Some(addr) = addrs.into_iter().next() {
                                announce.push((peer, addr));
                            }
                        }
                        if !announce.is_empty() {
                            let data = encode_peer_announcement(&announce);
                            let _ = swarm.behaviour_mut().publish_peers(data);
                        }
                    }
                    _ = dead_peer_tick.tick() => {
                        let now = Instant::now();
                        dead_peer_since.retain(|peer, since| {
                            if now.duration_since(*since) >= Duration::from_secs(60) {
                                swarm.behaviour_mut().kademlia.remove_peer(peer);
                                println!("[network] removed dead peer {peer} from kademlia table");
                                false
                            } else {
                                true
                            }
                        });
                    }
                    _ = stats_tick.tick() => {
                        let gs = swarm.behaviour().gossipsub_consensus_peers() as u32;
                        let kad = swarm.behaviour_mut().kademlia_known_peers() as u32;
                        peers_gossipsub_clone.store(gs, Ordering::SeqCst);
                        peers_kademlia_clone.store(kad, Ordering::SeqCst);
                    }
                    _ = shutdown_check.tick() => {
                        if shutdown_rx.try_recv().is_ok() {
                            println!("[network] shutting down consensus network");
                            return;
                        }
                    }
                }

                while let Ok(outbound) = outbound_rx.try_recv() {
                    match outbound {
                        OutboundConsensus::Broadcast(data) => {
                            if let Err(e) = swarm.behaviour_mut().publish_consensus(data) {
                                eprintln!("[network] failed to publish consensus message: {e}");
                            }
                        }
                        OutboundConsensus::Unicast { target, data } => {
                            if let Err(e) = swarm.behaviour_mut().publish_unicast(&target, data) {
                                eprintln!("[network] failed to publish unicast message to {target}: {e}");
                            }
                        }
                    }
                }

                while let Ok(op_bytes) = op_broadcast_rx.try_recv() {
                    if let Err(e) = swarm.behaviour_mut().publish_operation(op_bytes) {
                        eprintln!("[network] failed to publish operation: {e}");
                    }
                }
            }
        });
    });

    ConsensusNetworkHandle {
        outbound_tx,
        inbound_rx,
        op_inbound_rx,
        peers_connected,
        peers_gossipsub,
        peers_kademlia,
        shutdown_tx,
    }
}
