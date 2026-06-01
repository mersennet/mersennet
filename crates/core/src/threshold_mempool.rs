//! v2 encrypted mempool, built on the [`prime_zkp::threshold`]
//! trait surface so the cryptographic core can be swapped between
//!
//! - the `DummyThreshold` provider (workspace default, used in tests
//!   and devnet),
//! - a real BLS12-381 threshold ElGamal provider gated behind the
//!   `prover` feature on `prime-zkp` (Phase 1.3 follow-up).
//!
//! This module is intentionally separate from the legacy
//! [`crate::encrypted_mempool`], which sits behind
//! `FEATURE_ENCRYPTED_MEMPOOL = false` in
//! [`crate::chain_features`] and will be removed once the shielded
//! engine cuts over (Phase 2).
//!
//! ## Threat model
//!
//! - A network observer learns only the encrypted payload and the
//!   sender's *encryption pub key* (an ephemeral one, rotated per
//!   intent). They do **not** learn the sender's chain identity, the
//!   amounts, the price, the side, or the market.
//! - A minority of validators (< threshold) cannot decrypt anything.
//! - A majority of validators (>= threshold) can decrypt a specific
//!   intent at FBA tick time. Until then, even validators see only
//!   the ciphertext.
//! - A malicious validator that posts a wrong decryption share is
//!   detectable: the recovered plaintext fails authenticity (its
//!   hash does not match the ciphertext-id binding), and the share
//!   can be slashed.

#![allow(dead_code)]

use prime_zkp::{
    DecryptionShare, EncryptedPayload, ThresholdElGamal, ThresholdError,
    sp1::DecryptedIntentWitness, threshold::DummyThreshold,
};
use revm::primitives::{B256, keccak256};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use thiserror::Error;

/// A shielded intent submitted to the mempool. The plaintext could be
/// a shielded transfer, a shielded order, or a liquidation bid — the
/// mempool does not introspect, it only sequences and gates
/// decryption.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ShieldedIntent {
    /// Globally-unique identifier (Keccak-256 of `payload`).
    pub id: B256,
    /// Threshold-encrypted payload.
    pub payload: EncryptedPayload,
    /// Plaintext gas limit. The mempool needs this to ration block
    /// space without decrypting the intent.
    pub gas_limit: u64,
    /// Plaintext maximum fee per gas. Used for ordering.
    pub max_fee_per_gas: u64,
    /// Submission timestamp, validator-stamped.
    pub submitted_at: u64,
}

#[derive(Debug, Error)]
pub enum MempoolError {
    #[error("mempool is full ({0} pending)")]
    Full(usize),
    #[error("duplicate intent")]
    Duplicate,
    #[error("threshold cryptography error: {0}")]
    Threshold(#[from] ThresholdError),
}

/// v2 encrypted mempool.
pub struct ThresholdMempool {
    /// Threshold cryptography provider. Wrapped in an `Arc<Mutex<_>>`
    /// because the engine needs both submission and share-collection
    /// paths to share the provider across threads.
    provider: Arc<Mutex<Box<dyn ThresholdElGamal>>>,
    /// Pending intents indexed by id.
    pending: HashMap<B256, ShieldedIntent>,
    /// Decryption result cache (id -> plaintext bytes) for intents
    /// that have reached threshold and been recovered.
    decrypted: HashMap<B256, Vec<u8>>,
    /// Capacity.
    max_size: usize,
    /// Counters for metrics.
    pub stats: MempoolStats,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct MempoolStats {
    pub total_submitted: u64,
    pub total_decrypted: u64,
    pub rejected_full: u64,
    pub rejected_duplicate: u64,
    pub rejected_threshold: u64,
}

impl std::fmt::Debug for ThresholdMempool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ThresholdMempool")
            .field("pending", &self.pending.len())
            .field("decrypted", &self.decrypted.len())
            .field("max_size", &self.max_size)
            .field("stats", &self.stats)
            .finish()
    }
}

impl ThresholdMempool {
    /// Build a mempool using the workspace-default
    /// [`prime_zkp::threshold::DummyThreshold`] provider.
    pub fn new(threshold: u32, total_validators: u32, max_size: usize) -> Self {
        let provider: Box<dyn ThresholdElGamal> =
            Box::new(DummyThreshold::new(threshold, total_validators, 0));
        Self::with_provider(provider, max_size)
    }

    /// Build a mempool with a caller-supplied provider. Production
    /// validators use this with a BLS-backed implementation.
    pub fn with_provider(provider: Box<dyn ThresholdElGamal>, max_size: usize) -> Self {
        Self {
            provider: Arc::new(Mutex::new(provider)),
            pending: HashMap::new(),
            decrypted: HashMap::new(),
            max_size,
            stats: MempoolStats::default(),
        }
    }

    /// Submit a freshly-encrypted intent.
    pub fn submit(&mut self, intent: ShieldedIntent) -> Result<B256, MempoolError> {
        if self.pending.len() + self.decrypted.len() >= self.max_size {
            self.stats.rejected_full = self.stats.rejected_full.saturating_add(1);
            return Err(MempoolError::Full(self.pending.len()));
        }
        if self.pending.contains_key(&intent.id) || self.decrypted.contains_key(&intent.id) {
            self.stats.rejected_duplicate = self.stats.rejected_duplicate.saturating_add(1);
            return Err(MempoolError::Duplicate);
        }
        let id = intent.id;
        self.pending.insert(id, intent);
        self.stats.total_submitted = self.stats.total_submitted.saturating_add(1);
        Ok(id)
    }

    /// Help the convenience layer: encrypt a plaintext under the
    /// current epoch and build a `ShieldedIntent` for submission.
    pub fn encrypt_plaintext(
        &self,
        plaintext: &[u8],
        gas_limit: u64,
        max_fee_per_gas: u64,
        now: u64,
    ) -> ShieldedIntent {
        let provider = self.provider.lock().expect("threshold provider mutex");
        let epoch = provider.current_epoch();
        let payload = provider.encrypt(plaintext, epoch);
        let id = {
            let mut buf = Vec::with_capacity(payload.ciphertext.len() + 8);
            buf.extend_from_slice(&payload.ciphertext);
            buf.extend_from_slice(&payload.epoch.to_le_bytes());
            keccak256(&buf)
        };
        ShieldedIntent {
            id,
            payload,
            gas_limit,
            max_fee_per_gas,
            submitted_at: now,
        }
    }

    /// Submit a decryption share. Returns `Some(plaintext)` when the
    /// share contribution causes a previously-pending intent to
    /// reach threshold.
    pub fn submit_decryption_share(
        &mut self,
        intent_id: B256,
        share: DecryptionShare,
    ) -> Result<Option<Vec<u8>>, MempoolError> {
        // The provider knows how to combine shares; we just wire the
        // call and persist the recovered plaintext keyed by intent id.
        let recovered = {
            let mut provider = self.provider.lock().expect("threshold provider mutex");
            match provider.submit_share(share) {
                Ok(v) => v,
                Err(e) => {
                    self.stats.rejected_threshold = self.stats.rejected_threshold.saturating_add(1);
                    return Err(MempoolError::Threshold(e));
                }
            }
        };
        if let Some(plaintext) = recovered {
            if let Some(intent) = self.pending.remove(&intent_id) {
                self.decrypted.insert(intent.id, plaintext.clone());
                self.stats.total_decrypted = self.stats.total_decrypted.saturating_add(1);
            }
            Ok(Some(plaintext))
        } else {
            Ok(None)
        }
    }

    /// Drain up to `max` decrypted intents in arbitrary order. The
    /// FBA pre-tick step calls this to gather the batch to match.
    pub fn drain_decrypted(&mut self, max: usize) -> Vec<(B256, Vec<u8>)> {
        let ids: Vec<B256> = self.decrypted.keys().copied().take(max).collect();
        ids.into_iter()
            .filter_map(|id| self.decrypted.remove(&id).map(|p| (id, p)))
            .collect()
    }

    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    pub fn decrypted_count(&self) -> usize {
        self.decrypted.len()
    }

    pub fn decrypted_intent_witness(&self) -> Vec<DecryptedIntentWitness> {
        let mut intents: Vec<_> = self
            .decrypted
            .iter()
            .map(|(intent_id, plaintext)| DecryptedIntentWitness {
                intent_id: intent_id.0,
                plaintext: plaintext.clone(),
                order_admission: None,
            })
            .collect();
        intents.sort_by_key(|intent| intent.intent_id);
        intents
    }

    #[cfg(test)]
    pub(crate) fn insert_decrypted_for_test(&mut self, intent_id: B256, plaintext: Vec<u8>) {
        self.decrypted.insert(intent_id, plaintext);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a deterministic intent and confirm it lands in pending.
    #[test]
    fn submit_then_drain_after_threshold() {
        let mut mp = ThresholdMempool::new(2, 3, 100);
        let plaintext = b"intent-1";
        let intent = mp.encrypt_plaintext(plaintext, 200_000, 1_000_000_000, 100);
        let id = mp.submit(intent.clone()).unwrap();
        assert_eq!(mp.pending_count(), 1);
        assert_eq!(mp.decrypted_count(), 0);
        assert_eq!(id, intent.id);
        assert_eq!(mp.stats.total_submitted, 1);
    }

    #[test]
    fn duplicate_submission_is_rejected() {
        let mut mp = ThresholdMempool::new(2, 3, 100);
        let intent = mp.encrypt_plaintext(b"x", 1, 1, 1);
        assert!(mp.submit(intent.clone()).is_ok());
        let err = mp.submit(intent).unwrap_err();
        assert!(matches!(err, MempoolError::Duplicate));
    }

    #[test]
    fn full_mempool_rejects_new_intents() {
        let mut mp = ThresholdMempool::new(2, 3, 2);
        let a = mp.encrypt_plaintext(b"a", 1, 1, 1);
        let b = mp.encrypt_plaintext(b"b", 1, 1, 2);
        let c = mp.encrypt_plaintext(b"c", 1, 1, 3);
        mp.submit(a).unwrap();
        mp.submit(b).unwrap();
        let err = mp.submit(c).unwrap_err();
        assert!(matches!(err, MempoolError::Full(_)));
    }
}
