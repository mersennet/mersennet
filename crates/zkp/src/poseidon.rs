//! Poseidon2 permutation and sponge hash over [`crate::field::Fr`].
//!
//! This is the in-Rust mirror of the hash used by the Noir circuits at
//! the proof boundary. The circuits call `poseidon::Poseidon2::hash`
//! from the `noir-lang/poseidon` library, which delegates to the
//! compiler builtin `std::hash::poseidon2_permutation`. That builtin
//! is implemented (and executed by `nargo`/Barretenberg) by the ACVM
//! `bn254_blackbox_solver`. We reproduce that exact algorithm and
//! parameter set here so that values computed in Rust (note tree
//! Merkle roots, commitments, nullifiers) are **bit-identical** to the
//! values the circuits constrain.
//!
//! Configuration: Poseidon2 over BN254, width `t = 4`, capacity 1,
//! rate 3, `RF = 8` full rounds, `RP = 56` partial rounds, `x^5`
//! S-box. The round constants and internal-matrix diagonal live in
//! `crate::poseidon2_constants`, transcribed verbatim from the ACVM
//! reference.
//!
//! Parity is locked by `tests::permutation_matches_acvm_smoke_vector`
//! (the ACVM smoke-test vector) and
//! `tests::hash_matches_nargo_vector` (a value produced by running
//! `Poseidon2::hash` through `nargo execute`). Any change that
//! perturbs the hash trips these tests.

use crate::field::Fr;
use crate::poseidon2_constants::{DIAG_HEX, RC_HEX};
use std::sync::OnceLock;

/// State width.
const T: usize = 4;
/// Sponge rate (`T - capacity`). `Poseidon2::hash` absorbs in chunks of
/// this size.
const RATE: usize = 3;
/// Number of full rounds.
const ROUNDS_F: usize = 8;
/// Number of partial (internal) rounds.
const ROUNDS_P: usize = 56;
/// Total rounds; the constant table has one row per round.
const ROUNDS: usize = ROUNDS_F + ROUNDS_P;

/// Decoded Poseidon2 parameters. The constant table is fully populated
/// for the external rounds and only column 0 is non-zero for the
/// internal rounds, matching the upstream layout.
#[derive(Debug)]
struct Params {
    /// Internal-matrix diagonal, one entry per state element.
    diag: [Fr; T],
    /// Per-round constants `rc[round][element]`.
    rc: [[Fr; T]; ROUNDS],
}

static PARAMS: OnceLock<Params> = OnceLock::new();

/// Decode a 64-character (32-byte) big-endian hex constant into `Fr`.
fn decode_hex32(s: &str) -> Fr {
    debug_assert_eq!(s.len(), 64, "constant must be 32 bytes of hex");
    let mut bytes = [0u8; 32];
    for (i, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16)
            .expect("poseidon constant table contains only valid hex");
    }
    Fr::from_be_bytes_reduce(&bytes)
}

fn params() -> &'static Params {
    PARAMS.get_or_init(|| {
        debug_assert_eq!(DIAG_HEX.len(), T);
        debug_assert_eq!(RC_HEX.len(), ROUNDS * T);
        let mut diag = [Fr::ZERO; T];
        for (i, hex) in DIAG_HEX.iter().enumerate() {
            diag[i] = decode_hex32(hex);
        }
        let mut rc = [[Fr::ZERO; T]; ROUNDS];
        for (round, row) in rc.iter_mut().enumerate() {
            for (i, cell) in row.iter_mut().enumerate() {
                *cell = decode_hex32(RC_HEX[round * T + i]);
            }
        }
        Params { diag, rc }
    })
}

/// `x^5` S-box.
fn single_box(x: Fr) -> Fr {
    x.pow5()
}

/// External (full-round) linear layer. This is the exact `t = 4`
/// circulant multiply from Barretenberg, expressed as additions so it
/// avoids any field multiplication.
fn matmul_external(state: &mut [Fr; T]) {
    let t0 = state[0].add(&state[1]); // A + B
    let t1 = state[2].add(&state[3]); // C + D
    let mut t2 = state[1].add(&state[1]); // 2B
    t2 = t2.add(&t1); // 2B + C + D
    let mut t3 = state[3].add(&state[3]); // 2D
    t3 = t3.add(&t0); // 2D + A + B
    let mut t4 = t1.add(&t1);
    t4 = t4.add(&t4);
    t4 = t4.add(&t3); // A + B + 4C + 6D
    let mut t5 = t0.add(&t0);
    t5 = t5.add(&t5);
    t5 = t5.add(&t2); // 4A + 6B + C + D
    let t6 = t3.add(&t5); // 5A + 7B + C + 3D
    let t7 = t2.add(&t4); // A + 3B + 5C + 7D
    state[0] = t6;
    state[1] = t5;
    state[2] = t7;
    state[3] = t4;
}

/// Internal (partial-round) linear layer: `state[i] = state[i] *
/// diag[i] + sum(state)`.
fn matmul_internal(state: &mut [Fr; T], diag: &[Fr; T]) {
    let mut sum = Fr::ZERO;
    for x in state.iter() {
        sum = sum.add(x);
    }
    for (cell, d) in state.iter_mut().zip(diag.iter()) {
        *cell = cell.mul(d).add(&sum);
    }
}

/// Apply the Poseidon2 permutation to a width-4 state. Mirrors
/// `Poseidon2::permutation` in the ACVM reference exactly.
fn permutation(mut state: [Fr; T]) -> [Fr; T] {
    let p = params();

    // Initial external linear layer.
    matmul_external(&mut state);

    // First half of the full rounds.
    let rf_first = ROUNDS_F / 2;
    for round in p.rc.iter().take(rf_first) {
        for (cell, rc) in state.iter_mut().zip(round.iter()) {
            *cell = cell.add(rc);
        }
        for cell in state.iter_mut() {
            *cell = single_box(*cell);
        }
        matmul_external(&mut state);
    }

    // Partial (internal) rounds: S-box on element 0 only.
    let p_end = rf_first + ROUNDS_P;
    for round in rf_first..p_end {
        state[0] = state[0].add(&p.rc[round][0]);
        state[0] = single_box(state[0]);
        matmul_internal(&mut state, &p.diag);
    }

    // Second half of the full rounds.
    for round in p.rc.iter().take(ROUNDS).skip(p_end) {
        for (cell, rc) in state.iter_mut().zip(round.iter()) {
            *cell = cell.add(rc);
        }
        for cell in state.iter_mut() {
            *cell = single_box(*cell);
        }
        matmul_external(&mut state);
    }

    state
}

/// Sponge hash matching `Poseidon2::hash(input, N)` from
/// `noir-lang/poseidon` (the `hash_internal` construction): initialize
/// the capacity element with `len << 64`, absorb the inputs in
/// rate-sized chunks, and squeeze one element.
fn sponge_hash(inputs: &[Fr]) -> Fr {
    let in_len = inputs.len();
    // iv = (in_len as Field) * 2^64, i.e. in_len placed in limb 1.
    let iv = Fr::from_limbs([0, in_len as u64, 0, 0]);

    let mut state = [Fr::ZERO; T];
    state[RATE] = iv;

    let full_chunks = in_len / RATE;
    for chunk in 0..full_chunks {
        for i in 0..RATE {
            state[i] = state[i].add(&inputs[chunk * RATE + i]);
        }
        state = permutation(state);
    }

    let remainder = in_len % RATE;
    if remainder != 0 {
        let start = full_chunks * RATE;
        for j in 0..remainder {
            state[j] = state[j].add(&inputs[start + j]);
        }
    }

    // Run a final permutation unless we just completed a full chunk.
    // (Also runs for the empty input, matching the reference.)
    if in_len == 0 || !in_len.is_multiple_of(RATE) {
        state = permutation(state);
    }

    state[0]
}

/// Poseidon2 hasher. Stateless; cheap to construct (the parameter
/// table is decoded once, process-wide, behind a `OnceLock`).
#[derive(Clone, Copy, Debug, Default)]
pub struct Poseidon;

impl Poseidon {
    pub fn new() -> Self {
        Poseidon
    }

    /// Apply one raw permutation to a width-4 state. Exposed for
    /// cross-checking against the circuit builtin.
    pub fn permute(&self, state: &mut [Fr; T]) {
        *state = permutation(*state);
    }

    /// Hash two field elements to one. Equivalent to the circuit's
    /// `poseidon::hash_two(a, b)` == `Poseidon2::hash([a, b], 2)`.
    /// Used for Merkle node compression, so this MUST match the
    /// in-circuit hash for spend proofs to verify.
    pub fn hash_two(&self, a: &Fr, b: &Fr) -> Fr {
        sponge_hash(&[*a, *b])
    }

    /// Hash an arbitrary slice. Equivalent to the circuit's
    /// `poseidon::hash_many(inputs)` == `Poseidon2::hash(inputs, N)`.
    pub fn hash_many(&self, inputs: &[Fr]) -> Fr {
        sponge_hash(inputs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Decode the big-endian hex vectors the ACVM reference pins in its
    /// own `smoke_test`.
    fn be(hex: &str) -> Fr {
        decode_hex32(hex)
    }

    /// The ACVM `poseidon2.rs::smoke_test`: permuting the all-zero
    /// state yields a fixed vector. If our permutation matches
    /// Barretenberg's, this passes bit-for-bit.
    #[test]
    fn permutation_matches_acvm_smoke_vector() {
        let out = permutation([Fr::ZERO; T]);
        let expected = [
            be("18dfb8dc9b82229cff974efefc8df78b1ce96d9d844236b496785c698bc6732e"),
            be("095c230d1d37a246e8d2d5a63b165fe0fade040d442f61e25f0590e5fb76f839"),
            be("0bb9545846e1afa4fa3c97414a60a20fc4949f537a68cceca34c5ce71e28aa59"),
            be("18a4f34c9c6f99335ff7638b82aeed9018026618358873c982bbdde265b2ed6d"),
        ];
        assert_eq!(out, expected);
    }

    /// `Poseidon2::hash([100, 0, 0, 7, 11, 13], 6)` produced by
    /// `nargo execute` against the same `noir-lang/poseidon` library
    /// the circuits use. Locks the sponge construction (IV, chunking,
    /// final-permute rule) to the circuit's.
    #[test]
    fn hash_matches_nargo_vector() {
        let inputs = [
            Fr::from_u64(100),
            Fr::from_u64(0),
            Fr::from_u64(0),
            Fr::from_u64(7),
            Fr::from_u64(11),
            Fr::from_u64(13),
        ];
        let got = sponge_hash(&inputs);
        let expected = be("151e8a1d09c7d145308bedfdef746b7e09b24282053984287cf0a0adb45602e1");
        assert_eq!(got, expected);
    }

    #[test]
    fn hash_two_is_deterministic() {
        let p = Poseidon::new();
        let a = Fr::from_u64(11);
        let b = Fr::from_u64(22);
        assert_eq!(p.hash_two(&a, &b), p.hash_two(&a, &b));
    }

    #[test]
    fn hash_two_is_argument_sensitive() {
        let p = Poseidon::new();
        let a = Fr::from_u64(11);
        let b = Fr::from_u64(22);
        assert_ne!(p.hash_two(&a, &b), p.hash_two(&b, &a));
    }

    #[test]
    fn hash_two_equals_hash_many_of_two() {
        let p = Poseidon::new();
        let a = Fr::from_u64(123);
        let b = Fr::from_u64(456);
        assert_eq!(p.hash_two(&a, &b), p.hash_many(&[a, b]));
    }

    #[test]
    fn hash_many_chunks_are_deterministic() {
        let p = Poseidon::new();
        let xs: Vec<Fr> = (1u64..=7).map(Fr::from_u64).collect();
        assert_eq!(p.hash_many(&xs), p.hash_many(&xs));
    }
}
