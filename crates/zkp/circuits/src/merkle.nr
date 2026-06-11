// Sparse Merkle path verification.
//
// `MERKLE_DEPTH` must match `crates/zkp/src/merkle.rs`.

use crate::poseidon;

pub global MERKLE_DEPTH: u32 = 32;

// Re-derive the root from `leaf` walking up the tree via `siblings`.
// `index_bits` is the leaf's index expressed as `MERKLE_DEPTH` bits,
// LSB-first, where each bit is 0 (left child) or 1 (right child).
pub fn compute_root(leaf: Field, siblings: [Field; MERKLE_DEPTH], index_bits: [Field; MERKLE_DEPTH]) -> Field {
    let mut current = leaf;
    for level in 0..MERKLE_DEPTH {
        let is_right = index_bits[level];
        // is_right must be 0 or 1.
        assert(is_right * (is_right - 1) == 0);
        let left = current * (1 - is_right) + siblings[level] * is_right;
        let right = siblings[level] * (1 - is_right) + current * is_right;
        current = poseidon::hash_two(left, right);
    }
    current
}

// Assert that `leaf` is a member of the tree with `root`, at `index`
// witnessed by `siblings`.
pub fn assert_membership(root: Field, leaf: Field, siblings: [Field; MERKLE_DEPTH], index_bits: [Field; MERKLE_DEPTH]) {
    let derived = compute_root(leaf, siblings, index_bits);
    assert(derived == root);
}
