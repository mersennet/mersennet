//! Finality certificates: the signed votes that took a block to the 2/3-stake
//! quorum. A node keeps the certificates of recent heights and serves them to
//! peers, so a node that never saw those votes (a gossip gap, a restart, a
//! minority branch) can ask for the evidence, check every signature and roll
//! back onto the canonical chain through the existing finality path.
//!
//! Evidence only: votes, the quorum rule and block validity are unchanged, and
//! nothing here is part of the state root.

use revm::primitives::{Address, B256, U256};
use serde::{Deserialize, Serialize};

/// How many recent heights' certificates a node keeps on disk (~1 KB each with
/// 14 voters): ~11 hours of 2 s blocks, far longer than any recovery needs.
pub const CERT_KEEP_HEIGHTS: u64 = 20_000;

/// One validator's signature over `(height, block_hash)` (see
/// `crypto::sign_vote`); the signer is recovered, never trusted from the wire.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VoteSig {
    pub r: U256,
    pub s: U256,
    pub y_parity: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinalityCertificate {
    pub height: u64,
    pub block_hash: B256,
    pub votes: Vec<VoteSig>,
}

impl FinalityCertificate {
    /// The distinct signers whose signatures over `(height, block_hash)`
    /// verify, in order of first appearance. Bad signatures and repeats are
    /// skipped, so a certificate padded with junk proves no more than its
    /// valid part. Whether the signers carry enough stake is the engine's call.
    pub fn signers(&self) -> Vec<Address> {
        self.verified_votes().into_iter().map(|(a, _)| a).collect()
    }

    /// `signers` with each signer's signature.
    pub fn verified_votes(&self) -> Vec<(Address, VoteSig)> {
        let mut out: Vec<(Address, VoteSig)> = Vec::with_capacity(self.votes.len());
        for v in &self.votes {
            if let Ok(a) = crate::crypto::recover_vote_signer(
                self.height,
                self.block_hash,
                v.r,
                v.s,
                v.y_parity,
            ) && !out.iter().any(|(x, _)| *x == a)
            {
                out.push((a, *v));
            }
        }
        out
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(self).unwrap_or_default()
    }

    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        serde_json::from_slice(bytes).ok()
    }
}

/// `height_meta` key of a stored certificate.
pub(crate) fn cert_key(height: u64) -> [u8; 12] {
    let mut k = [0u8; 12];
    k[..4].copy_from_slice(b"cert");
    k[4..].copy_from_slice(&height.to_be_bytes());
    k
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(i: u8) -> k256::ecdsa::SigningKey {
        k256::ecdsa::SigningKey::from_bytes(&[i; 32].into()).unwrap()
    }

    #[test]
    fn signers_are_recovered_deduplicated_and_junk_is_ignored() {
        let hash = B256::repeat_byte(7);
        let sig = |i: u8| {
            let (r, s, y) = crate::crypto::sign_vote(42, hash, &key(i));
            VoteSig { r, s, y_parity: y }
        };
        let cert = FinalityCertificate {
            height: 42,
            block_hash: hash,
            votes: vec![
                sig(1),
                sig(2),
                sig(1),
                VoteSig {
                    r: U256::ZERO,
                    s: U256::ZERO,
                    y_parity: 0,
                },
            ],
        };
        let want: Vec<Address> = [1u8, 2]
            .iter()
            .map(|i| crate::crypto::address_from_signing_key(&key(*i)))
            .collect();
        assert_eq!(cert.signers(), want);
        // A signature over another height or hash proves nothing here.
        let other = FinalityCertificate {
            height: 43,
            ..cert.clone()
        };
        assert!(other.signers().iter().all(|a| !want.contains(a)));
        assert_eq!(
            FinalityCertificate::from_bytes(&cert.to_bytes()),
            Some(cert)
        );
    }
}
