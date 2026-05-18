use prime_chain::consensus::Finalization;
use prime_chain::engine::{Block, Engine, Receipt, Transaction};
use crate::net_transport::{GossipConfig, TcpSync, UdpGossip};
use prime_chain::network::Message as VoteMessage;
use anyhow::Result;
use revm::primitives::{Address, B256, Bytes, U256};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tracing::{info, warn};

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
pub struct WireBlock {
    pub number: u64,
    pub chain_id: u64,
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
    pub transactions: Vec<WireTx>,
    pub receipts: Vec<WireReceipt>,
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
    WireTx {
        from: hex_addr(&tx.from),
        to: tx.to.as_ref().map(hex_addr),
        value: hex_u256(&tx.value),
        data: hex_bytes(&tx.data),
        gas_limit: tx.gas_limit,
        gas_price: hex_u256(&tx.gas_price),
        nonce: tx.nonce,
        chain_id: tx.chain_id,
    }
}

pub fn wire_to_tx(wire: &WireTx) -> Option<Transaction> {
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
        signature: None,
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
        transactions: block.transactions.iter().map(tx_to_wire).collect(),
        receipts: block.receipts.iter().map(receipt_to_wire).collect(),
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

    Some(Block {
        number: wire.number,
        chain_id: wire.chain_id,
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
        rewards: Vec::new(),
        transactions: txs?,
        receipts: receipts?,
        bridge_orders_to_evm: Vec::new(),
        bridge_evm_to_orders: Vec::new(),
        domain_events: Vec::new(),
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
        let mut gossip =
            UdpGossip::bind_with_config(&config.listen_addr, config.clone())?;
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
                                if let Ok(wire) =
                                    serde_json::from_slice::<WireBlock>(&packet.data)
                                {
                                    if let Some(block) = wire_to_block(&wire) {
                                        info!(
                                            number = block.number,
                                            "received block from network"
                                        );
                                        if let Ok(mut eng) = engine.lock() {
                                            eng.import_block(block);
                                        }
                                    }
                                }
                            }
                            "tx" => {
                                if let Ok(wire) =
                                    serde_json::from_slice::<WireTx>(&packet.data)
                                {
                                    if let Some(tx) = wire_to_tx(&wire) {
                                        if let Ok(mut eng) = engine.lock() {
                                            let _ = eng.submit_tx(tx);
                                        }
                                    }
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
                            if let Ok(Some(request)) = TcpSync::recv_packet(&mut stream) {
                                if request.topic == "sync_request" {
                                    if let Ok(eng) = engine.lock() {
                                        let height = eng.latest_height();
                                        let blocks: Vec<WireBlock> = (1..=height)
                                            .filter_map(|n| eng.block_by_number(n))
                                            .map(block_to_wire)
                                            .collect();
                                        let data =
                                            serde_json::to_vec(&blocks).unwrap_or_default();
                                        let gossip_pkt = crate::net_transport::GossipPacket {
                                            topic: "sync_response".to_string(),
                                            data,
                                            id: String::new(),
                                            ttl: 0,
                                        };
                                        let _ =
                                            TcpSync::send_packet(&mut stream, &gossip_pkt);
                                    }
                                }
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
            std::thread::Builder::new()
                .name("peer-discovery".into())
                .spawn(move || {
                    info!("peer discovery loop started");
                    while running.load(Ordering::SeqCst) {
                        std::thread::sleep(Duration::from_secs(30));
                        if let Ok(mut g) = gossip.lock() {
                            let _ = g.discover_peers(2);
                            g.prune_peers();
                            info!(peers = g.peer_count(), "peer discovery tick");
                        }
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
    if let Some(colon) = udp_addr.rfind(':') {
        if let Ok(port) = udp_addr[colon + 1..].parse::<u16>() {
            let host = &udp_addr[..colon];
            return format!("{host}:{}", port.wrapping_add(1000));
        }
    }
    format!("{udp_addr}_tcp")
}
