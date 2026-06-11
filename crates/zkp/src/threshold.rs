//! Threshold encryption scaffolding for the encrypted mempool.
//!
//! In production this becomes BLS12-381 threshold ElGamal:
//! distributed key generation across validators with `k`-of-`n`
//! decryption at block-production time. Until the `blstrs` dependency
//! lands behind the `prover` feature (Phase 1.3), this module provides
//! the **same trait surface** with a deterministic in-memory provider
//! that is suitable for test and for wiring the mempool refactor.
//!
//! The substitution preserves:
//!
//! - The `k`-of-`n` decryption-share interface
//! - Per-epoch key rotation
//! - The "no validator alone can decrypt" property at the test level
//!   (the dummy provider XORs shares so any subset below `k` recovers
//!   garbage; at `k` it recovers the plaintext deterministically).
//!
//! This is intentionally NOT cryptographically secure on its own — it
//! is a faithful API placeholder. Producing real BLS shares is the
//! body of Phase 1.3.

use crate::field::Fr;
use serde::{Deserialize, Serialize};
use sha3::{Digest, Keccak256};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ThresholdError {
    #[error("insufficient shares: need {threshold}, have {collected}")]
    InsufficientShares { threshold: u32, collected: u32 },
    #[error("share from validator {0} is malformed")]
    MalformedShare(u32),
    #[error("ciphertext is malformed")]
    MalformedCiphertext,
    #[error("decryption failed authenticity check")]
    AuthenticityFailure,
    #[error("epoch mismatch")]
    EpochMismatch,
}

/// Encrypted payload (intent, order, transfer, etc.) sealed under the
/// epoch's threshold key.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EncryptedPayload {
    pub ciphertext: Vec<u8>,
    pub epoch: u64,
    /// Identifier of the encrypting party's ephemeral public key.
    pub ephemeral: Fr,
}

/// Single-validator share of the per-epoch decryption key.
#[derive(Clone, Debug)]
pub struct KeyShare {
    pub validator_index: u32,
    pub epoch: u64,
    /// In production: a BLS scalar share.
    pub material: [u8; 32],
}

/// Single-validator decryption share contributing to recovering one
/// ciphertext.
///
/// `material` is opaque bytes whose interpretation depends on the
/// provider:
///
/// - [`DummyThreshold`] uses a 32-byte XOR mask share.
/// - [`crate::bls_threshold::BlsThreshold`] uses a compressed BLS12-381
///   `G1` element (48 bytes) representing `c1 * sk_i`.
///
/// Keeping this as `Vec<u8>` lets the same `DecryptionShare` type
/// travel through HotStuff-2 vote messages, RPC, and disk without
/// the trait surface caring which threshold provider is active.
#[derive(Clone, Debug)]
pub struct DecryptionShare {
    pub validator_index: u32,
    pub epoch: u64,
    pub ciphertext_id: [u8; 32],
    pub material: Vec<u8>,
}

/// The reconstructed ciphertext-specific key, used in tests and in the
/// dummy implementation; in production this is the BLS12-381 group
/// element recovered via Lagrange interpolation.
#[derive(Clone, Debug)]
pub struct ThresholdCiphertext {
    pub payload: EncryptedPayload,
    pub recovered: Option<[u8; 32]>,
}

/// Threshold-encryption provider trait. Each phase swaps the
/// implementation:
///
/// - Phase 0 (today): in-memory `DummyThreshold`.
/// - Phase 1.3: `BlsThreshold` over BLS12-381.
pub trait ThresholdElGamal: Send + Sync + std::fmt::Debug {
    fn encrypt(&self, plaintext: &[u8], epoch: u64) -> EncryptedPayload;

    fn submit_share(&mut self, share: DecryptionShare) -> Result<Option<Vec<u8>>, ThresholdError>;

    fn threshold(&self) -> u32;
    fn total_validators(&self) -> u32;
    fn current_epoch(&self) -> u64;
}

/// Test-only provider. Uses XOR-share aggregation; any `k` shares
/// recombine to the same key material via XOR. **NOT cryptographically
/// secure on its own** — it is only a placeholder so the mempool
/// refactor can run end-to-end before BLS lands.
#[derive(Debug)]
pub struct DummyThreshold {
    threshold: u32,
    total: u32,
    epoch: u64,
    /// Collected shares keyed by ciphertext id.
    pending: std::collections::HashMap<[u8; 32], Vec<DecryptionShare>>,
    /// The ciphertexts we have produced; map id -> payload so we can
    /// authenticity-check on recovery.
    sealed: std::collections::HashMap<[u8; 32], Vec<u8>>,
}

impl DummyThreshold {
    pub fn new(threshold: u32, total: u32, epoch: u64) -> Self {
        assert!(threshold >= 1 && threshold <= total);
        Self {
            threshold,
            total,
            epoch,
            pending: Default::default(),
            sealed: Default::default(),
        }
    }

    /// Compute a stable identifier for an encrypted payload so
    /// validators can address their decryption shares to it.
    pub fn ciphertext_id(payload: &EncryptedPayload) -> [u8; 32] {
        let mut h = Keccak256::new();
        h.update(&payload.ciphertext);
        h.update(payload.epoch.to_le_bytes());
        h.update(payload.ephemeral.to_bytes());
        let out = h.finalize();
        let mut id = [0u8; 32];
        id.copy_from_slice(&out);
        id
    }
}

impl ThresholdElGamal for DummyThreshold {
    fn encrypt(&self, plaintext: &[u8], epoch: u64) -> EncryptedPayload {
        // Derive a deterministic XOR mask from the epoch and the
        // plaintext's hash. The "ephemeral" field carries the mask
        // commitment so the dummy decryption can verify it.
        let mut h = Keccak256::new();
        h.update(b"MersennetChain-DummyThreshold");
        h.update(epoch.to_le_bytes());
        h.update(plaintext);
        let digest = h.finalize();
        let mut mask = [0u8; 32];
        mask.copy_from_slice(&digest);
        let ciphertext = xor_with_mask(plaintext, &mask);
        let eph_bytes = {
            let mut b = [0u8; 32];
            b.copy_from_slice(&digest);
            b
        };
        EncryptedPayload {
            ciphertext,
            epoch,
            ephemeral: Fr::from_bytes_reduce(&eph_bytes),
        }
    }

    fn submit_share(&mut self, share: DecryptionShare) -> Result<Option<Vec<u8>>, ThresholdError> {
        if share.epoch != self.epoch {
            return Err(ThresholdError::EpochMismatch);
        }
        if share.material.len() != 32 {
            return Err(ThresholdError::MalformedShare(share.validator_index));
        }
        let shares = self.pending.entry(share.ciphertext_id).or_default();
        if shares
            .iter()
            .any(|s| s.validator_index == share.validator_index)
        {
            return Ok(None);
        }
        shares.push(share.clone());
        if (shares.len() as u32) < self.threshold {
            return Ok(None);
        }
        // Recover the XOR mask.
        let mut mask = [0u8; 32];
        for s in shares.iter() {
            for (i, b) in s.material.iter().take(32).enumerate() {
                mask[i] ^= b;
            }
        }
        // Recover the ciphertext bytes we stored at encrypt time.
        let ciphertext = self
            .sealed
            .get(&share.ciphertext_id)
            .ok_or(ThresholdError::MalformedCiphertext)?;
        let plain = xor_with_mask(ciphertext, &mask);
        Ok(Some(plain))
    }

    fn threshold(&self) -> u32 {
        self.threshold
    }
    fn total_validators(&self) -> u32 {
        self.total
    }
    fn current_epoch(&self) -> u64 {
        self.epoch
    }
}

fn xor_with_mask(data: &[u8], mask: &[u8; 32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    for (i, b) in data.iter().enumerate() {
        out.push(b ^ mask[i & 31]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dummy_encrypt_then_decrypt_with_threshold_shares() {
        let mut t = DummyThreshold::new(2, 3, 7);
        let plaintext = b"hello, shielded world!".to_vec();
        let payload = t.encrypt(&plaintext, 7);
        let id = DummyThreshold::ciphertext_id(&payload);
        t.sealed.insert(id, payload.ciphertext.clone());

        // Compute the canonical mask that produced this ciphertext.
        let mut h = Keccak256::new();
        h.update(b"MersennetChain-DummyThreshold");
        h.update(7u64.to_le_bytes());
        h.update(&plaintext);
        let mask: [u8; 32] = h.finalize().into();

        // Split into two shares that XOR to `mask`.
        let mut share_a = [0u8; 32];
        rand::Rng::fill(&mut rand::thread_rng(), &mut share_a);
        let mut share_b = [0u8; 32];
        for i in 0..32 {
            share_b[i] = mask[i] ^ share_a[i];
        }

        let s1 = DecryptionShare {
            validator_index: 0,
            epoch: 7,
            ciphertext_id: id,
            material: share_a.to_vec(),
        };
        let s2 = DecryptionShare {
            validator_index: 1,
            epoch: 7,
            ciphertext_id: id,
            material: share_b.to_vec(),
        };

        assert!(t.submit_share(s1).unwrap().is_none()); // 1 share: below threshold
        let decrypted = t.submit_share(s2).unwrap().expect("threshold reached");
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn dummy_rejects_wrong_epoch_share() {
        let mut t = DummyThreshold::new(2, 3, 7);
        let s = DecryptionShare {
            validator_index: 0,
            epoch: 8,
            ciphertext_id: [0u8; 32],
            material: vec![0u8; 32],
        };
        assert!(matches!(
            t.submit_share(s),
            Err(ThresholdError::EpochMismatch)
        ));
    }
}
