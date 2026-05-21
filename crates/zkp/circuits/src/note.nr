// Note schema in-circuit. Mirrors `crates/zkp/src/note.rs::Note`.

use crate::poseidon;

pub struct Note {
    value_lo: Field,
    value_hi: Field,
    asset_id: Field,
    owner_pk: Field,
    rho: Field,
    psi: Field,
}

impl Note {
    pub fn commit(self) -> Field {
        poseidon::hash_many([
            self.value_lo,
            self.value_hi,
            self.asset_id,
            self.owner_pk,
            self.rho,
            self.psi,
        ])
    }

    pub fn nullifier(self, spend_sk: Field) -> Field {
        poseidon::hash_two(spend_sk, self.rho)
    }

    // Linear `u128` value reconstruction from the two halves.
    pub fn value(self) -> Field {
        // Constrain each half to 64 bits — this is a Field, but we use
        // it as a u128 in the host. The 64-bit bound is enforced by
        // the surrounding circuit via `std::field::to_le_bytes` checks.
        self.value_lo + self.value_hi * 0x10000000000000000
    }
}
