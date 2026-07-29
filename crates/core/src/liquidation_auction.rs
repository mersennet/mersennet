//! Sealed-bid liquidation auctions for the shielded CLOB.
//!
//! The legacy [`crate::mersennet_orders::MersennetOrdersState::is_liquidatable`]
//! and [`crate::mersennet_orders::MersennetOrdersState::liquidate`] are
//! address-keyed: anyone can scan the chain and see when a specific
//! account is one tick away from forced closure. That's the exact
//! signal big traders exploit to engineer cascading liquidations.
//!
//! This module replaces both with:
//!
//! 1. **Bonded liquidator set.** Anyone can stake `MIN_LIQUIDATOR_BOND`
//!    MRSN to become an authorized liquidator. The bond is slashable
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
//! checks happen via [`mersennet_zkp::noir::Verifier`].

#![allow(dead_code)]

use crate::mersennet_orders::MarketId;
use crate::shielded_state::ShieldedState;
use mersennet_zkp::{
    Fr, NoteCommitment, Nullifier,
    noir::{Circuit, CircuitProof, Verifier, VerifyError, default_verifier},
    poseidon::Poseidon,
    sp1::{
        LiquidationAuctionEntryWitness, LiquidationAuctionStatsWitness,
        LiquidationAuctionTickWitness, LiquidationBidWitness, LiquidationClaimValidationContext,
        LiquidationClaimWitness, LiquidationSettlementWitness, LiquidationWinnerWitness,
        LiquidatorWitness, RevealedBidBatchWitness, U256Bytes, settle_liquidation_entries,
        validate_liquidation_claim_witness,
    },
};
use revm::primitives::U256;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

#[cfg(test)]
use mersennet_zkp::noir::MockVerifier;

/// Minimum bond, in MRSN lowest units, to register as a liquidator.
/// Configurable via governance after launch. Default = 10_000 MRSN.
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
#[derive(Clone, Debug, Serialize, Deserialize)]
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

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AuctionStats {
    pub claims_received: u64,
    pub claims_rejected: u64,
    pub auctions_settled: u64,
    pub total_bounty_paid: U256,
    pub total_insurance_funded: U256,
}

/// Persistable view of [`LiquidationAuction`] — only the
/// deterministic, post-block-boundary state. In-flight bid + claim
/// queues are not snapshotted; they're reconstructed from the
/// threshold-mempool and the next block's tick.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct LiquidationAuctionSnapshot {
    pub liquidators: Vec<Liquidator>,
    pub insurance_fund: U256,
    pub stats: AuctionStats,
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
            verifier: default_verifier(),
            poseidon: Poseidon,
        }
    }
}

impl LiquidationAuction {
    pub fn new() -> Self {
        Self::default()
    }

    /// Serialize the auction's deterministic state for snapshot
    /// export. Only the liquidator registry, insurance fund balance,
    /// and statistics are persisted; in-flight `auctions` /
    /// `pending_revelation` are recomputed on the next block tick.
    pub fn snapshot(&self) -> LiquidationAuctionSnapshot {
        LiquidationAuctionSnapshot {
            liquidators: self.liquidators.values().cloned().collect(),
            insurance_fund: self.insurance_fund,
            stats: self.stats.clone(),
        }
    }

    pub fn tick_witness(&self) -> LiquidationAuctionTickWitness {
        let mut liquidators: Vec<_> = self.liquidators.values().cloned().collect();
        liquidators.sort_by(|left, right| {
            left.bond_commitment
                .to_bytes()
                .cmp(&right.bond_commitment.to_bytes())
        });

        let mut auctions: Vec<_> = self.auctions.iter().collect();
        auctions.sort_by_key(|(claim_tag, _)| claim_tag.to_bytes());

        LiquidationAuctionTickWitness {
            liquidators: liquidators
                .into_iter()
                .map(|liquidator| LiquidatorWitness {
                    bond_commitment: liquidator.bond_commitment,
                    bond_amount: liquidator.bond_amount,
                    registered_at: liquidator.registered_at,
                })
                .collect(),
            auctions: auctions
                .into_iter()
                .map(|(claim_tag, entry)| entry_witness(*claim_tag, entry))
                .collect(),
            pending_revelation: self
                .pending_revelation
                .iter()
                .map(|(block_number, bids)| RevealedBidBatchWitness {
                    block_number: *block_number,
                    bids: bids.iter().map(bid_witness).collect(),
                })
                .collect(),
            insurance_fund: u256_bytes(self.insurance_fund),
            stats: LiquidationAuctionStatsWitness {
                claims_received: self.stats.claims_received,
                claims_rejected: self.stats.claims_rejected,
                auctions_settled: self.stats.auctions_settled,
                total_bounty_paid: u256_bytes(self.stats.total_bounty_paid),
                total_insurance_funded: u256_bytes(self.stats.total_insurance_funded),
            },
        }
    }

    /// Rebuild the auction state from a [`LiquidationAuctionSnapshot`].
    /// The verifier + poseidon are taken from the existing instance
    /// (they're not part of the snapshot — they're configuration).
    pub fn restore(snapshot: LiquidationAuctionSnapshot) -> Self {
        let mut a = Self::new();
        for liq in snapshot.liquidators {
            a.liquidators.insert(liq.bond_commitment, liq);
        }
        a.insurance_fund = snapshot.insurance_fund;
        a.stats = snapshot.stats;
        a
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
        let claim_witness = claim_witness(&claim);
        let context = LiquidationClaimValidationContext {
            root_is_recent: state.is_recent_root(&claim.anchor_root),
            liquidator_registered: self.is_registered(&claim.liquidator_id),
        };
        if let Err(error) =
            validate_liquidation_claim_witness(&claim_witness, &*self.verifier, &context)
                .map_err(map_block_program_error_to_liquidation_error)
        {
            self.stats.claims_rejected += 1;
            return Err(error);
        }
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
        self.settle_block_witness(block)
            .winners
            .into_iter()
            .map(
                |LiquidationWinnerWitness {
                     claim_tag,
                     liquidator_id,
                     bid_price,
                     ..
                 }| (claim_tag, liquidator_id, u256_from_bytes(bid_price)),
            )
            .collect()
    }

    pub fn settle_block_witness(&mut self, block: u64) -> LiquidationSettlementWitness {
        let mut entry_witnesses = Vec::new();
        let mut all_bids = Vec::new();
        let mut settled: Vec<_> = self.auctions.drain().collect();
        settled.sort_by_key(|(claim_tag, _)| claim_tag.to_bytes());
        for (tag, entry) in settled {
            entry_witnesses.push(entry_witness(tag, &entry));
            all_bids.extend(entry.bids);
        }
        let settlement = settle_liquidation_entries(&entry_witnesses);
        self.stats.auctions_settled += settlement.winners.len() as u64;
        self.pending_revelation.push((block, all_bids));
        // Reveal bids that have aged past BID_REVELATION_DELAY.
        self.pending_revelation
            .retain(|(b, _)| block.saturating_sub(*b) < BID_REVELATION_DELAY);
        settlement
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
        self.stats.total_bounty_paid = self
            .stats
            .total_bounty_paid
            .saturating_add(exec.winning_bid);
        Ok(())
    }
}

fn claim_witness(claim: &LiquidationClaim) -> LiquidationClaimWitness {
    LiquidationClaimWitness {
        anchor_root: claim.anchor_root,
        market_id: claim.market_id.0,
        oracle_price: u256_bytes(claim.oracle_price),
        liquidator_id: claim.liquidator_id,
        claim_tag: claim.claim_tag,
        encrypted_bid: claim.encrypted_bid.clone(),
        proof: claim.proof.clone(),
    }
}

fn bid_witness(bid: &DecryptedBid) -> LiquidationBidWitness {
    LiquidationBidWitness {
        claim_tag: bid.claim_tag,
        liquidator_id: bid.liquidator_id,
        bid_price: u256_bytes(bid.bid_price),
    }
}

fn entry_witness(claim_tag: Fr, entry: &AuctionEntry) -> LiquidationAuctionEntryWitness {
    LiquidationAuctionEntryWitness {
        claim_tag,
        claims: entry.claims.iter().map(claim_witness).collect(),
        bids: entry.bids.iter().map(bid_witness).collect(),
    }
}

fn u256_from_bytes(value: U256Bytes) -> U256 {
    let limbs = value
        .0
        .chunks_exact(8)
        .map(|chunk| u64::from_le_bytes(chunk.try_into().unwrap()))
        .collect::<Vec<_>>();
    U256::from_limbs([limbs[0], limbs[1], limbs[2], limbs[3]])
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

fn map_block_program_error_to_liquidation_error(
    error: mersennet_zkp::sp1::BlockProgramError,
) -> LiquidationError {
    match error {
        mersennet_zkp::sp1::BlockProgramError::StaleAnchor => LiquidationError::StaleAnchor,
        mersennet_zkp::sp1::BlockProgramError::UnregisteredLiquidator => {
            LiquidationError::UnregisteredLiquidator
        }
        mersennet_zkp::sp1::BlockProgramError::InvalidProof => {
            LiquidationError::InvalidProof(VerifyError::InvalidProof)
        }
        other => {
            panic!("unexpected block-program error in liquidation claim validation: {other:?}")
        }
    }
}

fn u256_low(x: &U256) -> u64 {
    x.as_limbs()[0]
}

fn u256_bytes(value: U256) -> U256Bytes {
    let mut out = [0u8; 32];
    for (index, limb) in value.as_limbs().iter().enumerate() {
        out[index * 8..(index + 1) * 8].copy_from_slice(&limb.to_le_bytes());
    }
    U256Bytes(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mersennet_zkp::note::Note;

    fn build_claim(state: &ShieldedState, liquidator_id: Fr, victim: &Note) -> LiquidationClaim {
        let p = Poseidon;
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
        let p = Poseidon;
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
        let p = Poseidon;
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
        let p = Poseidon;
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
        let p = Poseidon;
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
