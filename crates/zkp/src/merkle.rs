//! Sparse Poseidon Merkle tree of fixed depth.
//!
//! Used as the note commitment set. Depth 32 = up to ~4 B notes.
//! Empty subtrees are represented by a precomputed "empty subtree" hash
//! at each level so insertion costs `O(depth)` Poseidon hashes
//! regardless of tree size.
//!
//! The tree stores only the path nodes that have been written to.
//! Reading a non-existent leaf returns the "empty" hash at depth 0,
//! which is `Fr::ZERO` by convention.

use crate::field::Fr;
use crate::poseidon::Poseidon;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Fixed tree depth. 2^32 leaves is more than enough for the notional
/// note volume of a high-throughput perps venue (Hyperliquid did
/// ~3M trades per day in late 2025; at 32 the tree holds ~1300 years
/// at that rate).
pub const MERKLE_DEPTH: usize = 32;

/// Path from a leaf to the root, used to prove membership in a circuit.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MerkleProof {
    pub leaf: Fr,
    pub index: u64,
    /// Sibling at each level, level 0 being the leaf's sibling and
    /// level [`MERKLE_DEPTH`] - 1 being the sibling of the root's child.
    pub siblings: Vec<Fr>,
}

impl MerkleProof {
    pub fn root(&self, hasher: &Poseidon) -> Fr {
        let mut cur = self.leaf;
        let mut idx = self.index;
        for sib in &self.siblings {
            cur = if idx & 1 == 0 {
                hasher.hash_two(&cur, sib)
            } else {
                hasher.hash_two(sib, &cur)
            };
            idx >>= 1;
        }
        cur
    }
}

#[derive(Clone, Debug)]
pub struct MerkleTree {
    hasher: Poseidon,
    /// Hash of an empty subtree at each level.
    empty: Vec<Fr>,
    /// Sparse storage: (level, index) -> node hash. Level 0 is leaves.
    nodes: HashMap<(usize, u64), Fr>,
    next_index: u64,
}

impl Default for MerkleTree {
    fn default() -> Self {
        Self::new()
    }
}

impl MerkleTree {
    pub fn new() -> Self {
        let hasher = Poseidon;
        let mut empty = Vec::with_capacity(MERKLE_DEPTH + 1);
        let mut cur = Fr::ZERO;
        empty.push(cur);
        for _ in 0..MERKLE_DEPTH {
            cur = hasher.hash_two(&cur, &cur);
            empty.push(cur);
        }
        Self {
            hasher,
            empty,
            nodes: HashMap::new(),
            next_index: 0,
        }
    }

    pub fn root(&self) -> Fr {
        self.node(MERKLE_DEPTH, 0)
    }

    pub fn next_index(&self) -> u64 {
        self.next_index
    }

    /// Insert a leaf in append-only fashion. Returns the index used.
    pub fn insert(&mut self, leaf: Fr) -> u64 {
        let idx = self.next_index;
        self.set_leaf(idx, leaf);
        self.next_index += 1;
        idx
    }

    /// Generate a Merkle proof for the leaf at `index`.
    pub fn prove(&self, index: u64) -> MerkleProof {
        let leaf = self.node(0, index);
        let mut siblings = Vec::with_capacity(MERKLE_DEPTH);
        let mut idx = index;
        for level in 0..MERKLE_DEPTH {
            let sib = self.node(level, idx ^ 1);
            siblings.push(sib);
            idx >>= 1;
        }
        MerkleProof {
            leaf,
            index,
            siblings,
        }
    }

    /// Returns whether the proof matches the current root.
    pub fn verify(&self, proof: &MerkleProof) -> bool {
        if proof.siblings.len() != MERKLE_DEPTH {
            return false;
        }
        proof.root(&self.hasher) == self.root()
    }

    fn node(&self, level: usize, index: u64) -> Fr {
        if let Some(v) = self.nodes.get(&(level, index)) {
            return *v;
        }
        self.empty[level]
    }

    /// Read the leaf at `index`. Returns the empty-leaf
    /// (`Fr::ZERO`-hashed-up) value for indices past `next_index`.
    /// Used by persistence layers to snapshot the dense leaf range.
    pub fn leaf_at(&self, index: u64) -> Fr {
        self.node(0, index)
    }

    fn set_leaf(&mut self, index: u64, leaf: Fr) {
        self.nodes.insert((0, index), leaf);
        let mut idx = index;
        for level in 0..MERKLE_DEPTH {
            let left = self.node(level, idx & !1);
            let right = self.node(level, idx | 1);
            let parent = self.hasher.hash_two(&left, &right);
            let parent_idx = idx >> 1;
            self.nodes.insert((level + 1, parent_idx), parent);
            idx >>= 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_tree_root_is_stable() {
        let t1 = MerkleTree::new();
        let t2 = MerkleTree::new();
        assert_eq!(t1.root(), t2.root());
    }

    #[test]
    fn insert_then_prove() {
        let mut t = MerkleTree::new();
        let a = Fr::from_u64(42);
        let i = t.insert(a);
        let proof = t.prove(i);
        assert!(t.verify(&proof));
        assert_eq!(proof.leaf, a);
    }

    #[test]
    fn multiple_inserts_distinct_roots() {
        let mut t = MerkleTree::new();
        let r0 = t.root();
        t.insert(Fr::from_u64(1));
        let r1 = t.root();
        t.insert(Fr::from_u64(2));
        let r2 = t.root();
        assert_ne!(r0, r1);
        assert_ne!(r1, r2);
    }

    #[test]
    fn proof_for_old_leaf_is_invalidated_by_new_insert() {
        let mut t = MerkleTree::new();
        let i = t.insert(Fr::from_u64(1));
        let old_proof = t.prove(i);
        assert!(t.verify(&old_proof));
        // Insert another leaf. The proof for index i must still verify
        // against the new root because we didn't change it; only its
        // sibling on the right was updated, and that updated sibling
        // is captured in the proof for newly issued proofs but the
        // OLD proof was created before this insert and has stale
        // siblings only on levels above. Actually verify this works:
        t.insert(Fr::from_u64(2));
        // The proof captured the state at the time of generation.
        // Verifying against the CURRENT root must therefore fail —
        // this is the security property we want.
        assert!(!t.verify(&old_proof));
        // Re-generating gives a valid proof.
        let fresh = t.prove(i);
        assert!(t.verify(&fresh));
    }
}
