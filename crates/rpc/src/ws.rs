//! WebSocket subscription system for Prime Chain.
//!
//! Provides real-time event streaming via WebSocket with support for
//! newHeads, newPendingTransactions, logs, and Prime Orders subscriptions.

#![allow(dead_code)]

use revm::primitives::{Address, B256};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::net::TcpListener;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tungstenite::Message;
use tungstenite::accept;

static PRIVACY_MODE_ACTIVATED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

pub fn set_privacy_mode_activated(active: bool) {
    PRIVACY_MODE_ACTIVATED.store(active, Ordering::SeqCst);
}

fn privacy_mode_activated() -> bool {
    PRIVACY_MODE_ACTIVATED.load(Ordering::SeqCst)
}

fn is_transparent_subscription(kind: &SubscriptionKind) -> bool {
    matches!(
        kind,
        SubscriptionKind::NewPendingTransactions
            | SubscriptionKind::Logs { .. }
            | SubscriptionKind::PrimeOrdersTrades { .. }
            | SubscriptionKind::PrimeOrdersBook { .. }
            | SubscriptionKind::BatchAuctionResults { .. }
    )
}

fn transparent_subscription_error(kind: &SubscriptionKind) -> SubscriptionError {
    match kind {
        SubscriptionKind::PrimeOrdersTrades { .. }
        | SubscriptionKind::PrimeOrdersBook { .. }
        | SubscriptionKind::BatchAuctionResults { .. } => {
            SubscriptionError::TransparentPrimeOrdersDisabled
        }
        SubscriptionKind::NewPendingTransactions | SubscriptionKind::Logs { .. } => {
            SubscriptionError::TransparentEthSubscriptionDisabled
        }
        _ => SubscriptionError::TransparentEthSubscriptionDisabled,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubscriptionError {
    TransparentPrimeOrdersDisabled,
    TransparentEthSubscriptionDisabled,
}

/// Unique identifier for a subscription.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SubscriptionId(pub u64);

/// Type of subscription.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SubscriptionKind {
    NewHeads,
    NewPendingTransactions,
    Logs {
        topics: Vec<B256>,
        address: Option<Address>,
    },
    PrimeOrdersTrades {
        market: Option<u64>,
    },
    PrimeOrdersBook {
        market: u64,
    },
    BatchAuctionResults {
        market: Option<u64>,
    },

    // ───── Shielded-mode subscriptions (Workstream C3) ─────
    //
    // Each kind below carries *no* address-identifying data — they
    // describe market-level aggregates only. CI guard K2 enforces
    // this at the source-grep layer.
    /// Fires every block once the shielded note-commitment tree
    /// root advances.
    NewShieldedRoot,
    /// Fires after each frequent-batch-auction tick completes, with
    /// the per-market clearing price + matched size.
    NewClearingPrice {
        market: Option<u64>,
    },
    /// Fires when a liquidation auction settles — the winning bond
    /// commitment + bid are public, the victim isn't.
    NewAuctionSettled {
        market: Option<u64>,
    },
    /// Fires when a fresh `StateTransitionProof` is attached to a
    /// block by the SP1 prover.
    NewStateProof,
}

/// Per-subscriber state.
struct SubscriberInfo {
    kind: SubscriptionKind,
    sender: Sender<String>,
}

/// Manages WebSocket subscriptions and distributes events to subscribers.
pub struct WsSubscriptionManager {
    subscribers: HashMap<SubscriptionId, SubscriberInfo>,
    next_id: AtomicU64,
}

impl WsSubscriptionManager {
    /// Creates a new subscription manager.
    pub fn new() -> Self {
        Self {
            subscribers: HashMap::new(),
            next_id: AtomicU64::new(1),
        }
    }

    /// Creates a new subscription. Returns the subscription ID and a receiver for JSON messages.
    pub fn subscribe(
        &mut self,
        kind: SubscriptionKind,
    ) -> Result<(SubscriptionId, Receiver<String>), SubscriptionError> {
        if privacy_mode_activated() && is_transparent_subscription(&kind) {
            return Err(transparent_subscription_error(&kind));
        }

        let id = SubscriptionId(self.next_id.fetch_add(1, Ordering::SeqCst));
        let (tx, rx) = channel();
        self.subscribers.insert(
            id,
            SubscriberInfo {
                kind: kind.clone(),
                sender: tx,
            },
        );
        Ok((id, rx))
    }

    /// Removes a subscription. Returns true if it existed.
    pub fn unsubscribe(&mut self, id: SubscriptionId) -> bool {
        self.subscribers.remove(&id).is_some()
    }

    /// Sends a block to all NewHeads subscribers. Removes closed channels.
    pub fn notify_new_block(&mut self, block: &Value) {
        self.send_to_matching(|kind| matches!(kind, SubscriptionKind::NewHeads), block);
    }

    /// Sends a tx hash to all NewPendingTransactions subscribers.
    pub fn notify_new_tx(&mut self, tx_hash: &str) {
        let msg = json!({ "hash": tx_hash });
        self.send_to_matching(
            |kind| matches!(kind, SubscriptionKind::NewPendingTransactions),
            &msg,
        );
    }

    /// Sends a log to matching Logs subscribers (by address and topics).
    pub fn notify_log(&mut self, log: &Value, address: Address, topics: &[B256]) {
        let mut to_remove = Vec::new();
        for (id, info) in &self.subscribers {
            if let SubscriptionKind::Logs {
                topics: filter_topics,
                address: filter_addr,
            } = &info.kind
            {
                if let Some(addr) = filter_addr
                    && *addr != address
                {
                    continue;
                }
                if !filter_topics.is_empty() {
                    let matches = filter_topics
                        .iter()
                        .enumerate()
                        .all(|(i, ft)| topics.get(i).map(|t| t == ft).unwrap_or(false));
                    if !matches {
                        continue;
                    }
                }
                if info.sender.send(log.to_string()).is_err() {
                    to_remove.push(*id);
                }
            }
        }
        for id in to_remove {
            self.subscribers.remove(&id);
        }
    }

    /// Sends a trade to PrimeOrdersTrades subscribers (optionally filtered by market).
    pub fn notify_trade(&mut self, trade: &Value, market: u64) {
        self.send_to_matching(
            |kind| match kind {
                SubscriptionKind::PrimeOrdersTrades { market: m } => {
                    m.is_none_or(|mm| mm == market)
                }
                _ => false,
            },
            trade,
        );
    }

    /// Sends a book update to PrimeOrdersBook subscribers for the given market.
    pub fn notify_book_update(&mut self, update: &Value, market: u64) {
        self.send_to_matching(
            |kind| match kind {
                SubscriptionKind::PrimeOrdersBook { market: m } => *m == market,
                _ => false,
            },
            update,
        );
    }

    /// Sends an auction result to BatchAuctionResults subscribers.
    pub fn notify_auction_result(&mut self, result: &Value, market: u64) {
        self.send_to_matching(
            |kind| match kind {
                SubscriptionKind::BatchAuctionResults { market: m } => {
                    m.is_none_or(|mm| mm == market)
                }
                _ => false,
            },
            result,
        );
    }

    // ───── Shielded-mode notifiers (C3) ─────

    /// Notify `newShieldedRoot` subscribers. The `payload` must
    /// contain only block-level / aggregate fields:
    /// `block_number`, `new_root`, `notes_added`, `nullifiers_added`.
    /// CI K2 enforces address-free payloads at the source.
    pub fn notify_shielded_root(&mut self, payload: &Value) {
        self.send_to_matching(
            |kind| matches!(kind, SubscriptionKind::NewShieldedRoot),
            payload,
        );
    }

    /// Notify `newClearingPrice` subscribers. `payload` carries
    /// `market_id`, `clearing_price`, `matched_size`, `intent_count`.
    pub fn notify_clearing_price(&mut self, payload: &Value, market: u64) {
        self.send_to_matching(
            |kind| match kind {
                SubscriptionKind::NewClearingPrice { market: m } => m.is_none_or(|mm| mm == market),
                _ => false,
            },
            payload,
        );
    }

    /// Notify `newAuctionSettled` subscribers. `payload` carries
    /// `market_id`, `winner_bond_commitment`, `winning_bid`.
    pub fn notify_auction_settled(&mut self, payload: &Value, market: u64) {
        self.send_to_matching(
            |kind| match kind {
                SubscriptionKind::NewAuctionSettled { market: m } => {
                    m.is_none_or(|mm| mm == market)
                }
                _ => false,
            },
            payload,
        );
    }

    /// Notify `newStateProof` subscribers. `payload` carries
    /// `block_number`, `state_root`, `proof_bytes_hex`.
    pub fn notify_state_proof(&mut self, payload: &Value) {
        self.send_to_matching(
            |kind| matches!(kind, SubscriptionKind::NewStateProof),
            payload,
        );
    }

    /// Returns the number of active subscriptions.
    pub fn active_count(&self) -> usize {
        self.subscribers.len()
    }

    pub fn purge_transparent_subscriptions(&mut self) {
        self.subscribers
            .retain(|_, info| !is_transparent_subscription(&info.kind));
    }

    fn send_to_matching<F>(&mut self, pred: F, payload: &Value)
    where
        F: Fn(&SubscriptionKind) -> bool,
    {
        let msg = payload.to_string();
        let mut to_remove = Vec::new();
        for (id, info) in &self.subscribers {
            if pred(&info.kind) && info.sender.send(msg.clone()).is_err() {
                to_remove.push(*id);
            }
        }
        for id in to_remove {
            self.subscribers.remove(&id);
        }
    }
}

impl Default for WsSubscriptionManager {
    fn default() -> Self {
        Self::new()
    }
}

/// WebSocket server that accepts connections and handles JSON-RPC subscribe/unsubscribe.
pub struct WsServer {
    manager: Arc<Mutex<WsSubscriptionManager>>,
}

impl WsServer {
    /// Creates a new WebSocket server.
    pub fn new(manager: Arc<Mutex<WsSubscriptionManager>>) -> Self {
        Self { manager }
    }

    /// Starts the WebSocket server on the given address. Blocks the current thread.
    pub fn start(addr: &str, manager: Arc<Mutex<WsSubscriptionManager>>) {
        let listener = TcpListener::bind(addr).expect("failed to bind WebSocket listener");
        tracing::info!("WebSocket server listening on ws://{addr}");

        for stream in listener.incoming().flatten() {
            let manager_clone = Arc::clone(&manager);
            thread::spawn(move || {
                if let Err(e) = handle_connection(stream, manager_clone) {
                    tracing::debug!("WebSocket connection error: {e}");
                }
            });
        }
    }
}

fn handle_connection(
    stream: std::net::TcpStream,
    manager: Arc<Mutex<WsSubscriptionManager>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut ws = accept(stream)?;
    ws.get_ref().set_nonblocking(true)?;
    let mut subscriptions: Vec<(SubscriptionId, Receiver<String>)> = Vec::new();

    loop {
        match ws.read() {
            Ok(Message::Text(text)) => {
                if let Ok((response, id_opt, unsub_id)) = handle_json_rpc(&text, &manager) {
                    if let Some(s) = response {
                        let _ = ws.send(Message::Text(s));
                    }
                    if let Some((id, sub_rx)) = id_opt {
                        subscriptions.push((id, sub_rx));
                    }
                    if let Some(uid) = unsub_id {
                        subscriptions.retain(|(id, _)| *id != uid);
                        let mut m = manager.lock().unwrap();
                        m.unsubscribe(uid);
                    }
                }
            }
            Ok(Message::Close(_)) => break,
            Ok(Message::Ping(data)) => {
                let _ = ws.send(Message::Pong(data));
            }
            Ok(Message::Pong(_)) | Ok(Message::Binary(_)) | Ok(Message::Frame(_)) => {}
            Err(tungstenite::Error::ConnectionClosed) => break,
            Err(tungstenite::Error::Io(ref e)) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(e) => return Err(e.into()),
        }

        let mut i = 0;
        while i < subscriptions.len() {
            let (id, sub_rx) = &subscriptions[i];
            match sub_rx.try_recv() {
                Ok(msg) => {
                    let result = serde_json::from_str::<Value>(&msg).unwrap_or(Value::String(msg));
                    let notification = json!({
                        "jsonrpc": "2.0",
                        "method": "eth_subscription",
                        "params": { "subscription": format!("0x{:x}", id.0), "result": result }
                    });
                    if ws.send(Message::Text(notification.to_string())).is_err() {
                        return Ok(());
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    subscriptions.remove(i);
                    continue;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
            i += 1;
        }

        std::thread::sleep(Duration::from_millis(10));
    }

    Ok(())
}

#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    id: Option<Value>,
    method: String,
    params: Option<Value>,
}

type JsonRpcResponse = Option<String>;
type SubscriptionRegistration = Option<(SubscriptionId, Receiver<String>)>;
type SubscriptionRemoval = Option<SubscriptionId>;

/// Returns (response_json, new_subscription, unsub_id).
fn handle_json_rpc(
    text: &str,
    manager: &Arc<Mutex<WsSubscriptionManager>>,
) -> Result<
    (
        JsonRpcResponse,
        SubscriptionRegistration,
        SubscriptionRemoval,
    ),
    (),
> {
    let req: JsonRpcRequest = serde_json::from_str(text).map_err(|_| ())?;
    let response;
    let mut result_sub = None;
    let mut result_unsub = None;

    match req.method.as_str() {
        "eth_subscribe" => {
            let params = req.params.as_ref().and_then(|p| p.as_array()).ok_or(())?;
            let sub_type = params.first().and_then(|v| v.as_str()).ok_or(())?;
            let kind = parse_eth_subscription(sub_type, params.get(1))?;
            let mut m = manager.lock().unwrap();
            match m.subscribe(kind) {
                Ok((id, rx)) => {
                    response = Some(
                        json!({
                            "jsonrpc": "2.0",
                            "id": req.id,
                            "result": format!("0x{:x}", id.0)
                        })
                        .to_string(),
                    );
                    result_sub = Some((id, rx));
                }
                Err(SubscriptionError::TransparentPrimeOrdersDisabled) => {
                    response = Some(
                        json!({
                            "jsonrpc": "2.0",
                            "id": req.id,
                            "error": {
                                "code": -32605,
                                "message": "transparent PrimeOrders subscriptions disabled after privacy activation"
                            }
                        })
                        .to_string(),
                    );
                }
                Err(SubscriptionError::TransparentEthSubscriptionDisabled) => {
                    response = Some(
                        json!({
                            "jsonrpc": "2.0",
                            "id": req.id,
                            "error": {
                                "code": -32605,
                                "message": "transparent eth subscriptions disabled after privacy activation"
                            }
                        })
                        .to_string(),
                    );
                }
            }
        }
        "eth_unsubscribe" => {
            let params = req.params.as_ref().and_then(|p| p.as_array()).ok_or(())?;
            let id_val = params.first().ok_or(())?;
            let id = parse_subscription_id(id_val)?;
            let mut m = manager.lock().unwrap();
            let ok = m.unsubscribe(id);
            response = Some(
                json!({
                    "jsonrpc": "2.0",
                    "id": req.id,
                    "result": ok
                })
                .to_string(),
            );
            result_unsub = Some(id);
        }
        "prime_subscribe" => {
            let params = req.params.as_ref().and_then(|p| p.as_array()).ok_or(())?;
            let kind = parse_prime_subscription(params)?;
            let mut m = manager.lock().unwrap();
            match m.subscribe(kind) {
                Ok((id, rx)) => {
                    response = Some(
                        json!({
                            "jsonrpc": "2.0",
                            "id": req.id,
                            "result": format!("0x{:x}", id.0)
                        })
                        .to_string(),
                    );
                    result_sub = Some((id, rx));
                }
                Err(SubscriptionError::TransparentPrimeOrdersDisabled) => {
                    response = Some(
                        json!({
                            "jsonrpc": "2.0",
                            "id": req.id,
                            "error": {
                                "code": -32605,
                                "message": "transparent PrimeOrders subscriptions disabled after privacy activation"
                            }
                        })
                        .to_string(),
                    );
                }
                Err(SubscriptionError::TransparentEthSubscriptionDisabled) => {
                    response = Some(
                        json!({
                            "jsonrpc": "2.0",
                            "id": req.id,
                            "error": {
                                "code": -32605,
                                "message": "transparent eth subscriptions disabled after privacy activation"
                            }
                        })
                        .to_string(),
                    );
                }
            }
        }
        "prime_unsubscribe" => {
            let params = req.params.as_ref().and_then(|p| p.as_array()).ok_or(())?;
            let id_val = params.first().ok_or(())?;
            let id = parse_subscription_id(id_val)?;
            let mut m = manager.lock().unwrap();
            let ok = m.unsubscribe(id);
            response = Some(
                json!({
                    "jsonrpc": "2.0",
                    "id": req.id,
                    "result": ok
                })
                .to_string(),
            );
            result_unsub = Some(id);
        }
        other => {
            let err_response = json!({
                "jsonrpc": "2.0",
                "id": req.id,
                "error": { "code": -32601, "message": format!("WS: method not supported: {other}. Use eth_subscribe/eth_unsubscribe for subscriptions.") }
            });
            response = Some(err_response.to_string());
        }
    }

    Ok((response, result_sub, result_unsub))
}

fn parse_eth_subscription(sub_type: &str, filter: Option<&Value>) -> Result<SubscriptionKind, ()> {
    match sub_type {
        "newHeads" => Ok(SubscriptionKind::NewHeads),
        "newPendingTransactions" => Ok(SubscriptionKind::NewPendingTransactions),
        "logs" => {
            let (topics, address) = parse_log_filter(filter);
            Ok(SubscriptionKind::Logs { topics, address })
        }
        _ => Err(()),
    }
}

fn parse_log_filter(filter: Option<&Value>) -> (Vec<B256>, Option<Address>) {
    let mut topics = Vec::new();
    let mut address = None;

    if let Some(f) = filter.and_then(|v| v.as_object()) {
        if let Some(addr) = f.get("address") {
            if let Some(s) = addr.as_str() {
                address = parse_address_opt(s);
            } else if let Some(arr) = addr.as_array()
                && let Some(first) = arr.first().and_then(|v| v.as_str())
            {
                address = parse_address_opt(first);
            }
        }
        if let Some(t) = f.get("topics")
            && let Some(arr) = t.as_array()
        {
            for v in arr {
                if let Some(s) = v.as_str()
                    && let Some(b) = parse_b256(s)
                {
                    topics.push(b);
                }
            }
        }
    }

    (topics, address)
}

fn parse_address_opt(s: &str) -> Option<Address> {
    let stripped = s.strip_prefix("0x").unwrap_or(s);
    let bytes = hex::decode(stripped).ok()?;
    if bytes.len() != 20 {
        return None;
    }
    Some(Address::from_slice(&bytes))
}

fn parse_b256(s: &str) -> Option<B256> {
    let stripped = s.strip_prefix("0x").unwrap_or(s);
    let bytes = hex::decode(stripped).ok()?;
    if bytes.len() != 32 {
        return None;
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    Some(B256::from(arr))
}

fn parse_prime_subscription(params: &[Value]) -> Result<SubscriptionKind, ()> {
    let sub_type = params.first().and_then(|v| v.as_str()).ok_or(())?;
    let market_from_param = |v: &Value| v.as_u64().or_else(|| v.as_str().and_then(parse_hex_u64));
    match sub_type {
        "PrimeOrdersTrades" => {
            let market = params.get(1).and_then(market_from_param);
            Ok(SubscriptionKind::PrimeOrdersTrades { market })
        }
        "PrimeOrdersBook" => {
            let market = params.get(1).and_then(market_from_param).ok_or(())?;
            Ok(SubscriptionKind::PrimeOrdersBook { market })
        }
        "BatchAuctionResults" => {
            let market = params.get(1).and_then(market_from_param);
            Ok(SubscriptionKind::BatchAuctionResults { market })
        }
        // ───── Shielded-mode subscriptions (C3) ─────
        "newShieldedRoot" => Ok(SubscriptionKind::NewShieldedRoot),
        "newClearingPrice" => {
            let market = params.get(1).and_then(market_from_param);
            Ok(SubscriptionKind::NewClearingPrice { market })
        }
        "newAuctionSettled" => {
            let market = params.get(1).and_then(market_from_param);
            Ok(SubscriptionKind::NewAuctionSettled { market })
        }
        "newStateProof" => Ok(SubscriptionKind::NewStateProof),
        _ => Err(()),
    }
}

fn parse_hex_u64(s: &str) -> Option<u64> {
    let stripped = s.strip_prefix("0x").unwrap_or(s);
    u64::from_str_radix(stripped, 16).ok()
}

fn parse_subscription_id(v: &Value) -> Result<SubscriptionId, ()> {
    if let Some(n) = v.as_u64() {
        return Ok(SubscriptionId(n));
    }
    if let Some(s) = v.as_str()
        && let Some(n) = parse_hex_u64(s)
    {
        return Ok(SubscriptionId(n));
    }
    Err(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manager_rejects_transparent_prime_orders_subscriptions_after_privacy_activation() {
        set_privacy_mode_activated(true);

        let mut manager = WsSubscriptionManager::new();
        let err = manager
            .subscribe(SubscriptionKind::PrimeOrdersTrades { market: None })
            .expect_err("transparent subscription should be rejected");
        assert_eq!(err, SubscriptionError::TransparentPrimeOrdersDisabled);

        manager
            .subscribe(SubscriptionKind::NewShieldedRoot)
            .expect("shielded subscription should remain available");

        set_privacy_mode_activated(false);
    }

    #[test]
    fn manager_rejects_transparent_eth_subscriptions_after_privacy_activation() {
        set_privacy_mode_activated(true);

        let mut manager = WsSubscriptionManager::new();
        let pending_err = manager
            .subscribe(SubscriptionKind::NewPendingTransactions)
            .expect_err("pending transactions subscription should be rejected");
        assert_eq!(
            pending_err,
            SubscriptionError::TransparentEthSubscriptionDisabled
        );

        let logs_err = manager
            .subscribe(SubscriptionKind::Logs {
                topics: Vec::new(),
                address: None,
            })
            .expect_err("logs subscription should be rejected");
        assert_eq!(
            logs_err,
            SubscriptionError::TransparentEthSubscriptionDisabled
        );

        manager
            .subscribe(SubscriptionKind::NewHeads)
            .expect("block-level subscription should remain available");

        set_privacy_mode_activated(false);
    }
}
