//! Shielded state for Prime Chain.
//!
//! Holds the global note commitment tree, the nullifier set, and a
//! recent-roots ring so that wallets can prove against a slightly
//! stale root without grinding every block.
//!
//! The data structures live here for low-level access. The chain
//! engine wraps them with persistence (redb), block-level event
//! emission, and crash safety. This module owns the algorithm; the
//! engine owns the IO.

#![allow(dead_code)]

use prime_zkp::{Fr, MerkleProof, MerkleTree, NoteCommitment, Nullifier, NullifierSet};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use thiserror::Error;

/// How many historical roots to keep available for proofs. Wallets
/// build proofs against a recent root and submit within this window;
/// older roots are no longer accepted as anchors. 64 blocks at
/// ~200 ms target block time = ~13 seconds of latency budget for
/// client-side proving.
pub const RECENT_ROOTS_WINDOW: usize = 64;

#[derive(Debug, Error)]
pub enum ShieldedStateError {
    #[error("note commitment is already in the tree")]
    DuplicateCommitment,
    #[error("nullifier already spent")]
    DoubleSpend,
    #[error("anchor root is not in the recent-roots window")]
    StaleAnchor,
    #[error("proof does not validate against the stated anchor root")]
    InvalidMembershipProof,
}

#[derive(Clone, Debug, Default)]
pub struct ShieldedState {
    tree: MerkleTree,
    nullifiers: NullifierSet,
    recent_roots: VecDeque<Fr>,
}

impl ShieldedState {
    pub fn new() -> Self {
        let mut s = ShieldedState {
            tree: MerkleTree::new(),
            nullifiers: NullifierSet::new(),
            recent_roots: VecDeque::with_capacity(RECENT_ROOTS_WINDOW),
        };
        s.recent_roots.push_back(s.tree.root());
        s
    }

    /// Current root of the note commitment tree.
    pub fn current_root(&self) -> Fr {
        self.tree.root()
    }

    /// Insert a fresh note commitment. The chain calls this once per
    /// `output` produced by a shielded transaction, after the
    /// transaction's circuit proofs have verified.
    pub fn insert_note(&mut self, cm: NoteCommitment) -> Result<u64, ShieldedStateError> {
        // The commitment is in `Fr`; the tree de-dupes via index, not
        // value. We allow two notes with the same commitment (it's
        // cryptographically negligible and the nullifier set is what
        // enforces uniqueness on spend).
        let idx = self.tree.insert(cm.0);
        self.push_root(self.tree.root());
        Ok(idx)
    }

    /// Spend a note by inserting its nullifier. Returns
    /// [`ShieldedStateError::DoubleSpend`] if the nullifier was
    /// already spent.
    pub fn spend(&mut self, n: Nullifier) -> Result<(), ShieldedStateError> {
        if !self.nullifiers.insert(n) {
            return Err(ShieldedStateError::DoubleSpend);
        }
        Ok(())
    }

    pub fn is_spent(&self, n: &Nullifier) -> bool {
        self.nullifiers.contains(n)
    }

    /// Check that `root` is still acceptable as an anchor.
    pub fn is_recent_root(&self, root: &Fr) -> bool {
        self.recent_roots.iter().any(|r| r == root)
    }

    /// Validate a membership proof relative to an anchor that the
    /// caller supplies. The anchor must be a recent root.
    pub fn validate_proof(
        &self,
        proof: &MerkleProof,
        anchor: &Fr,
    ) -> Result<(), ShieldedStateError> {
        if !self.is_recent_root(anchor) {
            return Err(ShieldedStateError::StaleAnchor);
        }
        let derived = proof.root(&prime_zkp::poseidon::Poseidon::default());
        if &derived != anchor {
            return Err(ShieldedStateError::InvalidMembershipProof);
        }
        Ok(())
    }

    /// Build a fresh membership proof for an inserted leaf. Used by
    /// the local wallet helpers and by tests.
    pub fn prove(&self, index: u64) -> MerkleProof {
        self.tree.prove(index)
    }

    pub fn note_count(&self) -> u64 {
        self.tree.next_index()
    }

    pub fn nullifier_count(&self) -> usize {
        self.nullifiers.len()
    }

    fn push_root(&mut self, root: Fr) {
        if self.recent_roots.back().copied() != Some(root) {
            self.recent_roots.push_back(root);
            while self.recent_roots.len() > RECENT_ROOTS_WINDOW {
                self.recent_roots.pop_front();
            }
        }
    }
}

/// Block-level digest used inside the SP1 state-transition program
/// public output.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct ShieldedRootDigest {
    pub commitments_root: [u8; 32],
    pub note_count: u64,
    pub nullifier_count: u64,
}

impl ShieldedRootDigest {
    pub fn from_state(s: &ShieldedState) -> Self {
        Self {
            commitments_root: s.current_root().to_bytes(),
            note_count: s.note_count(),
            nullifier_count: s.nullifier_count() as u64,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prime_zkp::note::Note;
    use prime_zkp::poseidon::Poseidon;

    fn make_note(value: u128, seed: u64) -> Note {
        Note {
            value,
            asset_id: 0,
            owner_pk: Fr::from_u64(seed),
            rho: Fr::from_u64(seed.wrapping_mul(31) + 1),
            psi: Fr::from_u64(seed.wrapping_mul(17) + 2),
        }
    }

    #[test]
    fn insert_then_prove_membership() {
        let mut s = ShieldedState::new();
        let p = Poseidon::default();
        let note = make_note(1000, 7);
        let cm = note.commit(&p);
        let idx = s.insert_note(cm).unwrap();
        let proof = s.prove(idx);
        let anchor = s.current_root();
        s.validate_proof(&proof, &anchor).unwrap();
    }

    #[test]
    fn double_spend_is_rejected() {
        let mut s = ShieldedState::new();
        let p = Poseidon::default();
        let note = make_note(1000, 7);
        let _ = s.insert_note(note.commit(&p)).unwrap();
        let nul = note.nullifier(&p, &Fr::from_u64(0xdeadbeef));
        s.spend(nul).unwrap();
        let err = s.spend(nul).unwrap_err();
        assert!(matches!(err, ShieldedStateError::DoubleSpend));
    }

    #[test]
    fn proof_against_stale_anchor_is_rejected() {
        let mut s = ShieldedState::new();
        let p = Poseidon::default();
        let stale_root = s.current_root();
        // Generate proof now (against the empty tree)
        let proof = s.prove(0);
        // Then mutate the tree past the recent-roots window.
        for i in 0..(RECENT_ROOTS_WINDOW as u64 + 5) {
            let n = make_note((i + 1) as u128, i);
            let _ = s.insert_note(n.commit(&p)).unwrap();
        }
        // The original empty root is now outside the window.
        let err = s.validate_proof(&proof, &stale_root).unwrap_err();
        assert!(matches!(err, ShieldedStateError::StaleAnchor));
    }

    #[test]
    fn recent_root_within_window_is_accepted() {
        let mut s = ShieldedState::new();
        let p = Poseidon::default();
        // Insert one note, capture root, then check it is still recent.
        let note = make_note(42, 1);
        let idx = s.insert_note(note.commit(&p)).unwrap();
        let after_insert = s.current_root();
        let proof = s.prove(idx);
        s.validate_proof(&proof, &after_insert).unwrap();
    }
}
