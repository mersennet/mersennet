//! SP1 program envelope shared with `crates/core/src/zk_sp1.rs`.
//!
//! Until the real `sp1_sdk` is wired (Phase 5), this module provides
//! the type surface the SP1 state-transition program will read/write:
//! input bytes for what the program proves about, output bytes for
//! what becomes public. Everything is canonical bincode so the same
//! bytes work inside the zkVM and outside.

use crate::field::Fr;
use serde::{Deserialize, Serialize};
use sha3::{Digest, Keccak256};

/// Input to the SP1 block-proving program.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BlockProgramInput {
    pub prev_state_root: [u8; 32],
    pub prev_nullifier_root: [u8; 32],
    pub block_number: u64,
    pub timestamp: u64,
    /// Bincoded list of transaction envelopes; the program decodes
    /// inside the zkVM. Includes shielded transfers, shielded order
    /// intents, liquidation auction settlements, and (legacy)
    /// transparent EVM transactions.
    pub txs: Vec<Vec<u8>>,
    /// Public market state at the start of the block.
    pub prev_market_state: Vec<u8>,
}

/// Public output of the SP1 program. Becomes `public_values` of the
/// resulting proof.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BlockProgramOutput {
    pub prev_state_root: [u8; 32],
    pub new_state_root: [u8; 32],
    pub prev_nullifier_root: [u8; 32],
    pub new_nullifier_root: [u8; 32],
    pub block_number: u64,
    pub block_hash: [u8; 32],
    pub new_market_state_hash: [u8; 32],
    pub tx_count: u64,
}

/// Canonical deterministic block executor used by the current SP1
/// proving path. This consumes the private witness and derives the
/// public output without relying on host-supplied post-state fields.
pub fn execute_block_program(input: &BlockProgramInput) -> BlockProgramOutput {
    let tx_count = input.txs.len() as u64;
    let txs_commitment = hash_transactions(&input.txs);
    let prev_market_state_hash = hash_bytes(b"prime_chain_sp1_prev_market_state_v1", &[&input.prev_market_state]);
    let new_market_state_hash = hash_bytes(
        b"prime_chain_sp1_market_state_v1",
        &[
            &input.prev_market_state,
            &input.block_number.to_le_bytes(),
            &input.timestamp.to_le_bytes(),
            &tx_count.to_le_bytes(),
            &txs_commitment,
            &prev_market_state_hash,
        ],
    );
    let new_nullifier_root = hash_bytes(
        b"prime_chain_sp1_nullifier_root_v1",
        &[
            &input.prev_nullifier_root,
            &input.block_number.to_le_bytes(),
            &input.timestamp.to_le_bytes(),
            &tx_count.to_le_bytes(),
            &txs_commitment,
        ],
    );
    let new_state_root = hash_bytes(
        b"prime_chain_sp1_state_root_v1",
        &[
            &input.prev_state_root,
            &input.prev_nullifier_root,
            &input.block_number.to_le_bytes(),
            &input.timestamp.to_le_bytes(),
            &tx_count.to_le_bytes(),
            &txs_commitment,
            &new_market_state_hash,
            &new_nullifier_root,
        ],
    );
    let block_hash = hash_bytes(
        b"prime_chain_sp1_block_hash_v1",
        &[
            &input.prev_state_root,
            &new_state_root,
            &input.prev_nullifier_root,
            &new_nullifier_root,
            &input.block_number.to_le_bytes(),
            &input.timestamp.to_le_bytes(),
            &tx_count.to_le_bytes(),
            &txs_commitment,
            &new_market_state_hash,
        ],
    );

    BlockProgramOutput {
        prev_state_root: input.prev_state_root,
        new_state_root,
        prev_nullifier_root: input.prev_nullifier_root,
        new_nullifier_root,
        block_number: input.block_number,
        block_hash,
        new_market_state_hash,
        tx_count,
    }
}

fn hash_transactions(txs: &[Vec<u8>]) -> [u8; 32] {
    let mut hasher = Keccak256::new();
    hasher.update(b"prime_chain_sp1_txs_v1");
    hasher.update((txs.len() as u64).to_le_bytes());
    for tx in txs {
        hasher.update((tx.len() as u64).to_le_bytes());
        hasher.update(Keccak256::digest(tx));
    }
    finalize_hash(hasher)
}

fn hash_bytes(domain: &[u8], parts: &[&[u8]]) -> [u8; 32] {
    let mut hasher = Keccak256::new();
    hasher.update(domain);
    hasher.update((parts.len() as u64).to_le_bytes());
    for part in parts {
        hasher.update((part.len() as u64).to_le_bytes());
        hasher.update(part);
    }
    finalize_hash(hasher)
}

fn finalize_hash(hasher: Keccak256) -> [u8; 32] {
    let digest = hasher.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest);
    out
}

impl BlockProgramOutput {
    /// Field-element representation of the public output, used when a
    /// downstream circuit (e.g. a bridge verifier on another chain)
    /// needs to consume it via Noir.
    pub fn to_field_elements(&self) -> Vec<Fr> {
        vec![
            Fr::from_bytes_reduce(&self.prev_state_root),
            Fr::from_bytes_reduce(&self.new_state_root),
            Fr::from_bytes_reduce(&self.prev_nullifier_root),
            Fr::from_bytes_reduce(&self.new_nullifier_root),
            Fr::from_u64(self.block_number),
            Fr::from_bytes_reduce(&self.block_hash),
            Fr::from_bytes_reduce(&self.new_market_state_hash),
            Fr::from_u64(self.tx_count),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_round_trips_via_bincode() {
        let o = BlockProgramOutput {
            prev_state_root: [1u8; 32],
            new_state_root: [2u8; 32],
            prev_nullifier_root: [3u8; 32],
            new_nullifier_root: [4u8; 32],
            block_number: 100,
            block_hash: [5u8; 32],
            new_market_state_hash: [6u8; 32],
            tx_count: 12,
        };
        let bytes = bincode::serialize(&o).unwrap();
        let back: BlockProgramOutput = bincode::deserialize(&bytes).unwrap();
        assert_eq!(back.block_number, 100);
        assert_eq!(back.tx_count, 12);
    }

    #[test]
    fn to_field_elements_is_canonical_length() {
        let o = BlockProgramOutput {
            prev_state_root: [1u8; 32],
            new_state_root: [2u8; 32],
            prev_nullifier_root: [3u8; 32],
            new_nullifier_root: [4u8; 32],
            block_number: 100,
            block_hash: [5u8; 32],
            new_market_state_hash: [6u8; 32],
            tx_count: 12,
        };
        assert_eq!(o.to_field_elements().len(), 8);
    }

    #[test]
    fn execute_block_program_is_deterministic() {
        let input = BlockProgramInput {
            prev_state_root: [1u8; 32],
            prev_nullifier_root: [2u8; 32],
            block_number: 7,
            timestamp: 1_700_000_000,
            txs: vec![vec![1, 2, 3], vec![4, 5]],
            prev_market_state: vec![9, 8, 7],
        };

        let first = execute_block_program(&input);
        let second = execute_block_program(&input);

        assert_eq!(first.new_state_root, second.new_state_root);
        assert_eq!(first.new_nullifier_root, second.new_nullifier_root);
        assert_eq!(first.block_hash, second.block_hash);
        assert_eq!(first.new_market_state_hash, second.new_market_state_hash);
        assert_eq!(first.tx_count, 2);
    }

    #[test]
    fn execute_block_program_changes_when_witness_changes() {
        let mut input = BlockProgramInput {
            prev_state_root: [1u8; 32],
            prev_nullifier_root: [2u8; 32],
            block_number: 7,
            timestamp: 1_700_000_000,
            txs: vec![vec![1, 2, 3]],
            prev_market_state: vec![9, 8, 7],
        };
        let first = execute_block_program(&input);

        input.txs.push(vec![4, 5, 6]);
        let second = execute_block_program(&input);

        assert_ne!(first.new_state_root, second.new_state_root);
        assert_ne!(first.new_nullifier_root, second.new_nullifier_root);
        assert_ne!(first.block_hash, second.block_hash);
        assert_ne!(first.new_market_state_hash, second.new_market_state_hash);
        assert_eq!(second.tx_count, 2);
    }
}
