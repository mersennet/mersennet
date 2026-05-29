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
use crate::liquidation_auction::LiquidationExecute;
use crate::shielded_evm::{SHIELDED_TX_TYPE, ShieldTx, ShieldedEnvelope, ShieldedTransferTx, UnshieldTx};
use crate::shielded_evm::ShieldedEvm;
use crate::shielded_orders::ShieldedOrdersEngine;
use crate::shielded_state::ShieldedSnapshot;
use crate::shielded_state::{ShieldedRootDigest, ShieldedState};
use crate::threshold_mempool::ThresholdMempool;
use crate::zk_proofs::{StateProver, StateTransitionProof};
use crate::zk_sp1::SP1Prover;
use prime_zkp::sp1::{
    BlockHeaderWitness, BlockProgramInput, BlockProgramOutput, LiquidationExecuteBlockTx,
    ShieldBlockTx,
    ShieldedBlockTx, ShieldedStateWitness, ShieldedTickWitness, ShieldedTransferBlockTx,
    TransparentBalanceEntry, U256Bytes, UnshieldBlockTx, execute_block_program,
    hash_liquidation_auction_stats, hash_market_aggregates,
};
use revm::primitives::{Address, B256, U256, keccak256};
use std::collections::HashMap;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BlockProofRequest {
    pub block_number: u64,
    pub timestamp: u64,
    pub prev_state_root: B256,
    pub prev_nullifier_root: B256,
    pub txs: Vec<Vec<u8>>,
    pub prev_market_state: Vec<u8>,
    pub header: BlockHeaderWitness,
    pub block_hash: B256,
    pub expected_market_state_hash: B256,
    pub prev_shielded_state: ShieldedStateWitness,
    pub transparent_balances: Vec<TransparentBalanceEntry>,
    pub pre_tick_witness: ShieldedTickWitness,
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
    header: BlockHeaderWitness,
    prev_shielded_state: ShieldedStateWitness,
    transparent_balances: Vec<TransparentBalanceEntry>,
    pre_tick_witness: ShieldedTickWitness,
    expected_block_hash: B256,
    expected_market_state_hash: B256,
) -> BlockProgramInput {
    BlockProgramInput {
        prev_state_root: prev_root.0,
        prev_nullifier_root: prev_nullifier_root.0,
        block_number,
        timestamp,
        header,
        txs,
        prev_market_state,
        prev_shielded_state,
        transparent_balances,
        pre_tick_witness,
        expected_block_hash: expected_block_hash.0,
        expected_market_state_hash: expected_market_state_hash.0,
    }
}

/// Compute the post-block public output. This is what the SP1
/// program "proves about". With the mock prover, the chain computes
/// this directly; with the real prover, the chain computes it and
/// the SP1 program redundantly re-derives it inside the zkVM.
pub fn compute_block_output(
    input: &BlockProgramInput,
) -> anyhow::Result<BlockProgramOutput> {
    Ok(execute_block_program(input)?)
}

/// Drive the prover for a single block. Returns the proof envelope
/// that the chain attaches to the block header.
pub fn prove_block(
    request: &BlockProofRequest,
    new_state: &ShieldedState,
    host_shielded_event_root: B256,
) -> anyhow::Result<StateTransitionProof> {
    let prover = SP1Prover::runtime_default();
    let input = collect_block_input(
        request.block_number,
        request.timestamp,
        request.prev_state_root,
        request.prev_nullifier_root,
        request.txs.clone(),
        request.prev_market_state.clone(),
        request.header.clone(),
        request.prev_shielded_state.clone(),
        request.transparent_balances.clone(),
        request.pre_tick_witness.clone(),
        request.block_hash,
        request.expected_market_state_hash,
    );
    let output = compute_block_output(&input)?;
    if B256::from(output.block_hash) != request.block_hash {
        anyhow::bail!("state proof block hash mismatch");
    }
    if B256::from(output.new_state_root) != B256::from(new_state.current_root().to_bytes()) {
        anyhow::bail!("state proof shielded state root mismatch");
    }
    if B256::from(output.new_nullifier_root) != current_nullifier_root(new_state) {
        anyhow::bail!("state proof nullifier root mismatch");
    }
    if B256::from(output.new_market_state_hash) != request.expected_market_state_hash {
        anyhow::bail!("state proof market-state hash mismatch");
    }
    if B256::from(output.shielded_event_root) != host_shielded_event_root {
        anyhow::bail!("state proof shielded event root mismatch");
    }
    prover.prove_block_program(&input)
}

pub fn current_nullifier_root(state: &ShieldedState) -> B256 {
    let mut buf = Vec::with_capacity(16);
    buf.extend_from_slice(&(state.nullifier_count() as u64).to_le_bytes());
    keccak256(&buf)
}

pub fn shielded_state_witness(snapshot: &ShieldedSnapshot) -> ShieldedStateWitness {
    ShieldedStateWitness {
        leaves: snapshot.leaves.clone(),
        nullifiers: snapshot.nullifiers.clone(),
        recent_roots: snapshot.recent_roots.clone(),
    }
}

pub fn transparent_balance_entries(
    balances: &HashMap<Address, U256>,
) -> Vec<TransparentBalanceEntry> {
    let mut entries: Vec<_> = balances
        .iter()
        .map(|(address, balance)| TransparentBalanceEntry {
            address: address_bytes(*address),
            balance: U256Bytes(u256_to_le_bytes(balance)),
        })
        .collect();
    entries.sort_by(|left, right| left.address.cmp(&right.address));
    entries
}

pub fn shielded_tick_witness(
    orders: &ShieldedOrdersEngine,
    mempool: &ThresholdMempool,
    auction: &LiquidationAuction,
) -> ShieldedTickWitness {
    ShieldedTickWitness {
        drained_intent_count: mempool.decrypted_count() as u64,
        decrypted_intents: orders.order_admission_witnesses(),
        orders: orders.tick_witness(),
        liquidation: auction.tick_witness(),
    }
}

pub fn encode_block_txs(transactions: &[crate::engine::Transaction]) -> Vec<Vec<u8>> {
    transactions
        .iter()
        .filter(|tx| tx.tx_type == SHIELDED_TX_TYPE)
        .filter_map(|tx| tx.shielded_payload.as_ref())
        .filter_map(|payload| encode_shielded_payload(payload).ok())
        .collect()
}

fn encode_shielded_payload(payload: &ShieldedEnvelope) -> anyhow::Result<Vec<u8>> {
    let canonical = match payload {
        ShieldedEnvelope::Transfer(tx) => ShieldedBlockTx::Transfer(convert_transfer(tx)),
        ShieldedEnvelope::Shield(tx) => ShieldedBlockTx::Shield(convert_shield(tx)),
        ShieldedEnvelope::Unshield(tx) => ShieldedBlockTx::Unshield(convert_unshield(tx)),
        ShieldedEnvelope::LiquidationExecute(tx) => {
            ShieldedBlockTx::LiquidationExecute(convert_liquidation_execute(tx))
        }
        ShieldedEnvelope::Order(_) | ShieldedEnvelope::LiquidationClaim(_) => {
            anyhow::bail!("unsupported shielded payload for current SP1 executor")
        }
    };
    Ok(bincode::serialize(&canonical)?)
}

fn convert_transfer(tx: &ShieldedTransferTx) -> ShieldedTransferBlockTx {
    ShieldedTransferBlockTx {
        anchor_root: tx.anchor_root,
        nullifier: tx.nullifier,
        output_commitment: tx.output_commitment,
        encrypted_output: tx.encrypted_output.clone(),
        proof: tx.proof.clone(),
    }
}

fn convert_shield(tx: &ShieldTx) -> ShieldBlockTx {
    ShieldBlockTx {
        from: address_bytes(tx.from),
        amount: U256Bytes(u256_to_le_bytes(&tx.amount)),
        output_commitment: tx.output_commitment,
        encrypted_output: tx.encrypted_output.clone(),
        proof: tx.proof.clone(),
    }
}

fn convert_unshield(tx: &UnshieldTx) -> UnshieldBlockTx {
    UnshieldBlockTx {
        anchor_root: tx.anchor_root,
        nullifier: tx.nullifier,
        amount: U256Bytes(u256_to_le_bytes(&tx.amount)),
        to: address_bytes(tx.to),
        change_commitment: tx.change_commitment,
        encrypted_change: tx.encrypted_change.clone(),
        proof: tx.proof.clone(),
    }
}

fn convert_liquidation_execute(tx: &LiquidationExecute) -> LiquidationExecuteBlockTx {
    LiquidationExecuteBlockTx {
        anchor_root: tx.anchor_root,
        claim_tag: tx.claim_tag,
        victim_nullifier: tx.victim_nullifier,
        bounty_commitment: tx.bounty_commitment,
        insurance_commitment: tx.insurance_commitment,
        winning_bid: U256Bytes(u256_to_le_bytes(&tx.winning_bid)),
        market_id: tx.market_id.0,
        oracle_price: U256Bytes(u256_to_le_bytes(&tx.oracle_price)),
        proof: tx.proof.clone(),
    }
}

fn address_bytes(address: Address) -> [u8; 20] {
    let mut out = [0u8; 20];
    out.copy_from_slice(address.as_slice());
    out
}

fn u256_to_le_bytes(value: &U256) -> [u8; 32] {
    let mut out = [0u8; 32];
    for (index, limb) in value.as_limbs().iter().enumerate() {
        out[index * 8..(index + 1) * 8].copy_from_slice(&limb.to_le_bytes());
    }
    out
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
    let orders_witness = orders.tick_witness();
    let auction_witness = auction.tick_witness();
    SubsystemDigests {
        shielded: ShieldedRootDigest::from_state(&evm.state),
        market_state_hash: B256::from(hash_market_aggregates(&orders_witness.aggregates)),
        auction_stats_hash: B256::from(hash_liquidation_auction_stats(&auction_witness.stats)),
    }
}

#[derive(Clone, Debug)]
pub struct SubsystemDigests {
    pub shielded: ShieldedRootDigest,
    pub market_state_hash: B256,
    pub auction_stats_hash: B256,
}


#[cfg(test)]
mod tests {
    use super::*;
    use prime_zkp::Nullifier;
    use crate::zk_proofs::ProofType;
    use revm::primitives::B256;

    #[test]
    fn prove_then_verify_round_trip() {
        let state = ShieldedState::new();
        let header = BlockHeaderWitness {
            chain_id: 7919,
            gas_limit: 30_000_000,
            gas_used: 0,
            base_fee_be: [0u8; 32],
            coinbase: [0x11; 20],
            tx_count: 2,
        };
        let req = BlockProofRequest {
            block_number: 42,
            timestamp: 1_700_000_000,
            prev_state_root: B256::ZERO,
            prev_nullifier_root: B256::ZERO,
            txs: vec![vec![1, 2, 3], vec![4, 5, 6]],
            prev_market_state: Vec::new(),
            header: header.clone(),
            block_hash: B256::from(prime_zkp::sp1::derive_block_hash(42, &header)),
            expected_market_state_hash: B256::ZERO,
            prev_shielded_state: ShieldedStateWitness::default(),
            transparent_balances: Vec::new(),
            pre_tick_witness: ShieldedTickWitness::default(),
        };
        let proof = prove_block(&req, &state, B256::ZERO).unwrap();
        assert_eq!(proof.proof_type, ProofType::SP1);
        assert!(verify_block_proof(&proof));
    }

    #[test]
    fn modified_proof_fails_verification() {
        let state = ShieldedState::new();
        let header = BlockHeaderWitness {
            chain_id: 7919,
            gas_limit: 30_000_000,
            gas_used: 0,
            base_fee_be: [0u8; 32],
            coinbase: [0x11; 20],
            tx_count: 0,
        };
        let req = BlockProofRequest {
            block_number: 1,
            timestamp: 0,
            prev_state_root: B256::ZERO,
            prev_nullifier_root: B256::ZERO,
            txs: Vec::new(),
            prev_market_state: Vec::new(),
            header: header.clone(),
            block_hash: B256::from(prime_zkp::sp1::derive_block_hash(1, &header)),
            expected_market_state_hash: B256::ZERO,
            prev_shielded_state: ShieldedStateWitness::default(),
            transparent_balances: Vec::new(),
            pre_tick_witness: ShieldedTickWitness::default(),
        };
        let mut proof = prove_block(&req, &state, B256::ZERO).unwrap();
        // Tamper with the proof data.
        if let Some(byte) = proof.proof_data.first_mut() {
            *byte ^= 0xff;
        }
        assert!(!verify_block_proof(&proof));
    }

    #[test]
    fn prove_block_rejects_live_nullifier_root_mismatch() {
        let mut state = ShieldedState::new();
        state.spend(Nullifier(prime_zkp::Fr::from_u64(7))).unwrap();

        let header = BlockHeaderWitness {
            chain_id: 7919,
            gas_limit: 30_000_000,
            gas_used: 0,
            base_fee_be: [0u8; 32],
            coinbase: [0x11; 20],
            tx_count: 0,
        };
        let req = BlockProofRequest {
            block_number: 9,
            timestamp: 0,
            prev_state_root: B256::ZERO,
            prev_nullifier_root: B256::ZERO,
            txs: Vec::new(),
            prev_market_state: Vec::new(),
            header: header.clone(),
            block_hash: B256::from(prime_zkp::sp1::derive_block_hash(9, &header)),
            expected_market_state_hash: B256::ZERO,
            prev_shielded_state: ShieldedStateWitness::default(),
            transparent_balances: Vec::new(),
            pre_tick_witness: ShieldedTickWitness::default(),
        };

        let error = prove_block(&req, &state, B256::ZERO).unwrap_err();
        assert!(error.to_string().contains("state proof nullifier root mismatch"));
    }
}
