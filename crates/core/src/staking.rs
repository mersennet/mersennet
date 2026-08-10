//! Delegated staking on top of the fixed validator set.
//!
//! Design (testnet scope): delegation does NOT change the consensus
//! validator set, leader election, or vote weights — those remain a
//! function of validator self-stake. Delegated MRSN earns a pro-rata
//! share of each block reward the validator receives, minus the
//! validator's commission, using the classic accumulated-reward-per-share
//! scheme (MasterChef/F1-lite):
//!
//!   pending(d) = d.amount * pool.acc_reward_per_share / SCALE
//!                - d.reward_debt + d.pending_rewards
//!
//! Principal and delegator rewards are held in the staking precompile's
//! native-balance escrow; the reward split happens deterministically in
//! `Engine::apply_rewards` on both the produce and import paths.
//!
//! This state lives inside `MersennetOrdersState` so it shares the
//! precompile shared-context, snapshot commit/load, and produce/import
//! symmetry that the CLOB already has.

use revm::primitives::{Address, U256};
use std::collections::HashMap;

/// Fixed-point scale for `acc_reward_per_share`.
pub const REWARD_SCALE: u64 = 1_000_000_000_000_000_000;

/// Default validator commission on delegator rewards (10%).
pub const DEFAULT_COMMISSION_BPS: u64 = 1_000;

/// Default unbonding period in blocks (~4h at 2s blocks).
pub const DEFAULT_UNBONDING_BLOCKS: u64 = 7_200;

#[derive(Debug, Clone, Default)]
pub struct ValidatorPool {
    pub delegated_total: U256,
    /// Cumulative reward per delegated wei, scaled by `REWARD_SCALE`.
    pub acc_reward_per_share: U256,
    pub commission_bps: u64,
}

#[derive(Debug, Clone, Default)]
pub struct Delegation {
    pub amount: U256,
    /// `amount * acc_reward_per_share / SCALE` at the last checkpoint.
    pub reward_debt: U256,
    /// Rewards accrued through checkpoints but not yet claimed.
    pub pending_rewards: U256,
}

#[derive(Debug, Clone)]
pub struct UnbondingEntry {
    pub validator: Address,
    pub amount: U256,
    pub unlock_at: u64,
}

#[derive(Debug, Clone)]
pub struct StakingState {
    pub pools: HashMap<Address, ValidatorPool>,
    /// (delegator, validator) -> delegation
    pub delegations: HashMap<(Address, Address), Delegation>,
    pub unbondings: HashMap<Address, Vec<UnbondingEntry>>,
    pub unbonding_period_blocks: u64,
}

impl Default for StakingState {
    fn default() -> Self {
        Self {
            pools: HashMap::new(),
            delegations: HashMap::new(),
            unbondings: HashMap::new(),
            unbonding_period_blocks: DEFAULT_UNBONDING_BLOCKS,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StakingError {
    UnknownValidator,
    ZeroAmount,
    InsufficientDelegation,
}

impl StakingError {
    pub fn message(&self) -> &'static str {
        match self {
            StakingError::UnknownValidator => "unknown validator",
            StakingError::ZeroAmount => "amount must be > 0",
            StakingError::InsufficientDelegation => "insufficient delegated amount",
        }
    }
}

impl StakingState {
    /// Register a validator pool (idempotent). Called by the engine when a
    /// consensus validator is added (genesis included).
    pub fn ensure_pool(&mut self, validator: Address) {
        self.pools.entry(validator).or_insert(ValidatorPool {
            delegated_total: U256::ZERO,
            acc_reward_per_share: U256::ZERO,
            commission_bps: DEFAULT_COMMISSION_BPS,
        });
    }

    fn checkpoint(&mut self, delegator: Address, validator: Address) {
        let acc = match self.pools.get(&validator) {
            Some(p) => p.acc_reward_per_share,
            None => return,
        };
        if let Some(d) = self.delegations.get_mut(&(delegator, validator)) {
            let entitled = d
                .amount
                .saturating_mul(acc)
                .checked_div(U256::from(REWARD_SCALE))
                .unwrap_or(U256::ZERO);
            let newly = entitled.saturating_sub(d.reward_debt);
            d.pending_rewards = d.pending_rewards.saturating_add(newly);
            d.reward_debt = entitled;
        }
    }

    fn resync_debt(&mut self, delegator: Address, validator: Address) {
        let acc = match self.pools.get(&validator) {
            Some(p) => p.acc_reward_per_share,
            None => return,
        };
        if let Some(d) = self.delegations.get_mut(&(delegator, validator)) {
            d.reward_debt = d
                .amount
                .saturating_mul(acc)
                .checked_div(U256::from(REWARD_SCALE))
                .unwrap_or(U256::ZERO);
        }
    }

    pub fn delegate(
        &mut self,
        delegator: Address,
        validator: Address,
        amount: U256,
    ) -> Result<(), StakingError> {
        if amount.is_zero() {
            return Err(StakingError::ZeroAmount);
        }
        if !self.pools.contains_key(&validator) {
            return Err(StakingError::UnknownValidator);
        }
        self.checkpoint(delegator, validator);
        let d = self.delegations.entry((delegator, validator)).or_default();
        d.amount = d.amount.saturating_add(amount);
        self.resync_debt(delegator, validator);
        let pool = self.pools.get_mut(&validator).expect("checked above");
        pool.delegated_total = pool.delegated_total.saturating_add(amount);
        Ok(())
    }

    /// Starts unbonding `amount`. The principal stops earning immediately and
    /// becomes withdrawable at `current_block + unbonding_period_blocks`.
    pub fn undelegate(
        &mut self,
        delegator: Address,
        validator: Address,
        amount: U256,
        current_block: u64,
    ) -> Result<u64, StakingError> {
        if amount.is_zero() {
            return Err(StakingError::ZeroAmount);
        }
        let existing = self
            .delegations
            .get(&(delegator, validator))
            .map(|d| d.amount)
            .unwrap_or(U256::ZERO);
        if existing < amount {
            return Err(StakingError::InsufficientDelegation);
        }
        self.checkpoint(delegator, validator);
        {
            let d = self
                .delegations
                .get_mut(&(delegator, validator))
                .expect("checked above");
            d.amount = d.amount.saturating_sub(amount);
        }
        self.resync_debt(delegator, validator);
        if let Some(pool) = self.pools.get_mut(&validator) {
            pool.delegated_total = pool.delegated_total.saturating_sub(amount);
        }
        let unlock_at = current_block.saturating_add(self.unbonding_period_blocks);
        self.unbondings
            .entry(delegator)
            .or_default()
            .push(UnbondingEntry {
                validator,
                amount,
                unlock_at,
            });
        Ok(unlock_at)
    }

    /// Withdraw every matured unbonding entry. Returns the total amount the
    /// caller must be paid from the staking escrow.
    pub fn withdraw_unbonded(&mut self, delegator: Address, current_block: u64) -> U256 {
        let Some(entries) = self.unbondings.get_mut(&delegator) else {
            return U256::ZERO;
        };
        let mut total = U256::ZERO;
        entries.retain(|e| {
            if e.unlock_at <= current_block {
                total = total.saturating_add(e.amount);
                false
            } else {
                true
            }
        });
        if entries.is_empty() {
            self.unbondings.remove(&delegator);
        }
        total
    }

    /// Claim accrued rewards. Returns the amount to pay from the escrow.
    pub fn claim_rewards(&mut self, delegator: Address, validator: Address) -> U256 {
        self.checkpoint(delegator, validator);
        let Some(d) = self.delegations.get_mut(&(delegator, validator)) else {
            return U256::ZERO;
        };
        let payout = d.pending_rewards;
        d.pending_rewards = U256::ZERO;
        // Drop empty records so state stays bounded.
        if d.amount.is_zero() {
            self.delegations.remove(&(delegator, validator));
        }
        payout
    }

    /// Pending (claimable) rewards without mutating state.
    pub fn pending_rewards(&self, delegator: Address, validator: Address) -> U256 {
        let Some(d) = self.delegations.get(&(delegator, validator)) else {
            return U256::ZERO;
        };
        let acc = self
            .pools
            .get(&validator)
            .map(|p| p.acc_reward_per_share)
            .unwrap_or(U256::ZERO);
        let entitled = d
            .amount
            .saturating_mul(acc)
            .checked_div(U256::from(REWARD_SCALE))
            .unwrap_or(U256::ZERO);
        entitled
            .saturating_sub(d.reward_debt)
            .saturating_add(d.pending_rewards)
    }

    /// Split a block reward earned by `validator`. Returns the delegators'
    /// cut, which the caller credits to the staking escrow; the validator
    /// keeps the remainder. Deterministic given identical state.
    pub fn on_reward(&mut self, validator: Address, reward: U256, self_stake: U256) -> U256 {
        let Some(pool) = self.pools.get_mut(&validator) else {
            return U256::ZERO;
        };
        if pool.delegated_total.is_zero() || reward.is_zero() {
            return U256::ZERO;
        }
        let total_weight = self_stake.saturating_add(pool.delegated_total);
        if total_weight.is_zero() {
            return U256::ZERO;
        }
        let gross = reward
            .saturating_mul(pool.delegated_total)
            .checked_div(total_weight)
            .unwrap_or(U256::ZERO);
        let commission = gross
            .saturating_mul(U256::from(pool.commission_bps))
            .checked_div(U256::from(10_000u64))
            .unwrap_or(U256::ZERO);
        let net = gross.saturating_sub(commission);
        if net.is_zero() {
            return U256::ZERO;
        }
        pool.acc_reward_per_share = pool.acc_reward_per_share.saturating_add(
            net.saturating_mul(U256::from(REWARD_SCALE))
                .checked_div(pool.delegated_total)
                .unwrap_or(U256::ZERO),
        );
        net
    }

    /// Total delegated to a validator (view).
    pub fn delegated_total(&self, validator: Address) -> U256 {
        self.pools
            .get(&validator)
            .map(|p| p.delegated_total)
            .unwrap_or(U256::ZERO)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn addr(n: u8) -> Address {
        Address::from([n; 20])
    }

    #[test]
    fn delegate_accrues_and_claims() {
        let mut s = StakingState::default();
        let val = addr(1);
        let alice = addr(2);
        s.ensure_pool(val);
        s.delegate(alice, val, U256::from(1_000u64)).unwrap();

        // Validator self-stake 1000, delegated 1000 -> delegators get half,
        // minus 10% commission = 45% of the reward.
        let cut = s.on_reward(val, U256::from(1_000u64), U256::from(1_000u64));
        assert_eq!(cut, U256::from(450u64));
        assert_eq!(s.pending_rewards(alice, val), U256::from(450u64));

        let paid = s.claim_rewards(alice, val);
        assert_eq!(paid, U256::from(450u64));
        assert_eq!(s.pending_rewards(alice, val), U256::ZERO);
    }

    #[test]
    fn two_delegators_split_pro_rata() {
        let mut s = StakingState::default();
        let val = addr(1);
        let (a, b) = (addr(2), addr(3));
        s.ensure_pool(val);
        s.delegate(a, val, U256::from(300u64)).unwrap();
        s.delegate(b, val, U256::from(100u64)).unwrap();

        let cut = s.on_reward(val, U256::from(8_000u64), U256::from(400u64));
        // delegated 400 of 800 total -> gross 4000, net 3600
        assert_eq!(cut, U256::from(3_600u64));
        assert_eq!(s.pending_rewards(a, val), U256::from(2_700u64));
        assert_eq!(s.pending_rewards(b, val), U256::from(900u64));
    }

    #[test]
    fn undelegate_unbonds_then_withdraws() {
        let mut s = StakingState::default();
        let val = addr(1);
        let alice = addr(2);
        s.ensure_pool(val);
        s.delegate(alice, val, U256::from(500u64)).unwrap();

        let unlock = s
            .undelegate(alice, val, U256::from(200u64), 100)
            .unwrap();
        assert_eq!(unlock, 100 + DEFAULT_UNBONDING_BLOCKS);
        // Not matured yet.
        assert_eq!(s.withdraw_unbonded(alice, unlock - 1), U256::ZERO);
        assert_eq!(s.withdraw_unbonded(alice, unlock), U256::from(200u64));
        // Remaining principal still earns.
        let cut = s.on_reward(val, U256::from(600u64), U256::from(300u64));
        assert!(cut > U256::ZERO);
        assert!(s.pending_rewards(alice, val) > U256::ZERO);
    }

    #[test]
    fn undelegate_more_than_delegated_fails() {
        let mut s = StakingState::default();
        let val = addr(1);
        s.ensure_pool(val);
        s.delegate(addr(2), val, U256::from(10u64)).unwrap();
        assert_eq!(
            s.undelegate(addr(2), val, U256::from(11u64), 0),
            Err(StakingError::InsufficientDelegation)
        );
    }

    #[test]
    fn delegate_to_unknown_validator_fails() {
        let mut s = StakingState::default();
        assert_eq!(
            s.delegate(addr(2), addr(9), U256::from(10u64)),
            Err(StakingError::UnknownValidator)
        );
    }

    #[test]
    fn late_delegator_gets_no_past_rewards() {
        let mut s = StakingState::default();
        let val = addr(1);
        s.ensure_pool(val);
        s.delegate(addr(2), val, U256::from(100u64)).unwrap();
        s.on_reward(val, U256::from(1_000u64), U256::from(100u64));
        // Bob joins after the reward.
        s.delegate(addr(3), val, U256::from(100u64)).unwrap();
        assert_eq!(s.pending_rewards(addr(3), val), U256::ZERO);
        assert!(s.pending_rewards(addr(2), val) > U256::ZERO);
    }
}
