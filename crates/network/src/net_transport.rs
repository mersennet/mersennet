use anyhow::{Result, bail};
use metrics;
use revm::primitives::keccak256;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::hash::{Hash, Hasher};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::path::Path;
use std::time::{Duration, Instant};
use tracing::info;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GossipPacket {
    pub topic: String,
    pub data: Vec<u8>,
    pub id: String,
    pub ttl: u8,
}

#[derive(Clone, Debug)]
pub struct GossipConfig {
    pub listen_addr: String,
    pub bootstrap_peers: Vec<String>,
    pub fanout: usize,
    pub max_peers: usize,
    pub peer_ttl_secs: u64,
    pub max_seen: usize,
    pub seen_ttl: Duration,
    pub retry_base: Duration,
    pub retry_max: Duration,
    pub peer_discovery_topic: String,
    /// Accept loopback addresses as peers. Off on real networks (a peer
    /// announcing 127.0.0.1 would make every node gossip to itself); turned
    /// on automatically for local multi-node testnets bound to loopback.
    pub allow_loopback_peers: bool,
}

impl Default for GossipConfig {
    fn default() -> Self {
        Self {
            listen_addr: "0.0.0.0:30303".to_string(),
            bootstrap_peers: Vec::new(),
            fanout: 3,
            max_peers: 50,
            peer_ttl_secs: 60,
            max_seen: 4096,
            seen_ttl: Duration::from_secs(120),
            retry_base: Duration::from_millis(100),
            retry_max: Duration::from_secs(5),
            peer_discovery_topic: "peer".to_string(),
            allow_loopback_peers: false,
        }
    }
}

impl GossipConfig {
    /// True when this node or any of its bootstrap peers lives on loopback,
    /// i.e. a local development testnet where 127.0.0.1 peers are real.
    pub fn uses_loopback(&self) -> bool {
        let is_lo = |s: &str| {
            s.parse::<SocketAddr>()
                .map(|a| a.ip().is_loopback())
                .unwrap_or(false)
        };
        is_lo(&self.listen_addr) || self.bootstrap_peers.iter().any(|p| is_lo(p))
    }
}

/// One live gossip peer, for `mersennet_peers` and the explorer.
#[derive(Clone, Debug, Serialize)]
pub struct PeerInfo {
    pub addr: String,
    pub first_seen_secs: u64,
    pub last_seen_secs: u64,
    /// False for addresses only learned from another node's list and never
    /// heard from directly (unconfirmed; pruned unless they speak up).
    pub heard: bool,
}

/// Addresses that can never be a useful gossip destination: unspecified
/// (a node announcing its 0.0.0.0 bind address), multicast, broadcast,
/// port 0 — and loopback unless explicitly allowed. Learned peers that fail
/// this used to be inserted verbatim, inflating peer counts and taking
/// fanout slots from real peers.
fn is_routable_peer(peer: &SocketAddr, allow_loopback: bool) -> bool {
    if peer.port() == 0 {
        return false;
    }
    match peer.ip() {
        std::net::IpAddr::V4(ip) => {
            !(ip.is_unspecified()
                || ip.is_multicast()
                || ip.is_broadcast()
                || ip.is_documentation()
                || (ip.is_loopback() && !allow_loopback))
        }
        std::net::IpAddr::V6(ip) => {
            !(ip.is_unspecified() || ip.is_multicast() || (ip.is_loopback() && !allow_loopback))
        }
    }
}

pub struct PeerManager {
    peers: Vec<SocketAddr>,
    last_seen: HashMap<SocketAddr, Instant>,
    max_peers: usize,
    peer_ttl: Duration,
}

impl PeerManager {
    pub fn new(max_peers: usize, peer_ttl_secs: u64) -> Self {
        Self {
            peers: Vec::new(),
            last_seen: HashMap::new(),
            max_peers,
            peer_ttl: Duration::from_secs(peer_ttl_secs),
        }
    }

    pub fn insert(&mut self, peer: SocketAddr) -> bool {
        if self.peers.contains(&peer) {
            return false;
        }
        if self.peers.len() >= self.max_peers {
            return false;
        }
        self.peers.push(peer);
        self.last_seen.insert(peer, Instant::now());
        true
    }

    pub fn touch(&mut self, peer: SocketAddr) {
        self.last_seen.insert(peer, Instant::now());
    }

    pub fn remove_stale(&mut self) {
        let now = Instant::now();
        self.peers.retain(|peer| {
            self.last_seen
                .get(peer)
                .map(|seen| now.duration_since(*seen) <= self.peer_ttl)
                .unwrap_or(false)
        });
        self.last_seen.retain(|peer, _| self.peers.contains(peer));
    }

    pub fn peers(&self) -> &[SocketAddr] {
        &self.peers
    }

    pub fn peer_count(&self) -> usize {
        self.peers.len()
    }
}

pub struct UdpGossip {
    socket: UdpSocket,
    peers: Vec<SocketAddr>,
    last_seen: HashMap<SocketAddr, Instant>,
    first_seen: HashMap<SocketAddr, Instant>,
    /// Peers we have actually received a packet from (vs. addresses only
    /// learned from another node's discovery list).
    last_heard: HashMap<SocketAddr, Instant>,
    /// IPs of this host's own outbound interfaces (learned by probing the
    /// route to each bootstrap peer). A bootstrap list that contains this
    /// node — the canonical config names the bootnodes for everyone — would
    /// otherwise make it gossip to itself and count itself as a peer.
    self_ips: std::collections::HashSet<std::net::IpAddr>,
    seen: HashMap<String, Instant>,
    seen_order: VecDeque<String>,
    failure_counts: HashMap<SocketAddr, u32>,
    backoff_until: HashMap<SocketAddr, Instant>,
    config: GossipConfig,
}

#[derive(Debug, Serialize, Deserialize)]
struct PeerStoreRecord {
    peers: Vec<String>,
}

impl UdpGossip {
    pub fn bind_with_config(addr: &str, config: GossipConfig) -> Result<Self> {
        let socket = UdpSocket::bind(addr)?;
        socket.set_nonblocking(true)?;
        // Which of our interfaces would talk to each bootstrap peer? On a VPS
        // that is the public IP; behind NAT it is a private one (harmless —
        // NAT'd nodes never receive their own packets anyway).
        let mut self_ips = std::collections::HashSet::new();
        for peer in &config.bootstrap_peers {
            if let Ok(target) = peer.parse::<SocketAddr>()
                && let Ok(probe) = UdpSocket::bind(if target.is_ipv4() { "0.0.0.0:0" } else { "[::]:0" })
                && probe.connect(target).is_ok()
                && let Ok(local) = probe.local_addr()
                && !local.ip().is_loopback()
            {
                self_ips.insert(local.ip());
            }
        }
        Ok(Self {
            socket,
            peers: Vec::new(),
            last_seen: HashMap::new(),
            first_seen: HashMap::new(),
            last_heard: HashMap::new(),
            self_ips,
            seen: HashMap::new(),
            seen_order: VecDeque::new(),
            failure_counts: HashMap::new(),
            backoff_until: HashMap::new(),
            config,
        })
    }

    /// True if `peer` is this very node (one of our interface IPs on our
    /// listen port).
    fn is_self(&self, peer: &SocketAddr) -> bool {
        self.socket
            .local_addr()
            .map(|l| l.port() == peer.port() && self.self_ips.contains(&peer.ip()))
            .unwrap_or(false)
    }

    /// Configured (bootstrap) peer: trusted, so loopback is fine for local
    /// testnets, but an unspecified address is still meaningless, and a
    /// bootstrap entry that is this node itself is skipped.
    pub fn add_peer(&mut self, addr: &str) -> Result<()> {
        let peer: SocketAddr = addr.parse()?;
        if !is_routable_peer(&peer, true) || self.is_self(&peer) {
            return Ok(());
        }
        self.insert_peer_unchecked(peer);
        Ok(())
    }

    pub fn load_peers_from_file(&mut self, path: impl AsRef<Path>) -> Result<usize> {
        let path = path.as_ref();
        if !path.exists() {
            return Ok(0);
        }
        let data = std::fs::read_to_string(path)?;
        let record: PeerStoreRecord = serde_json::from_str(&data)?;
        let mut loaded = 0usize;
        for peer in record.peers {
            if let Ok(addr) = peer.parse::<SocketAddr>() {
                self.insert_peer(addr);
                loaded += 1;
            }
        }
        Ok(loaded)
    }

    pub fn save_peers_to_file(&self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let record = PeerStoreRecord {
            peers: self.peers.iter().map(|peer| peer.to_string()).collect(),
        };
        let data = serde_json::to_string_pretty(&record)?;
        std::fs::write(path, data)?;
        Ok(())
    }

    pub fn new_packet(&self, topic: impl Into<String>, data: Vec<u8>, ttl: u8) -> GossipPacket {
        let topic = topic.into();
        let id = message_id(&topic, &data);
        GossipPacket {
            topic,
            data,
            id,
            ttl,
        }
    }

    pub fn broadcast(&mut self, packet: &GossipPacket) -> Result<()> {
        let payload = serde_json::to_vec(packet)?;
        let peers = self.peers.clone();
        for peer in peers {
            let _ = self.send_to_peer(peer, &payload);
        }
        Ok(())
    }

    pub fn recv_once(&self, timeout: Duration) -> Result<Option<(GossipPacket, SocketAddr)>> {
        self.socket.set_read_timeout(Some(timeout))?;
        let mut buf = vec![0u8; 64 * 1024];
        match self.socket.recv_from(&mut buf) {
            Ok((size, from)) => {
                let packet = serde_json::from_slice::<GossipPacket>(&buf[..size])?;
                Ok(Some((packet, from)))
            }
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
            Err(err) if err.kind() == std::io::ErrorKind::TimedOut => Ok(None),
            Err(err) => Err(err.into()),
        }
    }

    pub fn recv_and_gossip(&mut self, timeout: Duration) -> Result<Option<GossipPacket>> {
        let Some((mut packet, from)) = self.recv_once(timeout)? else {
            return Ok(None);
        };

        self.insert_peer(from);
        let now = Instant::now();
        self.last_seen.insert(from, now);
        self.last_heard.insert(from, now);

        if self.is_seen(&packet.id) {
            return Ok(None);
        }

        self.mark_seen(packet.id.clone());

        if packet.topic == self.config.peer_discovery_topic
            && let Ok(addr) = std::str::from_utf8(&packet.data)
            && let Ok(peer) = addr.parse::<SocketAddr>()
        {
            self.insert_peer(peer);
        }

        if packet.ttl > 0 {
            packet.ttl = packet.ttl.saturating_sub(1);
            let payload = serde_json::to_vec(&packet)?;
            for peer in self.fanout_peers(from) {
                let _ = self.send_to_peer(peer, &payload);
            }
        }

        Ok(Some(packet))
    }

    pub fn prune_peers(&mut self) {
        let ttl = Duration::from_secs(self.config.peer_ttl_secs);
        let now = Instant::now();
        self.peers.retain(|peer| {
            self.last_seen
                .get(peer)
                .map(|seen| now.duration_since(*seen) <= ttl)
                .unwrap_or(false)
        });
        self.last_seen.retain(|peer, _| self.peers.contains(peer));
        self.first_seen.retain(|peer, _| self.peers.contains(peer));
        self.last_heard.retain(|peer, _| self.peers.contains(peer));
        self.failure_counts
            .retain(|peer, _| self.peers.contains(peer));
        self.backoff_until
            .retain(|peer, _| self.peers.contains(peer));
    }

    pub fn announce_self(&mut self, ttl: u8) -> Result<()> {
        let addr = self.socket.local_addr()?;
        // Bound to 0.0.0.0 (the normal production case): the bind address
        // says nothing useful about how to reach us, and announcing it made
        // every receiver insert 0.0.0.0:30303 as a peer. Peers learn our real
        // address from the source of the packets we send them instead.
        if !is_routable_peer(&addr, self.config.allow_loopback_peers) {
            return Ok(());
        }
        let packet = self.new_packet(
            self.config.peer_discovery_topic.clone(),
            addr.to_string().into_bytes(),
            ttl,
        );
        self.broadcast(&packet)
    }

    pub fn discover_peers(&mut self, ttl: u8) -> Result<()> {
        self.announce_self(ttl)?;
        // Relay only peers we have heard from ourselves within the TTL. Relaying
        // addresses that merely arrived in someone else's list let dead peers
        // ping-pong between nodes indefinitely (each relay looked like a fresh
        // sighting to the receiver).
        let ttl_dur = Duration::from_secs(self.config.peer_ttl_secs);
        let now = Instant::now();
        let peer_list: Vec<String> = self
            .peers
            .iter()
            .filter(|p| {
                self.last_heard
                    .get(p)
                    .map(|t| now.duration_since(*t) <= ttl_dur)
                    .unwrap_or(false)
            })
            .map(|p| p.to_string())
            .collect();
        for peer_addr in &peer_list {
            let packet = self.new_packet(
                self.config.peer_discovery_topic.clone(),
                peer_addr.as_bytes().to_vec(),
                ttl,
            );
            self.broadcast(&packet)?;
        }
        Ok(())
    }

    pub fn local_addr(&self) -> Result<SocketAddr> {
        Ok(self.socket.local_addr()?)
    }

    /// Connected peers: addresses we have actually received packets from.
    /// Unconfirmed entries learned from other nodes' lists are not counted.
    pub fn peer_count(&self) -> usize {
        self.peers
            .iter()
            .filter(|p| self.last_heard.contains_key(p))
            .count()
    }

    /// Live peers with how long ago each was first and last heard from.
    pub fn peers_snapshot(&self) -> Vec<PeerInfo> {
        let now = Instant::now();
        let mut out: Vec<PeerInfo> = self
            .peers
            .iter()
            .map(|p| PeerInfo {
                addr: p.to_string(),
                first_seen_secs: self
                    .first_seen
                    .get(p)
                    .map(|t| now.duration_since(*t).as_secs())
                    .unwrap_or(0),
                last_seen_secs: self
                    .last_seen
                    .get(p)
                    .map(|t| now.duration_since(*t).as_secs())
                    .unwrap_or(0),
                heard: self.last_heard.contains_key(p),
            })
            .collect();
        out.sort_by(|a, b| b.first_seen_secs.cmp(&a.first_seen_secs));
        out
    }

    /// Learned peer (packet source or a discovery announcement): only
    /// routable addresses are admitted.
    fn insert_peer(&mut self, peer: SocketAddr) {
        if !is_routable_peer(&peer, self.config.allow_loopback_peers) || self.is_self(&peer) {
            return;
        }
        self.insert_peer_unchecked(peer);
    }

    fn insert_peer_unchecked(&mut self, peer: SocketAddr) {
        if self.peers.contains(&peer) {
            return;
        }
        if self.peers.len() >= self.config.max_peers {
            // Full: make room by dropping the oldest unconfirmed entry so
            // stale candidates can never crowd out a real peer.
            let victim = self
                .peers
                .iter()
                .filter(|p| !self.last_heard.contains_key(p))
                .min_by_key(|p| self.last_seen.get(p).copied())
                .copied();
            match victim {
                Some(v) => {
                    self.peers.retain(|p| *p != v);
                    self.last_seen.remove(&v);
                    self.first_seen.remove(&v);
                    self.failure_counts.remove(&v);
                    self.backoff_until.remove(&v);
                }
                None => return,
            }
        }
        self.peers.push(peer);
        let now = Instant::now();
        self.last_seen.insert(peer, now);
        self.first_seen.entry(peer).or_insert(now);
        self.failure_counts.insert(peer, 0);
    }

    fn fanout_peers(&self, exclude: SocketAddr) -> Vec<SocketAddr> {
        let mut peers = self
            .peers
            .iter()
            .copied()
            .filter(|peer| *peer != exclude)
            .collect::<Vec<_>>();

        let mut weights: HashMap<SocketAddr, u64> = HashMap::new();
        for peer in &peers {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            peer.hash(&mut hasher);
            weights.insert(*peer, hasher.finish());
        }

        peers.sort_by_key(|peer| weights.get(peer).copied().unwrap_or_default());
        peers.truncate(self.config.fanout.min(peers.len()));
        peers
    }

    fn mark_seen(&mut self, id: String) {
        if self.seen.contains_key(&id) {
            return;
        }
        self.seen.insert(id.clone(), Instant::now());
        self.seen_order.push_back(id);
        while self.seen_order.len() > self.config.max_seen {
            if let Some(removed) = self.seen_order.pop_front() {
                self.seen.remove(&removed);
            }
        }
    }

    fn is_seen(&mut self, id: &str) -> bool {
        if let Some(seen_at) = self.seen.get(id).copied() {
            if Instant::now().duration_since(seen_at) <= self.config.seen_ttl {
                return true;
            }
            self.seen.remove(id);
        }
        false
    }

    fn send_to_peer(&mut self, peer: SocketAddr, payload: &[u8]) -> Result<()> {
        let now = Instant::now();
        if let Some(until) = self.backoff_until.get(&peer)
            && *until > now
        {
            return Ok(());
        }

        match self.socket.send_to(payload, peer) {
            Ok(_) => {
                self.failure_counts.insert(peer, 0);
                self.backoff_until.remove(&peer);
                Ok(())
            }
            Err(err) => {
                let failures = self
                    .failure_counts
                    .get(&peer)
                    .copied()
                    .unwrap_or(0)
                    .saturating_add(1);
                self.failure_counts.insert(peer, failures);
                let factor = 2u32.saturating_pow(failures.min(8));
                let backoff = self
                    .config
                    .retry_base
                    .checked_mul(factor)
                    .unwrap_or(self.config.retry_max)
                    .min(self.config.retry_max);
                self.backoff_until.insert(peer, now + backoff);
                Err(err.into())
            }
        }
    }
}

fn message_id(topic: &str, data: &[u8]) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    topic.hash(&mut hasher);
    data.hash(&mut hasher);
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    nonce.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

pub struct TcpSync {
    listener: TcpListener,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct SnapshotHeader {
    pub total_len: u64,
    pub chunk_size: u32,
    pub hash: [u8; 32],
}

impl TcpSync {
    pub fn bind(addr: &str) -> Result<Self> {
        let listener = TcpListener::bind(addr)?;
        listener.set_nonblocking(true)?;
        Ok(Self { listener })
    }

    pub fn accept_once(&self) -> Result<Option<TcpStream>> {
        match self.listener.accept() {
            Ok((stream, _)) => Ok(Some(stream)),
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
            Err(err) => Err(err.into()),
        }
    }

    pub fn connect(addr: &str, timeout: Duration) -> Result<TcpStream> {
        let stream = TcpStream::connect(addr)?;
        stream.set_read_timeout(Some(timeout))?;
        stream.set_write_timeout(Some(timeout))?;
        Ok(stream)
    }

    pub fn send_packet(stream: &mut TcpStream, packet: &GossipPacket) -> Result<()> {
        let payload = serde_json::to_vec(packet)?;
        let len = (payload.len() as u32).to_be_bytes();
        stream.write_all(&len)?;
        stream.write_all(&payload)?;
        Ok(())
    }

    pub fn recv_packet(stream: &mut TcpStream) -> Result<Option<GossipPacket>> {
        Self::recv_packet_limited(stream, 4 * 1024 * 1024)
    }

    /// Like `recv_packet` but with a caller-chosen size cap. Block-sync
    /// responses carry up to 256 full blocks whose JSON encoding easily
    /// exceeds the 4 MiB request-path cap, so the sync client must accept
    /// larger frames from the peer it deliberately connected to.
    pub fn recv_packet_limited(
        stream: &mut TcpStream,
        max_len: usize,
    ) -> Result<Option<GossipPacket>> {
        let mut len_buf = [0u8; 4];
        if stream.read_exact(&mut len_buf).is_err() {
            return Ok(None);
        }
        let len = u32::from_be_bytes(len_buf) as usize;
        if len > max_len {
            bail!("packet too large");
        }
        let mut payload = vec![0u8; len];
        stream.read_exact(&mut payload)?;
        Ok(Some(serde_json::from_slice::<GossipPacket>(&payload)?))
    }

    #[allow(dead_code)]
    pub fn send_snapshot(
        stream: &mut TcpStream,
        snapshot: &[u8],
        chunk_size: usize,
    ) -> Result<SnapshotHeader> {
        let chunk_size = chunk_size.clamp(1024, 512 * 1024);
        let hash = keccak256(snapshot).0;
        let header = SnapshotHeader {
            total_len: snapshot.len() as u64,
            chunk_size: chunk_size as u32,
            hash,
        };

        let mut header_bytes = Vec::with_capacity(49);
        header_bytes.extend_from_slice(b"PSNP");
        header_bytes.push(1u8);
        header_bytes.extend_from_slice(&header.chunk_size.to_be_bytes());
        header_bytes.extend_from_slice(&header.total_len.to_be_bytes());
        header_bytes.extend_from_slice(&header.hash);

        stream.write_all(&header_bytes)?;

        let mut sent = 0usize;
        let mut chunks = 0u64;
        while sent < snapshot.len() {
            let end = (sent + chunk_size).min(snapshot.len());
            stream.write_all(&snapshot[sent..end])?;
            sent = end;
            chunks += 1;
        }
        stream.flush()?;

        metrics::counter!("snapshot_chunks_sent_total", chunks);
        metrics::counter!("snapshot_bytes_sent_total", snapshot.len() as u64);
        info!(bytes = snapshot.len(), chunks, "snapshot sent over tcp");
        Ok(header)
    }

    #[allow(dead_code)]
    pub fn recv_snapshot(stream: &mut TcpStream, max_bytes: usize) -> Result<Vec<u8>> {
        let mut magic = [0u8; 4];
        stream.read_exact(&mut magic)?;
        if &magic != b"PSNP" {
            bail!("invalid snapshot header");
        }
        let mut version = [0u8; 1];
        stream.read_exact(&mut version)?;
        if version[0] != 1 {
            bail!("unsupported snapshot version");
        }
        let mut chunk_buf = [0u8; 4];
        stream.read_exact(&mut chunk_buf)?;
        let chunk_size = u32::from_be_bytes(chunk_buf) as usize;
        let mut len_buf = [0u8; 8];
        stream.read_exact(&mut len_buf)?;
        let total_len = u64::from_be_bytes(len_buf) as usize;
        let mut hash = [0u8; 32];
        stream.read_exact(&mut hash)?;

        if total_len > max_bytes {
            bail!("snapshot too large");
        }

        let mut data = vec![0u8; total_len];
        let mut read = 0usize;
        let mut chunks = 0u64;
        while read < total_len {
            let end = (read + chunk_size).min(total_len);
            stream.read_exact(&mut data[read..end])?;
            read = end;
            chunks += 1;
        }

        let computed = keccak256(&data).0;
        if computed != hash {
            bail!("snapshot hash mismatch");
        }

        metrics::counter!("snapshot_chunks_received_total", chunks);
        metrics::counter!("snapshot_bytes_received_total", data.len() as u64);
        info!(bytes = data.len(), chunks, "snapshot received over tcp");
        Ok(data)
    }
}
