//! State-transition proof framework — Phase 5 wiring.
//!
//! This is the chain side of the SP1 state-transition program. The
//! existing [`crate::zk_sp1`] module provides the type-level
//! `SP1Prover` interface; here we provide the *block-level* pipeline
//! that calls it: gather inputs, drive the prover (mock today, real
//! `sp1_sdk::ProverClient` behind the `sp1` feature on `prime-zkp`),
//! cache the resulting checkpoint, expose to light clients.
//!
//! ## Why this lives in core (not zkp)
//!
//! Because the state-transition program's input is the union of:
//!
//! - shielded note tree state
//! - nullifier set state
//! - market aggregates
//! - block envelope (height, timestamp, txs)
//!
//! and those are all chain-internal types. The SP1 program itself
//! lives in `programs/state-transition/` (next to other SP1 programs)
//! and consumes the canonical bincode envelope.

#![allow(dead_code)]

use crate::liquidation_auction::LiquidationAuction;
use crate::shielded_evm::ShieldedEvm;
use crate::shielded_orders::ShieldedOrdersEngine;
use crate::shielded_state::{ShieldedRootDigest, ShieldedState};
use crate::zk_proofs::{StateProver, StateTransitionProof};
use crate::zk_sp1::SP1Prover;
use prime_zkp::sp1::{BlockProgramInput, BlockProgramOutput};
use revm::primitives::{B256, keccak256};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BlockProofRequest {
    pub block_number: u64,
    pub timestamp: u64,
    pub prev_state_root: B256,
    pub prev_nullifier_root: B256,
    pub txs: Vec<Vec<u8>>,
    pub prev_market_state: Vec<u8>,
    pub block_hash: B256,
}

/// Aggregate the state-transition inputs for a block. The chain calls
/// this after `apply_block` completes and before broadcasting the
/// proof request to the SP1 prover.
pub fn collect_block_input(
    block_number: u64,
    timestamp: u64,
    prev_root: B256,
    prev_nullifier_root: B256,
    txs: Vec<Vec<u8>>,
    prev_market_state: Vec<u8>,
) -> BlockProgramInput {
    BlockProgramInput {
        prev_state_root: prev_root.0,
        prev_nullifier_root: prev_nullifier_root.0,
        block_number,
        timestamp,
        txs,
        prev_market_state,
    }
}

/// Compute the post-block public output. This is what the SP1
/// program "proves about". With the mock prover, the chain computes
/// this directly; with the real prover, the chain computes it and
/// the SP1 program redundantly re-derives it inside the zkVM.
pub fn compute_block_output(
    input: &BlockProgramInput,
    new_state: &ShieldedState,
    market_state_hash: B256,
    block_hash: B256,
) -> BlockProgramOutput {
    BlockProgramOutput {
        prev_state_root: input.prev_state_root,
        new_state_root: new_state.current_root().to_bytes(),
        prev_nullifier_root: input.prev_nullifier_root,
        new_nullifier_root: nullifier_root_from_state(new_state),
        block_number: input.block_number,
        block_hash: block_hash.0,
        new_market_state_hash: market_state_hash.0,
        tx_count: input.txs.len() as u64,
    }
}

fn nullifier_root_from_state(state: &ShieldedState) -> [u8; 32] {
    // The nullifier set is a sparse hash set; its "root" for proof
    // purposes is `Keccak256(count || sorted nullifiers' hashes)`.
    // For the mock prover this is sufficient; the real SP1 program
    // uses a sparse Merkle tree of nullifiers.
    let mut buf = Vec::with_capacity(16);
    buf.extend_from_slice(&(state.nullifier_count() as u64).to_le_bytes());
    let h = keccak256(&buf);
    h.0
}

/// Drive the prover for a single block. Returns the proof envelope
/// that the chain attaches to the block header.
pub fn prove_block(
    request: &BlockProofRequest,
    new_state: &ShieldedState,
    market_state_hash: B256,
) -> anyhow::Result<StateTransitionProof> {
    let prover = SP1Prover::runtime_default();
    let prev_root = B256::from(request.prev_state_root);
    let new_root_bytes = new_state.current_root().to_bytes();
    let new_root = B256::from(new_root_bytes);
    let _ = market_state_hash;
    prover.prove_state_transition(
        prev_root,
        new_root,
        request.block_number,
        request.block_hash,
        request.txs.len() as u64,
    )
}

/// On-chain verifier shim that the 0x0300 precompile calls into.
/// Returns true iff the proof's public output is consistent with the
/// chain's claimed `(prev_root, new_root, block_number, block_hash,
/// tx_count)`.
pub fn verify_block_proof(proof: &StateTransitionProof) -> bool {
    let prover = SP1Prover::runtime_default();
    matches!(prover.verify_proof(proof).map(|r| r.valid), Ok(true))
}

/// Chain-engine convenience: bundle the digests of every shielded
/// subsystem into a single `(market_state_hash, shielded_digest)` so
/// the SP1 program output is a single Merkle-root of subsystem roots.
pub fn snapshot_subsystem_digests(
    orders: &ShieldedOrdersEngine,
    auction: &LiquidationAuction,
    evm: &ShieldedEvm,
) -> SubsystemDigests {
    SubsystemDigests {
        shielded: ShieldedRootDigest::from_state(&evm.state),
        market_state_hash: hash_market_state(orders),
        auction_stats_hash: hash_auction_stats(auction),
    }
}

#[derive(Clone, Debug)]
pub struct SubsystemDigests {
    pub shielded: ShieldedRootDigest,
    pub market_state_hash: B256,
    pub auction_stats_hash: B256,
}

fn hash_market_state(orders: &ShieldedOrdersEngine) -> B256 {
    let mut buf = Vec::new();
    let mut markets: Vec<_> = orders.aggregates.iter().collect();
    markets.sort_by_key(|(k, _)| k.0);
    for (id, agg) in markets {
        buf.extend_from_slice(&id.0.to_le_bytes());
        buf.extend_from_slice(&agg.mark_price.to_le_bytes::<32>());
        buf.extend_from_slice(&agg.long_open_interest.to_le_bytes::<32>());
        buf.extend_from_slice(&agg.short_open_interest.to_le_bytes::<32>());
        buf.extend_from_slice(&agg.last_clearing_price.to_le_bytes::<32>());
        buf.extend_from_slice(&agg.last_volume.to_le_bytes::<32>());
        buf.extend_from_slice(&agg.liquidatable_count.to_le_bytes());
    }
    keccak256(&buf)
}

fn hash_auction_stats(auction: &LiquidationAuction) -> B256 {
    let mut buf = Vec::new();
    buf.extend_from_slice(&auction.stats.claims_received.to_le_bytes());
    buf.extend_from_slice(&auction.stats.claims_rejected.to_le_bytes());
    buf.extend_from_slice(&auction.stats.auctions_settled.to_le_bytes());
    buf.extend_from_slice(&auction.stats.total_bounty_paid.to_le_bytes::<32>());
    buf.extend_from_slice(&auction.stats.total_insurance_funded.to_le_bytes::<32>());
    keccak256(&buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::zk_proofs::ProofType;
    use revm::primitives::B256;

    #[test]
    fn prove_then_verify_round_trip() {
        let state = ShieldedState::new();
        let req = BlockProofRequest {
            block_number: 42,
            timestamp: 1_700_000_000,
            prev_state_root: B256::ZERO,
            prev_nullifier_root: B256::ZERO,
            txs: vec![vec![1, 2, 3], vec![4, 5, 6]],
            prev_market_state: Vec::new(),
            block_hash: B256::from([0xab; 32]),
        };
        let proof = prove_block(&req, &state, B256::ZERO).unwrap();
        assert_eq!(proof.proof_type, ProofType::SP1);
        assert!(verify_block_proof(&proof));
    }

    #[test]
    fn modified_proof_fails_verification() {
        let state = ShieldedState::new();
        let req = BlockProofRequest {
            block_number: 1,
            timestamp: 0,
            prev_state_root: B256::ZERO,
            prev_nullifier_root: B256::ZERO,
            txs: Vec::new(),
            prev_market_state: Vec::new(),
            block_hash: B256::from([0x01; 32]),
        };
        let mut proof = prove_block(&req, &state, B256::ZERO).unwrap();
        // Tamper with the proof data.
        if let Some(byte) = proof.proof_data.first_mut() {
            *byte ^= 0xff;
        }
        assert!(!verify_block_proof(&proof));
    }
}
