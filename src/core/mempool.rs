use revm::primitives::{Address, U256};
use hex;
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
        }
    }
}

#[derive(Default, Debug)]
pub struct Mempool {
    by_sender: HashMap<Address, BTreeMap<u64, Transaction>>,
    max_total: usize,
    max_per_sender: usize,
    min_replace_bump_bps: u64,
}

impl Mempool {
    fn update_metrics(&self) {
        metrics::gauge!("mempool_size_gauge", self.len() as f64);
    }

    pub fn with_limits(max_total: usize, max_per_sender: usize, min_replace_bump_bps: u64) -> Self {
        Self {
            by_sender: HashMap::new(),
            max_total: max_total.max(1),
            max_per_sender: max_per_sender.max(1),
            min_replace_bump_bps: min_replace_bump_bps.min(10_000),
        }
    }

    pub fn update_limits(&mut self, max_total: usize, max_per_sender: usize, min_replace_bump_bps: u64) {
        self.max_total = max_total.max(1);
        self.max_per_sender = max_per_sender.max(1);
        self.min_replace_bump_bps = min_replace_bump_bps.min(10_000);
    }

    pub fn insert(&mut self, tx: Transaction) -> Result<(), TxRejection> {
        if self.len() >= self.max_total {
            self.evict_for_fee(&tx)?;
        }

        let sender_len = self.by_sender.get(&tx.from).map(|q| q.len()).unwrap_or(0);
        if sender_len >= self.max_per_sender {
            self.evict_sender_for_fee(tx.from, tx.gas_price)?;
        }

        let entry = self.by_sender.entry(tx.from).or_default();
        if entry.contains_key(&tx.nonce) {
            return Err(TxRejection::DuplicateNonce);
        }

        entry.insert(tx.nonce, tx);
        self.update_metrics();
        Ok(())
    }

    pub fn senders(&self) -> Vec<Address> {
        self.by_sender.keys().copied().collect()
    }

    pub fn ready_gas(&self, sender: Address, nonce: u64) -> Option<u64> {
        self.by_sender
            .get(&sender)
            .and_then(|queue| queue.get(&nonce))
            .map(|tx| tx.gas_limit)
    }

    pub fn ready_fee(&self, sender: Address, nonce: u64) -> Option<U256> {
        self.by_sender
            .get(&sender)
            .and_then(|queue| queue.get(&nonce))
            .map(|tx| tx.gas_price)
    }

    pub fn take_ready(&mut self, sender: Address, nonce: u64) -> Option<Transaction> {
        let tx = self.by_sender.get_mut(&sender).and_then(|queue| queue.remove(&nonce));
        if let Some(queue) = self.by_sender.get(&sender) {
            if queue.is_empty() {
                self.by_sender.remove(&sender);
            }
        }
        self.update_metrics();
        tx
    }

    pub fn len(&self) -> usize {
        self.by_sender.values().map(|queue| queue.len()).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn evict_sender_for_fee(&mut self, sender: Address, fee: U256) -> Result<(), TxRejection> {
        let Some(queue) = self.by_sender.get_mut(&sender) else {
            return Ok(());
        };

        let Some((&nonce, lowest)) = queue
            .iter()
            .min_by_key(|(_, tx)| tx.gas_price)
        else {
            return Ok(());
        };

        let lowest_fee = lowest.gas_price;

        let required = min_replacement_fee(lowest_fee, self.min_replace_bump_bps);
        if fee < required {
            return Err(TxRejection::SenderQueueFull);
        }

        queue.remove(&nonce);
        self.update_metrics();
        debug!(
            "mempool: evicted sender=0x{} nonce={} fee={} replaced_by={}",
            hex::encode(sender),
            nonce,
            lowest_fee,
            fee
        );
        Ok(())
    }

    fn evict_for_fee(&mut self, tx: &Transaction) -> Result<(), TxRejection> {
        let mut lowest_sender = None;
        let mut lowest_nonce = 0u64;
        let mut lowest_fee = U256::MAX;

        for (sender, queue) in &self.by_sender {
            for (nonce, queued) in queue {
                if queued.gas_price < lowest_fee {
                    lowest_fee = queued.gas_price;
                    lowest_sender = Some(*sender);
                    lowest_nonce = *nonce;
                }
            }
        }

        if let Some(sender) = lowest_sender {
            let required = min_replacement_fee(lowest_fee, self.min_replace_bump_bps);
            if tx.gas_price < required {
                return Err(TxRejection::FeeTooLow);
            }
            if let Some(queue) = self.by_sender.get_mut(&sender) {
                queue.remove(&lowest_nonce);
            }
            self.update_metrics();
            debug!(
                "mempool: evicted sender=0x{} nonce={} fee={} replaced_by={}",
                hex::encode(sender),
                lowest_nonce,
                lowest_fee,
                tx.gas_price
            );
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
