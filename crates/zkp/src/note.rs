//! Shielded note schema, commitment, nullifier, and encrypted ciphertext.
//!
//! A note represents a value held inside the shielded pool. It is the
//! atomic unit of shielded state, analogous to a UTXO. Each note has:
//!
//! - `value`            — amount, in the smallest unit of `asset_id`.
//! - `asset_id`         — which asset (PRIM, USDC, position-share, etc).
//! - `owner_pk`         — owner's public spending key.
//! - `rho`              — uniformly random; used to derive the nullifier.
//! - `psi`              — uniformly random; used as commitment trapdoor.
//!
//! Public on-chain values per note:
//!
//! - The **commitment** `Poseidon(value, asset_id, owner_pk, rho, psi)`
//!   is inserted into the global note tree.
//! - The **encrypted ciphertext** carrying all the above values is
//!   posted in the block, addressed to `owner_pk`. The owner scans the
//!   chain with their viewing key and recovers the plaintext.
//!
//! The **nullifier** `Poseidon(spend_sk, rho)` is the only value that
//! is published on spend; together with the commitment tree's
//! membership proof, it proves the spender owned a valid note without
//! revealing which one.

use crate::field::Fr;
use crate::poseidon::Poseidon;
use serde::{Deserialize, Serialize};

/// Asset identifier. 0 = PRIM (native), 1 = USDC, 2 = USDT, etc.
/// Position notes use a tagged range starting at `1 << 16`.
pub type AssetId = u32;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Note {
    pub value: u128,
    pub asset_id: AssetId,
    pub owner_pk: Fr,
    pub rho: Fr,
    pub psi: Fr,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NoteCommitment(pub Fr);

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EncryptedNote {
    /// `owner_pk` of the recipient. Stored in plaintext so wallets can
    /// detect notes addressed to them with a single field-equality
    /// check rather than trial-decrypting every ciphertext on the chain.
    /// This is the same trade-off Aztec / Penumbra make.
    pub recipient: Fr,
    /// Ciphertext of the bincode-encoded [`Note`] under
    /// `expand_key(shared_secret(owner_pk, ephemeral_sk))`.
    pub ciphertext: Vec<u8>,
    /// Ephemeral public key used for the ECDH (here represented as a
    /// field element; on the real curve this becomes a compressed
    /// affine point).
    pub ephemeral_pk: Fr,
}

#[derive(Clone, Copy, Debug)]
pub struct ViewingKey {
    /// Owner's secret spend key. The wallet stores this; the chain
    /// never sees it.
    pub spend_sk: Fr,
}

impl Note {
    /// Compute the note commitment.
    pub fn commit(&self, hasher: &Poseidon) -> NoteCommitment {
        let inputs = [
            Fr::from_u64(self.value as u64),
            Fr::from_u64((self.value >> 64) as u64),
            Fr::from_u64(self.asset_id as u64),
            self.owner_pk,
            self.rho,
            self.psi,
        ];
        NoteCommitment(hasher.hash_many(&inputs))
    }

    /// Compute the nullifier for spending this note, given the
    /// owner's spend secret key.
    pub fn nullifier(&self, hasher: &Poseidon, spend_sk: &Fr) -> crate::nullifier::Nullifier {
        crate::nullifier::Nullifier(hasher.hash_two(spend_sk, &self.rho))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_note(v: u128, owner: u64) -> Note {
        Note {
            value: v,
            asset_id: 0,
            owner_pk: Fr::from_u64(owner),
            rho: Fr::from_u64(0xdead),
            psi: Fr::from_u64(0xbeef),
        }
    }

    #[test]
    fn commit_is_deterministic() {
        let p = Poseidon::default();
        let n = make_note(100, 42);
        assert_eq!(n.commit(&p), n.commit(&p));
    }

    #[test]
    fn commit_is_value_sensitive() {
        let p = Poseidon::default();
        let a = make_note(100, 42);
        let b = make_note(101, 42);
        assert_ne!(a.commit(&p), b.commit(&p));
    }

    #[test]
    fn nullifier_changes_with_secret() {
        let p = Poseidon::default();
        let n = make_note(100, 42);
        let n1 = n.nullifier(&p, &Fr::from_u64(1));
        let n2 = n.nullifier(&p, &Fr::from_u64(2));
        assert_ne!(n1, n2);
    }

    #[test]
    fn nullifier_changes_with_rho() {
        let p = Poseidon::default();
        let mut a = make_note(100, 42);
        a.rho = Fr::from_u64(11);
        let mut b = make_note(100, 42);
        b.rho = Fr::from_u64(22);
        let sk = Fr::from_u64(7);
        assert_ne!(a.nullifier(&p, &sk), b.nullifier(&p, &sk));
    }
}
