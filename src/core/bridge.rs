#![allow(dead_code)]

use revm::primitives::Bytes;
use std::collections::VecDeque;

#[derive(Debug, Clone)]
pub enum BridgeDomain {
    PrimeOrders,
    PrimeEvm,
}

#[derive(Debug, Clone)]
pub struct BridgeMessage {
    pub nonce: u64,
    pub from: BridgeDomain,
    pub to: BridgeDomain,
    pub payload: Bytes,
}

#[derive(Debug, Default)]
pub struct BridgeQueue {
    next_nonce: u64,
    inbound: VecDeque<BridgeMessage>,
    max_len: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct BridgeQueueSnapshot {
    pub next_nonce: u64,
    pub messages: Vec<BridgeMessage>,
}

impl BridgeQueue {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_max_len(&mut self, max_len: Option<usize>) {
        self.max_len = max_len.filter(|value| *value > 0);
        if let Some(limit) = self.max_len {
            while self.inbound.len() > limit {
                self.inbound.pop_front();
            }
        }
    }

    pub fn push(&mut self, from: BridgeDomain, to: BridgeDomain, payload: Bytes) -> BridgeMessage {
        if let Some(limit) = self.max_len {
            if self.inbound.len() >= limit {
                self.inbound.pop_front();
            }
        }
        let nonce = self.next_nonce.saturating_add(1);
        self.next_nonce = nonce;
        let msg = BridgeMessage { nonce, from, to, payload };
        self.inbound.push_back(msg.clone());
        msg
    }

    pub fn pop(&mut self) -> Option<BridgeMessage> {
        self.inbound.pop_front()
    }

    pub fn len(&self) -> usize {
        self.inbound.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inbound.is_empty()
    }

    pub fn snapshot(&self) -> BridgeQueueSnapshot {
        BridgeQueueSnapshot {
            next_nonce: self.next_nonce,
            messages: self.inbound.iter().cloned().collect(),
        }
    }

    pub fn restore(&mut self, snapshot: BridgeQueueSnapshot) {
        self.next_nonce = snapshot.next_nonce;
        self.inbound = snapshot.messages.into();
    }
}
