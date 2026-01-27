#![allow(dead_code)]

use revm::primitives::{Address, U256};
use std::collections::{BTreeMap, HashMap, VecDeque};
use crate::errors::PrimeOrdersError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MarketId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OrderId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Buy,
    Sell,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeInForce {
    Gtc,
    Ioc,
    Fok,
}

#[derive(Debug, Clone)]
pub struct Market {
    pub id: MarketId,
    pub symbol: String,
    pub tick_size: U256,
    pub lot_size: U256,
    pub last_price: U256,
}

#[derive(Debug, Clone)]
pub struct Order {
    pub id: OrderId,
    pub owner: Address,
    pub market: MarketId,
    pub side: Side,
    pub price: U256,
    pub size: U256,
    pub tif: TimeInForce,
}

#[derive(Debug, Clone)]
pub struct Trade {
    pub taker: Address,
    pub maker: Address,
    pub market: MarketId,
    pub side: Side,
    pub price: U256,
    pub size: U256,
}

#[derive(Debug, Clone)]
pub struct OrderOutcome {
    pub order_id: Option<OrderId>,
    pub filled: U256,
    pub remaining: U256,
    pub trades: Vec<Trade>,
}

#[derive(Debug, Clone)]
pub struct OrderBookLevel {
    pub price: U256,
    pub size: U256,
}

#[derive(Debug, Clone)]
pub struct OrderBookView {
    pub bids: Vec<OrderBookLevel>,
    pub asks: Vec<OrderBookLevel>,
}

#[derive(Debug, Clone, Default)]
pub struct Position {
    pub size: i128,
    pub entry_price: U256,
    pub realized_pnl: i128,
}

#[derive(Debug, Clone, Default)]
pub struct AccountState {
    pub collateral: U256,
    pub open_orders: Vec<OrderId>,
    pub positions: HashMap<MarketId, Position>,
}

#[derive(Debug, Default)]
pub struct OrderBook {
    pub bids: BTreeMap<U256, VecDeque<OrderId>>,
    pub asks: BTreeMap<U256, VecDeque<OrderId>>,
}

#[derive(Debug, Default)]
pub struct PrimeOrdersState {
    pub next_order_id: u64,
    pub markets: HashMap<MarketId, Market>,
    pub orders: HashMap<OrderId, Order>,
    pub accounts: HashMap<Address, AccountState>,
    pub books: HashMap<MarketId, OrderBook>,
    pub initial_margin_bps: u64,
    pub maintenance_margin_bps: u64,
}

impl PrimeOrdersState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_market(&mut self, symbol: impl Into<String>, tick_size: U256, lot_size: U256) -> MarketId {
        let id = MarketId(self.markets.len() as u64 + 1);
        let market = Market {
            id,
            symbol: symbol.into(),
            tick_size,
            lot_size,
            last_price: U256::ZERO,
        };
        self.markets.insert(id, market);
        self.books.entry(id).or_default();
        id
    }

    pub fn set_margin_params(&mut self, initial_bps: u64, maintenance_bps: u64) {
        self.initial_margin_bps = initial_bps.min(10_000);
        self.maintenance_margin_bps = maintenance_bps.min(10_000);
    }

    pub fn deposit_collateral(&mut self, owner: Address, amount: U256) {
        let account = self.accounts.entry(owner).or_default();
        account.collateral = account.collateral.saturating_add(amount);
    }

    pub fn place_order(
        &mut self,
        owner: Address,
        market: MarketId,
        side: Side,
        price: U256,
        size: U256,
        tif: TimeInForce,
    ) -> OrderId {
        let order_id = OrderId(self.next_order_id + 1);
        self.next_order_id += 1;

        let order = Order {
            id: order_id,
            owner,
            market,
            side,
            price,
            size,
            tif,
        };

        self.orders.insert(order_id, order);
        self.accounts.entry(owner).or_default().open_orders.push(order_id);

        let book = self.books.entry(market).or_default();
        match side {
            Side::Buy => book.bids.entry(price).or_default().push_back(order_id),
            Side::Sell => book.asks.entry(price).or_default().push_back(order_id),
        }

        order_id
    }

    pub fn submit_order(
        &mut self,
        owner: Address,
        market: MarketId,
        side: Side,
        price: U256,
        size: U256,
        tif: TimeInForce,
    ) -> Result<OrderOutcome, PrimeOrdersError> {
        if !self.markets.contains_key(&market) {
            return Err(PrimeOrdersError::UnknownMarket);
        }
        if size.is_zero() {
            return Err(PrimeOrdersError::InvalidSize);
        }

        self.ensure_initial_margin(owner, price, size)?;

        if tif == TimeInForce::Fok {
            let available = self.available_liquidity(market, side, price);
            if available < size {
                return Err(PrimeOrdersError::FokNotFillable);
            }
        }

        let mut remaining = size;
        let mut trades = Vec::new();

        let price_levels = match side {
            Side::Buy => self.matching_asks(market, price),
            Side::Sell => self.matching_bids(market, price),
        };

        let mut book = self.books.remove(&market).unwrap_or_default();
        for level_price in price_levels {
            if remaining.is_zero() {
                break;
            }
            let levels = match side {
                Side::Buy => &mut book.asks,
                Side::Sell => &mut book.bids,
            };

            if let Some(queue) = levels.get_mut(&level_price) {
                let mut idx = 0usize;
                while idx < queue.len() && !remaining.is_zero() {
                    let order_id = queue[idx];
                    let (maker_owner, maker_remaining, fill) = {
                        let Some(maker_order) = self.orders.get_mut(&order_id) else {
                            queue.remove(idx);
                            continue;
                        };

                        let fill = min_u256(remaining, maker_order.size);
                        maker_order.size = maker_order.size.saturating_sub(fill);
                        remaining = remaining.saturating_sub(fill);
                        (maker_order.owner, maker_order.size, fill)
                    };

                    let (buyer, seller) = match side {
                        Side::Buy => (owner, maker_owner),
                        Side::Sell => (maker_owner, owner),
                    };
                    self.apply_fill(market, buyer, seller, fill, level_price);

                    trades.push(Trade {
                        taker: owner,
                        maker: maker_owner,
                        market,
                        side,
                        price: level_price,
                        size: fill,
                    });

                    if maker_remaining.is_zero() {
                        if let Some(removed) = queue.remove(idx) {
                            self.orders.remove(&removed);
                            if let Some(account) = self.accounts.get_mut(&maker_owner) {
                                account.open_orders.retain(|id| *id != removed);
                            }
                        }
                    } else {
                        idx += 1;
                    }
                }

                let _ = queue.is_empty();
            }
        }

        let levels = match side {
            Side::Buy => &mut book.asks,
            Side::Sell => &mut book.bids,
        };
        levels.retain(|_, queue| !queue.is_empty());

        self.books.insert(market, book);

        let filled = size.saturating_sub(remaining);
        let order_id = if remaining.is_zero() {
            None
        } else if tif == TimeInForce::Gtc {
            Some(self.place_order(owner, market, side, price, remaining, tif))
        } else {
            None
        };

        Ok(OrderOutcome {
            order_id,
            filled,
            remaining,
            trades,
        })
    }

    pub fn order_book(&self, market: MarketId) -> Option<OrderBookView> {
        let book = self.books.get(&market)?;
        let bids = self.levels_from_book(&book.bids);
        let asks = self.levels_from_book(&book.asks);
        Some(OrderBookView { bids, asks })
    }

    pub fn open_orders(&self, owner: Address) -> Vec<Order> {
        let Some(account) = self.accounts.get(&owner) else {
            return Vec::new();
        };
        account
            .open_orders
            .iter()
            .filter_map(|id| self.orders.get(id).cloned())
            .collect()
    }

    pub fn positions(&self, owner: Address) -> Vec<(MarketId, Position)> {
        let Some(account) = self.accounts.get(&owner) else {
            return Vec::new();
        };
        account.positions.iter().map(|(k, v)| (*k, v.clone())).collect()
    }

    pub fn is_liquidatable(&self, owner: Address) -> bool {
        let equity = self.account_equity(owner);
        let maintenance = self.maintenance_margin_required(owner);
        equity < u256_to_i128(maintenance)
    }

    pub fn liquidate(&mut self, owner: Address) -> bool {
        if !self.is_liquidatable(owner) {
            return false;
        }
        if let Some(account) = self.accounts.get_mut(&owner) {
            let open_orders = account.open_orders.clone();
            account.open_orders.clear();
            account.positions.clear();
            for order_id in open_orders {
                let _ = self.cancel_order(order_id);
            }
        }
        true
    }

    fn levels_from_book(&self, levels: &BTreeMap<U256, VecDeque<OrderId>>) -> Vec<OrderBookLevel> {
        levels
            .iter()
            .map(|(price, queue)| {
                let mut size = U256::ZERO;
                for order_id in queue {
                    if let Some(order) = self.orders.get(order_id) {
                        size = size.saturating_add(order.size);
                    }
                }
                OrderBookLevel { price: *price, size }
            })
            .collect()
    }

    fn matching_asks(&self, market: MarketId, limit: U256) -> Vec<U256> {
        let Some(book) = self.books.get(&market) else {
            return Vec::new();
        };
        book.asks
            .keys()
            .filter(|price| **price <= limit)
            .copied()
            .collect()
    }

    fn matching_bids(&self, market: MarketId, limit: U256) -> Vec<U256> {
        let Some(book) = self.books.get(&market) else {
            return Vec::new();
        };
        let mut prices: Vec<U256> = book
            .bids
            .keys()
            .filter(|price| **price >= limit)
            .copied()
            .collect();
        prices.sort_by(|a, b| b.cmp(a));
        prices
    }

    fn available_liquidity(&self, market: MarketId, side: Side, limit: U256) -> U256 {
        let Some(book) = self.books.get(&market) else {
            return U256::ZERO;
        };
        let levels = match side {
            Side::Buy => &book.asks,
            Side::Sell => &book.bids,
        };

        let mut total = U256::ZERO;
        for (price, queue) in levels {
            let price_ok = match side {
                Side::Buy => *price <= limit,
                Side::Sell => *price >= limit,
            };
            if !price_ok {
                continue;
            }
            for order_id in queue {
                if let Some(order) = self.orders.get(order_id) {
                    total = total.saturating_add(order.size);
                }
            }
        }
        total
    }

    fn apply_fill(&mut self, market: MarketId, buyer: Address, seller: Address, size: U256, price: U256) {
        if let Some(market_data) = self.markets.get_mut(&market) {
            market_data.last_price = price;
        }
        let buy_pos = self
            .accounts
            .entry(buyer)
            .or_default()
            .positions
            .entry(market)
            .or_default();
        buy_pos.size = buy_pos.size.saturating_add(u256_to_i128(size));
        buy_pos.entry_price = price;

        let sell_pos = self
            .accounts
            .entry(seller)
            .or_default()
            .positions
            .entry(market)
            .or_default();
        sell_pos.size = sell_pos.size.saturating_sub(u256_to_i128(size));
        sell_pos.entry_price = price;
    }
    pub fn cancel_order(&mut self, order_id: OrderId) -> Option<Order> {
        let order = self.orders.remove(&order_id)?;
        if let Some(account) = self.accounts.get_mut(&order.owner) {
            account.open_orders.retain(|id| *id != order_id);
        }
        if let Some(book) = self.books.get_mut(&order.market) {
            let levels = match order.side {
                Side::Buy => &mut book.bids,
                Side::Sell => &mut book.asks,
            };
            if let Some(queue) = levels.get_mut(&order.price) {
                queue.retain(|id| *id != order_id);
                if queue.is_empty() {
                    levels.remove(&order.price);
                }
            }
        }
        Some(order)
    }

    fn ensure_initial_margin(
        &self,
        owner: Address,
        price: U256,
        size: U256,
    ) -> Result<(), PrimeOrdersError> {
        if self.initial_margin_bps == 0 {
            return Ok(());
        }
        let notional = price.saturating_mul(size);
        let required = notional
            .saturating_mul(U256::from(self.initial_margin_bps))
            .checked_div(U256::from(10_000u64))
            .unwrap_or(U256::MAX);
        let collateral = self
            .accounts
            .get(&owner)
            .map(|acct| acct.collateral)
            .unwrap_or_default();
        if collateral < required {
            return Err(PrimeOrdersError::InsufficientCollateral);
        }
        Ok(())
    }

    fn account_equity(&self, owner: Address) -> i128 {
        let Some(account) = self.accounts.get(&owner) else {
            return 0;
        };
        let mut equity = u256_to_i128(account.collateral);
        for (market_id, position) in &account.positions {
            let mark = self.mark_price(*market_id, position.entry_price);
            let pnl = position
                .size
                .saturating_mul(u256_to_i128(mark).saturating_sub(u256_to_i128(position.entry_price)));
            equity = equity.saturating_add(pnl);
        }
        equity
    }

    fn maintenance_margin_required(&self, owner: Address) -> U256 {
        if self.maintenance_margin_bps == 0 {
            return U256::ZERO;
        }
        let Some(account) = self.accounts.get(&owner) else {
            return U256::ZERO;
        };
        let mut total = U256::ZERO;
        for (market_id, position) in &account.positions {
            let mark = self.mark_price(*market_id, position.entry_price);
            let size_abs = abs_i128_to_u256(position.size);
            let notional = mark.saturating_mul(size_abs);
            let required = notional
                .saturating_mul(U256::from(self.maintenance_margin_bps))
                .checked_div(U256::from(10_000u64))
                .unwrap_or(U256::MAX);
            total = total.saturating_add(required);
        }
        total
    }

    fn mark_price(&self, market: MarketId, fallback: U256) -> U256 {
        self.markets
            .get(&market)
            .map(|m| if m.last_price.is_zero() { fallback } else { m.last_price })
            .unwrap_or(fallback)
    }
}

fn min_u256(a: U256, b: U256) -> U256 {
    if a <= b { a } else { b }
}

fn u256_to_i128(value: U256) -> i128 {
    let bytes = value.to_be_bytes::<32>();
    let mut v = [0u8; 16];
    v.copy_from_slice(&bytes[16..]);
    i128::from_be_bytes(v)
}

fn abs_i128_to_u256(value: i128) -> U256 {
    if value >= 0 {
        U256::from(value as u128)
    } else {
        U256::from(value.saturating_abs() as u128)
    }
}
