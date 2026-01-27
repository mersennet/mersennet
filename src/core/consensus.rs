use anyhow::{bail, Result};
use revm::primitives::{Address, B256, U256};
use crate::network::{Message, NetworkSim, RoundStage};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug)]
pub struct Unbonding {
    pub address: Address,
    pub amount: U256,
    pub unlock_height: u64,
}

#[derive(Clone, Debug)]
pub struct Slashing {
    pub address: Address,
    pub amount: U256,
    pub reason: String,
    pub height: u64,
}

#[derive(Clone, Debug)]
pub enum ValidatorChange {
    Stake { address: Address, amount: U256 },
    Unbond { address: Address, amount: U256 },
}

#[derive(Clone, Debug)]
pub struct Validator {
    pub address: Address,
    pub stake: U256,
}

#[derive(Clone, Debug)]
pub struct Vote {
    pub validator: Address,
    pub block_hash: B256,
    pub stake: U256,
}

#[derive(Clone, Debug)]
pub struct Finalization {
    pub block_hash: B256,
    pub proposer: Address,
    pub total_stake: U256,
    pub committed_stake: U256,
    pub threshold: U256,
    pub votes: Vec<Vote>,
    pub finalized: bool,
    pub rewards: Vec<Reward>,
    pub total_reward: U256,
    pub burned_reward: U256,
    pub scheduled_reward: U256,
    pub remaining_supply: U256,
}

#[derive(Clone, Debug)]
pub struct RoundResult {
    pub round: u64,
    pub prevote_stake: U256,
    pub precommit_stake: U256,
    pub finalized: bool,
    pub timeout_ms: u64,
    pub timed_out: bool,
}

#[derive(Clone, Debug)]
pub enum EvidenceKind {
    DoubleSign,
    PrecommitTimeout,
}

impl EvidenceKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            EvidenceKind::DoubleSign => "double_sign",
            EvidenceKind::PrecommitTimeout => "precommit_timeout",
        }
    }
}

#[derive(Clone, Debug)]
pub struct SlashingEvidence {
    pub validator: Address,
    pub kind: EvidenceKind,
    pub height: u64,
    pub round: u64,
    pub rounds_missed: u64,
}

#[derive(Clone, Debug)]
pub struct Reward {
    pub address: Address,
    pub amount: U256,
}

#[derive(Default, Debug)]
pub struct Consensus {
    validators: Vec<Validator>,
    unbonding: Vec<Unbonding>,
    unbonding_period: u64,
    max_supply: U256,
    total_minted: U256,
    initial_reward_per_block: U256,
    halving_interval: u64,
    pending_changes: Vec<ValidatorChange>,
    pending_slashes: Vec<Slashing>,
    round_timeout_ms: u64,
    double_sign_bps: u64,
    timeout_bps: u64,
    max_escalation_bps: u64,
    escalation_step_bps: u64,
    offense_counts: HashMap<Address, u64>,
    miss_prevote: HashSet<Address>,
    miss_precommit: HashSet<Address>,
}

impl Consensus {
    pub fn validators(&self) -> &[Validator] {
        &self.validators
    }

    pub fn set_round_timeout_ms(&mut self, timeout_ms: u64) {
        self.round_timeout_ms = timeout_ms.max(1);
    }

    pub fn set_slashing_bps(&mut self, double_sign_bps: u64, timeout_bps: u64) {
        self.double_sign_bps = double_sign_bps.min(10_000);
        self.timeout_bps = timeout_bps.min(10_000);
    }

    pub fn set_slashing_escalation(&mut self, step_bps: u64, max_bps: u64) {
        self.escalation_step_bps = step_bps.min(10_000);
        self.max_escalation_bps = max_bps.min(10_000);
    }

    pub fn set_miss_prevote(&mut self, validator: Address) {
        self.miss_prevote.insert(validator);
    }

    pub fn set_miss_precommit(&mut self, validator: Address) {
        self.miss_precommit.insert(validator);
    }

    pub fn run_finality_rounds(
        &self,
        block_hash: B256,
        height: u64,
        rounds: u64,
        network: &mut NetworkSim,
    ) -> (Vec<RoundResult>, Vec<SlashingEvidence>) {
        let mut results = Vec::new();
        let mut evidence = Vec::new();
        let mut timeout_streaks: HashMap<Address, u64> = HashMap::new();
        let total_stake = self.total_stake();
        let threshold = if total_stake.is_zero() {
            U256::ZERO
        } else {
            total_stake
                .saturating_mul(U256::from(2u64))
                .checked_div(U256::from(3u64))
                .unwrap_or(U256::ZERO)
                .saturating_add(U256::from(1u64))
        };

        for round in 0..rounds {
            for validator in &self.validators {
                if self.miss_prevote.contains(&validator.address) {
                    continue;
                }
                network.broadcast(Message {
                    from: validator.address,
                    round,
                    stage: RoundStage::Prevote,
                    block_hash,
                    height,
                });
            }

            let prevotes = network.drain_round(block_hash, height, round, RoundStage::Prevote);
            self.detect_double_sign(height, round, RoundStage::Prevote, &prevotes, &mut evidence);
            let prevote_set = prevotes
                .iter()
                .map(|message| message.from)
                .collect::<HashSet<_>>();
            let prevote_stake = prevotes
                .iter()
                .map(|message| self.stake_of(message.from))
                .fold(U256::ZERO, |acc, stake| acc.saturating_add(stake));

            for validator in &self.validators {
                if self.miss_precommit.contains(&validator.address) {
                    continue;
                }
                network.broadcast(Message {
                    from: validator.address,
                    round,
                    stage: RoundStage::Precommit,
                    block_hash,
                    height,
                });
            }

            let precommits = network.drain_round(block_hash, height, round, RoundStage::Precommit);
            self.detect_double_sign(height, round, RoundStage::Precommit, &precommits, &mut evidence);
            let precommit_set = precommits
                .iter()
                .map(|message| message.from)
                .collect::<HashSet<_>>();
            let precommit_stake = precommits
                .iter()
                .map(|message| self.stake_of(message.from))
                .fold(U256::ZERO, |acc, stake| acc.saturating_add(stake));

            for validator in prevote_set.difference(&precommit_set) {
                let streak = timeout_streaks.entry(*validator).or_insert(0);
                *streak = streak.saturating_add(1);
                evidence.push(SlashingEvidence {
                    validator: *validator,
                    kind: EvidenceKind::PrecommitTimeout,
                    height,
                    round,
                    rounds_missed: *streak,
                });
            }

            let finalized = !threshold.is_zero()
                && prevote_stake >= threshold
                && precommit_stake >= threshold;

            results.push(RoundResult {
                round,
                prevote_stake,
                precommit_stake,
                finalized,
                timeout_ms: self.round_timeout_ms,
                timed_out: !finalized,
            });

            if finalized {
                break;
            }
        }

        (results, evidence)
    }
    pub fn set_unbonding_period(&mut self, period: u64) {
        self.unbonding_period = period.max(1);
    }

    pub fn set_token_economics(
        &mut self,
        max_supply: U256,
        initial_reward_per_block: U256,
        halving_interval: u64,
    ) {
        self.max_supply = max_supply;
        self.initial_reward_per_block = initial_reward_per_block;
        self.halving_interval = halving_interval.max(1);
    }

    pub fn reward_per_block(&self, height: u64) -> U256 {
        if self.initial_reward_per_block.is_zero() {
            return U256::ZERO;
        }
        let halvings = height / self.halving_interval.max(1);
        let mut reward = self.initial_reward_per_block;
        for _ in 0..halvings {
            reward = reward.checked_div(U256::from(2u64)).unwrap_or(U256::ZERO);
            if reward.is_zero() {
                break;
            }
        }
        reward
    }

    #[allow(dead_code)]
    pub fn total_minted(&self) -> U256 {
        self.total_minted
    }

    pub fn stake(&mut self, address: Address, amount: U256) -> Result<()> {
        if amount.is_zero() {
            bail!("stake amount must be > 0");
        }

        if let Some(existing) = self
            .validators
            .iter_mut()
            .find(|validator| validator.address == address)
        {
            existing.stake = existing.stake.saturating_add(amount);
        } else {
            self.validators.push(Validator {
                address,
                stake: amount,
            });
        }
        Ok(())
    }

    pub fn begin_unbonding(&mut self, address: Address, amount: U256, height: u64) -> Result<()> {
        if amount.is_zero() {
            bail!("unbond amount must be > 0");
        }

        let Some(index) = self
            .validators
            .iter()
            .position(|validator| validator.address == address)
        else {
            bail!("validator not found");
        };

        if self.validators[index].stake < amount {
            bail!("insufficient stake");
        }

        self.validators[index].stake = self.validators[index].stake.saturating_sub(amount);
        if self.validators[index].stake.is_zero() {
            self.validators.remove(index);
        }

        let unlock_height = height.saturating_add(self.unbonding_period.max(1));
        self.unbonding.push(Unbonding {
            address,
            amount,
            unlock_height,
        });
        Ok(())
    }

    pub fn process_unbonding(&mut self, height: u64) -> Vec<Unbonding> {
        let mut released = Vec::new();
        let mut pending = Vec::new();

        for entry in self.unbonding.drain(..) {
            if height >= entry.unlock_height {
                released.push(entry);
            } else {
                pending.push(entry);
            }
        }

        self.unbonding = pending;
        released
    }

    pub fn queue_stake(&mut self, address: Address, amount: U256) {
        self.pending_changes
            .push(ValidatorChange::Stake { address, amount });
    }

    pub fn queue_unbond(&mut self, address: Address, amount: U256) {
        self.pending_changes
            .push(ValidatorChange::Unbond { address, amount });
    }

    pub fn queue_slash(&mut self, address: Address, amount: U256, reason: impl Into<String>) {
        if amount.is_zero() {
            return;
        }
        self.pending_slashes.push(Slashing {
            address,
            amount,
            reason: reason.into(),
            height: 0,
        });
    }

    pub fn apply_pending_slashes(&mut self, height: u64) -> Vec<Slashing> {
        let mut slashes = std::mem::take(&mut self.pending_slashes);
        for slash in &mut slashes {
            slash.height = height;
            let remaining = self.slash_from_unbonding(slash.address, slash.amount);
            let remaining = self.slash_from_stake(slash.address, remaining);
            slash.amount = slash.amount.saturating_sub(remaining);
        }
        slashes
    }

    pub fn apply_pending_changes(&mut self, height: u64) -> Vec<ValidatorChange> {
        let changes = std::mem::take(&mut self.pending_changes);
        for change in &changes {
            match *change {
                ValidatorChange::Stake { address, amount } => {
                    let _ = self.stake(address, amount);
                }
                ValidatorChange::Unbond { address, amount } => {
                    let _ = self.begin_unbonding(address, amount, height);
                }
            }
        }
        changes
    }

    fn slash_from_unbonding(&mut self, address: Address, amount: U256) -> U256 {
        if amount.is_zero() {
            return U256::ZERO;
        }

        let mut remaining = amount;
        let mut updated = Vec::new();

        for mut entry in self.unbonding.drain(..) {
            if entry.address == address && !remaining.is_zero() {
                if entry.amount > remaining {
                    entry.amount = entry.amount.saturating_sub(remaining);
                    remaining = U256::ZERO;
                } else {
                    remaining = remaining.saturating_sub(entry.amount);
                    entry.amount = U256::ZERO;
                }
            }

            if !entry.amount.is_zero() {
                updated.push(entry);
            }
        }

        self.unbonding = updated;
        remaining
    }

    fn slash_from_stake(&mut self, address: Address, amount: U256) -> U256 {
        if amount.is_zero() {
            return U256::ZERO;
        }

        if let Some(index) = self
            .validators
            .iter()
            .position(|validator| validator.address == address)
        {
            let stake = self.validators[index].stake;
            if stake > amount {
                self.validators[index].stake = stake.saturating_sub(amount);
                return U256::ZERO;
            }
            self.validators.remove(index);
            return amount.saturating_sub(stake);
        }

        amount
    }

    pub fn total_stake(&self) -> U256 {
        self.validators
            .iter()
            .fold(U256::ZERO, |acc, v| acc.saturating_add(v.stake))
    }

    pub fn slash_amount_for_evidence(&self, evidence: &SlashingEvidence) -> U256 {
        let stake = self.stake_of(evidence.validator);
        let base_bps = match evidence.kind {
            EvidenceKind::DoubleSign => self.double_sign_bps,
            EvidenceKind::PrecommitTimeout => self.timeout_bps,
        };
        let offenses = self.offense_counts.get(&evidence.validator).copied().unwrap_or(0);
        let rounds = evidence.rounds_missed.max(1).saturating_sub(1);
        let escalated = base_bps
            .saturating_add(self.escalation_step_bps.saturating_mul(offenses))
            .saturating_add(self.escalation_step_bps.saturating_mul(rounds));
        let bps = escalated.min(self.max_escalation_bps.max(base_bps));
        stake
            .saturating_mul(U256::from(bps))
            .checked_div(U256::from(10_000u64))
            .unwrap_or(U256::ZERO)
    }

    pub fn record_offense(&mut self, validator: Address) {
        let entry = self.offense_counts.entry(validator).or_insert(0);
        *entry = entry.saturating_add(1);
    }

    fn stake_of(&self, address: Address) -> U256 {
        self.validators
            .iter()
            .find(|validator| validator.address == address)
            .map(|validator| validator.stake)
            .unwrap_or_default()
    }

    fn detect_double_sign(
        &self,
        height: u64,
        round: u64,
        _stage: RoundStage,
        messages: &[Message],
        evidence: &mut Vec<SlashingEvidence>,
    ) {
        let mut seen: HashMap<Address, B256> = HashMap::new();
        for message in messages {
            if let Some(prev) = seen.insert(message.from, message.block_hash) {
                if prev != message.block_hash {
                    evidence.push(SlashingEvidence {
                        validator: message.from,
                        kind: EvidenceKind::DoubleSign,
                        height,
                        round,
                        rounds_missed: 0,
                    });
                }
            }
        }
    }

    pub fn proposer(&self, height: u64) -> Address {
        if self.validators.is_empty() {
            return Address::ZERO;
        }
        let index = (height as usize) % self.validators.len();
        self.validators[index].address
    }

    pub fn finalize(&mut self, block_hash: B256, height: u64) -> Finalization {
        let total_stake = self.total_stake();
        let threshold = if total_stake.is_zero() {
            U256::ZERO
        } else {
            total_stake
                .saturating_mul(U256::from(2u64))
                .checked_div(U256::from(3u64))
                .unwrap_or(U256::ZERO)
                .saturating_add(U256::from(1u64))
        };

        let votes = self
            .validators
            .iter()
            .map(|v| Vote {
                validator: v.address,
                block_hash,
                stake: v.stake,
            })
            .collect::<Vec<_>>();

        let committed_stake = votes
            .iter()
            .fold(U256::ZERO, |acc, v| acc.saturating_add(v.stake));

        let remaining_supply = self.max_supply.saturating_sub(self.total_minted);
        let scheduled_reward = self.reward_per_block(height);
        let effective_reward = if scheduled_reward > remaining_supply {
            remaining_supply
        } else {
            scheduled_reward
        };

        let rewards = if total_stake.is_zero() || effective_reward.is_zero() {
            Vec::new()
        } else {
            self.validators
                .iter()
                .map(|validator| Reward {
                    address: validator.address,
                    amount: self
                        .reward_per_block(height)
                        .saturating_mul(validator.stake)
                        .checked_div(total_stake)
                        .unwrap_or(U256::ZERO),
                })
                .collect::<Vec<_>>()
        };

        let total_reward = rewards
            .iter()
            .fold(U256::ZERO, |acc, reward| acc.saturating_add(reward.amount));
        let burned_reward = effective_reward.saturating_sub(total_reward);
        self.total_minted = self.total_minted.saturating_add(total_reward);

        Finalization {
            block_hash,
            proposer: self.proposer(height),
            total_stake,
            committed_stake,
            threshold,
            votes,
            finalized: !total_stake.is_zero() && committed_stake >= threshold,
            rewards,
            total_reward,
            burned_reward,
            scheduled_reward,
            remaining_supply,
        }
    }
}
