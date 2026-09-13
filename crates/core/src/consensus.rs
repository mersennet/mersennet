use crate::network::{Message, NetworkSim, RoundStage};
use anyhow::{Result, bail};
use revm::primitives::{Address, B256, U256};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Unbonding {
    pub address: Address,
    pub amount: U256,
    pub unlock_height: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Slashing {
    pub address: Address,
    pub amount: U256,
    pub reason: String,
    pub height: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ValidatorChange {
    Stake { address: Address, amount: U256 },
    Unbond { address: Address, amount: U256 },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Validator {
    pub address: Address,
    pub stake: U256,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Vote {
    pub validator: Address,
    pub block_hash: B256,
    pub stake: U256,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
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

impl Default for Finalization {
    fn default() -> Self {
        Self {
            block_hash: B256::ZERO,
            proposer: Address::ZERO,
            total_stake: U256::ZERO,
            committed_stake: U256::ZERO,
            threshold: U256::ZERO,
            votes: Vec::new(),
            finalized: false,
            rewards: Vec::new(),
            total_reward: U256::ZERO,
            burned_reward: U256::ZERO,
            scheduled_reward: U256::ZERO,
            remaining_supply: U256::ZERO,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RoundResult {
    pub round: u64,
    pub prevote_stake: U256,
    pub precommit_stake: U256,
    pub finalized: bool,
    pub timeout_ms: u64,
    pub timed_out: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SlashingEvidence {
    pub validator: Address,
    pub kind: EvidenceKind,
    pub height: u64,
    pub round: u64,
    pub rounds_missed: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Reward {
    pub address: Address,
    pub amount: U256,
}

#[derive(Clone, Debug, Default)]
pub struct ValidatorSigningInfo {
    pub start_height: u64,
    pub missed_blocks_counter: u64,
    pub jailed_until: Option<u64>,
    pub tombstoned: bool,
}

#[derive(Debug, Clone)]
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
    locked_round: Option<u64>,
    locked_hash: Option<B256>,
    tombstoned: HashSet<Address>,
    timeout_delta_ms: u64,
    signing_info: HashMap<Address, ValidatorSigningInfo>,
    proposer_priorities: HashMap<Address, i128>,
    max_evidence_age: u64,
}

impl Default for Consensus {
    fn default() -> Self {
        Self {
            validators: Vec::new(),
            unbonding: Vec::new(),
            unbonding_period: 0,
            max_supply: U256::ZERO,
            total_minted: U256::ZERO,
            initial_reward_per_block: U256::ZERO,
            halving_interval: 0,
            pending_changes: Vec::new(),
            pending_slashes: Vec::new(),
            round_timeout_ms: 0,
            double_sign_bps: 0,
            timeout_bps: 0,
            max_escalation_bps: 0,
            escalation_step_bps: 0,
            offense_counts: HashMap::new(),
            miss_prevote: HashSet::new(),
            miss_precommit: HashSet::new(),
            locked_round: None,
            locked_hash: None,
            tombstoned: HashSet::new(),
            timeout_delta_ms: 100,
            signing_info: HashMap::new(),
            proposer_priorities: HashMap::new(),
            max_evidence_age: 10_000,
        }
    }
}

impl Consensus {
    pub fn validators(&self) -> &[Validator] {
        &self.validators
    }

    /// Replace the whole set (open validator set epoch transition). Stakes
    /// are the voting stakes computed by the staking registry.
    pub fn replace_validators(&mut self, validators: Vec<Validator>) {
        self.validators = validators;
    }

    // --- Validator locking (CometBFT PoLC) ---

    pub fn lock_on(&mut self, round: u64, hash: B256) {
        self.locked_round = Some(round);
        self.locked_hash = Some(hash);
    }

    pub fn unlock(&mut self) {
        self.locked_round = None;
        self.locked_hash = None;
    }

    pub fn is_locked(&self) -> bool {
        self.locked_round.is_some()
    }

    // --- Tombstone support ---

    pub fn is_tombstoned(&self, addr: Address) -> bool {
        self.tombstoned.contains(&addr)
    }

    fn tombstone(&mut self, addr: Address) {
        self.tombstoned.insert(addr);
        if let Some(info) = self.signing_info.get_mut(&addr) {
            info.tombstoned = true;
        }
    }

    // --- Jail / unjail ---

    pub fn jail(&mut self, addr: Address, until_height: u64) {
        let info = self.signing_info.entry(addr).or_default();
        info.jailed_until = Some(until_height);
    }

    pub fn unjail(&mut self, addr: Address, current_height: u64) -> Result<()> {
        let info = self.signing_info.entry(addr).or_default();
        if info.tombstoned {
            bail!("validator is tombstoned and cannot be unjailed");
        }
        match info.jailed_until {
            Some(until) if current_height < until => {
                bail!("jail period has not expired (expires at height {})", until);
            }
            None => {
                bail!("validator is not jailed");
            }
            _ => {}
        }
        info.jailed_until = None;
        Ok(())
    }

    fn is_jailed(&self, addr: &Address) -> bool {
        self.signing_info
            .get(addr)
            .and_then(|info| info.jailed_until)
            .is_some()
    }

    fn is_eligible(&self, addr: &Address) -> bool {
        !self.is_jailed(addr) && !self.tombstoned.contains(addr)
    }

    // --- Timeout configuration ---

    pub fn set_timeout_delta_ms(&mut self, delta: u64) {
        self.timeout_delta_ms = delta;
    }

    pub fn set_max_evidence_age(&mut self, age: u64) {
        self.max_evidence_age = age;
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
        &mut self,
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

        let eligible_validators: Vec<Validator> = self
            .validators
            .iter()
            .filter(|v| self.is_eligible(&v.address))
            .cloned()
            .collect();

        for round in 0..rounds {
            let round_timeout = self
                .round_timeout_ms
                .saturating_add(round.saturating_mul(self.timeout_delta_ms));

            for validator in &eligible_validators {
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

            if !threshold.is_zero() && prevote_stake >= threshold {
                self.lock_on(round, block_hash);
            }

            for validator in &eligible_validators {
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
            self.detect_double_sign(
                height,
                round,
                RoundStage::Precommit,
                &precommits,
                &mut evidence,
            );
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

            let all_participating: HashSet<Address> =
                prevote_set.union(&precommit_set).copied().collect();
            for validator in &eligible_validators {
                if !all_participating.contains(&validator.address) {
                    let info = self.signing_info.entry(validator.address).or_default();
                    info.missed_blocks_counter = info.missed_blocks_counter.saturating_add(1);
                }
            }

            for ev in &evidence {
                if matches!(ev.kind, EvidenceKind::DoubleSign) {
                    self.tombstone(ev.validator);
                }
            }

            let finalized =
                !threshold.is_zero() && prevote_stake >= threshold && precommit_stake >= threshold;

            if finalized {
                self.unlock();
            }

            metrics::increment_counter!("mersennet_consensus_rounds");
            if finalized {
                metrics::increment_counter!("mersennet_consensus_finalized");
            }

            tracing::info!(
                round,
                prevote_stake = %prevote_stake,
                precommit_stake = %precommit_stake,
                finalized,
                timed_out = !finalized,
                "consensus round completed"
            );

            results.push(RoundResult {
                round,
                prevote_stake,
                precommit_stake,
                finalized,
                timeout_ms: round_timeout,
                timed_out: !finalized,
            });

            if finalized {
                break;
            }
        }

        for ev in &evidence {
            metrics::increment_counter!(
                "mersennet_slashing_events",
                "kind" => ev.kind.as_str()
            );
            tracing::warn!(
                validator = ?ev.validator,
                kind = ev.kind.as_str(),
                height = ev.height,
                round = ev.round,
                "slashing evidence detected"
            );
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
        if self.tombstoned.contains(&address) {
            bail!("validator is tombstoned and cannot rejoin");
        }

        let is_new = !self
            .validators
            .iter()
            .any(|validator| validator.address == address);

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

        if is_new {
            let total = self.total_voting_weight();
            let penalty = -(total.saturating_add(total / 8));
            self.proposer_priorities.insert(address, penalty);
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

    pub fn slash_amount_for_evidence(
        &self,
        evidence: &SlashingEvidence,
        current_height: u64,
    ) -> U256 {
        if self.is_evidence_expired(current_height, evidence.height) {
            return U256::ZERO;
        }
        let stake = self.stake_of(evidence.validator);
        let base_bps = match evidence.kind {
            EvidenceKind::DoubleSign => self.double_sign_bps,
            EvidenceKind::PrecommitTimeout => self.timeout_bps,
        };
        let offenses = self
            .offense_counts
            .get(&evidence.validator)
            .copied()
            .unwrap_or(0);
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
            if let Some(prev) = seen.insert(message.from, message.block_hash)
                && prev != message.block_hash
            {
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

    pub fn is_evidence_expired(&self, current_height: u64, evidence_height: u64) -> bool {
        current_height.saturating_sub(evidence_height) > self.max_evidence_age
    }

    fn voting_weight(&self, stake: U256) -> i128 {
        let total = self.total_stake();
        if total.is_zero() {
            return 0;
        }
        // Normalize stakes to a safe i128 range by scaling relative to total.
        // weight = (stake * 1_000_000) / total_stake — preserves proportions
        // without overflowing i128 regardless of 18-decimal U256 magnitudes.
        stake
            .saturating_mul(U256::from(1_000_000u64))
            .checked_div(total)
            .unwrap_or(U256::ZERO)
            .as_limbs()[0] as i128
    }

    fn total_voting_weight(&self) -> i128 {
        self.validators
            .iter()
            .filter(|v| self.is_eligible(&v.address))
            .map(|v| self.voting_weight(v.stake))
            .fold(0i128, |acc, w| acc.saturating_add(w))
    }

    pub fn proposer(&mut self, height: u64) -> Address {
        let eligible: Vec<Validator> = self
            .validators
            .iter()
            .filter(|v| self.is_eligible(&v.address))
            .cloned()
            .collect();

        if eligible.is_empty() {
            return Address::ZERO;
        }

        if eligible.len() == 1 {
            return eligible[0].address;
        }

        let total_voting = self.total_voting_weight();

        for v in &eligible {
            let weight = self.voting_weight(v.stake);
            let priority = self.proposer_priorities.entry(v.address).or_insert(0);
            *priority = priority.saturating_add(weight);
        }

        let winner = eligible
            .iter()
            .max_by_key(|v| {
                self.proposer_priorities
                    .get(&v.address)
                    .copied()
                    .unwrap_or(0)
            })
            .map(|v| v.address)
            .unwrap_or_else(|| {
                let _ = height;
                eligible[0].address
            });

        if let Some(priority) = self.proposer_priorities.get_mut(&winner) {
            *priority = priority.saturating_sub(total_voting);
        }

        winner
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
                    amount: effective_reward
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

        let active_count = self
            .validators
            .iter()
            .filter(|v| self.is_eligible(&v.address))
            .count();
        metrics::gauge!("mersennet_validators_active", active_count as f64);
        let stake_display = total_stake
            .checked_div(U256::from(1_000_000_000_000_000_000u128))
            .unwrap_or(U256::ZERO)
            .as_limbs()[0] as f64;
        metrics::gauge!("mersennet_total_stake", stake_display);

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
