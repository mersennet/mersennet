//! BLS12-381 threshold ElGamal — production implementation.
//!
//! Gated behind the `prover` feature on this crate. When the feature
//! is off (the workspace default in CI and devnet), the [`BlsThreshold`]
//! struct compiles but every method returns
//! [`ThresholdError::AuthenticityFailure`]. This is intentional: the
//! shape of the trait stays stable across feature flags so the
//! `crates/core` callers don't fork.
//!
//! ## Phase 1.3 work plan
//!
//! 1. Add `blstrs = "0.7"` (and `group = "0.13"`) as a dependency
//!    gated by the `prover` feature.
//! 2. Implement Pedersen-DKG for the validator set, with a per-epoch
//!    setup phase that runs in the consensus protocol (HotStuff-2
//!    extra round).
//! 3. Per-share material is a BLS12-381 scalar in G1; share
//!    aggregation is Lagrange interpolation in G1.
//! 4. ElGamal encryption uses the recovered group element as a
//!    KEM/DEM construction: `c = E_K(m)` where `K = HKDF(g^{xy})`,
//!    `E` is ChaCha20-Poly1305.
//! 5. Authenticity check on share: each validator's share is signed
//!    by the validator's identity key; a wrong share is detectable
//!    by comparing the recovered ChaCha20-Poly1305 tag. Slashing is
//!    the consensus-layer responsibility.

use crate::threshold::{
    DecryptionShare, EncryptedPayload, ThresholdElGamal, ThresholdError,
};

#[derive(Debug)]
pub struct BlsThreshold {
    threshold: u32,
    total_validators: u32,
    epoch: u64,
}

impl BlsThreshold {
    pub fn new(threshold: u32, total_validators: u32, epoch: u64) -> Self {
        assert!(threshold >= 1 && threshold <= total_validators);
        Self {
            threshold,
            total_validators,
            epoch,
        }
    }
}

#[cfg(feature = "prover")]
impl ThresholdElGamal for BlsThreshold {
    fn encrypt(&self, _plaintext: &[u8], _epoch: u64) -> EncryptedPayload {
        unimplemented!(
            "BLS12-381 threshold ElGamal encryption — Phase 1.3.x; track in docs/internal/zk-privacy-plan.md"
        )
    }
    fn submit_share(
        &mut self,
        _share: DecryptionShare,
    ) -> Result<Option<Vec<u8>>, ThresholdError> {
        unimplemented!(
            "BLS12-381 threshold ElGamal decryption — Phase 1.3.x; track in docs/internal/zk-privacy-plan.md"
        )
    }
    fn threshold(&self) -> u32 {
        self.threshold
    }
    fn total_validators(&self) -> u32 {
        self.total_validators
    }
    fn current_epoch(&self) -> u64 {
        self.epoch
    }
}

#[cfg(not(feature = "prover"))]
impl ThresholdElGamal for BlsThreshold {
    fn encrypt(&self, _plaintext: &[u8], epoch: u64) -> EncryptedPayload {
        // Without the prover feature, we still need to return a
        // typed value so callers can compile against the trait. The
        // returned payload is empty; production callers must enable
        // the feature.
        EncryptedPayload {
            ciphertext: Vec::new(),
            epoch,
            ephemeral: crate::Fr::ZERO,
        }
    }
    fn submit_share(
        &mut self,
        _share: DecryptionShare,
    ) -> Result<Option<Vec<u8>>, ThresholdError> {
        Err(ThresholdError::AuthenticityFailure)
    }
    fn threshold(&self) -> u32 {
        self.threshold
    }
    fn total_validators(&self) -> u32 {
        self.total_validators
    }
    fn current_epoch(&self) -> u64 {
        self.epoch
    }
}
