use revm::primitives::{Address, U256};
use std::collections::{BTreeMap, HashMap};
use thiserror::Error;
use tracing::debug;

use crate::engine::Transaction;

#[derive(Debug, Clone, Error)]
pub enum TxRejection {
    #[error("duplicate nonce in mempool")]
    DuplicateNonce,
    #[error("mempool full")]
    MempoolFull,
    #[error("sender queue full")]
    SenderQueueFull,
    #[error("fee too low for eviction")]
    FeeTooLow,
    #[error("tx gas limit exceeds block gas limit")]
    GasLimitTooHigh,
    #[error("invalid chain id")]
    InvalidChainId,
    #[error("nonce too low")]
    NonceTooLow,
    #[error("gas price below base fee")]
    GasPriceTooLow,
    #[error("insufficient balance for gas + value")]
    InsufficientBalance,
    #[error("database error")]
    DatabaseError,
    #[error("invalid signature: {0}")]
    InvalidSignature(String),
    #[error("future nonce (queued)")]
    FutureNonce,
}

impl TxRejection {
    pub fn code(&self) -> &'static str {
        match self {
            TxRejection::DuplicateNonce => "duplicate_nonce",
            TxRejection::MempoolFull => "mempool_full",
            TxRejection::SenderQueueFull => "sender_queue_full",
            TxRejection::FeeTooLow => "fee_too_low",
            TxRejection::GasLimitTooHigh => "gas_limit_too_high",
            TxRejection::InvalidChainId => "invalid_chain_id",
            TxRejection::NonceTooLow => "nonce_too_low",
            TxRejection::GasPriceTooLow => "gas_price_too_low",
            TxRejection::InsufficientBalance => "insufficient_balance",
            TxRejection::DatabaseError => "database_error",
            TxRejection::InvalidSignature(_) => "invalid_signature",
            TxRejection::FutureNonce => "future_nonce",
        }
    }
}

#[derive(Debug, Clone)]
pub struct MempoolStats {
    pub pending: usize,
    pub queued: usize,
    pub base_fee: usize,
    pub total: usize,
}

type SenderQueues = HashMap<Address, BTreeMap<u64, Transaction>>;

#[derive(Default, Debug)]
pub struct Mempool {
    pending: SenderQueues,
    queued: SenderQueues,
    base_fee_pool: SenderQueues,
    max_total: usize,
    max_per_sender: usize,
    min_replace_bump_bps: u64,
}

fn pool_len(pool: &SenderQueues) -> usize {
    pool.values().map(|q| q.len()).sum()
}

fn pool_clean(pool: &mut SenderQueues, sender: &Address) {
    if pool.get(sender).is_some_and(|q| q.is_empty()) {
        pool.remove(sender);
    }
}

fn try_replace_in_pool(
    pool: &mut SenderQueues,
    tx: &Transaction,
    bump_bps: u64,
) -> Option<Result<(), TxRejection>> {
    let queue = pool.get_mut(&tx.from)?;
    let existing = queue.get(&tx.nonce)?;
    let required = min_replacement_fee(existing.gas_price, bump_bps);
    if tx.gas_price < required {
        return Some(Err(TxRejection::FeeTooLow));
    }
    let old_price = existing.gas_price;
    queue.insert(tx.nonce, tx.clone());
    debug!(
        sender = %tx.from,
        nonce = tx.nonce,
        old_gas_price = %old_price,
        new_gas_price = %tx.gas_price,
        "transaction replaced"
    );
    Some(Ok(()))
}

impl Mempool {
    fn update_metrics(&self) {
        let stats = self.stats();
        metrics::gauge!("mersennet_mempool_size", stats.total as f64);
        metrics::gauge!("mersennet_mempool_pending", stats.pending as f64);
        metrics::gauge!("mersennet_mempool_queued", stats.queued as f64);
        metrics::gauge!("mersennet_mempool_basefee_pool", stats.base_fee as f64);
    }

    pub fn with_limits(max_total: usize, max_per_sender: usize, min_replace_bump_bps: u64) -> Self {
        Self {
            pending: HashMap::new(),
            queued: HashMap::new(),
            base_fee_pool: HashMap::new(),
            max_total: max_total.max(1),
            max_per_sender: max_per_sender.max(1),
            min_replace_bump_bps: min_replace_bump_bps.min(10_000),
        }
    }

    pub fn update_limits(
        &mut self,
        max_total: usize,
        max_per_sender: usize,
        min_replace_bump_bps: u64,
    ) {
        self.max_total = max_total.max(1);
        self.max_per_sender = max_per_sender.max(1);
        self.min_replace_bump_bps = min_replace_bump_bps.min(10_000);
    }

    pub fn validate(
        &self,
        tx: &Transaction,
        base_fee: U256,
        account_nonce: u64,
        account_balance: U256,
    ) -> Result<(), TxRejection> {
        if tx.nonce < account_nonce {
            return Err(TxRejection::NonceTooLow);
        }
        let gas_cost = U256::from(tx.gas_limit).saturating_mul(tx.gas_price);
        let total_cost = gas_cost.saturating_add(tx.value);
        if account_balance < total_cost {
            return Err(TxRejection::InsufficientBalance);
        }
        if tx.nonce > account_nonce {
            return Err(TxRejection::FutureNonce);
        }
        if tx.gas_price < base_fee {
            return Err(TxRejection::GasPriceTooLow);
        }
        Ok(())
    }

    pub fn insert(
        &mut self,
        tx: Transaction,
        base_fee: U256,
        account_nonce: u64,
    ) -> Result<(), TxRejection> {
        let result = self.try_insert(tx, base_fee, account_nonce);
        if let Err(ref e) = result {
            metrics::increment_counter!("mersennet_mempool_rejected", "reason" => e.code());
            debug!(reason = e.code(), "transaction rejected from mempool");
        }
        result
    }

    fn try_insert(
        &mut self,
        tx: Transaction,
        base_fee: U256,
        account_nonce: u64,
    ) -> Result<(), TxRejection> {
        let bump_bps = self.min_replace_bump_bps;
        if let Some(result) = try_replace_in_pool(&mut self.pending, &tx, bump_bps) {
            result?;
            self.update_metrics();
            return Ok(());
        }
        if let Some(result) = try_replace_in_pool(&mut self.queued, &tx, bump_bps) {
            result?;
            self.update_metrics();
            return Ok(());
        }
        if let Some(result) = try_replace_in_pool(&mut self.base_fee_pool, &tx, bump_bps) {
            result?;
            self.update_metrics();
            return Ok(());
        }

        if self.len() >= self.max_total {
            self.evict_for_fee(&tx)?;
        }

        let sender_count = self.sender_count(tx.from);
        if sender_count >= self.max_per_sender {
            self.evict_sender_for_fee(tx.from, tx.gas_price)?;
        }

        let sender = tx.from;
        let effective_next = self
            .pending
            .get(&sender)
            .and_then(|q| q.keys().last())
            .map(|n| n + 1)
            .map(|n| n.max(account_nonce))
            .unwrap_or(account_nonce);

        if tx.nonce > effective_next {
            self.queued.entry(tx.from).or_default().insert(tx.nonce, tx);
        } else if tx.gas_price < base_fee {
            self.base_fee_pool
                .entry(tx.from)
                .or_default()
                .insert(tx.nonce, tx);
        } else {
            self.pending
                .entry(tx.from)
                .or_default()
                .insert(tx.nonce, tx);
            self.fill_gaps(sender);
        }

        self.update_metrics();
        Ok(())
    }

    fn sender_count(&self, sender: Address) -> usize {
        let p = self.pending.get(&sender).map_or(0, |q| q.len());
        let q = self.queued.get(&sender).map_or(0, |q| q.len());
        let b = self.base_fee_pool.get(&sender).map_or(0, |q| q.len());
        p + q + b
    }

    pub fn fill_gaps(&mut self, sender: Address) {
        let next_expected = self
            .pending
            .get(&sender)
            .and_then(|q| q.keys().last())
            .map(|n| n + 1);

        let Some(mut next) = next_expected else {
            return;
        };

        loop {
            let tx = self.queued.get_mut(&sender).and_then(|q| q.remove(&next));
            match tx {
                Some(tx) => {
                    self.pending.entry(sender).or_default().insert(next, tx);
                    next += 1;
                }
                None => break,
            }
        }

        pool_clean(&mut self.queued, &sender);
    }

    pub fn promote(&mut self, base_fee: U256) {
        let senders: Vec<Address> = self.base_fee_pool.keys().copied().collect();
        for sender in senders {
            let Some(queue) = self.base_fee_pool.get_mut(&sender) else {
                continue;
            };
            let to_promote: Vec<u64> = queue
                .iter()
                .filter(|(_, tx)| tx.gas_price >= base_fee)
                .map(|(&nonce, _)| nonce)
                .collect();
            for nonce in to_promote {
                if let Some(tx) = queue.remove(&nonce) {
                    self.pending.entry(sender).or_default().insert(nonce, tx);
                }
            }
        }
        self.base_fee_pool.retain(|_, q| !q.is_empty());
        self.update_metrics();
    }

    pub fn demote(&mut self, base_fee: U256) {
        let senders: Vec<Address> = self.pending.keys().copied().collect();
        for sender in senders {
            let Some(queue) = self.pending.get_mut(&sender) else {
                continue;
            };
            let to_demote: Vec<u64> = queue
                .iter()
                .filter(|(_, tx)| tx.gas_price < base_fee)
                .map(|(&nonce, _)| nonce)
                .collect();
            for nonce in to_demote {
                if let Some(tx) = queue.remove(&nonce) {
                    self.base_fee_pool
                        .entry(sender)
                        .or_default()
                        .insert(nonce, tx);
                }
            }
        }
        self.pending.retain(|_, q| !q.is_empty());
        self.update_metrics();
    }

    pub fn senders(&self) -> Vec<Address> {
        self.pending.keys().copied().collect()
    }

    pub fn ready_gas(&self, sender: Address, nonce: u64) -> Option<u64> {
        self.pending
            .get(&sender)
            .and_then(|queue| queue.get(&nonce))
            .map(|tx| tx.gas_limit)
    }

    pub fn ready_fee(&self, sender: Address, nonce: u64) -> Option<U256> {
        self.pending
            .get(&sender)
            .and_then(|queue| queue.get(&nonce))
            .map(|tx| tx.gas_price)
    }

    pub fn take_ready(&mut self, sender: Address, nonce: u64) -> Option<Transaction> {
        let tx = self
            .pending
            .get_mut(&sender)
            .and_then(|queue| queue.remove(&nonce));
        pool_clean(&mut self.pending, &sender);
        self.update_metrics();
        tx
    }

    pub fn drain_ready(&mut self, max: usize) -> Vec<Transaction> {
        let mut all: Vec<Transaction> = self
            .pending
            .values()
            .flat_map(|q| q.values().cloned())
            .collect();
        all.sort_by_key(|tx| std::cmp::Reverse(tx.gas_price));
        all.truncate(max);

        for tx in &all {
            if let Some(queue) = self.pending.get_mut(&tx.from) {
                queue.remove(&tx.nonce);
            }
        }
        self.pending.retain(|_, q| !q.is_empty());
        self.update_metrics();
        all
    }

    pub fn remove_mined(&mut self, sender: Address, nonce: u64) {
        if let Some(queue) = self.pending.get_mut(&sender) {
            queue.remove(&nonce);
        }
        pool_clean(&mut self.pending, &sender);

        let mut next = nonce + 1;
        loop {
            let tx = self.queued.get_mut(&sender).and_then(|q| q.remove(&next));
            match tx {
                Some(tx) => {
                    self.pending.entry(sender).or_default().insert(next, tx);
                    next += 1;
                }
                None => break,
            }
        }
        pool_clean(&mut self.queued, &sender);
        self.update_metrics();
    }

    pub fn pending_count(&self) -> usize {
        pool_len(&self.pending)
    }

    pub fn queued_count(&self) -> usize {
        pool_len(&self.queued)
    }

    pub fn base_fee_count(&self) -> usize {
        pool_len(&self.base_fee_pool)
    }

    pub fn stats(&self) -> MempoolStats {
        let pending = self.pending_count();
        let queued = self.queued_count();
        let base_fee = self.base_fee_count();
        MempoolStats {
            pending,
            queued,
            base_fee,
            total: pending + queued + base_fee,
        }
    }

    pub fn len(&self) -> usize {
        pool_len(&self.pending) + pool_len(&self.queued) + pool_len(&self.base_fee_pool)
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty() && self.queued.is_empty() && self.base_fee_pool.is_empty()
    }

    fn evict_sender_for_fee(&mut self, sender: Address, fee: U256) -> Result<(), TxRejection> {
        let mut lowest_fee = U256::MAX;
        let mut lowest_nonce = 0u64;
        let mut lowest_pool: u8 = 0;

        for (pool_idx, pool) in [&self.pending, &self.queued, &self.base_fee_pool]
            .iter()
            .enumerate()
        {
            if let Some(queue) = pool.get(&sender) {
                for (&nonce, tx) in queue {
                    if tx.gas_price < lowest_fee {
                        lowest_fee = tx.gas_price;
                        lowest_nonce = nonce;
                        lowest_pool = pool_idx as u8;
                    }
                }
            }
        }

        if lowest_fee == U256::MAX {
            return Ok(());
        }

        let required = min_replacement_fee(lowest_fee, self.min_replace_bump_bps);
        if fee < required {
            return Err(TxRejection::SenderQueueFull);
        }

        let pool = match lowest_pool {
            0 => &mut self.pending,
            1 => &mut self.queued,
            _ => &mut self.base_fee_pool,
        };
        if let Some(queue) = pool.get_mut(&sender) {
            queue.remove(&lowest_nonce);
        }
        pool_clean(pool, &sender);
        debug!(
            sender = %sender,
            nonce = lowest_nonce,
            old_fee = %lowest_fee,
            new_fee = %fee,
            "evicted sender tx"
        );
        self.update_metrics();
        Ok(())
    }

    fn evict_for_fee(&mut self, tx: &Transaction) -> Result<(), TxRejection> {
        let mut lowest_sender = None;
        let mut lowest_nonce = 0u64;
        let mut lowest_fee = U256::MAX;
        let mut lowest_pool: u8 = 0;

        for (pool_idx, pool) in [&self.pending, &self.queued, &self.base_fee_pool]
            .iter()
            .enumerate()
        {
            for (sender, queue) in pool.iter() {
                for (&nonce, queued_tx) in queue {
                    if queued_tx.gas_price < lowest_fee {
                        lowest_fee = queued_tx.gas_price;
                        lowest_sender = Some(*sender);
                        lowest_nonce = nonce;
                        lowest_pool = pool_idx as u8;
                    }
                }
            }
        }

        if let Some(sender) = lowest_sender {
            let required = min_replacement_fee(lowest_fee, self.min_replace_bump_bps);
            if tx.gas_price < required {
                return Err(TxRejection::FeeTooLow);
            }
            let pool = match lowest_pool {
                0 => &mut self.pending,
                1 => &mut self.queued,
                _ => &mut self.base_fee_pool,
            };
            if let Some(queue) = pool.get_mut(&sender) {
                queue.remove(&lowest_nonce);
            }
            pool_clean(pool, &sender);
            debug!(
                sender = %sender,
                nonce = lowest_nonce,
                old_fee = %lowest_fee,
                new_fee = %tx.gas_price,
                "evicted global tx"
            );
            self.update_metrics();
            return Ok(());
        }

        Err(TxRejection::MempoolFull)
    }
}

fn min_replacement_fee(base_fee: U256, bump_bps: u64) -> U256 {
    let bump = base_fee
        .saturating_mul(U256::from(bump_bps))
        .checked_div(U256::from(10_000u64))
        .unwrap_or(U256::ZERO);
    base_fee.saturating_add(bump)
}
