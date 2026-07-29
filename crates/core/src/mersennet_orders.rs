#![allow(dead_code)]

use crate::errors::MersennetOrdersError;
use revm::primitives::{Address, U256};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, VecDeque};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MarketId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OrderId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Side {
    Buy,
    Sell,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TimeInForce {
    Gtc,
    Ioc,
    Fok,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum MarketStatus {
    #[default]
    Active,
    Halted,
    SettleOnly,
}

#[derive(Debug, Clone)]
pub struct Market {
    pub id: MarketId,
    pub symbol: String,
    pub tick_size: U256,
    pub lot_size: U256,
    pub last_price: U256,
    pub status: MarketStatus,
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

#[derive(Debug, Clone)]
pub struct AdlEvent {
    pub market: MarketId,
    pub deficit: U256,
    pub deleveraged_accounts: Vec<(Address, i128)>,
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

#[derive(Debug, Default, Clone)]
pub struct OrderBook {
    pub bids: BTreeMap<U256, VecDeque<OrderId>>,
    pub asks: BTreeMap<U256, VecDeque<OrderId>>,
}

#[derive(Debug, Default, Clone)]
pub struct MersennetOrdersState {
    pub next_order_id: u64,
    pub markets: HashMap<MarketId, Market>,
    pub orders: HashMap<OrderId, Order>,
    pub accounts: HashMap<Address, AccountState>,
    pub books: HashMap<MarketId, OrderBook>,
    pub initial_margin_bps: u64,
    pub maintenance_margin_bps: u64,
    pub insurance_fund: U256,
    pub insurance_contribution_rate_bps: u64,
}

impl MersennetOrdersState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_market(
        &mut self,
        symbol: impl Into<String>,
        tick_size: U256,
        lot_size: U256,
    ) -> MarketId {
        let id = MarketId(self.markets.len() as u64 + 1);
        let market = Market {
            id,
            symbol: symbol.into(),
            tick_size,
            lot_size,
            last_price: U256::ZERO,
            status: MarketStatus::Active,
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
        self.accounts
            .entry(owner)
            .or_default()
            .open_orders
            .push(order_id);

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
    ) -> Result<OrderOutcome, MersennetOrdersError> {
        match self.markets.get(&market) {
            None => return Err(MersennetOrdersError::UnknownMarket),
            Some(m) if m.status != MarketStatus::Active => {
                return Err(MersennetOrdersError::MarketHalted);
            }
            _ => {}
        }
        if size.is_zero() {
            return Err(MersennetOrdersError::InvalidSize);
        }

        self.ensure_initial_margin(owner, price, size)?;

        if tif == TimeInForce::Fok {
            let available = self.available_liquidity(market, side, price);
            if available < size {
                return Err(MersennetOrdersError::FokNotFillable);
            }
        }

        let mut remaining = size;
        let mut trades = Vec::new();

        let price_levels = match side {
            Side::Buy => self.matching_asks(market, price),
            Side::Sell => self.matching_bids(market, price),
        };

        let mut book = self.books.remove(&market).unwrap_or_default();
        'outer: for level_price in price_levels {
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
                    let (maker_owner, fill) = {
                        let Some(maker_order) = self.orders.get(&order_id) else {
                            queue.remove(idx);
                            continue;
                        };
                        let fill = min_u256(remaining, maker_order.size);
                        (maker_order.owner, fill)
                    };

                    let taker_new_size = self.projected_position_size(owner, market, side, fill);
                    let maker_side = match side {
                        Side::Buy => Side::Sell,
                        Side::Sell => Side::Buy,
                    };
                    let maker_new_size =
                        self.projected_position_size(maker_owner, market, maker_side, fill);

                    if !self.validate_fill_margin(owner, taker_new_size, market)
                        || !self.validate_fill_margin(maker_owner, maker_new_size, market)
                    {
                        break 'outer;
                    }

                    {
                        let maker_order = self.orders.get_mut(&order_id).unwrap();
                        maker_order.size = maker_order.size.saturating_sub(fill);
                    }
                    remaining = remaining.saturating_sub(fill);

                    let maker_remaining = self
                        .orders
                        .get(&order_id)
                        .map(|o| o.size)
                        .unwrap_or(U256::ZERO);

                    let (buyer, seller) = match side {
                        Side::Buy => (owner, maker_owner),
                        Side::Sell => (maker_owner, owner),
                    };
                    self.apply_fill(market, buyer, seller, fill, level_price);

                    let notional = level_price.saturating_mul(fill);
                    let insurance_fee = notional
                        .saturating_mul(U256::from(self.insurance_contribution_rate_bps))
                        .checked_div(U256::from(10_000u64))
                        .unwrap_or(U256::ZERO);
                    self.insurance_fund = self.insurance_fund.saturating_add(insurance_fee);

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

        metrics::increment_counter!("mersennet_orders_submitted");
        if remaining.is_zero() {
            metrics::increment_counter!("mersennet_orders_filled");
        }
        for trade in &trades {
            metrics::increment_counter!("mersennet_trades_executed");
            tracing::info!(
                taker = ?trade.taker,
                maker = ?trade.maker,
                market = trade.market.0,
                price = %trade.price,
                size = %trade.size,
                "trade executed"
            );
        }
        metrics::gauge!(
            "mersennet_insurance_fund_balance",
            self.insurance_fund.as_limbs()[0] as f64
        );
        let active_markets = self
            .markets
            .values()
            .filter(|m| m.status == MarketStatus::Active)
            .count();
        metrics::gauge!("mersennet_markets_active", active_markets as f64);

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
        account
            .positions
            .iter()
            .map(|(k, v)| (*k, v.clone()))
            .collect()
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

        let equity = self.account_equity(owner);
        tracing::warn!(owner = ?owner, equity, "liquidating account");

        if let Some(account) = self.accounts.get_mut(&owner) {
            let open_orders = account.open_orders.clone();
            account.open_orders.clear();
            let positions: Vec<(MarketId, Position)> = account.positions.drain().collect();
            for order_id in open_orders {
                let _ = self.cancel_order(order_id);
            }

            if equity < 0 {
                let deficit = U256::from(equity.unsigned_abs());
                let covered = min_u256(self.insurance_fund, deficit);
                self.insurance_fund = self.insurance_fund.saturating_sub(covered);

                if covered < deficit {
                    let remaining_deficit = deficit.saturating_sub(covered);
                    for (market_id, pos) in &positions {
                        if pos.size != 0 {
                            self.auto_deleverage(*market_id, remaining_deficit);
                        }
                    }
                }
            }
        }
        true
    }

    pub fn auto_deleverage(&mut self, market: MarketId, deficit: U256) -> AdlEvent {
        let mark = self.mark_price(market, U256::ZERO);
        let mut candidates: Vec<(Address, i128, u128)> = Vec::new();

        for (addr, account) in &self.accounts {
            if let Some(pos) = account.positions.get(&market)
                && pos.size != 0
            {
                let pnl = pos.size.saturating_mul(
                    u256_to_i128(mark).saturating_sub(u256_to_i128(pos.entry_price)),
                );
                if pnl > 0 {
                    let abs_size = pos.size.unsigned_abs();
                    candidates.push((
                        *addr,
                        pos.size,
                        (pnl as u128).checked_div(abs_size).unwrap_or(0),
                    ));
                }
            }
        }

        candidates.sort_by_key(|entry| std::cmp::Reverse(entry.2));

        let mut remaining = deficit;
        let mut deleveraged = Vec::new();

        for (addr, position_size, _) in &candidates {
            if remaining.is_zero() {
                break;
            }

            let abs_size = position_size.unsigned_abs();
            let position_notional = mark.saturating_mul(U256::from(abs_size));
            let close_amount = min_u256(remaining, position_notional);
            remaining = remaining.saturating_sub(close_amount);

            let close_size = if !position_notional.is_zero() {
                let ratio = close_amount
                    .saturating_mul(U256::from(abs_size))
                    .checked_div(position_notional)
                    .unwrap_or(U256::ZERO);
                u256_to_i128(ratio).min(abs_size as i128)
            } else {
                0
            };

            if let Some(account) = self.accounts.get_mut(addr)
                && let Some(pos) = account.positions.get_mut(&market)
            {
                if pos.size > 0 {
                    pos.size = pos.size.saturating_sub(close_size);
                } else {
                    pos.size = pos.size.saturating_add(close_size);
                }
                if pos.size == 0 {
                    account.positions.remove(&market);
                }
            }

            deleveraged.push((*addr, close_size));
        }

        AdlEvent {
            market,
            deficit,
            deleveraged_accounts: deleveraged,
        }
    }

    pub fn validate_fill_margin(
        &self,
        account: Address,
        new_position_size: i128,
        market: MarketId,
    ) -> bool {
        if self.maintenance_margin_bps == 0 {
            return true;
        }
        let equity = self.account_equity(account);
        let mark = self.mark_price(market, U256::ZERO);
        let abs_size = abs_i128_to_u256(new_position_size);
        let notional = mark.saturating_mul(abs_size);
        let required = notional
            .saturating_mul(U256::from(self.initial_margin_bps))
            .checked_div(U256::from(10_000u64))
            .unwrap_or(U256::MAX);
        equity >= u256_to_i128(required)
    }

    pub fn insurance_fund_balance(&self) -> U256 {
        self.insurance_fund
    }

    pub fn contribute_to_insurance(&mut self, amount: U256) {
        self.insurance_fund = self.insurance_fund.saturating_add(amount);
    }

    pub fn withdraw_collateral(
        &mut self,
        owner: Address,
        amount: U256,
    ) -> Result<(), MersennetOrdersError> {
        let account = self
            .accounts
            .get(&owner)
            .ok_or(MersennetOrdersError::InsufficientEquity)?;
        if account.collateral < amount {
            return Err(MersennetOrdersError::InsufficientEquity);
        }
        let new_collateral = account.collateral.saturating_sub(amount);
        let equity_after = {
            let mut simulated_equity = u256_to_i128(new_collateral);
            for (market_id, position) in &account.positions {
                let mark = self.mark_price(*market_id, position.entry_price);
                let pnl = position.size.saturating_mul(
                    u256_to_i128(mark).saturating_sub(u256_to_i128(position.entry_price)),
                );
                simulated_equity = simulated_equity.saturating_add(pnl);
            }
            simulated_equity
        };
        let maintenance = self.maintenance_margin_required(owner);
        if equity_after < u256_to_i128(maintenance) {
            return Err(MersennetOrdersError::WithdrawalExceedsEquity);
        }
        self.accounts.get_mut(&owner).unwrap().collateral = new_collateral;
        Ok(())
    }

    pub fn halt_market(&mut self, market: MarketId) {
        if let Some(m) = self.markets.get_mut(&market) {
            m.status = MarketStatus::Halted;
        }
    }

    pub fn resume_market(&mut self, market: MarketId) {
        if let Some(m) = self.markets.get_mut(&market) {
            m.status = MarketStatus::Active;
        }
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
                OrderBookLevel {
                    price: *price,
                    size,
                }
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

    fn apply_fill(
        &mut self,
        market: MarketId,
        buyer: Address,
        seller: Address,
        size: U256,
        price: U256,
    ) {
        if let Some(market_data) = self.markets.get_mut(&market) {
            market_data.last_price = price;
        }
        let fill_i128 = u256_to_i128(size);

        Self::update_position_vwap(
            self.accounts
                .entry(buyer)
                .or_default()
                .positions
                .entry(market)
                .or_default(),
            fill_i128,
            price,
        );

        Self::update_position_vwap(
            self.accounts
                .entry(seller)
                .or_default()
                .positions
                .entry(market)
                .or_default(),
            -fill_i128,
            price,
        );
    }

    fn update_position_vwap(pos: &mut Position, delta: i128, price: U256) {
        let old_size = pos.size;
        let same_direction = (old_size >= 0 && delta > 0) || (old_size < 0 && delta < 0);

        if same_direction || old_size == 0 {
            let abs_old = old_size.unsigned_abs();
            let abs_delta = delta.unsigned_abs();
            let old_notional = U256::from(abs_old).saturating_mul(pos.entry_price);
            let new_notional = U256::from(abs_delta).saturating_mul(price);
            let total_size = abs_old + abs_delta;
            if total_size > 0 {
                pos.entry_price = old_notional
                    .saturating_add(new_notional)
                    .checked_div(U256::from(total_size))
                    .unwrap_or(price);
            }
            pos.size = old_size.saturating_add(delta);
        } else {
            let abs_old = old_size.unsigned_abs();
            let abs_delta = delta.unsigned_abs();

            if abs_delta <= abs_old {
                let sign: i128 = if old_size > 0 { 1 } else { -1 };
                let pnl = sign
                    * (abs_delta as i128)
                    * (u256_to_i128(price) - u256_to_i128(pos.entry_price));
                pos.realized_pnl = pos.realized_pnl.saturating_add(pnl);
                pos.size = old_size.saturating_add(delta);
            } else {
                let sign: i128 = if old_size > 0 { 1 } else { -1 };
                let pnl = sign
                    * (abs_old as i128)
                    * (u256_to_i128(price) - u256_to_i128(pos.entry_price));
                pos.realized_pnl = pos.realized_pnl.saturating_add(pnl);
                pos.size = old_size.saturating_add(delta);
                pos.entry_price = price;
            }
        }
    }

    fn projected_position_size(
        &self,
        account: Address,
        market: MarketId,
        side: Side,
        fill: U256,
    ) -> i128 {
        let current = self
            .accounts
            .get(&account)
            .and_then(|a| a.positions.get(&market))
            .map(|p| p.size)
            .unwrap_or(0);
        let delta = u256_to_i128(fill);
        match side {
            Side::Buy => current.saturating_add(delta),
            Side::Sell => current.saturating_sub(delta),
        }
    }
    /// Cancel an order only if `caller` owns it. Returns `Err` when the order
    /// exists but is owned by someone else (authorization failure), `Ok(None)`
    /// when there is no such order, and `Ok(Some(order))` on success. This is
    /// the path reachable from the precompile / untrusted callers.
    pub fn cancel_order_owned(
        &mut self,
        order_id: OrderId,
        caller: Address,
    ) -> Result<Option<Order>, MersennetOrdersError> {
        match self.orders.get(&order_id) {
            None => Ok(None),
            Some(order) if order.owner != caller => Err(MersennetOrdersError::NotOrderOwner),
            Some(_) => Ok(self.cancel_order(order_id)),
        }
    }

    /// Unconditional cancel — no ownership check. Only for trusted internal
    /// callers (liquidation engine, admin). Untrusted paths must use
    /// [`Self::cancel_order_owned`].
    pub fn cancel_order(&mut self, order_id: OrderId) -> Option<Order> {
        let order = self.orders.remove(&order_id)?;
        metrics::increment_counter!("mersennet_orders_cancelled");
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
    ) -> Result<(), MersennetOrdersError> {
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
            return Err(MersennetOrdersError::InsufficientCollateral);
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
            let pnl = position.size.saturating_mul(
                u256_to_i128(mark).saturating_sub(u256_to_i128(position.entry_price)),
            );
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
            .map(|m| {
                if m.last_price.is_zero() {
                    fallback
                } else {
                    m.last_price
                }
            })
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

#[cfg(test)]
mod tests {
    use super::*;

    fn addr(b: u8) -> Address {
        Address::from_slice(&[b; 20])
    }

    fn market_with_order(owner: Address) -> (MersennetOrdersState, MarketId, OrderId) {
        let mut state = MersennetOrdersState::new();
        let m = state.add_market("TEST/USD", U256::from(1u64), U256::from(1u64));
        // Resting limit order well away from any cross so it stays on the book.
        let id = state.place_order(
            owner,
            m,
            Side::Buy,
            U256::from(10u64),
            U256::from(5u64),
            TimeInForce::Gtc,
        );
        (state, m, id)
    }

    #[test]
    fn cancel_order_owned_rejects_non_owner() {
        let alice = addr(0x11);
        let mallory = addr(0x99);
        let (mut state, _m, id) = market_with_order(alice);

        // Mallory cannot cancel Alice's order.
        let res = state.cancel_order_owned(id, mallory);
        assert!(matches!(res, Err(MersennetOrdersError::NotOrderOwner)));
        // The order is still on the book.
        assert!(state.orders.contains_key(&id));

        // The owner can cancel it.
        let ok = state
            .cancel_order_owned(id, alice)
            .expect("owner cancel ok");
        assert!(ok.is_some());
        assert!(!state.orders.contains_key(&id));
    }

    #[test]
    fn cancel_order_owned_missing_is_ok_none() {
        let mut state = MersennetOrdersState::new();
        let res = state.cancel_order_owned(OrderId(123), addr(0x11));
        assert!(matches!(res, Ok(None)));
    }
}
