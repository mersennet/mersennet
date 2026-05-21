//! SP1 program envelope shared with `crates/core/src/zk_sp1.rs`.
//!
//! Until the real `sp1_sdk` is wired (Phase 5), this module provides
//! the type surface the SP1 state-transition program will read/write:
//! input bytes for what the program proves about, output bytes for
//! what becomes public. Everything is canonical bincode so the same
//! bytes work inside the zkVM and outside.

use crate::field::Fr;
use serde::{Deserialize, Serialize};

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

impl BlockProgramOutput {
    /// Field-element representation of the public output, used when a
    /// downstream circuit (e.g. a bridge verifier on another chain)
    /// needs to consume it via Noir.
    pub fn to_field_elements(&self) -> Vec<Fr> {
        let mut out = Vec::with_capacity(8);
        out.push(Fr::from_bytes_reduce(&self.prev_state_root));
        out.push(Fr::from_bytes_reduce(&self.new_state_root));
        out.push(Fr::from_bytes_reduce(&self.prev_nullifier_root));
        out.push(Fr::from_bytes_reduce(&self.new_nullifier_root));
        out.push(Fr::from_u64(self.block_number));
        out.push(Fr::from_bytes_reduce(&self.block_hash));
        out.push(Fr::from_bytes_reduce(&self.new_market_state_hash));
        out.push(Fr::from_u64(self.tx_count));
        out
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
}
