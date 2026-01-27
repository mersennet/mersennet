use crate::engine::{Block, Engine, Transaction};
use crate::network::Message as VoteMessage;
use anyhow::Result;
use std::collections::HashMap;

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
