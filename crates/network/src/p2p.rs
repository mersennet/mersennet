use crate::net_transport::{GossipConfig, TcpSync, UdpGossip};
use anyhow::Result;
use mersennet::consensus::{Finalization, Reward};
use mersennet::engine::{Block, Engine, Receipt, Transaction};
use mersennet::network::Message as VoteMessage;
use revm::primitives::{Address, B256, Bytes, U256};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tracing::{info, warn};

#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug)]
pub enum P2pMessage {
    Tx(Transaction),
    Block(Block),
    Vote(VoteMessage),
}

#[derive(Debug)]
pub struct Node {
    pub id: String,
    pub engine: Engine,
    inbox: Vec<P2pMessage>,
}

impl Node {
    pub fn new(id: impl Into<String>, engine: Engine) -> Self {
        Self {
            id: id.into(),
            engine,
            inbox: Vec::new(),
        }
    }

    pub fn push_message(&mut self, message: P2pMessage) {
        self.inbox.push(message);
    }

    pub fn process_inbox(&mut self) -> Result<()> {
        for message in self.inbox.drain(..) {
            match message {
                P2pMessage::Tx(tx) => {
                    let _ = self.engine.submit_tx(tx);
                }
                P2pMessage::Block(block) => {
                    self.engine.import_block(block);
                }
                P2pMessage::Vote(vote) => {
                    self.engine.consensus.network.broadcast(vote);
                }
            }
        }
        Ok(())
    }
}

#[derive(Default, Debug)]
pub struct P2pNetwork {
    nodes: HashMap<String, Node>,
}

impl P2pNetwork {
    pub fn add_node(&mut self, node: Node) {
        self.nodes.insert(node.id.clone(), node);
    }

    pub fn node_mut(&mut self, id: &str) -> Option<&mut Node> {
        self.nodes.get_mut(id)
    }

    pub fn broadcast(&mut self, from: &str, message: P2pMessage) {
        for (id, node) in self.nodes.iter_mut() {
            if id != from {
                node.push_message(message.clone());
            }
        }
    }

    pub fn sync_blocks(&mut self, from: &str, peer: &str) {
        let Some(peer_node) = self.nodes.get(peer) else {
            return;
        };
        let peer_height = peer_node.engine.latest_height();
        let Some(from_node) = self.nodes.get(from) else {
            return;
        };
        let from_height = from_node.engine.latest_height();

        if peer_height <= from_height {
            return;
        }

        let blocks = (from_height + 1..=peer_height)
            .filter_map(|height| peer_node.engine.block_by_number(height).cloned())
            .collect::<Vec<_>>();

        if let Some(node) = self.nodes.get_mut(from) {
            for block in blocks {
                node.push_message(P2pMessage::Block(block));
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Wire types for network serialization
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize)]
pub struct WireTx {
    pub from: String,
    pub to: Option<String>,
    pub value: String,
    pub data: String,
    pub gas_limit: u64,
    pub gas_price: String,
    pub nonce: u64,
    pub chain_id: Option<u64>,
    // Signature (r, s, v). Carried so a receiving validator can
    // re-verify a relayed transaction — without it, gossiped txs
    // arrive unsigned and are silently rejected, so externally
    // submitted txs (faucet, trades) never get mined.
    #[serde(default)]
    pub sig_r: Option<String>,
    #[serde(default)]
    pub sig_s: Option<String>,
    #[serde(default)]
    pub sig_v: Option<String>,
    #[serde(default)]
    pub tx_type: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shielded_payload: Option<mersennet::shielded_evm::ShieldedEnvelope>,
    /// Canonical tx hash (keccak256 of the raw RLP) carried across
    /// gossip so every node reports the same wallet-computed hash for a
    /// MetaMask/ethers-submitted transaction.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hash: Option<String>,
    /// Raw signed envelope (hex) for wallet-submitted txs. Ethereum
    /// signatures verify against the RLP signing payload, which peers
    /// cannot reconstruct from the parsed fields — receivers re-decode
    /// this envelope so the tx stays self-authenticating end to end.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct WireReceipt {
    pub success: bool,
    pub gas_used: u64,
    pub output: String,
    pub created_address: Option<String>,
    pub error: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct WireReward {
    pub address: String,
    pub amount: String,
}

#[derive(Serialize, Deserialize)]
pub struct WireBlock {
    pub number: u64,
    pub chain_id: u64,
    #[serde(default)]
    pub timestamp: u64,
    pub gas_limit: u64,
    pub gas_used: u64,
    pub base_fee: String,
    pub coinbase: String,
    pub hash: String,
    #[serde(default)]
    pub parent_hash: String,
    pub proposer: String,
    pub finalized: bool,
    pub state_root: String,
    pub total_reward: String,
    pub burned_reward: String,
    // Per-recipient block rewards. Carried so importing nodes credit
    // the same proposer/validator rewards the producer applied and
    // converge on the producer's state. Defaulted for compatibility
    // with blocks gossiped by older nodes.
    #[serde(default)]
    pub rewards: Vec<WireReward>,
    pub transactions: Vec<WireTx>,
    pub receipts: Vec<WireReceipt>,
    // Shielded header fields + SP1 state-transition proof. Optional and
    // defaulted so blocks from older nodes (which never sent them) still
    // decode; without these the proof only existed on the producing node.
    #[serde(default)]
    pub shielded_state_root: Option<String>,
    #[serde(default)]
    pub nullifier_root: Option<String>,
    #[serde(default)]
    pub shielded_event_root: Option<String>,
    /// bincode-encoded `StateTransitionProof`, hex string.
    #[serde(default)]
    pub state_proof: Option<String>,
    /// Proposer signature over `(height, hash)` as hex `r`, `s`, and y-parity.
    /// Verified on import so only the elected validator's blocks are accepted.
    #[serde(default)]
    pub proposer_sig_r: Option<String>,
    #[serde(default)]
    pub proposer_sig_s: Option<String>,
    #[serde(default)]
    pub proposer_sig_y: Option<u64>,
}

/// A signed BFT finality vote, gossiped on the "vote" topic. The
/// signature is over `crypto::vote_digest(height, block_hash)`; the
/// signer address is recovered on receipt and must be a known
/// validator for the vote to count.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WireVote {
    pub height: u64,
    pub block_hash: String,
    pub r: String,
    pub s: String,
    pub y_parity: u64,
}

fn hex_u256(v: &U256) -> String {
    format!("{v:x}")
}

fn parse_hex_u256(s: &str) -> Option<U256> {
    U256::from_str_radix(s, 16).ok()
}

fn hex_addr(a: &Address) -> String {
    hex::encode(a.as_slice())
}

fn parse_hex_addr(s: &str) -> Option<Address> {
    let b = hex::decode(s).ok()?;
    if b.len() != 20 {
        return None;
    }
    Some(Address::from_slice(&b))
}

fn hex_b256(h: &B256) -> String {
    hex::encode(h.as_slice())
}

fn parse_hex_b256(s: &str) -> Option<B256> {
    let b = hex::decode(s).ok()?;
    if b.len() != 32 {
        return None;
    }
    Some(B256::from_slice(&b))
}

fn hex_bytes(b: &Bytes) -> String {
    hex::encode(b.as_ref())
}

fn parse_hex_bytes(s: &str) -> Option<Bytes> {
    hex::decode(s).ok().map(Bytes::from)
}

pub fn tx_to_wire(tx: &Transaction) -> WireTx {
    tx_to_wire_with_raw(tx, None)
}

pub fn tx_to_wire_with_raw(tx: &Transaction, raw: Option<&[u8]>) -> WireTx {
    let (sig_r, sig_s, sig_v) = match &tx.signature {
        Some((r, s, v)) => (Some(hex_u256(r)), Some(hex_u256(s)), Some(format!("{v:x}"))),
        None => (None, None, None),
    };
    WireTx {
        from: hex_addr(&tx.from),
        to: tx.to.as_ref().map(hex_addr),
        value: hex_u256(&tx.value),
        data: hex_bytes(&tx.data),
        gas_limit: tx.gas_limit,
        gas_price: hex_u256(&tx.gas_price),
        nonce: tx.nonce,
        chain_id: tx.chain_id,
        sig_r,
        sig_s,
        sig_v,
        tx_type: tx.tx_type,
        shielded_payload: tx.shielded_payload.clone(),
        hash: tx.hash.map(|h| hex_b256(&h)),
        raw: raw.map(hex::encode),
    }
}

pub fn wire_to_tx(wire: &WireTx) -> Option<Transaction> {
    let signature = match (&wire.sig_r, &wire.sig_s, &wire.sig_v) {
        (Some(r), Some(s), Some(v)) => Some((
            parse_hex_u256(r)?,
            parse_hex_u256(s)?,
            u64::from_str_radix(v.trim_start_matches("0x"), 16).ok()?,
        )),
        _ => None,
    };
    Some(Transaction {
        from: parse_hex_addr(&wire.from)?,
        to: match &wire.to {
            Some(s) => Some(parse_hex_addr(s)?),
            None => None,
        },
        value: parse_hex_u256(&wire.value)?,
        data: parse_hex_bytes(&wire.data)?,
        gas_limit: wire.gas_limit,
        gas_price: parse_hex_u256(&wire.gas_price)?,
        nonce: wire.nonce,
        chain_id: wire.chain_id,
        signature,
        tx_type: wire.tx_type,
        shielded_payload: wire.shielded_payload.clone(),
        hash: wire.hash.as_deref().and_then(parse_hex_b256),
    })
}

fn receipt_to_wire(r: &Receipt) -> WireReceipt {
    WireReceipt {
        success: r.success,
        gas_used: r.gas_used,
        output: hex_bytes(&r.output),
        created_address: r.created_address.as_ref().map(hex_addr),
        error: r.error.clone(),
    }
}

fn wire_to_receipt(wire: &WireReceipt) -> Option<Receipt> {
    Some(Receipt {
        success: wire.success,
        gas_used: wire.gas_used,
        output: parse_hex_bytes(&wire.output)?,
        created_address: match &wire.created_address {
            Some(s) => Some(parse_hex_addr(s)?),
            None => None,
        },
        error: wire.error.clone(),
        logs: Vec::new(),
    })
}

pub fn block_to_wire(block: &Block) -> WireBlock {
    WireBlock {
        number: block.number,
        chain_id: block.chain_id,
        timestamp: block.timestamp,
        gas_limit: block.gas_limit,
        gas_used: block.gas_used,
        base_fee: hex_u256(&block.base_fee),
        coinbase: hex_addr(&block.coinbase),
        hash: hex_b256(&block.hash),
        parent_hash: hex_b256(&block.parent_hash),
        proposer: hex_addr(&block.proposer),
        finalized: block.finalized,
        state_root: hex_b256(&block.state_root),
        total_reward: hex_u256(&block.total_reward),
        burned_reward: hex_u256(&block.burned_reward),
        rewards: block
            .rewards
            .iter()
            .map(|r| WireReward {
                address: hex_addr(&r.address),
                amount: hex_u256(&r.amount),
            })
            .collect(),
        transactions: block.transactions.iter().map(tx_to_wire).collect(),
        receipts: block.receipts.iter().map(receipt_to_wire).collect(),
        shielded_state_root: Some(hex_b256(&block.shielded_state_root)),
        nullifier_root: Some(hex_b256(&block.nullifier_root)),
        shielded_event_root: Some(hex_b256(&block.shielded_event_root)),
        state_proof: block
            .state_proof
            .as_ref()
            .and_then(|p| bincode::serialize(p).ok())
            .map(hex::encode),
        proposer_sig_r: block.proposer_sig.map(|(r, _, _)| hex_u256(&r)),
        proposer_sig_s: block.proposer_sig.map(|(_, s, _)| hex_u256(&s)),
        proposer_sig_y: block.proposer_sig.map(|(_, _, y)| y),
    }
}

pub fn wire_to_block(wire: &WireBlock) -> Option<Block> {
    let txs: Option<Vec<Transaction>> = wire.transactions.iter().map(wire_to_tx).collect();
    let receipts: Option<Vec<Receipt>> = wire.receipts.iter().map(wire_to_receipt).collect();
    let base_fee = parse_hex_u256(&wire.base_fee)?;
    let hash = parse_hex_b256(&wire.hash)?;
    let parent_hash = parse_hex_b256(&wire.parent_hash).unwrap_or(B256::ZERO);
    let proposer = parse_hex_addr(&wire.proposer)?;
    let coinbase = parse_hex_addr(&wire.coinbase)?;
    let state_root = parse_hex_b256(&wire.state_root)?;
    let total_reward = parse_hex_u256(&wire.total_reward)?;
    let burned_reward = parse_hex_u256(&wire.burned_reward)?;
    let rewards: Option<Vec<Reward>> = wire
        .rewards
        .iter()
        .map(|r| {
            Some(Reward {
                address: parse_hex_addr(&r.address)?,
                amount: parse_hex_u256(&r.amount)?,
            })
        })
        .collect();
    let rewards = rewards?;

    Some(Block {
        number: wire.number,
        chain_id: wire.chain_id,
        timestamp: if wire.timestamp > 0 {
            wire.timestamp
        } else {
            wire.number
        },
        gas_limit: wire.gas_limit,
        gas_used: wire.gas_used,
        base_fee,
        coinbase,
        hash,
        parent_hash,
        proposer,
        finalized: wire.finalized,
        state_root,
        total_reward,
        burned_reward,
        consensus: Finalization {
            block_hash: hash,
            proposer,
            total_stake: U256::ZERO,
            committed_stake: U256::ZERO,
            threshold: U256::ZERO,
            votes: Vec::new(),
            finalized: wire.finalized,
            rewards: Vec::new(),
            total_reward,
            burned_reward,
            scheduled_reward: U256::ZERO,
            remaining_supply: U256::ZERO,
        },
        unbonded: Vec::new(),
        applied_validator_changes: Vec::new(),
        slashes: Vec::new(),
        finality_rounds: Vec::new(),
        slashing_evidence: Vec::new(),
        rewards,
        transactions: txs?,
        receipts: receipts?,
        bridge_orders_to_evm: Vec::new(),
        bridge_evm_to_orders: Vec::new(),
        domain_events: Vec::new(),
        shielded_state_root: wire
            .shielded_state_root
            .as_deref()
            .and_then(parse_hex_b256)
            .unwrap_or(revm::primitives::B256::ZERO),
        nullifier_root: wire
            .nullifier_root
            .as_deref()
            .and_then(parse_hex_b256)
            .unwrap_or(revm::primitives::B256::ZERO),
        shielded_event_root: wire
            .shielded_event_root
            .as_deref()
            .and_then(parse_hex_b256)
            .unwrap_or(revm::primitives::B256::ZERO),
        state_proof: wire
            .state_proof
            .as_deref()
            .and_then(|s| hex::decode(s).ok())
            .and_then(|b| bincode::deserialize(&b).ok()),
        proposer_sig: match (
            wire.proposer_sig_r.as_deref().and_then(parse_hex_u256),
            wire.proposer_sig_s.as_deref().and_then(parse_hex_u256),
            wire.proposer_sig_y,
        ) {
            (Some(r), Some(s), Some(y)) => Some((r, s, y)),
            _ => None,
        },
    })
}

// ---------------------------------------------------------------------------
// NetworkNode - wraps Engine + UDP gossip for real P2P networking
// ---------------------------------------------------------------------------

/// What peers say about a height this node is about to propose for.
pub enum HeightProbe {
    /// A peer already holds it: the blocks it sent, from that height on.
    Found(Vec<Block>),
    /// At least one peer answered, and none holds it.
    NotFound,
    /// No peer answered in time.
    NoAnswer,
}

/// `NetworkNode::probe_height` against an explicit list of peers (gossip
/// addresses), asked in parallel. `NotFound` waits for every answer, so one
/// peer that is itself behind cannot hide a block another peer holds.
pub fn probe_peers(peers: &[String], height: u64, timeout: Duration) -> HeightProbe {
    let (tx, rx) = std::sync::mpsc::channel();
    for peer in peers {
        let tx = tx.clone();
        let addr = derive_tcp_addr(peer);
        let _ = std::thread::Builder::new()
            .name("height-probe".into())
            .spawn(move || {
                let _ = tx.send(ask_for_blocks_from(&addr, height, timeout));
            });
    }
    drop(tx);
    let deadline = std::time::Instant::now() + timeout;
    let mut answered = false;
    while let Ok(answer) =
        rx.recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))
    {
        match answer {
            Some(blocks) if !blocks.is_empty() => return HeightProbe::Found(blocks),
            Some(_) => answered = true,
            None => {}
        }
    }
    if answered {
        HeightProbe::NotFound
    } else {
        HeightProbe::NoAnswer
    }
}

/// One sync request for the blocks from `height`: what the peer sent (empty
/// if it does not hold `height` yet), or `None` if it did not answer in time.
fn ask_for_blocks_from(addr: &str, height: u64, timeout: Duration) -> Option<Vec<Block>> {
    let socket: std::net::SocketAddr = addr.parse().ok()?;
    let mut stream = std::net::TcpStream::connect_timeout(&socket, timeout).ok()?;
    stream.set_read_timeout(Some(timeout)).ok()?;
    stream.set_write_timeout(Some(timeout)).ok()?;
    let request = crate::net_transport::GossipPacket {
        topic: "sync_request".to_string(),
        data: height.to_be_bytes().to_vec(),
        id: String::new(),
        ttl: 0,
    };
    TcpSync::send_packet(&mut stream, &request).ok()?;
    let response = TcpSync::recv_packet_limited(&mut stream, 64 * 1024 * 1024).ok()??;
    if response.topic != "sync_response" {
        return None;
    }
    let wire: Vec<WireBlock> = serde_json::from_slice(&response.data).ok()?;
    Some(
        wire.iter()
            .filter_map(wire_to_block)
            .filter(|b| b.number >= height)
            .collect(),
    )
}

#[derive(Clone)]
pub struct NetworkNode {
    gossip: Arc<Mutex<UdpGossip>>,
    running: Arc<AtomicBool>,
}

impl NetworkNode {
    pub fn new(config: &GossipConfig) -> Result<Self> {
        let mut gossip = UdpGossip::bind_with_config(&config.listen_addr, config.clone())?;
        for peer in &config.bootstrap_peers {
            if let Err(e) = gossip.add_peer(peer) {
                warn!(peer = %peer, err = %e, "failed to add bootstrap peer");
            }
        }
        Ok(Self {
            gossip: Arc::new(Mutex::new(gossip)),
            running: Arc::new(AtomicBool::new(false)),
        })
    }

    pub fn start_networking(&self, engine: Arc<Mutex<Engine>>, config: &GossipConfig) {
        self.start_networking_with_attestor(engine, config, None);
    }

    /// `attestor` answers `whoami` requests on the TCP port with a signed
    /// identity/operator statement (verified node runners).
    pub fn start_networking_with_attestor(
        &self,
        engine: Arc<Mutex<Engine>>,
        config: &GossipConfig,
        attestor: Option<Arc<mersennet::identity::NodeAttestor>>,
    ) {
        self.running.store(true, Ordering::SeqCst);
        let listen_addr = config.listen_addr.clone();

        // UDP gossip listener thread
        {
            let gossip = self.gossip.clone();
            let engine = engine.clone();
            let running = self.running.clone();
            std::thread::Builder::new()
                .name("gossip-listener".into())
                .spawn(move || {
                    info!("gossip listener started");
                    let mut decode_errors: u64 = 0;
                    while running.load(Ordering::SeqCst) {
                        let packet = {
                            let mut g = match gossip.lock() {
                                Ok(g) => g,
                                Err(_) => break,
                            };
                            g.recv_and_gossip(Duration::from_millis(100))
                        };
                        let packet = match packet {
                            Ok(Some(p)) => p,
                            Ok(None) => continue,
                            Err(err) => {
                                // A datagram we could not parse (foreign
                                // traffic, or a peer on an incompatible wire
                                // format). Count it and say so occasionally.
                                metrics::increment_counter!("mersennet_gossip_decode_errors_total");
                                decode_errors += 1;
                                if decode_errors.is_power_of_two() {
                                    tracing::warn!(count = decode_errors, %err, "undecodable gossip datagram");
                                }
                                continue;
                            }
                        };
                        match packet.topic.as_str() {
                            "block" => {
                                if let Ok(wire) = serde_json::from_slice::<WireBlock>(&packet.data)
                                    && let Some(block) = wire_to_block(&wire)
                                {
                                    info!(number = block.number, "received block from network");
                                    if let Ok(mut eng) = engine.lock() {
                                        eng.import_block(block);
                                    }
                                }
                            }
                            "tx" => {
                                if let Ok(wire) = serde_json::from_slice::<WireTx>(&packet.data) {
                                    handle_relayed_tx(&engine, &wire);
                                }
                            }
                            "vote" => {
                                if let Ok(wire) = serde_json::from_slice::<WireVote>(&packet.data) {
                                    handle_vote(&engine, &wire);
                                }
                            }
                            _ => {}
                        }
                    }
                })
                .ok();
        }

        // TCP snapshot listener thread
        {
            let engine = engine.clone();
            let running = self.running.clone();
            let attestor_listener = attestor.clone();
            let tcp_addr = derive_tcp_addr(&listen_addr);
            std::thread::Builder::new()
                .name("tcp-snapshot".into())
                .spawn(move || {
                    let tcp = match TcpSync::bind(&tcp_addr) {
                        Ok(t) => t,
                        Err(e) => {
                            warn!(err = %e, addr = %tcp_addr, "tcp snapshot bind failed");
                            return;
                        }
                    };
                    info!(addr = %tcp_addr, "tcp snapshot listener started");
                    // Each request is served on its own thread (bounded) so a
                    // slow or half-open peer only ties up its own worker. The
                    // previous single-threaded loop held the ENGINE LOCK across
                    // an untimed socket write; one client that stopped reading
                    // blocked that write forever, and with it every thread that
                    // needed the engine — block import, RPC, discovery — i.e.
                    // the whole node froze until restarted.
                    const MAX_SYNC_WORKERS: usize = 8;
                    let in_flight = Arc::new(std::sync::atomic::AtomicUsize::new(0));
                    while running.load(Ordering::SeqCst) {
                        if let Ok(Some(mut stream)) = tcp.accept_once() {
                            if in_flight.load(Ordering::SeqCst) >= MAX_SYNC_WORKERS {
                                // Busy: drop the connection; the client retries.
                                continue;
                            }
                            in_flight.fetch_add(1, Ordering::SeqCst);
                            let engine = engine.clone();
                            let attestor_worker = attestor_listener.clone();
                            let in_flight_worker = in_flight.clone();
                            let in_flight_outer = in_flight.clone();
                            std::thread::Builder::new()
                                .name("tcp-sync-worker".into())
                                .spawn(move || {
                                    // The listener is non-blocking; the accepted
                                    // stream inherits that. Switch to bounded
                                    // blocking I/O in both directions.
                                    let _ = stream.set_nonblocking(false);
                                    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
                                    let _ = stream.set_write_timeout(Some(Duration::from_secs(10)));
                                    let request = match TcpSync::recv_packet(&mut stream) {
                                        Ok(Some(r)) => r,
                                        _ => {
                                            in_flight_worker.fetch_sub(1, Ordering::SeqCst);
                                            return;
                                        }
                                    };
                                    if request.topic == "block_push" {
                                        // Large block delivered peer-to-peer (see
                                        // broadcast_block). Same handling as gossip.
                                        if let Ok(wire) =
                                            serde_json::from_slice::<WireBlock>(&request.data)
                                            && let Some(block) = wire_to_block(&wire)
                                        {
                                            tracing::debug!(
                                                number = block.number,
                                                "received block over tcp"
                                            );
                                            if let Ok(mut eng) = engine.lock() {
                                                eng.import_block(block);
                                            }
                                        }
                                        in_flight_worker.fetch_sub(1, Ordering::SeqCst);
                                        return;
                                    }
                                    if request.topic == "tx_push" {
                                        // Large transaction delivered peer-to-peer
                                        // (see broadcast_tx_with_raw): a contract
                                        // deployment near the 24 KB code limit does
                                        // not fit a UDP datagram once encoded.
                                        if let Ok(wire) =
                                            serde_json::from_slice::<WireTx>(&request.data)
                                        {
                                            handle_relayed_tx(&engine, &wire);
                                        }
                                        in_flight_worker.fetch_sub(1, Ordering::SeqCst);
                                        return;
                                    }
                                    if request.topic == "whoami" {
                                        // Verified-node-runner probe: echo a signed
                                        // statement of who we are, bound to the
                                        // caller's nonce (16..=64 bytes). Cheap and
                                        // rate-limited by the worker pool like sync.
                                        if let Some(att) = attestor_worker.as_ref()
                                            && (16..=64).contains(&request.data.len())
                                        {
                                            let height = engine
                                                .lock()
                                                .map(|e| e.latest_height())
                                                .unwrap_or(0);
                                            let body = att.attest(&request.data, height);
                                            let pkt = crate::net_transport::GossipPacket {
                                                topic: "whoami_response".to_string(),
                                                data: serde_json::to_vec(&body).unwrap_or_default(),
                                                id: String::new(),
                                                ttl: 0,
                                            };
                                            let _ = TcpSync::send_packet(&mut stream, &pkt);
                                        }
                                        in_flight_worker.fetch_sub(1, Ordering::SeqCst);
                                        return;
                                    }
                                    if request.topic == "sync_request" {
                                        // The requester encodes the next height it
                                        // needs as 8 big-endian bytes; default to
                                        // the full chain for legacy/empty requests.
                                        let from = if request.data.len() == 8 {
                                            let mut buf = [0u8; 8];
                                            buf.copy_from_slice(&request.data);
                                            u64::from_be_bytes(buf).max(1)
                                        } else {
                                            1
                                        };
                                        // Build the response under the engine lock,
                                        // then RELEASE it before touching the socket.
                                        let data = match engine.lock() {
                                            Ok(eng) => {
                                                let height = eng.latest_height();
                                                // Serve up to a bounded batch per request.
                                                // Use get_block so heights below the
                                                // in-memory window are read from disk — a
                                                // peer far behind can still be caught up.
                                                let batch_end =
                                                    height.min(from.saturating_add(255));
                                                let blocks: Vec<WireBlock> = (from..=batch_end)
                                                    .filter_map(|n| eng.get_block(n))
                                                    .map(|b| block_to_wire(&b))
                                                    .collect();
                                                Some(
                                                    serde_json::to_vec(&blocks).unwrap_or_default(),
                                                )
                                            }
                                            Err(_) => None,
                                        };
                                        if let Some(data) = data {
                                            let gossip_pkt = crate::net_transport::GossipPacket {
                                                topic: "sync_response".to_string(),
                                                data,
                                                id: String::new(),
                                                ttl: 0,
                                            };
                                            let _ = TcpSync::send_packet(&mut stream, &gossip_pkt);
                                        }
                                    }
                                    in_flight_worker.fetch_sub(1, Ordering::SeqCst);
                                })
                                .map_err(|_| in_flight_outer.fetch_sub(1, Ordering::SeqCst))
                                .ok();
                        }
                        std::thread::sleep(Duration::from_millis(20));
                    }
                })
                .ok();
        }

        // Peer discovery loop
        {
            let gossip = self.gossip.clone();
            let running = self.running.clone();
            let engine_for_peers = engine.clone();
            std::thread::Builder::new()
                .name("peer-discovery".into())
                .spawn(move || {
                    info!("peer discovery loop started");
                    while running.load(Ordering::SeqCst) {
                        std::thread::sleep(Duration::from_secs(30));
                        if let Ok(mut g) = gossip.lock() {
                            let _ = g.discover_peers(2);
                            g.prune_peers();
                            let count = g.peer_count();
                            let snapshot = g.peers_snapshot();
                            info!(peers = count, "peer discovery tick");
                            if let Ok(mut eng) = engine_for_peers.lock() {
                                eng.peer_count
                                    .store(count, std::sync::atomic::Ordering::Relaxed);
                                eng.peer_list = snapshot
                                    .into_iter()
                                    .map(|p| mersennet::engine::PeerSnapshot {
                                        addr: p.addr,
                                        first_seen_secs: p.first_seen_secs,
                                        last_seen_secs: p.last_seen_secs,
                                        heard: p.heard,
                                    })
                                    .collect();
                            }
                        }
                    }
                })
                .ok();
        }

        // Block-sync client loop. Gossip only delivers blocks going
        // forward, so a node that starts (or falls) behind the fleet
        // buffers future blocks and never advances. This periodically
        // pulls the missing range from a peer over TCP so the node
        // catches up, then gossip keeps it in sync.
        {
            let engine = engine.clone();
            let running = self.running.clone();
            let peers = config.bootstrap_peers.clone();
            std::thread::Builder::new()
                .name("block-sync".into())
                .spawn(move || {
                    info!("block-sync loop started");
                    // Let listeners come up before the first request.
                    std::thread::sleep(Duration::from_secs(2));
                    let mut full_batch;
                    let mut round: usize = 0;
                    while running.load(Ordering::SeqCst) {
                        let from = match engine.lock() {
                            Ok(eng) => eng.latest_height().saturating_add(1),
                            Err(_) => {
                                std::thread::sleep(Duration::from_secs(4));
                                continue;
                            }
                        };
                        full_batch = false;
                        round = round.wrapping_add(1);
                        // Start from a different peer each round and move on
                        // when a peer's blocks do not advance our head: a
                        // bootstrap peer sitting on a fork used to answer
                        // first every time and starve the others.
                        let n = peers.len().max(1);
                        let ordered: Vec<&String> = (0..peers.len()).map(|i| &peers[(round + i) % n]).collect();
                        for peer in ordered {
                            let tcp_addr = derive_tcp_addr(peer);
                            let Ok(mut stream) =
                                TcpSync::connect(&tcp_addr, Duration::from_secs(5))
                            else {
                                continue;
                            };
                            let req = crate::net_transport::GossipPacket {
                                topic: "sync_request".to_string(),
                                data: from.to_be_bytes().to_vec(),
                                id: String::new(),
                                ttl: 0,
                            };
                            if TcpSync::send_packet(&mut stream, &req).is_err() {
                                continue;
                            }
                            // 256 dense blocks can serialize well past the
                            // default 4 MiB frame cap — allow up to 64 MiB
                            // from the peer we chose to sync from.
                            let blocks: Vec<WireBlock> = match TcpSync::recv_packet_limited(
                                &mut stream,
                                64 * 1024 * 1024,
                            ) {
                                Ok(Some(resp)) if resp.topic == "sync_response" => {
                                    serde_json::from_slice(&resp.data).unwrap_or_default()
                                }
                                _ => continue,
                            };
                            if blocks.is_empty() {
                                continue;
                            }
                            let mut applied = 0u64;
                            let mut advanced = false;
                            if let Ok(mut eng) = engine.lock() {
                                let before = eng.latest_height();
                                for wire in &blocks {
                                    if let Some(block) = wire_to_block(wire) {
                                        eng.import_block(block);
                                        applied += 1;
                                    }
                                }
                                advanced = eng.latest_height() > before;
                            }
                            if advanced {
                                info!(peer = %peer, from, count = applied, "synced blocks from peer");
                            } else {
                                // This peer's view did not move us (fork or
                                // stale): try the next one this round.
                                continue;
                            }
                            // A full batch means the peer likely has more —
                            // keep pulling back-to-back so initial sync runs
                            // at wire speed instead of one batch per tick.
                            full_batch = blocks.len() >= 256;
                            break;
                        }
                        if !full_batch {
                            std::thread::sleep(Duration::from_secs(4));
                        }
                    }
                })
                .ok();
        }
    }

    /// Largest gossip packet we hand to UDP. A datagram tops out at 65,507
    /// bytes and the packet JSON encodes `data` as a number array (3-4x the
    /// raw size), so anything bigger than this fails in `send_to` and was
    /// silently dropped — a ~25-tx block never reached peers by gossip, they
    /// only got it through the 4 s TCP poll, leaders timed out and competing
    /// proposals followed. Blocks above the limit go peer-to-peer over TCP.
    const MAX_UDP_PACKET_BYTES: usize = 60_000;

    pub fn broadcast_block(&self, block: &Block) -> Result<()> {
        let wire = block_to_wire(block);
        let data = serde_json::to_vec(&wire)?;
        let (packet, peers) = {
            let mut gossip = self.gossip.lock().map_err(|e| anyhow::anyhow!("{e}"))?;
            let packet = gossip.new_packet("block", data, 3);
            let encoded = serde_json::to_vec(&packet)?.len();
            if encoded <= Self::MAX_UDP_PACKET_BYTES {
                // Fast path: one datagram to every peer. Blocks are tens of
                // kilobytes, i.e. many IP fragments, and some providers drop
                // fragments while small packets pass (the first outside
                // validator's pings reached us, its blocks never did). So the
                // datagram is a hint, not the delivery: the TCP push below
                // goes out as well; receivers already ignore a block they hold.
                let _ = gossip.broadcast(&packet);
            } else {
                metrics::increment_counter!("mersennet_block_push_tcp_total");
            }
            let peers: Vec<String> = gossip
                .peers_snapshot()
                .into_iter()
                .map(|p| p.addr)
                .collect();
            (packet, peers)
        };
        // Deliver directly to every known peer over the TCP sync port.
        self.push_over_tcp("block_push", packet, peers);
        Ok(())
    }

    /// Ask the most recently heard peers (up to `max_peers`) whether they
    /// already hold `height`, over the TCP sync port, waiting at most `timeout`.
    pub fn probe_height(&self, height: u64, max_peers: usize, timeout: Duration) -> HeightProbe {
        let mut peers = match self.gossip.lock() {
            Ok(g) => g.peers_snapshot(),
            Err(_) => return HeightProbe::NoAnswer,
        };
        peers.retain(|p| p.heard);
        peers.sort_by_key(|p| p.last_seen_secs);
        let addrs: Vec<String> = peers.into_iter().take(max_peers).map(|p| p.addr).collect();
        probe_peers(&addrs, height, timeout)
    }

    pub fn broadcast_tx(&self, tx: &Transaction) -> Result<()> {
        self.broadcast_tx_with_raw(tx, None)
    }

    /// Broadcast a tx, attaching its raw signed envelope when available so
    /// receivers can re-verify wallet (Ethereum-format) signatures.
    pub fn broadcast_tx_with_raw(&self, tx: &Transaction, raw: Option<&[u8]>) -> Result<()> {
        let wire = tx_to_wire_with_raw(tx, raw);
        let data = serde_json::to_vec(&wire)?;
        let (packet, peers) = {
            let mut gossip = self.gossip.lock().map_err(|e| anyhow::anyhow!("{e}"))?;
            let packet = gossip.new_packet("tx", data, 3);
            let encoded = serde_json::to_vec(&packet)?.len();
            if encoded <= Self::MAX_UDP_PACKET_BYTES {
                return gossip.broadcast(&packet);
            }
            metrics::increment_counter!("mersennet_tx_push_tcp_total");
            let peers: Vec<String> = gossip
                .peers_snapshot()
                .into_iter()
                .map(|p| p.addr)
                .collect();
            (packet, peers)
        };
        // Same path as large blocks: the datagram would be dropped silently,
        // so deliver to every known peer over the TCP sync port instead.
        self.push_over_tcp("tx_push", packet, peers);
        Ok(())
    }

    fn push_over_tcp(
        &self,
        topic: &'static str,
        packet: crate::net_transport::GossipPacket,
        peers: Vec<String>,
    ) {
        let pushed = std::sync::Arc::new(packet);
        for peer in peers {
            let pkt = pushed.clone();
            let addr = derive_tcp_addr(&peer);
            std::thread::Builder::new()
                .name(format!("{topic}-push"))
                .spawn(move || {
                    if let Ok(mut stream) = TcpSync::connect(&addr, Duration::from_secs(3)) {
                        let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
                        let push = crate::net_transport::GossipPacket {
                            topic: topic.to_string(),
                            data: pkt.data.clone(),
                            id: pkt.id.clone(),
                            ttl: 0,
                        };
                        let _ = TcpSync::send_packet(&mut stream, &push);
                    }
                })
                .ok();
        }
    }

    /// Gossip a signed BFT finality vote for `(height, block_hash)`.
    pub fn broadcast_vote(
        &self,
        height: u64,
        block_hash: B256,
        r: U256,
        s: U256,
        y_parity: u64,
    ) -> Result<()> {
        let wire = WireVote {
            height,
            block_hash: hex_b256(&block_hash),
            r: hex_u256(&r),
            s: hex_u256(&s),
            y_parity,
        };
        let data = serde_json::to_vec(&wire)?;
        let mut gossip = self.gossip.lock().map_err(|e| anyhow::anyhow!("{e}"))?;
        let packet = gossip.new_packet("vote", data, 3);
        gossip.broadcast(&packet)
    }

    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
}

/// Verify a gossiped finality vote and, if the signer is a known
/// validator, record it in the engine. Recovering the signer from the
/// signature means a forged vote (bad signature) simply fails to
/// recover a validator address and is dropped.
/// Admit a relayed transaction (UDP gossip or TCP `tx_push`). Wallet txs carry
/// their raw signed envelope: re-decode it so the true Ethereum (EIP-155/typed)
/// signature is verified; the parsed fields alone cannot reproduce that signing
/// payload. Native-format txs must carry a valid signature (submit_tx checks the
/// signer matches `from`); unsigned relays are rejected outright since they
/// could execute as any account.
fn handle_relayed_tx(engine: &Arc<Mutex<Engine>>, wire: &WireTx) {
    let Ok(mut eng) = engine.lock() else { return };
    if let Some(raw_hex) = &wire.raw {
        match hex::decode(raw_hex)
            .ok()
            .and_then(|raw| mersennet::crypto::decode_raw_signed_tx(&raw).ok())
        {
            Some(signed) => {
                if let Err(err) = eng.submit_tx_unsigned(signed.tx) {
                    tracing::debug!(reason = err.code(), "dropped relayed raw tx");
                }
            }
            None => tracing::debug!("dropped relayed tx: bad raw envelope"),
        }
    } else if let Some(tx) = wire_to_tx(wire) {
        if tx.signature.is_some() {
            if let Err(err) = eng.submit_tx(tx) {
                tracing::debug!(reason = err.code(), "dropped relayed tx");
            }
        } else {
            tracing::debug!("dropped relayed unsigned tx (signatures required)");
        }
    }
}

fn handle_vote(engine: &Arc<Mutex<Engine>>, wire: &WireVote) {
    let Some(block_hash) = parse_hex_b256(&wire.block_hash) else {
        return;
    };
    let Some(r) = parse_hex_u256(&wire.r) else {
        return;
    };
    let Some(s) = parse_hex_u256(&wire.s) else {
        return;
    };
    let signer = match mersennet::crypto::recover_vote_signer(
        wire.height,
        block_hash,
        r,
        s,
        wire.y_parity,
    ) {
        Ok(addr) => addr,
        Err(_) => return,
    };
    if let Ok(mut eng) = engine.lock() {
        eng.record_finality_vote(wire.height, block_hash, signer);
    }
}

fn derive_tcp_addr(udp_addr: &str) -> String {
    // TCP block-sync shares the same port as UDP gossip. TCP and UDP
    // are independent L4 protocols, so binding both on the same port is
    // fine, and deployment firewalls open `<port>/tcp` ("P2P sync") for
    // exactly this. (A previous +1000 offset landed on a firewalled
    // port, so sync connections were silently refused.)
    udp_addr.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Height 1 with one transfer, stamped with its proposer the way the node
    /// binary does after hashing it.
    fn produced_block(dir: &std::path::Path) -> Block {
        let path = dir.join("producer");
        std::fs::create_dir_all(&path).unwrap();
        let mut producer = Engine::new_with_backend(131071, path, "redb");
        let validator = Address::from_slice(&[0x44; 20]);
        producer
            .add_validator(validator, U256::from(1_000_000u64))
            .unwrap();
        producer.set_local_validator(validator);
        let alice = Address::from_slice(&[0x11; 20]);
        let bob = Address::from_slice(&[0x22; 20]);
        producer.fund_account(alice, U256::from(2_000_000u64), 0);
        producer
            .transfer(
                alice,
                bob,
                U256::from(1_000u64),
                21_000,
                U256::from(1u64),
                0,
            )
            .unwrap();
        let mut block = producer.execute_block().unwrap();
        block.coinbase = validator;
        block.proposer = validator;
        block
    }

    /// The engine trusts a finalized block's parent (to roll back blocks it
    /// built on a dead branch) only if the block's hash re-derives from its
    /// header, so that must still hold after the wire round trip.
    #[test]
    fn a_block_keeps_a_verifiable_hash_across_the_wire() {
        let dir = tempfile::tempdir().unwrap();
        let block = produced_block(dir.path());
        assert!(!block.transactions.is_empty() && !block.receipts.is_empty());

        let bytes = serde_json::to_vec(&block_to_wire(&block)).unwrap();
        let back = wire_to_block(&serde_json::from_slice::<WireBlock>(&bytes).unwrap()).unwrap();
        let path = dir.path().join("importer");
        std::fs::create_dir_all(&path).unwrap();
        let importer = Engine::new_with_backend(131071, path, "redb");
        assert_eq!(back.hash, block.hash);
        assert!(
            importer.header_commits_to_hash(&back),
            "hash re-derives after the wire round trip"
        );
    }

    /// A stand-in for a peer's sync port that answers one request with
    /// `blocks`, or never answers when `blocks` is `None`.
    fn fake_peer(blocks: Option<Vec<Block>>) -> String {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap().to_string();
        std::thread::spawn(move || {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            let _ = TcpSync::recv_packet(&mut stream);
            let Some(blocks) = blocks else {
                std::thread::sleep(Duration::from_secs(5));
                return;
            };
            let wire: Vec<WireBlock> = blocks.iter().map(block_to_wire).collect();
            let response = crate::net_transport::GossipPacket {
                topic: "sync_response".to_string(),
                data: serde_json::to_vec(&wire).unwrap(),
                id: String::new(),
                ttl: 0,
            };
            let _ = TcpSync::send_packet(&mut stream, &response);
        });
        addr
    }

    #[test]
    fn probe_finds_a_block_that_one_peer_holds() {
        let dir = tempfile::tempdir().unwrap();
        let block = produced_block(dir.path());
        let peers = [
            fake_peer(Some(vec![])),
            fake_peer(Some(vec![block.clone()])),
        ];
        match probe_peers(&peers, 1, Duration::from_secs(2)) {
            HeightProbe::Found(blocks) => assert_eq!(blocks[0].hash, block.hash),
            _ => panic!("the block one peer holds must be found"),
        }
    }

    #[test]
    fn probe_is_not_found_when_every_peer_answers_without_it() {
        let peers = [fake_peer(Some(vec![])), fake_peer(Some(vec![]))];
        assert!(matches!(
            probe_peers(&peers, 1, Duration::from_secs(2)),
            HeightProbe::NotFound
        ));
    }

    #[test]
    fn probe_is_no_answer_when_no_peer_replies_in_time() {
        let closed = {
            let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            l.local_addr().unwrap().to_string()
        };
        let peers = [closed, fake_peer(None)];
        let started = std::time::Instant::now();
        assert!(matches!(
            probe_peers(&peers, 1, Duration::from_millis(400)),
            HeightProbe::NoAnswer
        ));
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "bounded by the timeout"
        );
    }
}
