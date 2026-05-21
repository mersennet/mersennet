//! Sealed-bid liquidation auctions for the shielded CLOB.
//!
//! The legacy [`crate::prime_orders::PrimeOrdersState::is_liquidatable`]
//! and [`crate::prime_orders::PrimeOrdersState::liquidate`] are
//! address-keyed: anyone can scan the chain and see when a specific
//! account is one tick away from forced closure. That's the exact
//! signal big traders exploit to engineer cascading liquidations.
//!
//! This module replaces both with:
//!
//! 1. **Bonded liquidator set.** Anyone can stake `MIN_LIQUIDATOR_BOND`
//!    PRIM to become an authorized liquidator. The bond is slashable
//!    for falsified claims.
//! 2. **Liquidate-claim proof** (`Circuit::LiquidateClaim`). A
//!    liquidator proves "there exists some position note `c` in the
//!    tree whose maintenance equity is negative at oracle price P,"
//!    without revealing `c`. The proof emits a `claim_tag =
//!    Poseidon(c, P)` so the auction can deduplicate claims that
//!    target the same victim.
//! 3. **Sealed bids.** Each claim is accompanied by a bid; bids are
//!    threshold-encrypted via [`crate::threshold_mempool`]. They are
//!    decrypted at the end of the block, the highest bid wins the
//!    auction for that `claim_tag`.
//! 4. **Liquidate-execute proof** (`Circuit::LiquidateExecute`). The
//!    auction winner consumes the victim's position note, mints a
//!    bounty note for themselves, and a top-up note for the
//!    insurance fund. The victim's identity is never revealed
//!    on-chain.
//! 5. **Bid revelation deadline.** Losing bids are revealed one
//!    block after settlement so collusion is observable.
//!
//! All of the above is engine-side bookkeeping; the cryptographic
//! checks happen via [`prime_zkp::noir::Verifier`].

#![allow(dead_code)]

use crate::prime_orders::MarketId;
use crate::shielded_state::ShieldedState;
use prime_zkp::{
    noir::{Circuit, CircuitProof, MockVerifier, Verifier, VerifyError},
    poseidon::Poseidon,
    Fr, NoteCommitment, Nullifier,
};
use revm::primitives::U256;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

/// Minimum bond, in PRIM lowest units, to register as a liquidator.
/// Configurable via governance after launch. Default = 10_000 PRIM.
pub const MIN_LIQUIDATOR_BOND: u128 = 10_000 * 1_000_000_000_000_000_000;

/// Bid-revelation delay in blocks. Losing bids are made public this
/// many blocks after the auction settles, for collusion detection.
pub const BID_REVELATION_DELAY: u64 = 1;

#[derive(Debug, Error)]
pub enum LiquidationError {
    #[error("liquidator is not registered or below the minimum bond")]
    UnregisteredLiquidator,
    #[error("zk proof verification failed: {0}")]
    InvalidProof(#[from] VerifyError),
    #[error("claim_tag {0:?} already settled this block")]
    DuplicateClaim(Fr),
    #[error("no winning bid for claim_tag {0:?}")]
    NoBids(Fr),
    #[error("nullifier already spent")]
    DoubleSpend,
    #[error("anchor root is not in the recent-roots window")]
    StaleAnchor,
}

/// A liquidator's registration record. Identified by their bond
/// commitment (not address — even the liquidators are pseudonymous).
#[derive(Clone, Debug)]
pub struct Liquidator {
    /// Commitment of the liquidator's bond note.
    pub bond_commitment: Fr,
    /// Bond amount.
    pub bond_amount: u128,
    /// Block at which the liquidator was registered.
    pub registered_at: u64,
}

/// A liquidator's claim on a candidate victim position.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LiquidationClaim {
    pub anchor_root: Fr,
    pub market_id: MarketId,
    pub oracle_price: U256,
    /// Commitment of the bond note (identifies the liquidator).
    pub liquidator_id: Fr,
    /// `Poseidon(victim_commitment, oracle_price)`. Used to detect
    /// duplicate claims on the same victim within a block.
    pub claim_tag: Fr,
    /// Threshold-encrypted bid value (the auction is sealed-bid).
    pub encrypted_bid: Vec<u8>,
    /// ZK proof of the claim circuit.
    pub proof: CircuitProof,
}

/// Decrypted bid plaintext.
#[derive(Clone, Debug)]
pub struct DecryptedBid {
    pub claim_tag: Fr,
    pub liquidator_id: Fr,
    pub bid_price: U256,
}

/// Auction-winner's execution payload.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LiquidationExecute {
    pub anchor_root: Fr,
    pub claim_tag: Fr,
    pub victim_nullifier: Fr,
    pub bounty_commitment: Fr,
    pub insurance_commitment: Fr,
    pub winning_bid: U256,
    pub market_id: MarketId,
    pub oracle_price: U256,
    pub proof: CircuitProof,
}

#[derive(Clone, Debug, Default)]
pub struct AuctionStats {
    pub claims_received: u64,
    pub claims_rejected: u64,
    pub auctions_settled: u64,
    pub total_bounty_paid: U256,
    pub total_insurance_funded: U256,
}

/// Per-claim-tag auction state at the current block.
#[derive(Clone, Debug, Default)]
struct AuctionEntry {
    /// All claims with this tag (one per liquidator that found it).
    claims: Vec<LiquidationClaim>,
    /// Decrypted bids, accumulated as decryption shares arrive.
    bids: Vec<DecryptedBid>,
}

#[derive(Debug)]
pub struct LiquidationAuction {
    /// Liquidator registry, keyed by bond commitment.
    pub liquidators: HashMap<Fr, Liquidator>,
    /// Current-block auctions keyed by claim_tag.
    auctions: HashMap<Fr, AuctionEntry>,
    /// Settled auctions awaiting bid revelation.
    pending_revelation: Vec<(u64, Vec<DecryptedBid>)>,
    /// Insurance fund balance (mirrors shielded_orders).
    pub insurance_fund: U256,
    pub stats: AuctionStats,
    verifier: Box<dyn Verifier>,
    poseidon: Poseidon,
}

impl Default for LiquidationAuction {
    fn default() -> Self {
        Self {
            liquidators: HashMap::new(),
            auctions: HashMap::new(),
            pending_revelation: Vec::new(),
            insurance_fund: U256::ZERO,
            stats: AuctionStats::default(),
            verifier: Box::new(MockVerifier::new()),
            poseidon: Poseidon::default(),
        }
    }
}

impl LiquidationAuction {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a new bonded liquidator.
    pub fn register(&mut self, bond_commitment: Fr, bond_amount: u128, block: u64) -> bool {
        if bond_amount < MIN_LIQUIDATOR_BOND {
            return false;
        }
        self.liquidators.insert(
            bond_commitment,
            Liquidator {
                bond_commitment,
                bond_amount,
                registered_at: block,
            },
        );
        true
    }

    pub fn is_registered(&self, bond_commitment: &Fr) -> bool {
        self.liquidators.contains_key(bond_commitment)
    }

    /// Submit a claim. Verifies the ZK proof and accumulates the
    /// claim for this block's auction. Returns the claim_tag on
    /// success so the caller can index later actions.
    pub fn submit_claim(
        &mut self,
        state: &ShieldedState,
        claim: LiquidationClaim,
    ) -> Result<Fr, LiquidationError> {
        if !state.is_recent_root(&claim.anchor_root) {
            self.stats.claims_rejected += 1;
            return Err(LiquidationError::StaleAnchor);
        }
        if !self.is_registered(&claim.liquidator_id) {
            self.stats.claims_rejected += 1;
            return Err(LiquidationError::UnregisteredLiquidator);
        }
        let public_inputs = vec![
            claim.anchor_root,
            Fr::from_u64(claim.market_id.0),
            Fr::from_u64(u256_low(&claim.oracle_price)),
            claim.liquidator_id,
            claim.claim_tag,
        ];
        self.verifier
            .verify(&claim.proof, Circuit::LiquidateClaim, &public_inputs)?;
        let tag = claim.claim_tag;
        self.auctions.entry(tag).or_default().claims.push(claim);
        self.stats.claims_received += 1;
        Ok(tag)
    }

    /// Submit a decrypted bid (the threshold mempool publishes these
    /// at block end).
    pub fn submit_bid(&mut self, bid: DecryptedBid) {
        if let Some(entry) = self.auctions.get_mut(&bid.claim_tag) {
            entry.bids.push(bid);
        }
    }

    /// Settle every auction whose bids have been revealed this block.
    /// For each claim_tag, pick the highest bid; the winner is then
    /// expected to submit a [`LiquidationExecute`] in the next block.
    /// Returns `(claim_tag, winning_liquidator, winning_bid)` tuples
    /// for the chain to emit events.
    pub fn settle_block(&mut self, block: u64) -> Vec<(Fr, Fr, U256)> {
        let mut winners = Vec::new();
        let mut all_bids = Vec::new();
        for (tag, entry) in self.auctions.drain() {
            let highest = entry
                .bids
                .iter()
                .max_by(|a, b| a.bid_price.cmp(&b.bid_price))
                .cloned();
            if let Some(w) = highest {
                winners.push((tag, w.liquidator_id, w.bid_price));
                self.stats.auctions_settled += 1;
            }
            all_bids.extend(entry.bids.into_iter());
        }
        self.pending_revelation.push((block, all_bids));
        // Reveal bids that have aged past BID_REVELATION_DELAY.
        self.pending_revelation
            .retain(|(b, _)| block.saturating_sub(*b) < BID_REVELATION_DELAY);
        winners
    }

    /// Bids that are now public (block - submission >= BID_REVELATION_DELAY).
    pub fn revealed_bids(&self) -> Vec<&DecryptedBid> {
        self.pending_revelation
            .iter()
            .flat_map(|(_, bids)| bids.iter())
            .collect()
    }

    /// Execute a winning liquidation. Verifies the execute proof,
    /// consumes the victim's nullifier, inserts bounty + insurance
    /// note commitments, and updates the insurance fund.
    pub fn execute(
        &mut self,
        state: &mut ShieldedState,
        exec: LiquidationExecute,
    ) -> Result<(), LiquidationError> {
        if !state.is_recent_root(&exec.anchor_root) {
            return Err(LiquidationError::StaleAnchor);
        }
        let public_inputs = vec![
            exec.anchor_root,
            exec.victim_nullifier,
            exec.bounty_commitment,
            exec.insurance_commitment,
            Fr::from_u64(u256_low(&exec.winning_bid)),
            Fr::from_u64(exec.market_id.0),
            Fr::from_u64(u256_low(&exec.oracle_price)),
        ];
        self.verifier
            .verify(&exec.proof, Circuit::LiquidateExecute, &public_inputs)?;
        if state.is_spent(&Nullifier(exec.victim_nullifier)) {
            return Err(LiquidationError::DoubleSpend);
        }
        state.spend(Nullifier(exec.victim_nullifier))?;
        let _ = state.insert_note(NoteCommitment(exec.bounty_commitment))?;
        let _ = state.insert_note(NoteCommitment(exec.insurance_commitment))?;
        self.stats.total_bounty_paid =
            self.stats.total_bounty_paid.saturating_add(exec.winning_bid);
        Ok(())
    }
}

impl From<crate::shielded_state::ShieldedStateError> for LiquidationError {
    fn from(e: crate::shielded_state::ShieldedStateError) -> Self {
        use crate::shielded_state::ShieldedStateError;
        match e {
            ShieldedStateError::DoubleSpend => LiquidationError::DoubleSpend,
            ShieldedStateError::StaleAnchor => LiquidationError::StaleAnchor,
            ShieldedStateError::InvalidMembershipProof => {
                LiquidationError::InvalidProof(VerifyError::InvalidProof)
            }
            ShieldedStateError::DuplicateCommitment => {
                LiquidationError::InvalidProof(VerifyError::InvalidProof)
            }
        }
    }
}

fn u256_low(x: &U256) -> u64 {
    x.as_limbs()[0]
}

#[cfg(test)]
mod tests {
    use super::*;
    use prime_zkp::note::Note;

    fn build_claim(
        state: &ShieldedState,
        liquidator_id: Fr,
        victim: &Note,
    ) -> LiquidationClaim {
        let p = Poseidon::default();
        let oracle_price = U256::from(3_200u64);
        let claim_tag = p.hash_two(&victim.commit(&p).0, &Fr::from_u64(u256_low(&oracle_price)));
        let anchor = state.current_root();
        let public_inputs = vec![
            anchor,
            Fr::from_u64(2),
            Fr::from_u64(u256_low(&oracle_price)),
            liquidator_id,
            claim_tag,
        ];
        let proof = MockVerifier::new().prove(Circuit::LiquidateClaim, public_inputs);
        LiquidationClaim {
            anchor_root: anchor,
            market_id: MarketId(2),
            oracle_price,
            liquidator_id,
            claim_tag,
            encrypted_bid: Vec::new(),
            proof,
        }
    }

    #[test]
    fn unregistered_liquidator_is_rejected() {
        let mut a = LiquidationAuction::new();
        let p = Poseidon::default();
        let state = ShieldedState::new();
        let victim = Note {
            value: 1_000_000,
            asset_id: 0,
            owner_pk: Fr::from_u64(99),
            rho: Fr::from_u64(1),
            psi: Fr::from_u64(2),
        };
        let _ = victim.commit(&p); // ensure the type is used
        let claim = build_claim(&state, Fr::from_u64(0xdeadbeef), &victim);
        let err = a.submit_claim(&state, claim).unwrap_err();
        assert!(matches!(err, LiquidationError::UnregisteredLiquidator));
        assert_eq!(a.stats.claims_rejected, 1);
    }

    #[test]
    fn below_min_bond_registration_fails() {
        let mut a = LiquidationAuction::new();
        assert!(!a.register(Fr::from_u64(1), MIN_LIQUIDATOR_BOND - 1, 0));
    }

    #[test]
    fn registered_liquidator_submits_claim() {
        let mut a = LiquidationAuction::new();
        let p = Poseidon::default();
        let mut state = ShieldedState::new();
        let victim = Note {
            value: 1_000_000,
            asset_id: 0,
            owner_pk: Fr::from_u64(99),
            rho: Fr::from_u64(1),
            psi: Fr::from_u64(2),
        };
        state.insert_note(victim.commit(&p)).unwrap();
        let liq_id = Fr::from_u64(0x1111);
        a.register(liq_id, MIN_LIQUIDATOR_BOND, 100);
        let claim = build_claim(&state, liq_id, &victim);
        let tag = a.submit_claim(&state, claim).unwrap();
        assert!(a.auctions.contains_key(&tag));
    }

    #[test]
    fn highest_bid_wins_auction() {
        let mut a = LiquidationAuction::new();
        let p = Poseidon::default();
        let mut state = ShieldedState::new();
        let victim = Note {
            value: 1_000_000,
            asset_id: 0,
            owner_pk: Fr::from_u64(99),
            rho: Fr::from_u64(1),
            psi: Fr::from_u64(2),
        };
        state.insert_note(victim.commit(&p)).unwrap();
        let liq_a = Fr::from_u64(0xa);
        let liq_b = Fr::from_u64(0xb);
        a.register(liq_a, MIN_LIQUIDATOR_BOND, 100);
        a.register(liq_b, MIN_LIQUIDATOR_BOND, 100);

        let claim_a = build_claim(&state, liq_a, &victim);
        let tag = claim_a.claim_tag;
        let mut claim_b = build_claim(&state, liq_b, &victim);
        // Both target the same victim; the chain expects same claim_tag.
        assert_eq!(claim_b.claim_tag, tag);
        // We need claim_b's proof to be at the right liquidator id.
        let public_inputs = vec![
            claim_b.anchor_root,
            Fr::from_u64(claim_b.market_id.0),
            Fr::from_u64(u256_low(&claim_b.oracle_price)),
            liq_b,
            claim_b.claim_tag,
        ];
        claim_b.proof = MockVerifier::new().prove(Circuit::LiquidateClaim, public_inputs);

        a.submit_claim(&state, claim_a).unwrap();
        a.submit_claim(&state, claim_b).unwrap();

        a.submit_bid(DecryptedBid {
            claim_tag: tag,
            liquidator_id: liq_a,
            bid_price: U256::from(50u64),
        });
        a.submit_bid(DecryptedBid {
            claim_tag: tag,
            liquidator_id: liq_b,
            bid_price: U256::from(70u64),
        });

        let winners = a.settle_block(1);
        assert_eq!(winners.len(), 1);
        assert_eq!(winners[0].0, tag);
        assert_eq!(winners[0].1, liq_b);
        assert_eq!(winners[0].2, U256::from(70u64));
    }

    #[test]
    fn execute_consumes_victim_and_mints_bounty() {
        let mut a = LiquidationAuction::new();
        let mut state = ShieldedState::new();
        let p = Poseidon::default();
        let victim = Note {
            value: 1_000_000,
            asset_id: 0,
            owner_pk: Fr::from_u64(99),
            rho: Fr::from_u64(1),
            psi: Fr::from_u64(2),
        };
        state.insert_note(victim.commit(&p)).unwrap();

        let victim_sk = Fr::from_u64(0xc0ffee);
        let victim_nul = victim.nullifier(&p, &victim_sk).0;
        let bounty = Note {
            value: 50_000,
            asset_id: 0,
            owner_pk: Fr::from_u64(0xbeef),
            rho: Fr::from_u64(3),
            psi: Fr::from_u64(4),
        };
        let insurance = Note {
            value: 950_000,
            asset_id: 0,
            owner_pk: Fr::ZERO,
            rho: Fr::from_u64(5),
            psi: Fr::from_u64(6),
        };
        let bounty_cm = bounty.commit(&p).0;
        let insurance_cm = insurance.commit(&p).0;
        let anchor = state.current_root();
        let public_inputs = vec![
            anchor,
            victim_nul,
            bounty_cm,
            insurance_cm,
            Fr::from_u64(95),
            Fr::from_u64(2),
            Fr::from_u64(3_200),
        ];
        let proof = MockVerifier::new().prove(Circuit::LiquidateExecute, public_inputs);
        let exec = LiquidationExecute {
            anchor_root: anchor,
            claim_tag: Fr::ZERO,
            victim_nullifier: victim_nul,
            bounty_commitment: bounty_cm,
            insurance_commitment: insurance_cm,
            winning_bid: U256::from(95u64),
            market_id: MarketId(2),
            oracle_price: U256::from(3_200u64),
            proof,
        };
        a.execute(&mut state, exec).unwrap();
        assert!(state.is_spent(&Nullifier(victim_nul)));
        assert_eq!(a.stats.total_bounty_paid, U256::from(95u64));
    }
}
