//! State sharding for Prime Chain: partition PrimeOrders markets across shards
//! for horizontal throughput scaling.

use anyhow::Result;
use revm::primitives::{Address, B256};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[allow(dead_code)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ShardId(pub u64);

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct Shard {
    pub id: ShardId,
    pub markets: HashSet<u64>,
    pub validators: HashSet<Address>,
    pub state_root: B256,
    pub block_height: u64,
    pub tx_count: u64,
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct ShardConfig {
    pub num_shards: u64,
    pub min_validators_per_shard: usize,
    pub rebalance_interval: u64,
    pub cross_shard_timeout_ms: u64,
}

impl Default for ShardConfig {
    fn default() -> Self {
        Self {
            num_shards: 4,
            min_validators_per_shard: 3,
            rebalance_interval: 1000,
            cross_shard_timeout_ms: 500,
        }
    }
}

/// Cross-shard transaction that requires atomic commit across shards.
#[allow(dead_code)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CrossShardTx {
    pub id: B256,
    pub source_shard: ShardId,
    pub dest_shard: ShardId,
    pub sender: Address,
    pub data: Vec<u8>,
    pub status: CrossShardStatus,
}

#[allow(dead_code)]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum CrossShardStatus {
    Pending,
    Prepared,
    Committed,
    Aborted,
}

#[allow(dead_code)]
pub struct ShardStats {
    pub total_shards: u64,
    pub total_markets: usize,
    pub total_validators: usize,
    pub cross_shard_txs: u64,
    pub cross_shard_committed: u64,
    pub cross_shard_aborted: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum ShardError {
    #[error("shard not found")]
    ShardNotFound,
    #[error("market already assigned")]
    MarketAlreadyAssigned,
    #[error("cross-shard tx not found")]
    CrossShardTxNotFound,
    #[error("invalid status transition")]
    InvalidStatusTransition,
    #[error("insufficient validators")]
    InsufficientValidators,
}

#[allow(dead_code)]
pub struct ShardManager {
    shards: HashMap<u64, Shard>,
    market_to_shard: HashMap<u64, u64>,
    cross_shard_txs: HashMap<B256, CrossShardTx>,
    config: ShardConfig,
    stats: ShardStats,
}

impl ShardManager {
    pub fn new(config: ShardConfig) -> Self {
        let num_shards = config.num_shards.max(1);
        let mut shards = HashMap::new();
        for i in 0..num_shards {
            shards.insert(
                i,
                Shard {
                    id: ShardId(i),
                    markets: HashSet::new(),
                    validators: HashSet::new(),
                    state_root: B256::ZERO,
                    block_height: 0,
                    tx_count: 0,
                },
            );
        }
        Self {
            shards,
            market_to_shard: HashMap::new(),
            cross_shard_txs: HashMap::new(),
            config,
            stats: ShardStats {
                total_shards: num_shards,
                total_markets: 0,
                total_validators: 0,
                cross_shard_txs: 0,
                cross_shard_committed: 0,
                cross_shard_aborted: 0,
            },
        }
    }

    pub fn assign_market(&mut self, market_id: u64) -> Result<ShardId, ShardError> {
        if self.market_to_shard.contains_key(&market_id) {
            return Err(ShardError::MarketAlreadyAssigned);
        }

        let mut least_loaded: Option<(u64, usize)> = None;
        for (shard_id, shard) in &self.shards {
            let load = shard.markets.len();
            if least_loaded
                .map(|(_, l)| load < l)
                .unwrap_or(true)
            {
                least_loaded = Some((*shard_id, load));
            }
        }

        let (shard_id, _) = least_loaded.ok_or(ShardError::ShardNotFound)?;
        self.shards
            .get_mut(&shard_id)
            .unwrap()
            .markets
            .insert(market_id);
        self.market_to_shard.insert(market_id, shard_id);
        self.stats.total_markets += 1;
        Ok(ShardId(shard_id))
    }

    pub fn assign_validator(&mut self, validator: Address, shard_id: u64) -> Result<(), ShardError> {
        let shard = self
            .shards
            .get_mut(&shard_id)
            .ok_or(ShardError::ShardNotFound)?;
        shard.validators.insert(validator);
        self.stats.total_validators = self
            .shards
            .values()
            .map(|s| s.validators.len())
            .sum();
        Ok(())
    }

    pub fn get_shard_for_market(&self, market_id: u64) -> Option<ShardId> {
        self.market_to_shard.get(&market_id).map(|&id| ShardId(id))
    }

    pub fn rebalance(&mut self) {
        let total_markets: usize = self.shards.values().map(|s| s.markets.len()).sum();
        if total_markets == 0 {
            return;
        }
        let target_per_shard = total_markets / self.shards.len();
        let mut overloaded: Vec<_> = self
            .shards
            .iter()
            .filter(|(_, s)| s.markets.len() > target_per_shard)
            .map(|(&id, s)| (id, s.markets.len()))
            .collect();
        let mut underloaded: Vec<_> = self
            .shards
            .iter()
            .filter(|(_, s)| s.markets.len() < target_per_shard)
            .map(|(&id, _)| id)
            .collect();

        overloaded.sort_by(|a, b| b.1.cmp(&a.1));

        for (src_id, _) in overloaded {
            let src = self.shards.get_mut(&src_id).unwrap();
            let to_move: Vec<u64> = src
                .markets
                .iter()
                .take(src.markets.len() - target_per_shard)
                .copied()
                .collect();
            for m in &to_move {
                src.markets.remove(m);
            }
            if let Some(&dst_id) = underloaded.first() {
                let dst = self.shards.get_mut(&dst_id).unwrap();
                for m in &to_move {
                    dst.markets.insert(*m);
                    *self.market_to_shard.get_mut(m).unwrap() = dst_id;
                }
                if dst.markets.len() >= target_per_shard {
                    underloaded.remove(0);
                }
            }
        }
    }

    pub fn prepare_cross_shard(&mut self, tx: CrossShardTx) -> Result<(), ShardError> {
        if tx.status != CrossShardStatus::Pending {
            return Err(ShardError::InvalidStatusTransition);
        }
        let mut prepared = tx;
        prepared.status = CrossShardStatus::Prepared;
        self.cross_shard_txs.insert(prepared.id, prepared);
        self.stats.cross_shard_txs += 1;
        Ok(())
    }

    pub fn commit_cross_shard(&mut self, tx_id: B256) -> Result<(), ShardError> {
        let tx = self
            .cross_shard_txs
            .get_mut(&tx_id)
            .ok_or(ShardError::CrossShardTxNotFound)?;
        if tx.status != CrossShardStatus::Prepared {
            return Err(ShardError::InvalidStatusTransition);
        }
        tx.status = CrossShardStatus::Committed;
        self.stats.cross_shard_committed += 1;
        Ok(())
    }

    pub fn abort_cross_shard(&mut self, tx_id: B256) -> Result<(), ShardError> {
        let tx = self
            .cross_shard_txs
            .get_mut(&tx_id)
            .ok_or(ShardError::CrossShardTxNotFound)?;
        if tx.status != CrossShardStatus::Prepared && tx.status != CrossShardStatus::Pending {
            return Err(ShardError::InvalidStatusTransition);
        }
        tx.status = CrossShardStatus::Aborted;
        self.stats.cross_shard_aborted += 1;
        Ok(())
    }

    pub fn shard_count(&self) -> u64 {
        self.config.num_shards
    }

    pub fn markets_in_shard(&self, shard_id: u64) -> Vec<u64> {
        self.shards
            .get(&shard_id)
            .map(|s| s.markets.iter().copied().collect())
            .unwrap_or_default()
    }

    pub fn stats(&self) -> ShardStats {
        ShardStats {
            total_shards: self.stats.total_shards,
            total_markets: self.stats.total_markets,
            total_validators: self.stats.total_validators,
            cross_shard_txs: self.stats.cross_shard_txs,
            cross_shard_committed: self.stats.cross_shard_committed,
            cross_shard_aborted: self.stats.cross_shard_aborted,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use revm::primitives::Address;

    fn addr(b: u8) -> Address {
        Address::from([b; 20])
    }

    #[test]
    fn test_shard_assignment() {
        let config = ShardConfig {
            num_shards: 4,
            ..Default::default()
        };
        let mut mgr = ShardManager::new(config);
        for i in 0..8 {
            let shard = mgr.assign_market(i).unwrap();
            assert!(shard.0 < 4);
        }
        let stats = mgr.stats();
        assert_eq!(stats.total_markets, 8);
        assert_eq!(stats.total_shards, 4);
    }

    #[test]
    fn test_market_routing() {
        let mut mgr = ShardManager::new(ShardConfig::default());
        mgr.assign_market(1).unwrap();
        mgr.assign_market(2).unwrap();
        let shard1 = mgr.get_shard_for_market(1).unwrap();
        let shard2 = mgr.get_shard_for_market(2).unwrap();
        assert!(shard1.0 < mgr.shard_count());
        assert!(shard2.0 < mgr.shard_count());
        assert!(mgr.get_shard_for_market(99).is_none());
    }

    #[test]
    fn test_cross_shard_2pc() {
        let mut mgr = ShardManager::new(ShardConfig::default());
        let tx = CrossShardTx {
            id: B256::from([1u8; 32]),
            source_shard: ShardId(0),
            dest_shard: ShardId(1),
            sender: addr(1),
            data: vec![1, 2, 3],
            status: CrossShardStatus::Pending,
        };
        mgr.prepare_cross_shard(tx).unwrap();
        mgr.commit_cross_shard(B256::from([1u8; 32])).unwrap();
        let stats = mgr.stats();
        assert_eq!(stats.cross_shard_committed, 1);
    }

    #[test]
    fn test_rebalancing() {
        let config = ShardConfig {
            num_shards: 4,
            ..Default::default()
        };
        let mut mgr = ShardManager::new(config);
        for i in 0..12 {
            mgr.assign_market(i).unwrap();
        }
        let _before: Vec<usize> = (0..4)
            .map(|id| mgr.markets_in_shard(id).len())
            .collect();
        mgr.rebalance();
        let after: Vec<usize> = (0..4)
            .map(|id| mgr.markets_in_shard(id).len())
            .collect();
        assert_eq!(after.iter().sum::<usize>(), 12);
        for market_id in 0..12 {
            let shard = mgr.get_shard_for_market(market_id).unwrap();
            assert!(shard.0 < 4);
        }
    }

    #[test]
    fn test_assign_validator() {
        let mut mgr = ShardManager::new(ShardConfig::default());
        mgr.assign_validator(addr(1), 0).unwrap();
        mgr.assign_validator(addr(2), 0).unwrap();
        assert_eq!(mgr.stats().total_validators, 2);
    }

    #[test]
    fn test_abort_cross_shard() {
        let mut mgr = ShardManager::new(ShardConfig::default());
        let tx = CrossShardTx {
            id: B256::from([2u8; 32]),
            source_shard: ShardId(0),
            dest_shard: ShardId(1),
            sender: addr(1),
            data: vec![],
            status: CrossShardStatus::Pending,
        };
        mgr.prepare_cross_shard(tx).unwrap();
        mgr.abort_cross_shard(B256::from([2u8; 32])).unwrap();
        let stats = mgr.stats();
        assert_eq!(stats.cross_shard_aborted, 1);
    }
}
