use revm::primitives::{Address, B256, U256};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

use crate::consensus::Validator;

// ---------------------------------------------------------------------------
// Data structures
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QuorumCertificate {
    pub block_hash: B256,
    pub height: u64,
    pub round: u64,
    pub signers: Vec<Address>,
    pub aggregate_stake: U256,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Proposal {
    pub block_hash: B256,
    pub height: u64,
    pub round: u64,
    pub proposer: Address,
    pub parent_qc: Option<QuorumCertificate>,
    pub justify: Option<QuorumCertificate>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum HotStuffMessage {
    Propose(Proposal),
    Vote {
        block_hash: B256,
        height: u64,
        round: u64,
        voter: Address,
        voter_stake: U256,
    },
    NewView {
        round: u64,
        high_qc: Option<QuorumCertificate>,
        sender: Address,
    },
    Timeout {
        round: u64,
        high_qc: Option<QuorumCertificate>,
        sender: Address,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CommitDecision {
    pub block_hash: B256,
    pub height: u64,
    pub round: u64,
    pub qc: QuorumCertificate,
}

/// Result returned to the engine from a single HotStuff-2 round invocation.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HotStuff2Result {
    pub committed: Option<CommitDecision>,
    pub round: u64,
    pub finalized: bool,
    pub votes_collected: usize,
    pub timeout: bool,
}

// ---------------------------------------------------------------------------
// HotStuff-2 protocol state machine
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct HotStuff2 {
    // Identity
    pub address: Address,
    pub validators: Vec<Validator>,

    // Protocol state
    pub current_round: u64,
    pub current_height: u64,
    pub locked_qc: Option<QuorumCertificate>,
    pub high_qc: Option<QuorumCertificate>,
    pub last_committed_round: u64,

    // Vote collection: (height, round) -> list of (voter, stake)
    pub pending_votes: HashMap<(u64, u64), Vec<(Address, U256)>>,
    // Timeout collection: round -> list of (sender, their high_qc)
    pub pending_timeouts: HashMap<u64, Vec<(Address, Option<QuorumCertificate>)>>,

    // Safety
    pub last_voted_round: u64,
    pub tombstoned: HashSet<Address>,

    // Timing
    pub round_timeout_base_ms: u64,
    pub round_timeout_delta_ms: u64,

    // Weighted round-robin proposer election
    pub proposer_priorities: HashMap<Address, i128>,
}

impl HotStuff2 {
    pub fn new(address: Address, validators: Vec<Validator>) -> Self {
        Self {
            address,
            validators,
            current_round: 0,
            current_height: 0,
            locked_qc: None,
            high_qc: None,
            last_committed_round: 0,
            pending_votes: HashMap::new(),
            pending_timeouts: HashMap::new(),
            last_voted_round: 0,
            tombstoned: HashSet::new(),
            round_timeout_base_ms: 3000,
            round_timeout_delta_ms: 500,
            proposer_priorities: HashMap::new(),
        }
    }

    // -----------------------------------------------------------------------
    // Stake helpers
    // -----------------------------------------------------------------------

    fn total_stake(&self) -> U256 {
        self.validators
            .iter()
            .filter(|v| !self.tombstoned.contains(&v.address))
            .fold(U256::ZERO, |acc, v| acc.saturating_add(v.stake))
    }

    fn threshold(&self) -> U256 {
        let total = self.total_stake();
        if total.is_zero() {
            return U256::ZERO;
        }
        // ⌊2*total / 3⌋ + 1
        total
            .saturating_mul(U256::from(2u64))
            .checked_div(U256::from(3u64))
            .unwrap_or(U256::ZERO)
            .saturating_add(U256::from(1u64))
    }

    pub fn has_quorum(&self, stake: U256) -> bool {
        let t = self.threshold();
        !t.is_zero() && stake >= t
    }

    fn stake_of(&self, addr: Address) -> U256 {
        self.validators
            .iter()
            .find(|v| v.address == addr)
            .map(|v| v.stake)
            .unwrap_or(U256::ZERO)
    }

    fn is_validator(&self, addr: &Address) -> bool {
        self.validators.iter().any(|v| v.address == *addr)
            && !self.tombstoned.contains(addr)
    }

    // -----------------------------------------------------------------------
    // Leader election – deterministic weighted round-robin
    // -----------------------------------------------------------------------

    pub fn leader_for_round(&self, round: u64) -> Address {
        let eligible: Vec<&Validator> = self
            .validators
            .iter()
            .filter(|v| !self.tombstoned.contains(&v.address))
            .collect();

        if eligible.is_empty() {
            return Address::ZERO;
        }
        if eligible.len() == 1 {
            return eligible[0].address;
        }

        // Deterministic: sum of stakes gives a modulus; walk through validators
        // accumulating stake until we pass (round % total_weight).
        let total_weight: u64 = eligible
            .iter()
            .map(|v| v.stake.as_limbs()[0])
            .fold(0u64, |a, b| a.saturating_add(b));

        if total_weight == 0 {
            return eligible[0].address;
        }

        let target = round % total_weight;
        let mut acc = 0u64;
        for v in &eligible {
            acc = acc.saturating_add(v.stake.as_limbs()[0]);
            if acc > target {
                return v.address;
            }
        }
        eligible.last().unwrap().address
    }

    // -----------------------------------------------------------------------
    // Phase 1 – Propose
    // -----------------------------------------------------------------------

    /// Called by the leader of `self.current_round`. Returns a `Propose` message
    /// if this node is the leader, otherwise `None`.
    pub fn propose(&mut self, block_hash: B256) -> Option<HotStuffMessage> {
        let leader = self.leader_for_round(self.current_round);
        if leader != self.address {
            return None;
        }

        let proposal = Proposal {
            block_hash,
            height: self.current_height,
            round: self.current_round,
            proposer: self.address,
            parent_qc: self.high_qc.clone(),
            justify: self.high_qc.clone(),
        };

        Some(HotStuffMessage::Propose(proposal))
    }

    // -----------------------------------------------------------------------
    // Phase 2 – Vote on proposal
    // -----------------------------------------------------------------------

    /// Validate proposal and return a Vote message if the proposal is safe.
    pub fn on_proposal(&mut self, proposal: &Proposal) -> Option<HotStuffMessage> {
        if proposal.round < self.current_round {
            return None;
        }

        let expected_leader = self.leader_for_round(proposal.round);
        if proposal.proposer != expected_leader {
            tracing::warn!(
                expected = ?expected_leader,
                got = ?proposal.proposer,
                round = proposal.round,
                "proposal from non-leader"
            );
            return None;
        }

        if !self.is_safe_to_vote(proposal) {
            tracing::debug!(round = proposal.round, "not safe to vote");
            return None;
        }

        // Update high_qc from the proposal's justify
        if let Some(justify) = &proposal.justify {
            self.update_high_qc(justify);
        }

        // Advance round if proposal is from a future round
        if proposal.round > self.current_round {
            self.advance_round(proposal.round, proposal.justify.clone());
        }

        self.last_voted_round = proposal.round;

        let voter_stake = self.stake_of(self.address);

        Some(HotStuffMessage::Vote {
            block_hash: proposal.block_hash,
            height: proposal.height,
            round: proposal.round,
            voter: self.address,
            voter_stake,
        })
    }

    // -----------------------------------------------------------------------
    // Vote collection & QC formation
    // -----------------------------------------------------------------------

    /// Process an incoming vote. If a quorum is reached, returns the new QC.
    pub fn on_vote(
        &mut self,
        voter: Address,
        block_hash: B256,
        height: u64,
        round: u64,
        stake: U256,
    ) -> Option<QuorumCertificate> {
        if !self.is_validator(&voter) {
            return None;
        }

        let key = (height, round);
        let votes = self.pending_votes.entry(key).or_default();

        if votes.iter().any(|(addr, _)| *addr == voter) {
            return None;
        }

        votes.push((voter, stake));

        let aggregate: U256 = votes.iter().fold(U256::ZERO, |acc, (_, s)| acc.saturating_add(*s));
        let signers: Vec<Address> = votes.iter().map(|(a, _)| *a).collect();

        // Release the mutable borrow on `self.pending_votes` before calling has_quorum
        let threshold = self.threshold();
        if !threshold.is_zero() && aggregate >= threshold {
            let qc = QuorumCertificate {
                block_hash,
                height,
                round,
                signers,
                aggregate_stake: aggregate,
            };
            self.update_high_qc(&qc);
            Some(qc)
        } else {
            None
        }
    }

    // -----------------------------------------------------------------------
    // 2-chain commit rule  (THE key HotStuff-2 innovation)
    //
    //   Round r  : QC_r  certifies block B
    //   Round r+1: QC_{r+1} certifies block B' that extends B
    //   → B is committed
    // -----------------------------------------------------------------------

    /// Given a newly formed QC, check if we can commit an earlier block via
    /// the 2-chain rule, then advance the lock.
    ///
    /// Commit fires when `locked_qc` is at round *r* and the new QC is at
    /// round *r + 1* (consecutive rounds).  After the check, `locked_qc` is
    /// always advanced to the higher QC.
    pub fn try_commit(&mut self, qc: &QuorumCertificate) -> Option<CommitDecision> {
        if !self.is_safe_to_commit(qc) {
            return None;
        }

        // Check 2-chain BEFORE updating the lock
        let decision = self.locked_qc.as_ref().and_then(|locked| {
            if qc.round == locked.round.saturating_add(1) {
                let d = CommitDecision {
                    block_hash: locked.block_hash,
                    height: locked.height,
                    round: locked.round,
                    qc: qc.clone(),
                };
                Some(d)
            } else {
                None
            }
        });

        if let Some(ref d) = decision {
            self.last_committed_round = d.round;
            metrics::increment_counter!("hotstuff2_commits");
            tracing::info!(
                committed_round = d.round,
                committed_height = d.height,
                certifying_round = qc.round,
                "2-chain commit"
            );
        }

        // Always advance the lock to the higher QC
        let should_advance = self
            .locked_qc
            .as_ref()
            .map_or(true, |cur| qc.round > cur.round);
        if should_advance {
            self.locked_qc = Some(qc.clone());
        }

        decision
    }

    // -----------------------------------------------------------------------
    // Timeout handling
    // -----------------------------------------------------------------------

    /// Called when our local timer fires. Broadcasts a Timeout message.
    pub fn on_timeout(&mut self) -> HotStuffMessage {
        metrics::increment_counter!("hotstuff2_timeouts");
        tracing::warn!(round = self.current_round, "round timed out");

        HotStuffMessage::Timeout {
            round: self.current_round,
            high_qc: self.high_qc.clone(),
            sender: self.address,
        }
    }

    /// Process an incoming Timeout message from another validator.
    /// If f+1 timeouts are collected, trigger a round advance via NewView.
    pub fn on_timeout_msg(&mut self, msg: &HotStuffMessage) -> Option<HotStuffMessage> {
        let (round, high_qc, sender) = match msg {
            HotStuffMessage::Timeout {
                round,
                high_qc,
                sender,
            } => (*round, high_qc.clone(), *sender),
            _ => return None,
        };

        if !self.is_validator(&sender) {
            return None;
        }

        let timeouts = self.pending_timeouts.entry(round).or_default();
        if timeouts.iter().any(|(addr, _)| *addr == sender) {
            return None;
        }
        timeouts.push((sender, high_qc.clone()));

        // Snapshot the timeout entries so we release the mutable borrow
        let timeout_snapshot: Vec<(Address, Option<QuorumCertificate>)> =
            timeouts.iter().cloned().collect();

        let mut best_qc: Option<QuorumCertificate> = None;
        let mut timeout_stake = U256::ZERO;
        for (addr, qc_opt) in &timeout_snapshot {
            timeout_stake = timeout_stake.saturating_add(self.stake_of(*addr));
            if let Some(qc) = qc_opt {
                let is_higher = best_qc
                    .as_ref()
                    .map_or(true, |cur| qc.round > cur.round);
                if is_higher {
                    best_qc = Some(qc.clone());
                }
            }
        }

        if self.has_quorum(timeout_stake) {
            if let Some(ref qc) = best_qc {
                self.update_high_qc(qc);
            }
            self.advance_round(round.saturating_add(1), best_qc.clone());

            Some(HotStuffMessage::NewView {
                round: self.current_round,
                high_qc: best_qc,
                sender: self.address,
            })
        } else {
            None
        }
    }

    /// Exponential back-off timeout for a given round.
    pub fn timeout_for_round(&self, round: u64) -> u64 {
        self.round_timeout_base_ms
            .saturating_add(round.saturating_mul(self.round_timeout_delta_ms))
    }

    // -----------------------------------------------------------------------
    // Safety predicates
    // -----------------------------------------------------------------------

    fn is_safe_to_vote(&self, proposal: &Proposal) -> bool {
        // Rule 1: never vote for a round we already voted on
        if proposal.round <= self.last_voted_round {
            return false;
        }

        // Rule 2: proposal must extend the locked QC, or bring a higher QC
        if let Some(locked) = &self.locked_qc {
            // The proposal's justify must be at least as high as our lock
            let justify_round = proposal
                .justify
                .as_ref()
                .map(|qc| qc.round)
                .unwrap_or(0);
            if justify_round < locked.round {
                // Unless the proposal directly extends the locked block
                if let Some(parent) = &proposal.parent_qc {
                    if parent.block_hash != locked.block_hash {
                        return false;
                    }
                } else {
                    return false;
                }
            }
        }

        true
    }

    fn is_safe_to_commit(&self, qc: &QuorumCertificate) -> bool {
        qc.round >= self.last_committed_round && self.has_quorum(qc.aggregate_stake)
    }

    // -----------------------------------------------------------------------
    // View / round management
    // -----------------------------------------------------------------------

    pub fn advance_round(&mut self, new_round: u64, justify: Option<QuorumCertificate>) {
        if new_round <= self.current_round {
            return;
        }

        tracing::debug!(
            from = self.current_round,
            to = new_round,
            "advancing round"
        );

        self.current_round = new_round;

        if let Some(qc) = justify {
            self.update_high_qc(&qc);
        }

        // Garbage-collect old vote/timeout state
        self.pending_votes
            .retain(|&(_, r), _| r >= new_round.saturating_sub(2));
        self.pending_timeouts
            .retain(|&r, _| r >= new_round.saturating_sub(2));
    }

    fn update_high_qc(&mut self, qc: &QuorumCertificate) {
        let is_higher = self
            .high_qc
            .as_ref()
            .map_or(true, |cur| qc.round > cur.round);

        if is_higher {
            self.high_qc = Some(qc.clone());
        }
    }

    // -----------------------------------------------------------------------
    // Convenience: run a full simulated round (for engine integration)
    // -----------------------------------------------------------------------

    /// Simulates a complete HotStuff-2 round in a single-node / test setting.
    /// All validators vote, QC is formed, and commit is attempted.
    pub fn run_simulated_round(&mut self, block_hash: B256, height: u64) -> HotStuff2Result {
        self.current_height = height;

        let proposal = Proposal {
            block_hash,
            height,
            round: self.current_round,
            proposer: self.leader_for_round(self.current_round),
            parent_qc: self.high_qc.clone(),
            justify: self.high_qc.clone(),
        };

        let eligible: Vec<Validator> = self
            .validators
            .iter()
            .filter(|v| !self.tombstoned.contains(&v.address))
            .cloned()
            .collect();

        // Collect votes from all eligible validators
        let mut formed_qc: Option<QuorumCertificate> = None;
        let mut vote_count = 0usize;

        for v in &eligible {
            let stake = v.stake;
            if let Some(qc) = self.on_vote(
                v.address,
                proposal.block_hash,
                proposal.height,
                proposal.round,
                stake,
            ) {
                formed_qc = Some(qc);
            }
            vote_count += 1;
        }

        let committed = formed_qc.as_ref().and_then(|qc| self.try_commit(qc));
        let finalized = committed.is_some();
        let round = self.current_round;

        // Advance to next round
        self.advance_round(
            self.current_round.saturating_add(1),
            formed_qc.or_else(|| self.high_qc.clone()),
        );

        HotStuff2Result {
            committed,
            round,
            finalized,
            votes_collected: vote_count,
            timeout: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_validators(n: usize) -> Vec<Validator> {
        (0..n)
            .map(|i| {
                let mut bytes = [0u8; 20];
                bytes[19] = (i + 1) as u8;
                Validator {
                    address: Address::from(bytes),
                    stake: U256::from(100u64),
                }
            })
            .collect()
    }

    fn addr(i: u8) -> Address {
        let mut bytes = [0u8; 20];
        bytes[19] = i;
        Address::from(bytes)
    }

    #[test]
    fn test_quorum_threshold() {
        let validators = test_validators(4); // 400 total stake
        let hs = HotStuff2::new(addr(1), validators);
        // threshold = ⌊2*400/3⌋ + 1 = 267
        assert_eq!(hs.threshold(), U256::from(267u64));
        assert!(!hs.has_quorum(U256::from(266u64)));
        assert!(hs.has_quorum(U256::from(267u64)));
    }

    #[test]
    fn test_leader_election_deterministic() {
        let validators = test_validators(4);
        let hs = HotStuff2::new(addr(1), validators);
        let l0 = hs.leader_for_round(0);
        let l0_again = hs.leader_for_round(0);
        assert_eq!(l0, l0_again);
    }

    #[test]
    fn test_vote_collection_forms_qc() {
        let validators = test_validators(4);
        let mut hs = HotStuff2::new(addr(1), validators.clone());
        let hash = B256::ZERO;

        assert!(hs.on_vote(validators[0].address, hash, 1, 0, U256::from(100u64)).is_none());
        assert!(hs.on_vote(validators[1].address, hash, 1, 0, U256::from(100u64)).is_none());
        let qc = hs.on_vote(validators[2].address, hash, 1, 0, U256::from(100u64));
        assert!(qc.is_some(), "QC should form at 300/400 stake (threshold 267)");
    }

    #[test]
    fn test_duplicate_vote_rejected() {
        let validators = test_validators(4);
        let mut hs = HotStuff2::new(addr(1), validators.clone());
        let hash = B256::ZERO;

        hs.on_vote(validators[0].address, hash, 1, 0, U256::from(100u64));
        let dup = hs.on_vote(validators[0].address, hash, 1, 0, U256::from(100u64));
        assert!(dup.is_none());
    }

    #[test]
    fn test_two_chain_commit() {
        let validators = test_validators(4);
        let mut hs = HotStuff2::new(addr(1), validators.clone());
        let hash_a = B256::with_last_byte(0xAA);
        let hash_b = B256::with_last_byte(0xBB);

        // Round 0: form QC for block A
        for v in &validators[..3] {
            hs.on_vote(v.address, hash_a, 1, 0, v.stake);
        }
        let qc_0 = hs.high_qc.clone().expect("QC should exist");
        assert_eq!(qc_0.round, 0);

        // try_commit should NOT commit yet (no 2-chain)
        let decision = hs.try_commit(&qc_0);
        assert!(decision.is_none());

        // Round 1: form QC for block B
        hs.current_round = 1;
        for v in &validators[..3] {
            hs.on_vote(v.address, hash_b, 2, 1, v.stake);
        }
        let qc_1 = hs.high_qc.clone().expect("QC should exist");
        assert_eq!(qc_1.round, 1);

        // Now 2-chain commit should fire for block A (locked at round 0, QC at round 1)
        // We need to reset locked_qc to qc_0 to simulate the 2-chain
        hs.locked_qc = Some(qc_0.clone());
        let decision = hs.try_commit(&qc_1);
        assert!(decision.is_some());
        let d = decision.unwrap();
        assert_eq!(d.block_hash, hash_a);
        assert_eq!(d.round, 0);
    }

    #[test]
    fn test_simulated_round_produces_result() {
        let validators = test_validators(4);
        let mut hs = HotStuff2::new(validators[0].address, validators.clone());

        let hash = B256::with_last_byte(0x01);
        let r0 = hs.run_simulated_round(hash, 1);
        assert_eq!(r0.votes_collected, 4);
        assert!(!r0.finalized, "first round should not commit (no 2-chain yet)");

        let hash2 = B256::with_last_byte(0x02);
        let r1 = hs.run_simulated_round(hash2, 2);
        assert!(r1.finalized, "second consecutive round should commit via 2-chain");
        assert!(r1.committed.is_some());
    }

    #[test]
    fn test_timeout_round_advance() {
        let validators = test_validators(4);
        let mut hs = HotStuff2::new(addr(1), validators.clone());
        assert_eq!(hs.current_round, 0);

        // Simulate f+1 = quorum timeouts
        for v in &validators[..3] {
            let msg = HotStuffMessage::Timeout {
                round: 0,
                high_qc: None,
                sender: v.address,
            };
            hs.on_timeout_msg(&msg);
        }
        assert_eq!(hs.current_round, 1, "should advance after quorum of timeouts");
    }

    #[test]
    fn test_safety_no_vote_on_old_round() {
        let validators = test_validators(4);
        let mut hs = HotStuff2::new(addr(1), validators.clone());
        hs.last_voted_round = 5;

        let proposal = Proposal {
            block_hash: B256::ZERO,
            height: 1,
            round: 3,
            proposer: hs.leader_for_round(3),
            parent_qc: None,
            justify: None,
        };
        assert!(!hs.is_safe_to_vote(&proposal));
    }

    #[test]
    fn test_timeout_backoff() {
        let validators = test_validators(4);
        let hs = HotStuff2::new(addr(1), validators);

        let t0 = hs.timeout_for_round(0);
        let t5 = hs.timeout_for_round(5);
        assert!(t5 > t0, "timeout should grow with round");
    }
}
