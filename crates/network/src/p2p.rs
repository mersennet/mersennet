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
    }
}

pub fn wire_to_block(wire: &WireBlock) -> Option<Block> {
    let txs: Option<Vec<Transaction>> = wire.transactions.iter().map(wire_to_tx).collect();
    let receipts: Option<Vec<Receipt>> = wire.receipts.iter().map(wire_to_receipt).collect();
    let base_fee = parse_hex_u256(&wire.base_fee)?;
    let hash = parse_hex_b256(&wire.hash)?;
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
    })
}

// ---------------------------------------------------------------------------
// NetworkNode - wraps Engine + UDP gossip for real P2P networking
// ---------------------------------------------------------------------------

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
                    while running.load(Ordering::SeqCst) {
                        let packet = {
                            let mut g = match gossip.lock() {
                                Ok(g) => g,
                                Err(_) => break,
                            };
                            g.recv_and_gossip(Duration::from_millis(100))
                        };
                        let Ok(Some(packet)) = packet else {
                            continue;
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
                                if let Ok(wire) = serde_json::from_slice::<WireTx>(&packet.data)
                                    && let Some(tx) = wire_to_tx(&wire)
                                    && let Ok(mut eng) = engine.lock()
                                    && let Err(err) = eng.submit_tx(tx)
                                {
                                    tracing::debug!(reason = err.code(), "dropped relayed tx");
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
                    while running.load(Ordering::SeqCst) {
                        if let Ok(Some(mut stream)) = tcp.accept_once() {
                            // The listener is non-blocking; the accepted
                            // stream inherits that, which would race
                            // recv_packet against the client's send.
                            // Switch to a bounded blocking read.
                            let _ = stream.set_nonblocking(false);
                            let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
                            if let Ok(Some(request)) = TcpSync::recv_packet(&mut stream)
                                && request.topic == "sync_request"
                                && let Ok(eng) = engine.lock()
                            {
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
                                let height = eng.latest_height();
                                let blocks: Vec<WireBlock> = (from..=height)
                                    .filter_map(|n| eng.block_by_number(n))
                                    .map(block_to_wire)
                                    .collect();
                                let data = serde_json::to_vec(&blocks).unwrap_or_default();
                                let gossip_pkt = crate::net_transport::GossipPacket {
                                    topic: "sync_response".to_string(),
                                    data,
                                    id: String::new(),
                                    ttl: 0,
                                };
                                let _ = TcpSync::send_packet(&mut stream, &gossip_pkt);
                            }
                        }
                        std::thread::sleep(Duration::from_millis(100));
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
                            info!(peers = count, "peer discovery tick");
                            if let Ok(eng) = engine_for_peers.lock() {
                                eng.peer_count
                                    .store(count, std::sync::atomic::Ordering::Relaxed);
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
                    while running.load(Ordering::SeqCst) {
                        let from = match engine.lock() {
                            Ok(eng) => eng.latest_height().saturating_add(1),
                            Err(_) => {
                                std::thread::sleep(Duration::from_secs(4));
                                continue;
                            }
                        };
                        for peer in &peers {
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
                            let blocks: Vec<WireBlock> = match TcpSync::recv_packet(&mut stream) {
                                Ok(Some(resp)) if resp.topic == "sync_response" => {
                                    serde_json::from_slice(&resp.data).unwrap_or_default()
                                }
                                _ => continue,
                            };
                            if blocks.is_empty() {
                                continue;
                            }
                            let mut applied = 0u64;
                            if let Ok(mut eng) = engine.lock() {
                                for wire in &blocks {
                                    if let Some(block) = wire_to_block(wire) {
                                        eng.import_block(block);
                                        applied += 1;
                                    }
                                }
                            }
                            if applied > 0 {
                                info!(peer = %peer, from, count = applied, "synced blocks from peer");
                            }
                            // One responsive peer per round is enough.
                            break;
                        }
                        std::thread::sleep(Duration::from_secs(4));
                    }
                })
                .ok();
        }
    }

    pub fn broadcast_block(&self, block: &Block) -> Result<()> {
        let wire = block_to_wire(block);
        let data = serde_json::to_vec(&wire)?;
        let mut gossip = self.gossip.lock().map_err(|e| anyhow::anyhow!("{e}"))?;
        let packet = gossip.new_packet("block", data, 3);
        gossip.broadcast(&packet)
    }

    pub fn broadcast_tx(&self, tx: &Transaction) -> Result<()> {
        let wire = tx_to_wire(tx);
        let data = serde_json::to_vec(&wire)?;
        let mut gossip = self.gossip.lock().map_err(|e| anyhow::anyhow!("{e}"))?;
        let packet = gossip.new_packet("tx", data, 3);
        gossip.broadcast(&packet)
    }

    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
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
