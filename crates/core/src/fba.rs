#![allow(dead_code)]

use crate::mersennet_orders::{MarketId, MersennetOrdersState, Side, TimeInForce};
use revm::primitives::{Address, U256};
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub struct BatchOrder {
    pub owner: Address,
    pub market: MarketId,
    pub side: Side,
    pub price: U256,
    pub size: U256,
    pub tif: TimeInForce,
    pub sequence: u64,
}

#[derive(Clone, Debug)]
pub struct BatchAuction {
    pub market: MarketId,
    pub orders: Vec<BatchOrder>,
    pub next_sequence: u64,
}

#[derive(Clone, Debug)]
pub struct AuctionResult {
    pub clearing_price: U256,
    pub matched_volume: U256,
    pub fills: Vec<AuctionFill>,
    pub unmatched_buys: Vec<BatchOrder>,
    pub unmatched_sells: Vec<BatchOrder>,
}

#[derive(Clone, Debug)]
pub struct AuctionFill {
    pub buyer: Address,
    pub seller: Address,
    pub price: U256,
    pub size: U256,
    pub market: MarketId,
}

impl BatchAuction {
    pub fn new(market: MarketId) -> Self {
        Self {
            market,
            orders: Vec::new(),
            next_sequence: 0,
        }
    }

    pub fn submit(&mut self, mut order: BatchOrder) {
        order.sequence = self.next_sequence;
        self.next_sequence += 1;
        self.orders.push(order);
    }

    /// Run the FBA: find the clearing price that maximises matched volume,
    /// then fill all eligible orders at that uniform price with pro-rata
    /// allocation when one side is oversubscribed.
    pub fn execute(&mut self) -> AuctionResult {
        let mut buys: Vec<BatchOrder> = self
            .orders
            .iter()
            .filter(|o| o.side == Side::Buy)
            .cloned()
            .collect();
        let mut sells: Vec<BatchOrder> = self
            .orders
            .iter()
            .filter(|o| o.side == Side::Sell)
            .cloned()
            .collect();

        buys.sort_by(|a, b| b.price.cmp(&a.price).then(a.sequence.cmp(&b.sequence)));
        sells.sort_by(|a, b| a.price.cmp(&b.price).then(a.sequence.cmp(&b.sequence)));

        if buys.is_empty() || sells.is_empty() {
            return AuctionResult {
                clearing_price: U256::ZERO,
                matched_volume: U256::ZERO,
                fills: Vec::new(),
                unmatched_buys: buys,
                unmatched_sells: sells,
            };
        }

        // Best ask must be <= best bid for any crossing to occur
        if sells[0].price > buys[0].price {
            return AuctionResult {
                clearing_price: U256::ZERO,
                matched_volume: U256::ZERO,
                fills: Vec::new(),
                unmatched_buys: buys,
                unmatched_sells: sells,
            };
        }

        // Collect all candidate price levels from both sides
        let mut price_levels: Vec<U256> = buys
            .iter()
            .map(|o| o.price)
            .chain(sells.iter().map(|o| o.price))
            .collect();
        price_levels.sort();
        price_levels.dedup();

        // Find the clearing price that maximises matched volume.
        // Among ties, pick the highest price (benefits the sell side,
        // standard FBA convention).
        let mut best_price = U256::ZERO;
        let mut best_volume = U256::ZERO;

        for &candidate in &price_levels {
            let demand: U256 = buys
                .iter()
                .filter(|o| o.price >= candidate)
                .fold(U256::ZERO, |acc, o| acc.saturating_add(o.size));
            let supply: U256 = sells
                .iter()
                .filter(|o| o.price <= candidate)
                .fold(U256::ZERO, |acc, o| acc.saturating_add(o.size));

            let matched = if demand < supply { demand } else { supply };
            if matched > best_volume || (matched == best_volume && candidate > best_price) {
                best_volume = matched;
                best_price = candidate;
            }
        }

        if best_volume.is_zero() {
            return AuctionResult {
                clearing_price: best_price,
                matched_volume: U256::ZERO,
                fills: Vec::new(),
                unmatched_buys: buys,
                unmatched_sells: sells,
            };
        }

        let clearing_price = best_price;

        // Eligible orders at the clearing price
        let eligible_buys: Vec<&BatchOrder> =
            buys.iter().filter(|o| o.price >= clearing_price).collect();
        let eligible_sells: Vec<&BatchOrder> =
            sells.iter().filter(|o| o.price <= clearing_price).collect();

        let total_demand: U256 = eligible_buys
            .iter()
            .fold(U256::ZERO, |acc, o| acc.saturating_add(o.size));
        let total_supply: U256 = eligible_sells
            .iter()
            .fold(U256::ZERO, |acc, o| acc.saturating_add(o.size));

        // Pro-rata allocation: the oversubscribed side gets scaled down
        let buy_fills: Vec<(Address, U256)> = if total_demand > total_supply {
            // Buys are oversubscribed — pro-rata
            eligible_buys
                .iter()
                .map(|o| {
                    let fill = o
                        .size
                        .saturating_mul(best_volume)
                        .checked_div(total_demand)
                        .unwrap_or(U256::ZERO);
                    (o.owner, fill)
                })
                .collect()
        } else {
            eligible_buys.iter().map(|o| (o.owner, o.size)).collect()
        };

        let sell_fills: Vec<(Address, U256)> = if total_supply > total_demand {
            eligible_sells
                .iter()
                .map(|o| {
                    let fill = o
                        .size
                        .saturating_mul(best_volume)
                        .checked_div(total_supply)
                        .unwrap_or(U256::ZERO);
                    (o.owner, fill)
                })
                .collect()
        } else {
            eligible_sells.iter().map(|o| (o.owner, o.size)).collect()
        };

        // Pair buyers with sellers to produce fills
        let mut fills = Vec::new();
        let mut sell_iter = sell_fills.iter().peekable();
        let mut sell_remaining = U256::ZERO;
        let mut current_seller = Address::ZERO;

        for &(buyer, buy_fill_qty) in &buy_fills {
            if buy_fill_qty.is_zero() {
                continue;
            }
            let mut buy_qty = buy_fill_qty;
            while !buy_qty.is_zero() {
                if sell_remaining.is_zero() {
                    match sell_iter.next() {
                        Some(&(seller, qty)) => {
                            current_seller = seller;
                            sell_remaining = qty;
                        }
                        None => break,
                    }
                }
                if sell_remaining.is_zero() {
                    continue;
                }
                let trade_size = if buy_qty < sell_remaining {
                    buy_qty
                } else {
                    sell_remaining
                };
                if !trade_size.is_zero() {
                    fills.push(AuctionFill {
                        buyer,
                        seller: current_seller,
                        price: clearing_price,
                        size: trade_size,
                        market: self.market,
                    });
                }
                buy_qty = buy_qty.saturating_sub(trade_size);
                sell_remaining = sell_remaining.saturating_sub(trade_size);
            }
        }

        let matched_volume = fills
            .iter()
            .fold(U256::ZERO, |acc, f| acc.saturating_add(f.size));

        // Partition remaining orders into unmatched
        let matched_buy_addrs: HashMap<Address, U256> =
            buy_fills
                .iter()
                .fold(HashMap::new(), |mut map, (addr, qty)| {
                    *map.entry(*addr).or_insert(U256::ZERO) =
                        map.get(addr).unwrap_or(&U256::ZERO).saturating_add(*qty);
                    map
                });
        let matched_sell_addrs: HashMap<Address, U256> =
            sell_fills
                .iter()
                .fold(HashMap::new(), |mut map, (addr, qty)| {
                    *map.entry(*addr).or_insert(U256::ZERO) =
                        map.get(addr).unwrap_or(&U256::ZERO).saturating_add(*qty);
                    map
                });

        let mut unmatched_buys = Vec::new();
        for order in &buys {
            let filled = matched_buy_addrs
                .get(&order.owner)
                .copied()
                .unwrap_or(U256::ZERO);
            if order.size > filled {
                let mut remaining = order.clone();
                remaining.size = order.size.saturating_sub(filled);
                unmatched_buys.push(remaining);
            }
        }

        let mut unmatched_sells = Vec::new();
        for order in &sells {
            let filled = matched_sell_addrs
                .get(&order.owner)
                .copied()
                .unwrap_or(U256::ZERO);
            if order.size > filled {
                let mut remaining = order.clone();
                remaining.size = order.size.saturating_sub(filled);
                unmatched_sells.push(remaining);
            }
        }

        AuctionResult {
            clearing_price,
            matched_volume,
            fills,
            unmatched_buys,
            unmatched_sells,
        }
    }

    pub fn clear(&mut self) {
        self.orders.clear();
    }
}

#[derive(Debug, Default)]
pub struct FBAEngine {
    pub auctions: HashMap<MarketId, BatchAuction>,
    pub batch_interval_ms: u64,
    pub current_batch_start: u64,
}

impl FBAEngine {
    pub fn new(batch_interval_ms: u64) -> Self {
        Self {
            auctions: HashMap::new(),
            batch_interval_ms,
            current_batch_start: 0,
        }
    }

    pub fn submit_order(&mut self, order: BatchOrder) {
        let auction = self
            .auctions
            .entry(order.market)
            .or_insert_with(|| BatchAuction::new(order.market));
        auction.submit(order);
    }

    pub fn execute_all(&mut self) -> Vec<AuctionResult> {
        let mut results = Vec::new();
        for auction in self.auctions.values_mut() {
            if auction.orders.is_empty() {
                continue;
            }
            let result = auction.execute();
            auction.clear();
            results.push(result);
        }
        results
    }

    pub fn apply_results(&self, results: &[AuctionResult], state: &mut MersennetOrdersState) {
        for result in results {
            for fill in &result.fills {
                state.deposit_collateral(fill.buyer, U256::ZERO);
                state.deposit_collateral(fill.seller, U256::ZERO);

                let buyer_pos = state
                    .accounts
                    .get(&fill.buyer)
                    .and_then(|a| a.positions.get(&fill.market))
                    .map(|p| p.size)
                    .unwrap_or(0);
                let seller_pos = state
                    .accounts
                    .get(&fill.seller)
                    .and_then(|a| a.positions.get(&fill.market))
                    .map(|p| p.size)
                    .unwrap_or(0);

                let fill_i128 = u256_to_i128(fill.size);

                {
                    let pos = state
                        .accounts
                        .entry(fill.buyer)
                        .or_default()
                        .positions
                        .entry(fill.market)
                        .or_default();
                    pos.size = buyer_pos.saturating_add(fill_i128);
                    pos.entry_price = fill.price;
                }
                {
                    let pos = state
                        .accounts
                        .entry(fill.seller)
                        .or_default()
                        .positions
                        .entry(fill.market)
                        .or_default();
                    pos.size = seller_pos.saturating_sub(fill_i128);
                    pos.entry_price = fill.price;
                }

                if let Some(market) = state.markets.get_mut(&fill.market) {
                    market.last_price = fill.price;
                }
            }
        }
    }
}

fn u256_to_i128(value: U256) -> i128 {
    let bytes = value.to_be_bytes::<32>();
    let mut v = [0u8; 16];
    v.copy_from_slice(&bytes[16..]);
    i128::from_be_bytes(v)
}
