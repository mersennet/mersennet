// Poseidon-2 wrapper.
//
// Until the Aztec parameter set is pinned in `params/poseidon-bn254.bin`
// we delegate to `std::hash::poseidon2`. Swapping to a custom param
// table happens in Phase 1.4 and requires only this file to change.

use poseidon::poseidon2::Poseidon2;

pub fn hash_two(a: Field, b: Field) -> Field {
    Poseidon2::hash([a, b], 2)
}

pub fn hash_many<let N: u32>(inputs: [Field; N]) -> Field {
    Poseidon2::hash(inputs, N)
}
