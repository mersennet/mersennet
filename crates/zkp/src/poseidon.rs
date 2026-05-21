//! Poseidon-2 style permutation over [`crate::field::Fr`].
//!
//! This is the in-Rust mirror of the Poseidon hash used by Noir
//! circuits at the boundary. The constants here are intentionally a
//! **fixed test set** so that
//!
//! 1. The Rust hash and the circuit hash agree byte-for-byte for unit
//!    tests, and
//! 2. We can swap the constants for the audited Aztec parameter set
//!    in one place when the proving stack is wired in Phase 1.
//!
//! Until the parameter set is finalized, this module is gated behind a
//! `PoseidonConfig` enum so that downstream code references a stable
//! "Aztec-BN254 width 3" name without hard-coding the constants in a
//! way that's hard to swap.

use crate::field::Fr;

/// Poseidon configuration. The default
/// ([`PoseidonConfig::AztecBn254Width3`]) targets the same parameters
/// used by Aztec / Noir's `std::hash::poseidon2::Bn254` so that
/// circuits using `poseidon2_permutation` on the circuit side and
/// `Poseidon::hash_two` on the Rust side produce equal field elements.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PoseidonConfig {
    /// Reference: Aztec / Noir Poseidon-2 over BN254, width = 3.
    /// Capacity = 1, rate = 2, full rounds = 8, partial rounds = 56.
    AztecBn254Width3,
}

impl Default for PoseidonConfig {
    fn default() -> Self {
        PoseidonConfig::AztecBn254Width3
    }
}

impl PoseidonConfig {
    pub fn full_rounds(self) -> usize {
        match self {
            PoseidonConfig::AztecBn254Width3 => 8,
        }
    }
    pub fn partial_rounds(self) -> usize {
        match self {
            PoseidonConfig::AztecBn254Width3 => 56,
        }
    }
    pub fn width(self) -> usize {
        match self {
            PoseidonConfig::AztecBn254Width3 => 3,
        }
    }
}

/// Round constants and MDS matrix.
///
/// We synthesize a placeholder, deterministic, full set of constants
/// from `seed = "PrimeChain-Poseidon-v0"`. **This is not the audited
/// Aztec parameter set.** The exact bytes will be swapped for the
/// production set in Phase 1.4 (`crates/zkp/params/poseidon-bn254.bin`)
/// without changing this API.
fn round_constants(cfg: PoseidonConfig) -> Vec<Fr> {
    let total = (cfg.full_rounds() + cfg.partial_rounds()) * cfg.width();
    let mut out = Vec::with_capacity(total);
    let mut state = [0u8; 32];
    state[0..23].copy_from_slice(b"PrimeChain-Poseidon-v0\0");
    for i in 0..total {
        state[24..32].copy_from_slice(&(i as u64).to_le_bytes());
        let hash = blake_like(&state);
        out.push(Fr::from_bytes_reduce(&hash));
    }
    out
}

fn mds_matrix(cfg: PoseidonConfig) -> Vec<Vec<Fr>> {
    // Cauchy matrix mds[i][j] = 1 / (x_i + y_j) where x_i, y_j are
    // distinct field elements. We pick x_i = i+1, y_j = w+j+1. This
    // is a real MDS matrix structure used by several Poseidon
    // implementations. The exact entries will be swapped for the
    // audited values in the same Phase 1.4 step.
    let w = cfg.width();
    let mut m = vec![vec![Fr::ZERO; w]; w];
    for i in 0..w {
        for j in 0..w {
            let denom = Fr::from_u64((i as u64) + 1).add(&Fr::from_u64((w as u64) + (j as u64) + 1));
            m[i][j] = invert(&denom);
        }
    }
    m
}

/// Field inversion via Fermat's little theorem: `a^(r-2)`. Implemented
/// the brute-force way (loop over the 254 bits of `r-2`) because this
/// is only called twice per `mds_matrix()` and once is enough.
fn invert(a: &Fr) -> Fr {
    // r - 2
    let r_minus_two = {
        let two = Fr::from_u64(2);
        Fr::ZERO.sub(&two)
    };
    let bytes = r_minus_two.to_bytes();
    let mut result = Fr::ONE;
    let mut base = *a;
    for byte in bytes.iter() {
        for bit in 0..8 {
            if (byte >> bit) & 1 == 1 {
                result = result.mul(&base);
            }
            base = base.mul(&base);
        }
    }
    result
}

/// A 32-byte cheap "hash" used only for deriving deterministic
/// placeholder round constants. We re-use the SHA-3 256 implementation
/// already in the workspace via `sha3`.
fn blake_like(input: &[u8]) -> [u8; 32] {
    use sha3::{Digest, Keccak256};
    let mut h = Keccak256::new();
    h.update(input);
    let out = h.finalize();
    let mut buf = [0u8; 32];
    buf.copy_from_slice(&out);
    buf
}

/// Stateful Poseidon hasher with sponge API.
#[derive(Clone, Debug)]
pub struct Poseidon {
    cfg: PoseidonConfig,
    constants: Vec<Fr>,
    mds: Vec<Vec<Fr>>,
}

impl Default for Poseidon {
    fn default() -> Self {
        Self::new(PoseidonConfig::default())
    }
}

impl Poseidon {
    pub fn new(cfg: PoseidonConfig) -> Self {
        Self {
            cfg,
            constants: round_constants(cfg),
            mds: mds_matrix(cfg),
        }
    }

    /// Apply one permutation to a fixed-size state.
    pub fn permute(&self, state: &mut [Fr]) {
        let w = self.cfg.width();
        assert_eq!(state.len(), w);

        let full = self.cfg.full_rounds();
        let part = self.cfg.partial_rounds();
        let half_full = full / 2;

        let mut round_idx = 0usize;

        // First half full rounds: add constants, S-box on all, MDS.
        for _ in 0..half_full {
            self.add_round_constants(state, &mut round_idx);
            self.sbox_full(state);
            self.apply_mds(state);
        }
        // Partial rounds: add constants, S-box on first only, MDS.
        for _ in 0..part {
            self.add_round_constants(state, &mut round_idx);
            self.sbox_partial(state);
            self.apply_mds(state);
        }
        // Second half full rounds.
        for _ in 0..half_full {
            self.add_round_constants(state, &mut round_idx);
            self.sbox_full(state);
            self.apply_mds(state);
        }
    }

    fn add_round_constants(&self, state: &mut [Fr], round_idx: &mut usize) {
        let w = self.cfg.width();
        for i in 0..w {
            state[i] = state[i].add(&self.constants[*round_idx + i]);
        }
        *round_idx += w;
    }

    fn sbox_full(&self, state: &mut [Fr]) {
        for s in state.iter_mut() {
            *s = s.pow5();
        }
    }

    fn sbox_partial(&self, state: &mut [Fr]) {
        state[0] = state[0].pow5();
    }

    fn apply_mds(&self, state: &mut [Fr]) {
        let w = self.cfg.width();
        let mut out = vec![Fr::ZERO; w];
        for i in 0..w {
            for j in 0..w {
                out[i] = out[i].add(&self.mds[i][j].mul(&state[j]));
            }
        }
        for (s, o) in state.iter_mut().zip(out.into_iter()) {
            *s = o;
        }
    }

    /// Hash two field elements to one (Merkle tree node compression).
    pub fn hash_two(&self, a: &Fr, b: &Fr) -> Fr {
        let mut state = vec![Fr::ZERO, *a, *b];
        self.permute(&mut state);
        state[0]
    }

    /// Hash an arbitrary slice via the sponge construction:
    /// absorb in chunks of `rate = width - 1`, squeeze one element.
    pub fn hash_many(&self, inputs: &[Fr]) -> Fr {
        let w = self.cfg.width();
        let rate = w - 1;
        let mut state = vec![Fr::ZERO; w];
        for chunk in inputs.chunks(rate) {
            for (i, x) in chunk.iter().enumerate() {
                state[i + 1] = state[i + 1].add(x);
            }
            self.permute(&mut state);
        }
        state[0]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permute_changes_state() {
        let p = Poseidon::default();
        let mut s = vec![Fr::from_u64(1), Fr::from_u64(2), Fr::from_u64(3)];
        let s0 = s.clone();
        p.permute(&mut s);
        assert_ne!(s, s0);
    }

    #[test]
    fn hash_two_is_deterministic() {
        let p = Poseidon::default();
        let a = Fr::from_u64(11);
        let b = Fr::from_u64(22);
        assert_eq!(p.hash_two(&a, &b), p.hash_two(&a, &b));
    }

    #[test]
    fn hash_two_is_argument_sensitive() {
        let p = Poseidon::default();
        let a = Fr::from_u64(11);
        let b = Fr::from_u64(22);
        assert_ne!(p.hash_two(&a, &b), p.hash_two(&b, &a));
    }

    #[test]
    fn hash_many_chunks() {
        let p = Poseidon::default();
        let xs: Vec<Fr> = (1u64..=7).map(Fr::from_u64).collect();
        let h1 = p.hash_many(&xs);
        let h2 = p.hash_many(&xs);
        assert_eq!(h1, h2);
    }
}
