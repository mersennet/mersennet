use crate::bridge::BridgeMessage;
use crate::prime_orders::{MarketId, OrderId, Side, TimeInForce};
use revm::primitives::{Address, U256};
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DomainEvent {
    PrimeOrders(PrimeOrdersEvent),
    Bridge(BridgeEvent),
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
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            DomainEvent::PrimeOrders(event) => event.kind(),
            DomainEvent::Bridge(event) => event.kind(),
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
