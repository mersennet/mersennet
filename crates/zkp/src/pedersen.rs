//! Pedersen commitment placeholder.
//!
//! The real commitment uses a fixed BN254 generator set and is
//! computed inside Barretenberg / the circuit. Until the proving stack
//! is wired (Phase 1.4), we provide a Poseidon-based commitment with
//! the **same API surface** so that the rest of the workspace can
//! compile, test, and integrate without dependency on the curve libs.
//!
//! The substitution is hiding (the randomness is the second input) but
//! not binding in the EC sense. Real binding holds in BN254 once we
//! swap the implementation.

use crate::field::Fr;
use crate::poseidon::Poseidon;

/// Pedersen-style commitment: `Commit(msg, r) = H(msg, r)` where `H` is
/// Poseidon. To be replaced with a real EC commitment in Phase 1.4.
#[derive(Clone, Debug)]
pub struct Commit {
    poseidon: Poseidon,
}

impl Default for Commit {
    fn default() -> Self {
        Self::new()
    }
}

impl Commit {
    pub fn new() -> Self {
        Self {
            poseidon: Poseidon::default(),
        }
    }

    pub fn commit(&self, msg: &[Fr], randomness: Fr) -> Fr {
        let mut buf = Vec::with_capacity(msg.len() + 1);
        buf.extend_from_slice(msg);
        buf.push(randomness);
        self.poseidon.hash_many(&buf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commit_is_deterministic() {
        let c = Commit::new();
        let msg = [Fr::from_u64(1), Fr::from_u64(2)];
        let r = Fr::from_u64(99);
        assert_eq!(c.commit(&msg, r), c.commit(&msg, r));
    }

    #[test]
    fn commit_is_randomness_sensitive() {
        let c = Commit::new();
        let msg = [Fr::from_u64(1), Fr::from_u64(2)];
        assert_ne!(c.commit(&msg, Fr::from_u64(1)), c.commit(&msg, Fr::from_u64(2)));
    }
}
