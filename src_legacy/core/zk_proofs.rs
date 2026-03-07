//! ZK state proof verification module for Prime Chain.
//!
//! Provides trustless bridges and light clients via state transition proofs.
//! Supports mock prover for testing and extensible interface for future STARK/SNARK backends.

use anyhow::Result;
use revm::primitives::{keccak256, B256};
use serde::{Deserialize, Serialize};
use std::time::Instant;

// ---------------------------------------------------------------------------
// State Transition Proof Types
// ---------------------------------------------------------------------------

#[allow(dead_code)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StateTransitionProof {
    pub prev_state_root: B256,
    pub new_state_root: B256,
    pub block_height: u64,
    pub block_hash: B256,
    pub tx_count: u64,
    pub proof_data: Vec<u8>,
    pub proof_type: ProofType,
    pub timestamp: u64,
}

#[allow(dead_code)]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum ProofType {
    Mock,    // For testing
    SP1,     // Future: Succinct SP1 STARK
    Groth16, // Future: Groth16 SNARK
    Plonk,   // Future: PLONK
}

#[allow(dead_code)]
pub struct ProofVerificationResult {
    pub valid: bool,
    pub prev_root: B256,
    pub new_root: B256,
    pub proof_type: ProofType,
    pub verification_time_ms: f64,
}

// ---------------------------------------------------------------------------
// Prover Trait
// ---------------------------------------------------------------------------

#[allow(dead_code)]
pub trait StateProver: Send + Sync {
    fn prove_state_transition(
        &self,
        prev_root: B256,
        new_root: B256,
        block_height: u64,
        block_hash: B256,
        tx_count: u64,
    ) -> Result<StateTransitionProof>;

    fn verify_proof(&self, proof: &StateTransitionProof) -> Result<ProofVerificationResult>;

    fn proof_type(&self) -> ProofType;
}

// ---------------------------------------------------------------------------
// Mock Prover (for testing)
// ---------------------------------------------------------------------------

#[allow(dead_code)]
pub struct MockProver;

impl MockProver {
    pub fn new() -> Self {
        Self
    }

    fn compute_proof_hash(
        prev_root: B256,
        new_root: B256,
        block_height: u64,
        block_hash: B256,
        tx_count: u64,
    ) -> B256 {
        let mut buf = Vec::with_capacity(32 * 3 + 8 + 8);
        buf.extend_from_slice(prev_root.as_slice());
        buf.extend_from_slice(new_root.as_slice());
        buf.extend_from_slice(block_hash.as_slice());
        buf.extend_from_slice(&block_height.to_be_bytes());
        buf.extend_from_slice(&tx_count.to_be_bytes());
        keccak256(&buf)
    }
}

impl Default for MockProver {
    fn default() -> Self {
        Self::new()
    }
}

impl StateProver for MockProver {
    fn prove_state_transition(
        &self,
        prev_root: B256,
        new_root: B256,
        block_height: u64,
        block_hash: B256,
        tx_count: u64,
    ) -> Result<StateTransitionProof> {
        let proof_data = Self::compute_proof_hash(
            prev_root,
            new_root,
            block_height,
            block_hash,
            tx_count,
        )
        .0
        .to_vec();

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Ok(StateTransitionProof {
            prev_state_root: prev_root,
            new_state_root: new_root,
            block_height,
            block_hash,
            tx_count,
            proof_data,
            proof_type: ProofType::Mock,
            timestamp,
        })
    }

    fn verify_proof(&self, proof: &StateTransitionProof) -> Result<ProofVerificationResult> {
        let start = Instant::now();

        let expected = Self::compute_proof_hash(
            proof.prev_state_root,
            proof.new_state_root,
            proof.block_height,
            proof.block_hash,
            proof.tx_count,
        );

        let valid = proof.proof_data.len() == 32
            && proof.proof_data.as_slice() == expected.as_slice();

        let verification_time_ms = start.elapsed().as_secs_f64() * 1000.0;

        Ok(ProofVerificationResult {
            valid,
            prev_root: proof.prev_state_root,
            new_root: proof.new_state_root,
            proof_type: proof.proof_type.clone(),
            verification_time_ms,
        })
    }

    fn proof_type(&self) -> ProofType {
        ProofType::Mock
    }
}

// ---------------------------------------------------------------------------
// BatchProofAggregator
// ---------------------------------------------------------------------------

#[allow(dead_code)]
pub struct BatchProofAggregator {
    proofs: Vec<StateTransitionProof>,
    batch_size: usize,
}

impl BatchProofAggregator {
    pub fn new(batch_size: usize) -> Self {
        Self {
            proofs: Vec::new(),
            batch_size,
        }
    }

    pub fn add_proof(&mut self, proof: StateTransitionProof) {
        self.proofs.push(proof);
    }

    pub fn should_aggregate(&self) -> bool {
        self.proofs.len() >= self.batch_size
    }

    pub fn aggregate(&mut self) -> Option<StateTransitionProof> {
        if self.proofs.len() < self.batch_size {
            return None;
        }

        let batch: Vec<_> = self.proofs.drain(..self.batch_size).collect();
        let first = batch.first()?;
        let last = batch.last()?;

        let mut combined = Vec::new();
        for p in &batch {
            combined.extend_from_slice(&p.proof_data);
        }
        let aggregated_data = keccak256(&combined).0.to_vec();

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let aggregate_proof = StateTransitionProof {
            prev_state_root: first.prev_state_root,
            new_state_root: last.new_state_root,
            block_height: last.block_height,
            block_hash: last.block_hash,
            tx_count: batch.iter().map(|p| p.tx_count).sum(),
            proof_data: aggregated_data,
            proof_type: ProofType::Mock,
            timestamp,
        };

        Some(aggregate_proof)
    }

    pub fn pending_count(&self) -> usize {
        self.proofs.len()
    }
}

// ---------------------------------------------------------------------------
// ProofCheckpoint and CheckpointStore
// ---------------------------------------------------------------------------

#[allow(dead_code)]
pub struct ProofCheckpoint {
    pub height: u64,
    pub state_root: B256,
    pub proof: StateTransitionProof,
    pub timestamp: u64,
}

#[allow(dead_code)]
pub struct CheckpointStore {
    checkpoints: Vec<ProofCheckpoint>,
    max_checkpoints: usize,
}

impl CheckpointStore {
    pub fn new(max_checkpoints: usize) -> Self {
        Self {
            checkpoints: Vec::new(),
            max_checkpoints,
        }
    }

    pub fn add(&mut self, checkpoint: ProofCheckpoint) {
        self.checkpoints.push(checkpoint);
        if self.checkpoints.len() > self.max_checkpoints {
            self.checkpoints.remove(0);
        }
    }

    pub fn latest(&self) -> Option<&ProofCheckpoint> {
        self.checkpoints.last()
    }

    pub fn at_height(&self, height: u64) -> Option<&ProofCheckpoint> {
        self.checkpoints.iter().find(|c| c.height == height)
    }

    pub fn verify_chain(&self, prover: &dyn StateProver) -> Result<bool> {
        if self.checkpoints.len() < 2 {
            return Ok(true);
        }

        let mut sorted: Vec<_> = self.checkpoints.iter().collect();
        sorted.sort_by_key(|c| c.height);

        for i in 0..sorted.len() - 1 {
            let curr = sorted[i];
            let next = sorted[i + 1];

            let result = prover.verify_proof(&curr.proof)?;
            if !result.valid {
                return Ok(false);
            }

            if curr.proof.new_state_root != next.proof.prev_state_root {
                return Ok(false);
            }
        }

        let last_result = prover.verify_proof(&sorted.last().unwrap().proof)?;
        Ok(last_result.valid)
    }
}
