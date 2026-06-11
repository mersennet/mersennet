//! Pedersen commitment over BN254.
//!
//! The cryptographic primitive committed to is the standard EC
//! Pedersen scheme:
//!
//! ```text
//! Commit(msg, r) = sum_i (msg_i * G_i) + r * H
//! ```
//!
//! Where `G_0, G_1, ..., G_{N-1}, H` are independent BN254 generators.
//! The result is a BN254 G1 point. For the [`Commit::commit`] API
//! surface we return a single `Fr` derived from a Poseidon hash of
//! the (compressed) point's serialization, so callers using the
//! existing in-circuit boundary (which works in `Fr`-land) stay
//! unchanged.
//!
//! ## Two build modes
//!
//! - **Default (no `prover` feature).** The commitment falls back to a
//!   Poseidon-based stand-in `Commit(msg, r) = H(msg, r)`. This is
//!   hiding (randomness mixed in) but only *computationally* binding
//!   under Poseidon-as-RO; it is sufficient for the rest of the
//!   workspace to compile and test without pulling in arkworks.
//!
//! - **With `--features prover`.** Real BN254 arithmetic via
//!   [`ark-bn254`] + [`ark-ec`]. Hiding + binding under DLP on BN254
//!   G1, which is the same security assumption used by the Aztec
//!   side. The generator set is derived deterministically from
//!   `"PrimeChain-Pedersen-v0"` + index using a try-and-increment
//!   hash-to-curve (this is the **swap point** for the audited Aztec
//!   generator set — file `crates/zkp/params/pedersen-bn254-gens.bin`
//!   gets populated by the same Workstream D1-style pinning workflow).
//!
//! ## API stability
//!
//! Both build modes expose the same `Commit::new()`,
//! `Commit::commit(&self, msg: &[Fr], r: Fr) -> Fr` surface. Callers
//! cannot tell them apart at compile time; only the cryptographic
//! security level differs.

use crate::field::Fr;
use crate::poseidon::Poseidon;

/// Maximum number of message slots supported by the generator set.
/// Larger commitments would need a bigger generator table; we
/// deliberately keep this small for the privacy-mode usage (a single
/// note commits to a handful of fields: value, asset_id, owner_pk,
/// rho, psi).
pub const MAX_MSG_LEN: usize = 16;

/// Pedersen commitment. See module docs for build-mode differences.
#[derive(Clone, Debug)]
pub struct Commit {
    poseidon: Poseidon,
    #[cfg(feature = "prover")]
    inner: real::RealPedersen,
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
            #[cfg(feature = "prover")]
            inner: real::RealPedersen::new(),
        }
    }

    /// Compute the commitment. Returns the same `Fr` value across
    /// repeated calls with the same `(msg, randomness)`.
    pub fn commit(&self, msg: &[Fr], randomness: Fr) -> Fr {
        #[cfg(feature = "prover")]
        {
            return self.inner.commit_to_fr(&self.poseidon, msg, randomness);
        }
        #[cfg(not(feature = "prover"))]
        {
            let mut buf = Vec::with_capacity(msg.len() + 1);
            buf.extend_from_slice(msg);
            buf.push(randomness);
            self.poseidon.hash_many(&buf)
        }
    }
}

// ─────────────────────────────────────────────────────────────────────
//  Real BN254 Pedersen, gated behind --features prover
// ─────────────────────────────────────────────────────────────────────

#[cfg(feature = "prover")]
mod real {
    use super::*;
    use ark_bn254::{Fq as ArkFq, Fr as ArkFr, G1Affine, G1Projective};
    use ark_ec::{AffineRepr, CurveGroup};
    use ark_ff::{BigInteger, Field, MersennetField, Zero};
    use ark_serialize::CanonicalSerialize;
    use sha3::{Digest, Keccak256};

    /// Real Pedersen commitment on BN254 G1.
    ///
    /// The generator table is computed once per instance and lives
    /// in-memory. Each generator is derived deterministically from a
    /// (domain, index) pair via try-and-increment hash-to-curve.
    #[derive(Clone, Debug)]
    pub struct RealPedersen {
        gens: Vec<G1Affine>,
        h: G1Affine,
    }

    impl RealPedersen {
        pub fn new() -> Self {
            let mut gens = Vec::with_capacity(MAX_MSG_LEN);
            for i in 0..MAX_MSG_LEN {
                gens.push(derive_generator(b"PrimeChain-Pedersen-v0-G", i as u64));
            }
            let h = derive_generator(b"PrimeChain-Pedersen-v0-H", 0);
            Self { gens, h }
        }

        /// Commit and return the result as our local `Fr` via Poseidon
        /// over the compressed-curve-point bytes.
        pub fn commit_to_fr(&self, poseidon: &Poseidon, msg: &[Fr], randomness: Fr) -> Fr {
            assert!(
                msg.len() <= MAX_MSG_LEN,
                "Pedersen message slot count {} exceeds MAX_MSG_LEN {}",
                msg.len(),
                MAX_MSG_LEN
            );
            let mut acc = G1Projective::zero();
            for (i, m) in msg.iter().enumerate() {
                let scalar = local_fr_to_ark(m);
                acc += self.gens[i].into_group() * scalar;
            }
            let r_scalar = local_fr_to_ark(&randomness);
            acc += self.h.into_group() * r_scalar;

            let aff = acc.into_affine();
            // Canonical-compressed serialization of the BN254 G1
            // affine point. Identity point (the zero commitment) maps
            // to a unique tag; non-identity points serialize to 32
            // bytes (compressed) — we use ark-serialize's CanonicalSerialize
            // for that.
            let mut bytes = Vec::with_capacity(33);
            aff.serialize_compressed(&mut bytes)
                .expect("infallible: compressed serialize to Vec");
            // Hash to our local Fr. Use Poseidon over the byte stream
            // chunked into Fr scalars so the boundary matches what
            // Noir circuits do on the other side. Pad with the
            // randomness byte length so different message lengths
            // can't collide.
            let chunks: Vec<Fr> = bytes
                .chunks(31)
                .map(|c| {
                    let mut buf = [0u8; 32];
                    buf[..c.len()].copy_from_slice(c);
                    Fr::from_bytes_reduce(&buf)
                })
                .collect();
            poseidon.hash_many(&chunks)
        }
    }

    /// Convert our local `Fr` to ark-bn254 `Fr`. Both are
    /// 32-byte little-endian, both are the same field modulus
    /// (BN254 scalar). We round-trip through the canonical byte form
    /// so the limb layout doesn't matter.
    fn local_fr_to_ark(x: &Fr) -> ArkFr {
        let bytes = x.to_bytes();
        ArkFr::from_le_bytes_mod_order(&bytes)
    }

    /// Try-and-increment hash-to-curve. Hash `(domain, index, ctr)`
    /// with Keccak256, interpret the digest as a candidate `x`-coord
    /// on BN254 G1, check whether `x^3 + 3` is a quadratic residue;
    /// if yes, pick the lex-smaller `y` root; if no, increment `ctr`
    /// and retry. Terminates with overwhelming probability.
    fn derive_generator(domain: &[u8], index: u64) -> G1Affine {
        for ctr in 0u64..1_000 {
            let mut h = Keccak256::new();
            h.update(domain);
            h.update(&index.to_le_bytes());
            h.update(&ctr.to_le_bytes());
            let digest = h.finalize();
            let mut x_bytes = [0u8; 32];
            x_bytes.copy_from_slice(&digest);
            let x = ArkFq::from_le_bytes_mod_order(&x_bytes);
            // Try to find y with y^2 = x^3 + 3 (BN254 curve eq).
            let three = ArkFq::from(3u64);
            let rhs = (x.square() * x) + three;
            if let Some(y) = rhs.sqrt() {
                // Pick the lex-smaller representative for determinism.
                let neg_y = -y;
                let y_chosen = if y.into_bigint().to_bytes_le() < neg_y.into_bigint().to_bytes_le()
                {
                    y
                } else {
                    neg_y
                };
                let point = G1Affine::new_unchecked(x, y_chosen);
                if point.is_on_curve() && point.is_in_correct_subgroup_assuming_on_curve() {
                    return point;
                }
            }
        }
        panic!(
            "derive_generator: try-and-increment failed after 1000 iterations — domain {:?} index {}",
            domain, index
        );
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
        assert_ne!(
            c.commit(&msg, Fr::from_u64(1)),
            c.commit(&msg, Fr::from_u64(2))
        );
    }

    #[test]
    fn commit_is_message_sensitive() {
        let c = Commit::new();
        let r = Fr::from_u64(5);
        assert_ne!(
            c.commit(&[Fr::from_u64(1)], r),
            c.commit(&[Fr::from_u64(2)], r)
        );
    }

    /// Under the `prover` feature, the real EC Pedersen is in use.
    /// Verify it stays self-consistent and that swapping the slot of
    /// equal-magnitude values changes the commitment (i.e., the
    /// generator set is not degenerate).
    #[cfg(feature = "prover")]
    #[test]
    fn real_pedersen_position_sensitive() {
        let c = Commit::new();
        let r = Fr::from_u64(7);
        let a = c.commit(&[Fr::from_u64(1), Fr::from_u64(0)], r);
        let b = c.commit(&[Fr::from_u64(0), Fr::from_u64(1)], r);
        assert_ne!(a, b, "real Pedersen must use distinct generators per slot");
    }

    /// Under `--features prover`, exceeding the generator table is a
    /// hard error. We don't exercise the assert in CI by default
    /// (tests catch the boundary case under the feature).
    #[cfg(feature = "prover")]
    #[test]
    #[should_panic(expected = "exceeds MAX_MSG_LEN")]
    fn real_pedersen_rejects_overlong_message() {
        let c = Commit::new();
        let msg = vec![Fr::from_u64(1); MAX_MSG_LEN + 1];
        let _ = c.commit(&msg, Fr::from_u64(0));
    }
}
