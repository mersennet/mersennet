#![allow(dead_code)]

use revm::primitives::{Address, B256, U256, keccak256};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use thiserror::Error;

// -----------------------------------------------------------------------------
// Core Types
// -----------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ChainId {
    PrimeChain,
    Ethereum,
    Arbitrum,
    Optimism,
    Base,
    Custom(u64),
}

impl ChainId {
    #[allow(dead_code)]
    fn as_u64(&self) -> u64 {
        match self {
            ChainId::PrimeChain => 0,
            ChainId::Ethereum => 1,
            ChainId::Arbitrum => 42161,
            ChainId::Optimism => 10,
            ChainId::Base => 8453,
            ChainId::Custom(id) => *id,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum DepositStatus {
    Pending,
    Confirmed,
    Executed,
    Failed,
    Refunded,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BridgeDeposit {
    pub id: B256,
    pub source_chain: ChainId,
    pub dest_chain: ChainId,
    pub sender: Address,
    pub recipient: Address,
    pub token: Address,
    pub amount: U256,
    pub nonce: u64,
    pub block_height: u64,
    pub status: DepositStatus,
    pub proof: Option<Vec<u8>>,
    pub timestamp: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum WithdrawalStatus {
    Initiated,
    ProofGenerated,
    Submitted,
    Finalized,
    Failed,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BridgeWithdrawal {
    pub id: B256,
    pub source_chain: ChainId,
    pub dest_chain: ChainId,
    pub sender: Address,
    pub recipient: Address,
    pub token: Address,
    pub amount: U256,
    pub nonce: u64,
    pub block_height: u64,
    pub status: WithdrawalStatus,
    pub proof: Option<Vec<u8>>,
    pub timestamp: u64,
}

// -----------------------------------------------------------------------------
// Token Config
// -----------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TokenConfig {
    pub address: Address,
    pub symbol: String,
    pub decimals: u8,
    pub min_amount: U256,
    pub max_amount: U256,
    pub daily_limit: U256,
}

// -----------------------------------------------------------------------------
// Bridge Error
// -----------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum BridgeError {
    #[error("unsupported chain: {0:?}")]
    UnsupportedChain(ChainId),
    #[error("unsupported token")]
    UnsupportedToken,
    #[error("amount below minimum")]
    AmountBelowMinimum,
    #[error("amount above maximum")]
    AmountAboveMaximum,
    #[error("daily limit exceeded")]
    DailyLimitExceeded,
    #[error("deposit not found")]
    DepositNotFound,
    #[error("withdrawal not found")]
    WithdrawalNotFound,
    #[error("invalid proof")]
    InvalidProof,
    #[error("unauthorized relayer")]
    UnauthorizedRelayer,
    #[error("invalid status transition")]
    InvalidStatusTransition,
    #[error("insufficient locked balance")]
    InsufficientLockedBalance,
}

// -----------------------------------------------------------------------------
// Bridge Stats
// -----------------------------------------------------------------------------

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct BridgeStats {
    pub total_deposits: u64,
    pub total_withdrawals: u64,
    pub pending_deposits: u64,
    pub pending_withdrawals: u64,
    pub total_value_locked: U256,
    pub supported_chains: usize,
}

// -----------------------------------------------------------------------------
// Cross-Chain Bridge
// -----------------------------------------------------------------------------

pub struct CrossChainBridge {
    deposits: HashMap<B256, BridgeDeposit>,
    withdrawals: HashMap<B256, BridgeWithdrawal>,
    supported_chains: HashSet<ChainId>,
    supported_tokens: HashMap<ChainId, Vec<TokenConfig>>,
    next_nonce: HashMap<ChainId, u64>,
    confirmations_required: HashMap<ChainId, u64>,
    relayers: HashSet<Address>,
    total_locked: HashMap<(ChainId, Address), U256>,
}

fn compute_deposit_id(
    sender: &Address,
    recipient: &Address,
    amount: U256,
    nonce: u64,
    chain_id: u64,
) -> B256 {
    let mut buf = Vec::with_capacity(20 + 20 + 32 + 8 + 8);
    buf.extend_from_slice(sender.as_slice());
    buf.extend_from_slice(recipient.as_slice());
    buf.extend_from_slice(&amount.to_be_bytes::<32>());
    buf.extend_from_slice(&nonce.to_be_bytes());
    buf.extend_from_slice(&chain_id.to_be_bytes());
    keccak256(&buf)
}

impl CrossChainBridge {
    pub fn new() -> Self {
        Self {
            deposits: HashMap::new(),
            withdrawals: HashMap::new(),
            supported_chains: HashSet::new(),
            supported_tokens: HashMap::new(),
            next_nonce: HashMap::new(),
            confirmations_required: HashMap::new(),
            relayers: HashSet::new(),
            total_locked: HashMap::new(),
        }
    }

    pub fn add_supported_chain(&mut self, chain: ChainId, confirmations: u64) {
        self.supported_chains.insert(chain.clone());
        self.confirmations_required.insert(chain, confirmations);
    }

    pub fn add_supported_token(&mut self, chain: ChainId, config: TokenConfig) {
        self.supported_tokens.entry(chain).or_default().push(config);
    }

    pub fn add_relayer(&mut self, address: Address) {
        self.relayers.insert(address);
    }

    pub fn is_relayer(&self, address: &Address) -> bool {
        self.relayers.contains(address)
    }

    fn get_token_config(&self, chain: &ChainId, token: &Address) -> Option<&TokenConfig> {
        self.supported_tokens
            .get(chain)
            .and_then(|configs| configs.iter().find(|c| &c.address == token))
    }

    fn validate_deposit(
        &self,
        source_chain: &ChainId,
        token: &Address,
        amount: U256,
    ) -> Result<(), BridgeError> {
        if !self.supported_chains.contains(source_chain) {
            return Err(BridgeError::UnsupportedChain(source_chain.clone()));
        }
        let config = self
            .get_token_config(source_chain, token)
            .ok_or(BridgeError::UnsupportedToken)?;
        if amount < config.min_amount {
            return Err(BridgeError::AmountBelowMinimum);
        }
        if amount > config.max_amount {
            return Err(BridgeError::AmountAboveMaximum);
        }
        let key = (source_chain.clone(), *token);
        let current = self.total_locked.get(&key).copied().unwrap_or(U256::ZERO);
        if current.saturating_add(amount) > config.daily_limit {
            return Err(BridgeError::DailyLimitExceeded);
        }
        Ok(())
    }

    fn validate_withdrawal(
        &self,
        source_chain: &ChainId,
        token: &Address,
        amount: U256,
    ) -> Result<(), BridgeError> {
        if !self.supported_chains.contains(source_chain) {
            return Err(BridgeError::UnsupportedChain(source_chain.clone()));
        }
        let config = self
            .get_token_config(source_chain, token)
            .ok_or(BridgeError::UnsupportedToken)?;
        if amount < config.min_amount {
            return Err(BridgeError::AmountBelowMinimum);
        }
        if amount > config.max_amount {
            return Err(BridgeError::AmountAboveMaximum);
        }
        let key = (source_chain.clone(), *token);
        let current = self.total_locked.get(&key).copied().unwrap_or(U256::ZERO);
        if current.saturating_add(amount) > config.daily_limit {
            return Err(BridgeError::DailyLimitExceeded);
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn initiate_deposit(
        &mut self,
        source_chain: ChainId,
        sender: Address,
        recipient: Address,
        token: Address,
        amount: U256,
        dest_chain: ChainId,
        block_height: u64,
    ) -> Result<BridgeDeposit, BridgeError> {
        self.validate_deposit(&source_chain, &token, amount)?;
        if !self.supported_chains.contains(&dest_chain) {
            return Err(BridgeError::UnsupportedChain(dest_chain));
        }

        let nonce = self.next_nonce.entry(source_chain.clone()).or_insert(0);
        let n = *nonce;
        *nonce = nonce.saturating_add(1);

        let chain_id = source_chain.as_u64();
        let id = compute_deposit_id(&sender, &recipient, amount, n, chain_id);

        let deposit = BridgeDeposit {
            id,
            source_chain: source_chain.clone(),
            dest_chain,
            sender,
            recipient,
            token,
            amount,
            nonce: n,
            block_height,
            status: DepositStatus::Pending,
            proof: None,
            timestamp: 0,
        };

        let key = (source_chain.clone(), token);
        let locked = self.total_locked.entry(key).or_insert(U256::ZERO);
        *locked = locked.saturating_add(amount);

        self.deposits.insert(id, deposit.clone());
        Ok(deposit)
    }

    pub fn confirm_deposit(
        &mut self,
        id: B256,
        proof: Vec<u8>,
        relayer: Address,
    ) -> Result<(), BridgeError> {
        if !self.is_relayer(&relayer) {
            return Err(BridgeError::UnauthorizedRelayer);
        }
        if proof.is_empty() {
            return Err(BridgeError::InvalidProof);
        }
        let deposit = self
            .deposits
            .get_mut(&id)
            .ok_or(BridgeError::DepositNotFound)?;
        if deposit.status != DepositStatus::Pending {
            return Err(BridgeError::InvalidStatusTransition);
        }
        deposit.status = DepositStatus::Confirmed;
        deposit.proof = Some(proof);
        Ok(())
    }

    pub fn execute_deposit(&mut self, id: B256) -> Result<U256, BridgeError> {
        let deposit = self
            .deposits
            .get_mut(&id)
            .ok_or(BridgeError::DepositNotFound)?;
        if deposit.status != DepositStatus::Confirmed {
            return Err(BridgeError::InvalidStatusTransition);
        }
        let amount = deposit.amount;
        let key = (deposit.source_chain.clone(), deposit.token);
        if let Some(locked) = self.total_locked.get_mut(&key) {
            *locked = locked.saturating_sub(amount);
        }
        deposit.status = DepositStatus::Executed;
        Ok(amount)
    }

    pub fn get_deposit(&self, id: &B256) -> Option<&BridgeDeposit> {
        self.deposits.get(id)
    }

    pub fn initiate_withdrawal(
        &mut self,
        source_chain: ChainId,
        sender: Address,
        recipient: Address,
        token: Address,
        amount: U256,
        dest_chain: ChainId,
    ) -> Result<BridgeWithdrawal, BridgeError> {
        self.validate_withdrawal(&source_chain, &token, amount)?;
        if !self.supported_chains.contains(&dest_chain) {
            return Err(BridgeError::UnsupportedChain(dest_chain));
        }

        let nonce = self.next_nonce.entry(source_chain.clone()).or_insert(0);
        let n = *nonce;
        *nonce = nonce.saturating_add(1);

        let chain_id = source_chain.as_u64();
        let id = compute_deposit_id(&sender, &recipient, amount, n, chain_id);

        let key = (source_chain.clone(), token);
        let locked = self.total_locked.entry(key).or_insert(U256::ZERO);
        *locked = locked.saturating_add(amount);

        let withdrawal = BridgeWithdrawal {
            id,
            source_chain: source_chain.clone(),
            dest_chain,
            sender,
            recipient,
            token,
            amount,
            nonce: n,
            block_height: 0,
            status: WithdrawalStatus::Initiated,
            proof: None,
            timestamp: 0,
        };

        self.withdrawals.insert(id, withdrawal.clone());
        Ok(withdrawal)
    }

    pub fn generate_withdrawal_proof(&mut self, id: B256) -> Result<Vec<u8>, BridgeError> {
        let withdrawal = self
            .withdrawals
            .get_mut(&id)
            .ok_or(BridgeError::WithdrawalNotFound)?;
        if withdrawal.status != WithdrawalStatus::Initiated {
            return Err(BridgeError::InvalidStatusTransition);
        }
        let proof = id.as_slice().to_vec();
        withdrawal.proof = Some(proof.clone());
        withdrawal.status = WithdrawalStatus::ProofGenerated;
        Ok(proof)
    }

    pub fn finalize_withdrawal(&mut self, id: B256) -> Result<(), BridgeError> {
        let withdrawal = self
            .withdrawals
            .get_mut(&id)
            .ok_or(BridgeError::WithdrawalNotFound)?;
        if withdrawal.status != WithdrawalStatus::Submitted
            && withdrawal.status != WithdrawalStatus::ProofGenerated
        {
            return Err(BridgeError::InvalidStatusTransition);
        }
        let amount = withdrawal.amount;
        let key = (withdrawal.source_chain.clone(), withdrawal.token);
        if let Some(locked) = self.total_locked.get_mut(&key) {
            *locked = locked.saturating_sub(amount);
        }
        withdrawal.status = WithdrawalStatus::Finalized;
        Ok(())
    }

    pub fn get_withdrawal(&self, id: &B256) -> Option<&BridgeWithdrawal> {
        self.withdrawals.get(id)
    }

    pub fn pending_deposits(&self, chain: &ChainId) -> Vec<&BridgeDeposit> {
        self.deposits
            .values()
            .filter(|d| &d.source_chain == chain && d.status == DepositStatus::Pending)
            .collect()
    }

    pub fn pending_withdrawals(&self, chain: &ChainId) -> Vec<&BridgeWithdrawal> {
        self.withdrawals
            .values()
            .filter(|w| {
                &w.source_chain == chain
                    && w.status != WithdrawalStatus::Finalized
                    && w.status != WithdrawalStatus::Failed
            })
            .collect()
    }

    pub fn total_locked(&self, chain: &ChainId, token: &Address) -> U256 {
        self.total_locked
            .get(&(chain.clone(), *token))
            .copied()
            .unwrap_or(U256::ZERO)
    }

    pub fn daily_volume(&self, chain: &ChainId, token: &Address) -> U256 {
        self.total_locked(chain, token)
    }

    pub fn stats(&self) -> BridgeStats {
        let pending_deposits = self
            .deposits
            .values()
            .filter(|d| d.status == DepositStatus::Pending)
            .count() as u64;
        let pending_withdrawals = self
            .withdrawals
            .values()
            .filter(|w| {
                w.status != WithdrawalStatus::Finalized && w.status != WithdrawalStatus::Failed
            })
            .count() as u64;
        let total_value_locked = self
            .total_locked
            .values()
            .fold(U256::ZERO, |a, b| a.saturating_add(*b));

        BridgeStats {
            total_deposits: self.deposits.len() as u64,
            total_withdrawals: self.withdrawals.len() as u64,
            pending_deposits,
            pending_withdrawals,
            total_value_locked,
            supported_chains: self.supported_chains.len(),
        }
    }
}

impl Default for CrossChainBridge {
    fn default() -> Self {
        Self::new()
    }
}
