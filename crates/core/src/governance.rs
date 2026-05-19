use anyhow::{Result, bail};
use revm::primitives::{Address, U256};
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub enum ProposalKind {
    SetFeeMarket {
        gas_limit_per_block: u64,
        elasticity_multiplier: u64,
        max_change_denominator: u64,
    },
    SetTokenEconomics {
        max_supply: U256,
        initial_reward_per_block: U256,
        halving_interval: u64,
    },
}

#[derive(Clone, Debug)]
pub struct Proposal {
    pub id: u64,
    pub title: String,
    pub kind: ProposalKind,
    pub start_height: u64,
    pub end_height: u64,
    pub yes_stake: U256,
    pub no_stake: U256,
    pub executed: bool,
}

#[derive(Clone, Debug)]
pub struct Vote {
    pub voter: Address,
    pub support: bool,
    pub stake: U256,
}

#[derive(Clone, Debug)]
pub struct Tally {
    pub yes_stake: U256,
    pub no_stake: U256,
    pub total_stake: U256,
    pub quorum_reached: bool,
    pub passed: bool,
}

pub struct Governance {
    proposals: Vec<Proposal>,
    votes: HashMap<(u64, Address), Vote>,
    quorum_bps: u64,
    pass_bps: u64,
    vote_delay: u64,
    voting_window: u64,
    next_id: u64,
}

impl Governance {
    pub fn new(quorum_bps: u64, pass_bps: u64, vote_delay: u64, voting_window: u64) -> Self {
        Self {
            proposals: Vec::new(),
            votes: HashMap::new(),
            quorum_bps: quorum_bps.min(10_000),
            pass_bps: pass_bps.min(10_000),
            vote_delay,
            voting_window: voting_window.max(1),
            next_id: 1,
        }
    }

    pub fn submit(
        &mut self,
        title: impl Into<String>,
        kind: ProposalKind,
        current_height: u64,
    ) -> Result<u64> {
        let start_height = current_height.saturating_add(self.vote_delay);
        let end_height = start_height.saturating_add(self.voting_window);
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        self.proposals.push(Proposal {
            id,
            title: title.into(),
            kind,
            start_height,
            end_height,
            yes_stake: U256::ZERO,
            no_stake: U256::ZERO,
            executed: false,
        });
        Ok(id)
    }

    pub fn vote(
        &mut self,
        proposal_id: u64,
        height: u64,
        voter: Address,
        stake: U256,
        support: bool,
    ) -> Result<()> {
        let proposal = self
            .proposals
            .iter_mut()
            .find(|proposal| proposal.id == proposal_id)
            .ok_or_else(|| anyhow::anyhow!("proposal not found"))?;

        if height < proposal.start_height || height > proposal.end_height {
            bail!("proposal not active");
        }

        let key = (proposal_id, voter);
        if self.votes.contains_key(&key) {
            bail!("already voted");
        }

        if support {
            proposal.yes_stake = proposal.yes_stake.saturating_add(stake);
        } else {
            proposal.no_stake = proposal.no_stake.saturating_add(stake);
        }

        self.votes.insert(
            key,
            Vote {
                voter,
                support,
                stake,
            },
        );
        Ok(())
    }

    pub fn tally(&self, proposal_id: u64, total_stake: U256) -> Result<Tally> {
        let proposal = self
            .proposals
            .iter()
            .find(|proposal| proposal.id == proposal_id)
            .ok_or_else(|| anyhow::anyhow!("proposal not found"))?;

        let yes = proposal.yes_stake;
        let no = proposal.no_stake;
        let total = total_stake;
        let quorum = total
            .saturating_mul(U256::from(self.quorum_bps))
            .checked_div(U256::from(10_000u64))
            .unwrap_or(U256::ZERO);

        let quorum_reached = yes.saturating_add(no) >= quorum && !total.is_zero();
        let pass_threshold = yes
            .saturating_add(no)
            .saturating_mul(U256::from(self.pass_bps))
            .checked_div(U256::from(10_000u64))
            .unwrap_or(U256::ZERO);
        let passed = quorum_reached && yes >= pass_threshold;

        Ok(Tally {
            yes_stake: yes,
            no_stake: no,
            total_stake: total,
            quorum_reached,
            passed,
        })
    }

    pub fn execute(
        &mut self,
        proposal_id: u64,
        height: u64,
        finalized: bool,
        total_stake: U256,
        mut apply: impl FnMut(&ProposalKind) -> Result<()>,
    ) -> Result<Tally> {
        let proposal = self
            .proposals
            .iter()
            .find(|proposal| proposal.id == proposal_id)
            .ok_or_else(|| anyhow::anyhow!("proposal not found"))?;

        if !finalized {
            bail!("block not finalized");
        }

        if height <= proposal.end_height {
            bail!("voting window not closed");
        }

        let tally = self.tally(proposal_id, total_stake)?;
        if tally.passed {
            let proposal = self
                .proposals
                .iter_mut()
                .find(|proposal| proposal.id == proposal_id)
                .ok_or_else(|| anyhow::anyhow!("proposal not found"))?;

            if proposal.executed {
                bail!("proposal already executed");
            }

            apply(&proposal.kind)?;
            proposal.executed = true;
        }

        Ok(tally)
    }

    pub fn proposal(&self, proposal_id: u64) -> Option<&Proposal> {
        self.proposals
            .iter()
            .find(|proposal| proposal.id == proposal_id)
    }

    pub fn vote_count(&self, proposal_id: u64) -> usize {
        self.votes
            .iter()
            .filter(|((id, _), _)| *id == proposal_id)
            .count()
    }

    pub fn sample_vote(&self, proposal_id: u64) -> Option<&Vote> {
        self.votes
            .iter()
            .find(|((id, _), _)| *id == proposal_id)
            .map(|(_, vote)| vote)
    }
}
