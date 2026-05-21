//! Nullifier set: prevents double-spend of shielded notes.
//!
//! A nullifier is a one-shot tag derived from the secret key and the
//! note's randomness. Spending a note inserts its nullifier; any
//! second spend of the same note produces the same nullifier and is
//! rejected.
//!
//! For performance the set is implemented as a hash set with an
//! optional bloom-filter fast-path. The persistent backend is
//! redb (used by [`crate::field`]'s parent crate); this module exposes
//! the in-memory algorithmic surface.

use crate::field::Fr;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Nullifier(pub Fr);

#[derive(Clone, Debug, Default)]
pub struct NullifierSet {
    set: HashSet<Nullifier>,
}

impl NullifierSet {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns `true` if the nullifier was not previously present and
    /// has been inserted. Returns `false` if the nullifier was already
    /// present (i.e. double-spend attempt).
    pub fn insert(&mut self, n: Nullifier) -> bool {
        self.set.insert(n)
    }

    pub fn contains(&self, n: &Nullifier) -> bool {
        self.set.contains(n)
    }

    pub fn len(&self) -> usize {
        self.set.len()
    }

    pub fn is_empty(&self) -> bool {
        self.set.is_empty()
    }

    /// Iterate the spent nullifiers in arbitrary order. Used by
    /// persistence layers to snapshot the set.
    pub fn iter(&self) -> impl Iterator<Item = &Nullifier> {
        self.set.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn double_spend_is_detected() {
        let mut s = NullifierSet::new();
        let n = Nullifier(Fr::from_u64(0xabcd));
        assert!(s.insert(n));
        assert!(s.contains(&n));
        assert!(!s.insert(n));
    }

    #[test]
    fn distinct_nullifiers_coexist() {
        let mut s = NullifierSet::new();
        assert!(s.insert(Nullifier(Fr::from_u64(1))));
        assert!(s.insert(Nullifier(Fr::from_u64(2))));
        assert_eq!(s.len(), 2);
    }
}
