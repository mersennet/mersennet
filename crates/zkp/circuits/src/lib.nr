// Mersennet circuit library.
//
// Each shielded operation is implemented as its own circuit (.nr file
// in this directory). This `lib.nr` re-exports the common helpers
// (Poseidon hash binding, Merkle path verification, value range
// checks) so that each circuit stays focused on its semantics.

mod poseidon;
mod merkle;
mod note;

use dep::std;

// Common utility: assert a `u128` value fits in `bits` bits.
pub fn assert_value_in_range(value: u128, bits: u32) {
    let bound: u128 = 1 << bits;
    assert(value < bound);
}

// Bind together a note's fields into its commitment, matching the
// Rust-side `Note::commit` so the two roots agree byte-for-byte.
pub fn note_commitment(value_lo: Field, value_hi: Field, asset_id: Field, owner_pk: Field, rho: Field, psi: Field) -> Field {
    poseidon::hash_many([value_lo, value_hi, asset_id, owner_pk, rho, psi])
}

// Nullifier derivation: H(spend_sk, rho).
pub fn note_nullifier(spend_sk: Field, rho: Field) -> Field {
    poseidon::hash_two(spend_sk, rho)
}
