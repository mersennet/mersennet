//! Programmable Market Maker module for Mersennet CLOB.
//!
//! Enables smart contract-like market making via the CLOB precompile with
//! configurable strategies: Grid, Avellaneda-Stoikov, Inventory-based, and TWAP.

use crate::mersennet_orders::Side;
use revm::primitives::{Address, U256};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, thiserror::Error)]
pub enum MMError {
    #[error("market maker already registered")]
    AlreadyRegistered,
    #[error("market maker not found")]
    NotFound,
    #[error("position limit exceeded")]
    PositionLimitExceeded,
    #[error("invalid configuration")]
    InvalidConfig,
}

#[allow(dead_code)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MarketMakerConfig {
    pub owner: Address,
    pub market_id: u64,
    pub strategy: MMStrategy,
    pub max_position: U256,
    pub spread_bps: u64,      // Spread in basis points
    pub order_size: U256,     // Size per order level
    pub num_levels: u32,      // Number of price levels
    pub min_edge_bps: u64,    // Minimum edge to quote
    pub inventory_skew: bool, // Adjust quotes based on inventory
    pub enabled: bool,
}

#[allow(dead_code)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum MMStrategy {
    Grid,           // Fixed grid around mid price
    Avellaneda,     // Avellaneda-Stoikov optimal market making
    InventoryBased, // Adjust spread based on inventory
    TWAP,           // Time-weighted average price execution
}

#[derive(Clone, Debug, Default)]
#[allow(dead_code)]
struct MMState {
    position: i128, // Signed position (positive = long)
    avg_entry_price: u64,
    realized_pnl: i128,
    unrealized_pnl: i128,
    mid_price: u64,
    last_update_block: u64,
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct MMOrder {
    pub side: Side,
    pub price: u64,
    pub size: U256,
    pub level: u32,
}

#[derive(Clone, Debug, Default)]
pub struct MMStats {
    pub total_trades: u64,
    pub total_volume: U256,
    pub total_pnl: i128,
    pub uptime_blocks: u64,
    pub fill_rate: f64,
    pub avg_spread: f64,
}

#[derive(Clone, Debug)]
pub struct MMQuote {
    pub owner: Address,
    pub market_id: u64,
    pub bids: QuoteLevels, // (price, size)
    pub asks: QuoteLevels,
}

type QuoteLevels = Vec<(u64, U256)>;

pub struct MarketMaker {
    config: MarketMakerConfig,
    state: MMState,
    #[allow(dead_code)]
    orders: Vec<MMOrder>,
    stats: MMStats,
}

impl MarketMaker {
    pub fn new(config: MarketMakerConfig) -> Self {
        Self {
            config: config.clone(),
            state: MMState {
                position: 0,
                avg_entry_price: 0,
                realized_pnl: 0,
                unrealized_pnl: 0,
                mid_price: 0,
                last_update_block: 0,
            },
            orders: Vec::new(),
            stats: MMStats {
                total_trades: 0,
                total_volume: U256::ZERO,
                total_pnl: 0,
                uptime_blocks: 0,
                fill_rate: 0.0,
                avg_spread: config.spread_bps as f64 / 100.0,
            },
        }
    }

    pub fn config(&self) -> &MarketMakerConfig {
        &self.config
    }

    pub fn stats(&self) -> &MMStats {
        &self.stats
    }

    fn compute_grid_quotes(&self, mid_price: u64) -> (QuoteLevels, QuoteLevels) {
        let spread_bps = self.config.spread_bps as f64 / 10_000.0;
        let half_spread = (mid_price as f64 * spread_bps / 2.0) as u64;
        let tick = half_spread.max(1) / self.config.num_levels.max(1) as u64;
        let tick = tick.max(1);

        let mut bids = Vec::with_capacity(self.config.num_levels as usize);
        let mut asks = Vec::with_capacity(self.config.num_levels as usize);

        for i in 1..=self.config.num_levels {
            let i = i as u64;
            let bid_price = mid_price.saturating_sub(tick * i);
            let ask_price = mid_price.saturating_add(tick * i);
            if bid_price > 0 {
                bids.push((bid_price, self.config.order_size));
            }
            asks.push((ask_price, self.config.order_size));
        }

        (bids, asks)
    }

    fn compute_avellaneda_quotes(
        &self,
        mid_price: u64,
        volatility: f64,
        inventory: i128,
    ) -> (QuoteLevels, QuoteLevels) {
        let gamma = 0.1; // Risk aversion
        let _k = 1.5; // Order arrival intensity (reserved for future use)
        let sigma = volatility.max(0.0001);
        let inv_pct = inventory as f64 / self.config.max_position.as_limbs()[0] as f64;
        let inv_pct = inv_pct.clamp(-1.0, 1.0);

        let reserve_spread = gamma * sigma * sigma;
        let skew = gamma * sigma * sigma * inv_pct;
        let half_spread_bps =
            (self.config.spread_bps as f64 / 10_000.0 / 2.0) + reserve_spread + skew.abs();
        let half_spread = (mid_price as f64 * half_spread_bps) as u64;
        let tick = half_spread.max(1) / self.config.num_levels.max(1) as u64;
        let tick = tick.max(1);

        let mut bids = Vec::with_capacity(self.config.num_levels as usize);
        let mut asks = Vec::with_capacity(self.config.num_levels as usize);

        for i in 1..=self.config.num_levels {
            let i = i as u64;
            let bid_offset = if inventory > 0 { tick * 2 } else { tick };
            let ask_offset = if inventory < 0 { tick * 2 } else { tick };
            let bid_price = mid_price.saturating_sub(bid_offset * i);
            let ask_price = mid_price.saturating_add(ask_offset * i);
            if bid_price > 0 {
                bids.push((bid_price, self.config.order_size));
            }
            asks.push((ask_price, self.config.order_size));
        }

        (bids, asks)
    }

    fn compute_inventory_quotes(&self, mid_price: u64) -> (QuoteLevels, QuoteLevels) {
        let base_spread = self.config.spread_bps as f64 / 10_000.0;
        let inv = self.state.position as f64;
        let max = self.config.max_position.as_limbs()[0] as f64;
        let skew = if max > 0.0 {
            (inv / max).clamp(-1.0, 1.0) * 0.5
        } else {
            0.0
        };

        let bid_spread = if self.config.inventory_skew {
            base_spread * (1.0 - skew)
        } else {
            base_spread
        };
        let ask_spread = if self.config.inventory_skew {
            base_spread * (1.0 + skew)
        } else {
            base_spread
        };

        let half_bid = (mid_price as f64 * bid_spread / 2.0) as u64;
        let half_ask = (mid_price as f64 * ask_spread / 2.0) as u64;
        let tick_bid = half_bid.max(1) / self.config.num_levels.max(1) as u64;
        let tick_ask = half_ask.max(1) / self.config.num_levels.max(1) as u64;
        let tick_bid = tick_bid.max(1);
        let tick_ask = tick_ask.max(1);

        let mut bids = Vec::with_capacity(self.config.num_levels as usize);
        let mut asks = Vec::with_capacity(self.config.num_levels as usize);

        for i in 1..=self.config.num_levels {
            let i = i as u64;
            let bid_price = mid_price.saturating_sub(tick_bid * i);
            let ask_price = mid_price.saturating_add(tick_ask * i);
            if bid_price > 0 {
                bids.push((bid_price, self.config.order_size));
            }
            asks.push((ask_price, self.config.order_size));
        }

        (bids, asks)
    }

    fn compute_quotes(&self, mid_price: u64, volatility: f64) -> (QuoteLevels, QuoteLevels) {
        match &self.config.strategy {
            MMStrategy::Grid => self.compute_grid_quotes(mid_price),
            MMStrategy::Avellaneda => {
                self.compute_avellaneda_quotes(mid_price, volatility, self.state.position)
            }
            MMStrategy::InventoryBased => self.compute_inventory_quotes(mid_price),
            MMStrategy::TWAP => self.compute_grid_quotes(mid_price),
        }
    }
}

pub struct MarketMakerEngine {
    makers: HashMap<(Address, u64), MarketMaker>,
    registry: Vec<MarketMakerConfig>,
}

impl MarketMakerEngine {
    pub fn new() -> Self {
        Self {
            makers: HashMap::new(),
            registry: Vec::new(),
        }
    }

    pub fn register(&mut self, config: MarketMakerConfig) -> Result<(), MMError> {
        let key = (config.owner, config.market_id);
        if self.makers.contains_key(&key) {
            return Err(MMError::AlreadyRegistered);
        }
        if config.order_size.is_zero() || config.num_levels == 0 {
            return Err(MMError::InvalidConfig);
        }
        let mm = MarketMaker::new(config.clone());
        self.registry.push(config);
        self.makers.insert(key, mm);
        Ok(())
    }

    pub fn unregister(&mut self, owner: Address, market_id: u64) -> Result<(), MMError> {
        let key = (owner, market_id);
        self.makers.remove(&key).ok_or(MMError::NotFound)?;
        self.registry
            .retain(|c| c.owner != owner || c.market_id != market_id);
        Ok(())
    }

    pub fn update_mid_price(&mut self, market_id: u64, price: u64) {
        for mm in self.makers.values_mut() {
            if mm.config.market_id == market_id {
                mm.state.mid_price = price;
                mm.state.last_update_block = mm.state.last_update_block.saturating_add(1);
                mm.stats.uptime_blocks = mm.stats.uptime_blocks.saturating_add(1);
            }
        }
    }

    pub fn generate_quotes(&self, market_id: u64) -> Vec<MMQuote> {
        let mut quotes = Vec::new();
        for ((owner, mid), mm) in &self.makers {
            if *mid != market_id || !mm.config.enabled {
                continue;
            }
            let (bids, asks) = mm.compute_quotes(mm.state.mid_price, 0.01);
            quotes.push(MMQuote {
                owner: *owner,
                market_id,
                bids,
                asks,
            });
        }
        quotes
    }

    pub fn on_fill(
        &mut self,
        owner: &Address,
        market_id: u64,
        side: Side,
        _price: u64,
        amount: U256,
    ) {
        let key = (*owner, market_id);
        if let Some(mm) = self.makers.get_mut(&key) {
            let amt = amount.as_limbs()[0] as i128;
            let delta = match side {
                Side::Buy => amt,
                Side::Sell => -amt,
            };
            mm.state.position = mm.state.position.saturating_add(delta);
            mm.stats.total_trades = mm.stats.total_trades.saturating_add(1);
            mm.stats.total_volume = mm.stats.total_volume.saturating_add(amount);

            let max_pos = mm.config.max_position.as_limbs()[0] as i128;
            if mm.state.position.abs() > max_pos {
                mm.config.enabled = false;
            }
        }
    }

    pub fn stats(&self, owner: &Address, market_id: u64) -> Option<&MMStats> {
        self.makers.get(&(*owner, market_id)).map(|mm| mm.stats())
    }

    pub fn active_makers(&self, market_id: u64) -> Vec<&MarketMakerConfig> {
        self.registry
            .iter()
            .filter(|c| c.market_id == market_id && c.enabled)
            .collect()
    }
}

impl std::fmt::Debug for MarketMakerEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MarketMakerEngine")
            .field("makers", &self.makers.len())
            .finish()
    }
}

impl Default for MarketMakerEngine {
    fn default() -> Self {
        Self::new()
    }
}
