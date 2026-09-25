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
use serde::{Deserialize, Serialize};
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
    /// Open validator set (permissionless registration). Keyed by node
    /// identity (the block-signing address). Persisted with the rest of the
    /// staking ledger, so every node derives the same active set.
    pub registry: HashMap<Address, ValidatorRegistration>,
    /// Identities producing blocks in the current epoch (sorted).
    pub active_set: Vec<Address>,
    pub current_epoch: u64,
    /// Parameters, copied from the canonical config at startup (not a
    /// governance surface yet).
    pub params: ValidatorSetParams,
}

impl Default for StakingState {
    fn default() -> Self {
        Self {
            pools: HashMap::new(),
            delegations: HashMap::new(),
            unbondings: HashMap::new(),
            unbonding_period_blocks: DEFAULT_UNBONDING_BLOCKS,
            registry: HashMap::new(),
            active_set: Vec::new(),
            current_epoch: 0,
            params: ValidatorSetParams::default(),
        }
    }
}

/// Rules of the open validator set. `activation_height == 0` means the
/// feature is off and the genesis set is used unchanged (today's behaviour).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ValidatorSetParams {
    pub activation_height: u64,
    pub epoch_blocks: u64,
    pub min_self_stake: U256,
    pub max_validators: usize,
    pub unbonding_blocks: u64,
    /// Missing more than this share of a validator's leader slots in an
    /// epoch (basis points) jails it for the following epoch. Needs at least
    /// `jail_min_slots` slots in the epoch to be judged at all.
    pub jail_miss_bps: u64,
    pub jail_min_slots: u64,
    /// From this height, a validator's block reward is credited to the
    /// operator wallet in its registration instead of the node identity
    /// (0 = disabled: identity keeps receiving it). Consensus-critical.
    pub rewards_to_operator_height: u64,
    /// From this height, repeat offenders are jailed for 1, 2, 4, 8, 16 then
    /// 24 epochs (`times_jailed` counts *consecutive* jails and resets after
    /// a full epoch served without being jailed). Before it, every jail is
    /// one epoch and `times_jailed` is a lifetime count. 0 = off.
    pub jail_escalation_height: u64,
    /// From this height a validator that has missed at least `BENCH_MISSES`
    /// leader slots in the current epoch, and whose misses are at least a
    /// tenth of what it proposed, is taken out of the leader rotation for
    /// the rest of the epoch (it keeps voting and its stake). A dead leader
    /// then costs the network a few failover rounds instead of an hour of
    /// them. Derived from the per-epoch counters every node keeps from block
    /// data, so it is identical everywhere. 0 = off.
    pub bench_height: u64,
}

/// Missed leader slots in an epoch before a validator is benched.
pub const BENCH_MISSES: u64 = 3;

impl Default for ValidatorSetParams {
    fn default() -> Self {
        Self {
            activation_height: 0,
            epoch_blocks: 1_800,
            min_self_stake: U256::from(1_000u64) * U256::from(10u64).pow(U256::from(18u64)),
            max_validators: 12,
            unbonding_blocks: 7_200,
            jail_miss_bps: 2_000,
            jail_min_slots: 5,
            rewards_to_operator_height: 0,
            jail_escalation_height: 0,
            bench_height: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ValidatorRegistration {
    /// Wallet that registered and controls this entry (msg.sender).
    pub operator: Address,
    /// Block-signing address (node key). The registry is keyed by it.
    pub identity: Address,
    /// Native MRSN escrowed by the operator.
    pub self_stake: U256,
    pub commission_bps: u64,
    pub registered_at: u64,
    /// Genesis validators are seeded at activation and never need the
    /// one-epoch waiting period.
    pub genesis: bool,
    /// Jailed while `current_epoch < jailed_until_epoch`.
    pub jailed_until_epoch: u64,
    /// Operator asked to leave: removed at the next epoch, stake unbonds.
    pub exiting: bool,
    /// Key rotation requested; applied at the next epoch.
    pub pending_identity: Option<Address>,
    /// Leader-slot accounting for the current epoch.
    pub proposed_slots: u64,
    pub missed_slots: u64,
    pub total_proposed: u64,
    /// Lifetime jail count before `jail_escalation_height`; from then on the
    /// number of consecutive jails (cleared by a clean epoch as an active
    /// validator), which sets the length of the next jail.
    pub times_jailed: u64,
}

/// What a validator is doing right now, for RPC/UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidatorStatus {
    /// Registered this epoch; eligible from the next one.
    Pending,
    Active,
    /// Eligible but ranked below `max_validators`.
    Standby,
    Jailed,
    Exiting,
}

/// Outcome of an epoch transition, for logs and the block-level record.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EpochTransition {
    pub epoch: u64,
    pub active_set: Vec<Address>,
    pub jailed: Vec<Address>,
    pub removed: Vec<Address>,
    pub rotated: Vec<(Address, Address)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StakingError {
    UnknownValidator,
    ZeroAmount,
    InsufficientDelegation,
    NotActive,
    AlreadyRegistered,
    StakeTooLow,
    NotOperator,
    Exiting,
    GenesisImmutable,
}

impl StakingError {
    pub fn message(&self) -> &'static str {
        match self {
            StakingError::UnknownValidator => "unknown validator",
            StakingError::ZeroAmount => "amount must be > 0",
            StakingError::InsufficientDelegation => "insufficient delegated amount",
            StakingError::NotActive => "validator registration is not active yet",
            StakingError::AlreadyRegistered => "this node identity is already registered",
            StakingError::StakeTooLow => "self-stake below the minimum",
            StakingError::NotOperator => "caller is not this validator's operator",
            StakingError::Exiting => "validator is exiting",
            StakingError::GenesisImmutable => "genesis validators cannot use this call",
        }
    }
}

// ─── Open validator set ─────────────────────────────────────────────────────

impl StakingState {
    pub fn set_params(&mut self, params: ValidatorSetParams) {
        self.params = params;
    }

    /// Where a validator's share of the block reward goes at `height`: the
    /// registered operator wallet once `rewards_to_operator_height` is
    /// reached, otherwise (and for unregistered identities) the identity.
    pub fn reward_recipient(&self, identity: Address, height: u64) -> Address {
        let h = self.params.rewards_to_operator_height;
        if h == 0 || height < h {
            return identity;
        }
        self.registry
            .get(&identity)
            .map(|r| r.operator)
            .filter(|op| *op != Address::ZERO)
            .unwrap_or(identity)
    }

    /// Benched for the rest of the epoch (see `ValidatorSetParams::bench_height`).
    pub fn is_benched(&self, identity: Address) -> bool {
        self.registry
            .get(&identity)
            .map(|r| r.missed_slots >= BENCH_MISSES && r.missed_slots * 10 >= r.proposed_slots)
            .unwrap_or(false)
    }

    pub fn is_open_set_active(&self, height: u64) -> bool {
        self.params.activation_height > 0 && height >= self.params.activation_height
    }

    /// Height at which the epoch containing `height` began.
    pub fn epoch_start(&self, height: u64) -> u64 {
        let e = self.params.epoch_blocks.max(1);
        height - (height % e)
    }

    pub fn epoch_of(&self, height: u64) -> u64 {
        height / self.params.epoch_blocks.max(1)
    }

    /// Total stake ranking a validator: self-stake + delegations to its pool.
    pub fn voting_stake(&self, identity: Address) -> U256 {
        let self_stake = self
            .registry
            .get(&identity)
            .map(|r| r.self_stake)
            .unwrap_or(U256::ZERO);
        self_stake.saturating_add(self.delegated_total(identity))
    }

    /// Seed the registry with the genesis validators the first time the open
    /// set activates (idempotent).
    pub fn seed_genesis(&mut self, genesis: &[(Address, U256)], _height: u64) {
        // Constant across nodes: a node that activates late (upgraded after
        // the activation height) must derive the identical registry.
        let registered_at = self.params.activation_height;
        for (addr, stake) in genesis {
            self.registry.entry(*addr).or_insert(ValidatorRegistration {
                operator: *addr,
                identity: *addr,
                self_stake: *stake,
                commission_bps: DEFAULT_COMMISSION_BPS,
                registered_at,
                genesis: true,
                jailed_until_epoch: 0,
                exiting: false,
                pending_identity: None,
                proposed_slots: 0,
                missed_slots: 0,
                total_proposed: 0,
                times_jailed: 0,
            });
            self.ensure_pool(*addr);
        }
    }

    pub fn register_validator(
        &mut self,
        operator: Address,
        identity: Address,
        self_stake: U256,
        commission_bps: u64,
        height: u64,
    ) -> Result<(), StakingError> {
        if !self.is_open_set_active(height) {
            return Err(StakingError::NotActive);
        }
        if self.registry.contains_key(&identity) {
            return Err(StakingError::AlreadyRegistered);
        }
        if self_stake < self.params.min_self_stake {
            return Err(StakingError::StakeTooLow);
        }
        self.registry.insert(
            identity,
            ValidatorRegistration {
                operator,
                identity,
                self_stake,
                commission_bps: commission_bps.min(10_000),
                registered_at: height,
                genesis: false,
                jailed_until_epoch: 0,
                exiting: false,
                pending_identity: None,
                proposed_slots: 0,
                missed_slots: 0,
                total_proposed: 0,
                times_jailed: 0,
            },
        );
        self.ensure_pool(identity);
        if let Some(pool) = self.pools.get_mut(&identity) {
            pool.commission_bps = commission_bps.min(10_000);
        }
        Ok(())
    }

    pub fn add_self_stake(
        &mut self,
        operator: Address,
        identity: Address,
        amount: U256,
    ) -> Result<(), StakingError> {
        if amount.is_zero() {
            return Err(StakingError::ZeroAmount);
        }
        let r = self
            .registry
            .get_mut(&identity)
            .ok_or(StakingError::UnknownValidator)?;
        if r.operator != operator {
            return Err(StakingError::NotOperator);
        }
        if r.exiting {
            return Err(StakingError::Exiting);
        }
        r.self_stake = r.self_stake.saturating_add(amount);
        Ok(())
    }

    /// Leave the set at the next epoch; self-stake starts unbonding then.
    pub fn unregister_validator(
        &mut self,
        operator: Address,
        identity: Address,
    ) -> Result<(), StakingError> {
        let r = self
            .registry
            .get_mut(&identity)
            .ok_or(StakingError::UnknownValidator)?;
        if r.operator != operator {
            return Err(StakingError::NotOperator);
        }
        if r.genesis {
            return Err(StakingError::GenesisImmutable);
        }
        r.exiting = true;
        Ok(())
    }

    /// Switch the block-signing key at the next epoch.
    pub fn rotate_identity(
        &mut self,
        operator: Address,
        identity: Address,
        new_identity: Address,
    ) -> Result<(), StakingError> {
        if self.registry.contains_key(&new_identity) {
            return Err(StakingError::AlreadyRegistered);
        }
        let r = self
            .registry
            .get_mut(&identity)
            .ok_or(StakingError::UnknownValidator)?;
        if r.operator != operator {
            return Err(StakingError::NotOperator);
        }
        if r.genesis {
            return Err(StakingError::GenesisImmutable);
        }
        r.pending_identity = Some(new_identity);
        Ok(())
    }

    pub fn status_of(&self, identity: Address) -> Option<ValidatorStatus> {
        let r = self.registry.get(&identity)?;
        Some(if r.exiting {
            ValidatorStatus::Exiting
        } else if self.current_epoch < r.jailed_until_epoch {
            ValidatorStatus::Jailed
        } else if self.active_set.contains(&identity) {
            ValidatorStatus::Active
        } else if !r.genesis && self.epoch_of(r.registered_at) >= self.current_epoch {
            ValidatorStatus::Pending
        } else {
            ValidatorStatus::Standby
        })
    }

    /// Record who proposed height `h` and who missed their slot before them.
    /// `leaders` is the leader for rounds 0..=r where `leaders[r] == proposer`.
    pub fn record_slots(&mut self, proposer: Address, missed: &[Address]) {
        for m in missed {
            if let Some(r) = self.registry.get_mut(m) {
                r.missed_slots = r.missed_slots.saturating_add(1);
            }
        }
        if let Some(r) = self.registry.get_mut(&proposer) {
            r.proposed_slots = r.proposed_slots.saturating_add(1);
            r.total_proposed = r.total_proposed.saturating_add(1);
        }
    }

    /// Recompute the active set at the first block of an epoch. Deterministic
    /// from state, so every node agrees. Returns what changed.
    pub fn epoch_transition(&mut self, height: u64) -> EpochTransition {
        let epoch = self.epoch_of(height);
        let mut out = EpochTransition {
            epoch,
            ..Default::default()
        };
        let p = self.params.clone();
        let escalate = p.jail_escalation_height > 0 && height >= p.jail_escalation_height;

        // 1. Judge the epoch that just ended: jail, exit, rotate.
        let ids: Vec<Address> = self.registry.keys().copied().collect();
        for id in ids {
            let Some(r) = self.registry.get_mut(&id) else {
                continue;
            };
            let slots = r.proposed_slots + r.missed_slots;
            // Benching stops the misses from accumulating, so a benched
            // validator is judged on what it had before the bench.
            let benched = p.bench_height > 0
                && height >= p.bench_height
                && r.missed_slots >= BENCH_MISSES
                && r.missed_slots * 10 >= r.proposed_slots;
            if (slots >= p.jail_min_slots || benched)
                && r.missed_slots * 10_000 > slots * p.jail_miss_bps
                && !r.genesis
            {
                r.times_jailed += 1;
                let epochs = if escalate {
                    // 1, 2, 4, 8, 16, then 24: a node that is down all day
                    // stops costing the network a slow hour every other hour.
                    (1u64 << (r.times_jailed.min(6) - 1)).min(24)
                } else {
                    1
                };
                r.jailed_until_epoch = epoch + epochs;
                out.jailed.push(id);
            } else if escalate && slots >= p.jail_min_slots && r.times_jailed > 0 {
                // A full epoch served as an active validator without being
                // jailed clears the streak.
                r.times_jailed = 0;
            }
            r.proposed_slots = 0;
            r.missed_slots = 0;
            if r.exiting {
                let (operator, amount) = (r.operator, r.self_stake);
                self.registry.remove(&id);
                self.unbondings
                    .entry(operator)
                    .or_default()
                    .push(UnbondingEntry {
                        validator: id,
                        amount,
                        unlock_at: height + p.unbonding_blocks,
                    });
                out.removed.push(id);
                continue;
            }
            if let Some(new_id) = r.pending_identity.take() {
                let mut moved = r.clone();
                moved.identity = new_id;
                self.registry.remove(&id);
                self.registry.insert(new_id, moved);
                // Delegations follow the validator: re-key its pool.
                if let Some(pool) = self.pools.remove(&id) {
                    self.pools.insert(new_id, pool);
                }
                let keys: Vec<(Address, Address)> = self
                    .delegations
                    .keys()
                    .filter(|(_, v)| *v == id)
                    .copied()
                    .collect();
                for (d, _) in keys {
                    if let Some(del) = self.delegations.remove(&(d, id)) {
                        self.delegations.insert((d, new_id), del);
                    }
                }
                out.rotated.push((id, new_id));
            }
        }

        // 2. Eligible: registered in an earlier epoch, not jailed, enough stake.
        let mut eligible: Vec<(Address, U256)> = self
            .registry
            .values()
            .filter(|r| r.genesis || self.epoch_of(r.registered_at) < epoch)
            .filter(|r| epoch >= r.jailed_until_epoch)
            .filter(|r| r.self_stake >= p.min_self_stake)
            .map(|r| (r.identity, self.voting_stake(r.identity)))
            .collect();
        // Highest stake first; ties by address for determinism.
        eligible.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        let mut active: Vec<Address> = eligible
            .into_iter()
            .take(p.max_validators.max(1))
            .map(|(a, _)| a)
            .collect();
        active.sort();
        self.active_set = active.clone();
        self.current_epoch = epoch;
        out.active_set = active;
        out
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

    /// Jails escalate 1, 2, 4… epochs for consecutive offences once
    /// `jail_escalation_height` is reached, and a clean epoch as an active
    /// validator clears the streak. Before the height every jail is one epoch.
    #[test]
    fn repeat_jails_escalate_and_a_clean_epoch_resets() {
        let mrsn = U256::from(10u64).pow(U256::from(18u64));
        let mut s = StakingState::default();
        let p = ValidatorSetParams {
            activation_height: 10,
            epoch_blocks: 10,
            jail_min_slots: 2,
            jail_escalation_height: 100,
            ..ValidatorSetParams::default()
        };
        s.set_params(p);
        let genesis = [
            (addr(1), U256::from(1_000_000u64) * mrsn),
            (addr(2), U256::from(1_000_000u64) * mrsn),
        ];
        s.seed_genesis(&genesis, 10);
        let v = addr(9);
        s.register_validator(addr(8), v, U256::from(5_000u64) * mrsn, 0, 12)
            .unwrap();
        // Epoch boundary 20: v becomes active (registered in epoch 1 < 2).
        s.epoch_transition(20);
        assert!(s.active_set.contains(&v));

        // Before the switch: miss everything → one-epoch jail, lifetime count 1.
        for _ in 0..4 {
            s.record_slots(addr(1), &[v]);
        }
        s.epoch_transition(30);
        assert_eq!(s.registry[&v].jailed_until_epoch, 4, "epoch 3 + 1");
        assert_eq!(s.registry[&v].times_jailed, 1);
        s.epoch_transition(40); // sits out epoch 3 (no slots) — nothing changes
        assert_eq!(s.registry[&v].times_jailed, 1);

        // Past the switch (height >= 100): relapse → 2 epochs, again → 4 epochs.
        s.epoch_transition(100); // boundary of epoch 10; v eligible again since 4
        assert!(s.active_set.contains(&v));
        for _ in 0..4 {
            s.record_slots(addr(1), &[v]);
        }
        s.epoch_transition(110);
        assert_eq!(s.registry[&v].times_jailed, 2);
        assert_eq!(
            s.registry[&v].jailed_until_epoch,
            11 + 2,
            "second consecutive jail: 2 epochs"
        );
        // out for epochs 11 and 12, back at 13
        s.epoch_transition(120);
        s.epoch_transition(130);
        assert!(s.active_set.contains(&v), "back after two epochs");
        for _ in 0..4 {
            s.record_slots(addr(1), &[v]);
        }
        s.epoch_transition(140);
        assert_eq!(s.registry[&v].times_jailed, 3);
        assert_eq!(s.registry[&v].jailed_until_epoch, 14 + 4, "third: 4 epochs");

        // Serve a clean epoch → streak cleared → the next jail is 1 epoch again.
        for h in (150..=180).step_by(10) {
            s.epoch_transition(h);
        }
        assert!(s.active_set.contains(&v));
        for _ in 0..4 {
            s.record_slots(v, &[]);
        } // proposes all its slots
        s.epoch_transition(190);
        assert_eq!(
            s.registry[&v].times_jailed, 0,
            "clean epoch resets the streak"
        );
        for _ in 0..4 {
            s.record_slots(addr(1), &[v]);
        }
        s.epoch_transition(200);
        assert_eq!(
            s.registry[&v].jailed_until_epoch,
            20 + 1,
            "streak restarted at one epoch"
        );
    }

    /// A validator that misses three leader slots (and has not proposed ten
    /// times as many) is benched; a busy validator with a few misses is not.
    /// At the boundary a benched validator is judged even when it saw fewer
    /// than `jail_min_slots` slots, because the bench stopped the count.
    #[test]
    fn bench_predicate_and_boundary_jail() {
        let mrsn = U256::from(10u64).pow(U256::from(18u64));
        let mut s = StakingState::default();
        let p = ValidatorSetParams {
            activation_height: 10,
            epoch_blocks: 10,
            jail_min_slots: 5,
            bench_height: 10,
            ..ValidatorSetParams::default()
        };
        s.set_params(p);
        s.seed_genesis(&[(addr(1), U256::from(1_000_000u64) * mrsn)], 10);
        let dead = addr(9);
        let busy = addr(7);
        for v in [dead, busy] {
            s.register_validator(addr(8), v, U256::from(5_000u64) * mrsn, 0, 12)
                .unwrap();
        }
        s.epoch_transition(20);
        assert!(s.active_set.contains(&dead) && s.active_set.contains(&busy));

        // Two misses: not yet.
        s.record_slots(addr(1), &[dead]);
        s.record_slots(addr(1), &[dead]);
        assert!(!s.is_benched(dead));
        // Third miss: benched.
        s.record_slots(addr(1), &[dead]);
        assert!(s.is_benched(dead));

        // Busy validator: 40 proposed, 3 missed → 3*10 < 40 → stays in rotation.
        for _ in 0..40 {
            s.record_slots(busy, &[]);
        }
        for _ in 0..3 {
            s.record_slots(addr(1), &[busy]);
        }
        assert!(!s.is_benched(busy));
        // A genuinely dead one after a good start: 40 proposed then 4 misses → benched.
        s.record_slots(addr(1), &[busy]);
        assert!(s.is_benched(busy));

        // Boundary: `dead` saw only 3 slots (< jail_min_slots 5) but was benched → jailed.
        s.epoch_transition(30);
        assert_eq!(s.registry[&dead].jailed_until_epoch, 4);
        assert!(!s.active_set.contains(&dead));
        // `busy` missed 4 of 44 slots (9%) → below jail_miss_bps → not jailed, counters reset.
        assert_eq!(s.registry[&busy].jailed_until_epoch, 0);
        assert_eq!(s.registry[&busy].missed_slots, 0);
        assert!(!s.is_benched(busy), "bench clears with the epoch counters");
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

        let unlock = s.undelegate(alice, val, U256::from(200u64), 100).unwrap();
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
