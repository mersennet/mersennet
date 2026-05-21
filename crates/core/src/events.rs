use crate::bridge::BridgeMessage;
use crate::prime_orders::{MarketId, OrderId, Side, TimeInForce};
use revm::primitives::{Address, U256};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DomainEvent {
    PrimeOrders(PrimeOrdersEvent),
    Bridge(BridgeEvent),
    /// Privacy-redesign Phase 4 events: market-level public data
    /// emitted by shielded subsystems. **No account-level fields.**
    Shielded(ShieldedEvent),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PrimeOrdersEvent {
    MarketAdded {
        market_id: MarketId,
        symbol: String,
        tick_size: U256,
        lot_size: U256,
    },
    OrderSubmitted {
        order_id: Option<OrderId>,
        owner: Address,
        market_id: MarketId,
        side: Side,
        price: U256,
        size: U256,
        tif: TimeInForce,
        filled: U256,
        remaining: U256,
    },
    OrderCancelled {
        order_id: OrderId,
        owner: Address,
        market_id: MarketId,
    },
    Trade {
        taker: Address,
        maker: Address,
        market_id: MarketId,
        side: Side,
        price: U256,
        size: U256,
    },
    MarginParamsUpdated {
        initial_bps: u64,
        maintenance_bps: u64,
    },
    CollateralDeposited {
        owner: Address,
        amount: U256,
    },
    Liquidation {
        owner: Address,
        liquidated: bool,
    },
}

/// Public, market-aggregate-only events emitted by the shielded
/// subsystems. CRITICAL: every field here MUST be aggregate or
/// scrubbed. Address fields, traders' positions, or anything that
/// re-links a trader to a market are forbidden. The CI privacy-grep
/// check (Workstream K2) enforces this.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ShieldedEvent {
    /// One discrete-time uniform-price auction cleared for `market_id`.
    FbaCleared {
        market_id: MarketId,
        clearing_price: U256,
        matched_size: U256,
        /// Number of intents that participated (aggregate count, no
        /// per-intent metadata).
        intent_count: u64,
    },
    /// Threshold-encrypted mempool admitted a batch of intents into
    /// the current block (the encrypted ciphertext list, not the
    /// plaintext).
    MempoolBatchAdmitted {
        block_number: u64,
        intent_count: u64,
    },
    /// Sealed-bid liquidation auction settled for `market_id`. The
    /// winner is identified solely by their `winner_bond_commitment`
    /// (a Poseidon commitment to the liquidator's bond, not their
    /// address).
    LiquidationSettled {
        market_id: MarketId,
        winner_bond_commitment: [u8; 32],
        winning_bid: U256,
    },
    /// New shielded-state root after the block's shielded txs were
    /// applied. Light clients use this to advance.
    ShieldedRootAdvanced {
        block_number: u64,
        new_root: [u8; 32],
        notes_added: u64,
        nullifiers_added: u64,
    },
}

impl ShieldedEvent {
    pub fn kind(&self) -> &'static str {
        match self {
            ShieldedEvent::FbaCleared { .. } => "shielded_fba_cleared",
            ShieldedEvent::MempoolBatchAdmitted { .. } => "shielded_mempool_batch_admitted",
            ShieldedEvent::LiquidationSettled { .. } => "shielded_liquidation_settled",
            ShieldedEvent::ShieldedRootAdvanced { .. } => "shielded_root_advanced",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BridgeEvent {
    Enqueued {
        queue: BridgeQueueKind,
        message: BridgeMessage,
    },
    Dequeued {
        queue: BridgeQueueKind,
        message: BridgeMessage,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum BridgeQueueKind {
    OrdersToEvm,
    EvmToOrders,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainEventRecord {
    pub block_number: u64,
    pub event_index: u64,
    pub event: DomainEvent,
}

impl DomainEvent {
    pub fn domain(&self) -> &'static str {
        match self {
            DomainEvent::PrimeOrders(_) => "primeorders",
            DomainEvent::Bridge(_) => "bridge",
            DomainEvent::Shielded(_) => "shielded",
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            DomainEvent::PrimeOrders(event) => event.kind(),
            DomainEvent::Bridge(event) => event.kind(),
            DomainEvent::Shielded(event) => event.kind(),
        }
    }
}

impl PrimeOrdersEvent {
    pub fn kind(&self) -> &'static str {
        match self {
            PrimeOrdersEvent::MarketAdded { .. } => "market_added",
            PrimeOrdersEvent::OrderSubmitted { .. } => "order_submitted",
            PrimeOrdersEvent::OrderCancelled { .. } => "order_cancelled",
            PrimeOrdersEvent::Trade { .. } => "trade",
            PrimeOrdersEvent::MarginParamsUpdated { .. } => "margin_params_updated",
            PrimeOrdersEvent::CollateralDeposited { .. } => "collateral_deposited",
            PrimeOrdersEvent::Liquidation { .. } => "liquidation",
        }
    }
}

impl BridgeEvent {
    pub fn kind(&self) -> &'static str {
        match self {
            BridgeEvent::Enqueued { .. } => "bridge_enqueued",
            BridgeEvent::Dequeued { .. } => "bridge_dequeued",
        }
    }
}
