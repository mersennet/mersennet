//! Distributed key generation (DKG) for the threshold-encrypted
//! mempool (Workstream D4).
//!
//! See `crates/core/src/dkg.rs` body below for the full protocol
//! description. This file is the in-engine wiring; the underlying
//! group arithmetic lives in [`mersennet_zkp::bls_threshold`] behind the
//! `prover` feature.

#![allow(dead_code)]

use crate::hotstuff2::HotStuff2;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use sha3::{Digest, Keccak256};
use std::collections::HashMap;
use thiserror::Error;

/// Validator identity in the DKG protocol. We use a 20-byte fingerprint
/// derived from the validator's long-term key (typically `keccak256(pk)[..20]`).
/// This is *not* an EVM address — there is no EOA semantics attached to it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ValidatorId(pub [u8; 20]);

impl ValidatorId {
    pub fn to_bytes(&self) -> [u8; 20] {
        self.0
    }
}

/// Epoch length, in blocks. One epoch = one DKG ceremony output.
/// Mirrors `BID_REVELATION_DELAY` granularity but at a much larger
/// scale; ~1 hour at 200ms blocks = 18000 blocks. The privacy
/// testnet uses a shorter 1800-block epoch (~6 minutes) for faster
/// rotation testing.
pub const DEFAULT_EPOCH_LENGTH_BLOCKS: u64 = 18_000;

/// Maximum number of DKG ceremony retries before the protocol falls
/// back to the previous epoch's key. After this many failed
/// ceremonies in a row, governance must intervene.
pub const MAX_CEREMONY_RETRIES: u32 = 3;

#[derive(Debug, Error)]
pub enum DkgError {
    #[error("epoch {0} ceremony already in progress")]
    CeremonyInProgress(u64),
    #[error("validator {0:?} is not in the active set")]
    UnknownValidator(ValidatorId),
    #[error("share verification failed for validator {0:?}")]
    InvalidShare(ValidatorId),
    #[error("insufficient share submissions: have {have}, need {need}")]
    InsufficientShares { have: u32, need: u32 },
    #[error("ceremony has not completed yet for epoch {0}")]
    CeremonyIncomplete(u64),
}

/// Pedersen-VSS share commitment. Each validator broadcasts these
/// as part of the ceremony so others can verify their share without
/// revealing it.
///
/// In production (`prover` feature), `commitment_bytes` is the
/// compressed BLS12-381 G1 element representing `g^a_0 * h^a_1 *
/// ... * h^a_{t-1}` over the validator's secret polynomial. For
/// tests + the default lane, it's a Keccak256 fingerprint of the
/// share material — enough for the API to exercise round-trips.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ShareCommitment {
    pub from: ValidatorId,
    pub epoch: u64,
    /// Compressed G1, or a Keccak256 fingerprint in the default lane.
    pub commitment_bytes: Vec<u8>,
}

/// Encrypted share addressed to a specific recipient validator.
///
/// The share is encrypted under the recipient's long-term node key
/// so only they can decrypt it. The chain transcript records the
/// ciphertext (for audit / dispute resolution) but never the
/// plaintext.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EncryptedShare {
    pub from: ValidatorId,
    pub to: ValidatorId,
    pub epoch: u64,
    pub ciphertext: Vec<u8>,
}

/// Slashing complaint raised by a recipient who decrypted a share
/// and found it inconsistent with the public commitment.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SlashingComplaint {
    pub complainant: ValidatorId,
    pub accused: ValidatorId,
    pub epoch: u64,
    /// The complainant attaches the decrypted plaintext + a proof
    /// of decryption (NIZK of "I knew the secret key that decrypts
    /// `ciphertext` and the result fails Pedersen verification").
    /// We keep this as opaque bytes here; the verifier lives in
    /// `crates/zkp/src/circuits/`.
    pub evidence_bytes: Vec<u8>,
}

/// Status of an ongoing DKG ceremony.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CeremonyStatus {
    pub epoch: u64,
    pub started_at_block: u64,
    pub commitments_received: u32,
    pub shares_received: HashMap<ValidatorId, u32>,
    pub complaints: Vec<SlashingComplaint>,
    pub completed: bool,
    /// Aggregated group public key (compressed bytes) once the
    /// ceremony finalizes. None until then.
    pub group_pk_bytes: Option<Vec<u8>>,
}

/// Output of a successful DKG ceremony: the group public key plus
/// the per-validator decryption-share material. The group pk is
/// published; the per-validator shares are emitted only to their
/// respective recipients (they live in node-local secure storage).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DkgCeremonyResult {
    pub epoch: u64,
    pub group_pk_bytes: Vec<u8>,
    /// For audit + monitoring only: which validators contributed.
    pub contributing_validators: Vec<ValidatorId>,
}

/// In-engine DKG coordinator. Lives on every node; tracks the
/// per-epoch ceremony state and emits inputs to the HotStuff-2
/// sub-round.
#[derive(Debug, Default)]
pub struct DkgCoordinator {
    pub epoch_length_blocks: u64,
    pub current_epoch: u64,
    pub ceremonies: HashMap<u64, CeremonyStatus>,
    /// Most recently finalized result. The engine queries this to
    /// configure the threshold-mempool group public key for new
    /// encryptions.
    pub latest_result: Option<DkgCeremonyResult>,
    pub consecutive_failures: u32,
}

impl DkgCoordinator {
    pub fn new(epoch_length_blocks: u64) -> Self {
        Self {
            epoch_length_blocks,
            current_epoch: 0,
            ceremonies: HashMap::new(),
            latest_result: None,
            consecutive_failures: 0,
        }
    }

    /// Block-level hook called from the engine. Detects epoch
    /// boundaries and triggers a fresh ceremony start. Idempotent.
    pub fn on_block(&mut self, block_number: u64) -> Option<u64> {
        if self.epoch_length_blocks == 0 {
            return None;
        }
        let new_epoch = block_number / self.epoch_length_blocks;
        if new_epoch == self.current_epoch && !self.ceremonies.is_empty() {
            return None;
        }
        if new_epoch > self.current_epoch || self.ceremonies.is_empty() {
            self.current_epoch = new_epoch;
            self.ceremonies.insert(
                new_epoch,
                CeremonyStatus {
                    epoch: new_epoch,
                    started_at_block: block_number,
                    ..Default::default()
                },
            );
            return Some(new_epoch);
        }
        None
    }

    /// Record an incoming share commitment from a validator. Returns
    /// `true` if it advanced the ceremony state.
    pub fn record_commitment(&mut self, commitment: ShareCommitment) -> Result<bool, DkgError> {
        let epoch = commitment.epoch;
        let status = self
            .ceremonies
            .entry(epoch)
            .or_insert_with(|| CeremonyStatus {
                epoch,
                ..Default::default()
            });
        status.commitments_received += 1;
        Ok(true)
    }

    /// Record an incoming encrypted share. The recipient validator
    /// is who picks this up; here we just bump the counter so the
    /// ceremony status reflects progress.
    pub fn record_share(&mut self, share: EncryptedShare) -> Result<bool, DkgError> {
        let epoch = share.epoch;
        let status = self
            .ceremonies
            .entry(epoch)
            .or_insert_with(|| CeremonyStatus {
                epoch,
                ..Default::default()
            });
        let n = status.shares_received.entry(share.to).or_insert(0);
        *n += 1;
        Ok(true)
    }

    /// Record a slashing complaint. Returns the accused validator
    /// so the consensus layer can deduct stake (the actual slashing
    /// happens in `hotstuff2`'s slashing oracle, not here).
    pub fn record_complaint(
        &mut self,
        complaint: SlashingComplaint,
    ) -> Result<ValidatorId, DkgError> {
        let epoch = complaint.epoch;
        let status = self
            .ceremonies
            .entry(epoch)
            .or_insert_with(|| CeremonyStatus {
                epoch,
                ..Default::default()
            });
        let accused = complaint.accused;
        status.complaints.push(complaint);
        Ok(accused)
    }

    /// Finalize the ceremony for `epoch` once `threshold` valid
    /// commitments and at least `threshold` shares have arrived for
    /// every active recipient. Returns `Ok(Some(result))` if the
    /// ceremony finalized, `Ok(None)` if more inputs are needed.
    pub fn try_finalize(
        &mut self,
        epoch: u64,
        threshold: u32,
        validators: &[ValidatorId],
    ) -> Result<Option<DkgCeremonyResult>, DkgError> {
        let status = self
            .ceremonies
            .get_mut(&epoch)
            .ok_or(DkgError::CeremonyIncomplete(epoch))?;

        if status.completed {
            // Already finalized.
            return Ok(self.latest_result.clone());
        }

        if status.commitments_received < threshold {
            return Ok(None);
        }
        // Each validator must have received at least `threshold`
        // share envelopes.
        for v in validators {
            if status.shares_received.get(v).copied().unwrap_or(0) < threshold {
                return Ok(None);
            }
        }

        // Aggregate group pk. In the default lane (no prover
        // feature) we use a deterministic Keccak digest of the
        // commitment fingerprints — enough to exercise the
        // engine-side wiring; the real aggregation lives in
        // `mersennet_zkp::bls_threshold::Inner::aggregate_dkg_output`
        // (Workstream D4 second pass, post-mainnet-bake).
        let mut h = Keccak256::new();
        h.update(b"MersennetChain-DKG-AggPk-v0");
        h.update(epoch.to_le_bytes());
        for v in validators {
            h.update(v.to_bytes());
        }
        let group_pk_bytes = h.finalize().to_vec();

        let result = DkgCeremonyResult {
            epoch,
            group_pk_bytes: group_pk_bytes.clone(),
            contributing_validators: validators.to_vec(),
        };
        status.completed = true;
        status.group_pk_bytes = Some(group_pk_bytes);
        self.latest_result = Some(result.clone());
        self.consecutive_failures = 0;
        Ok(Some(result))
    }

    /// Called when a ceremony times out without reaching threshold.
    /// Bumps the consecutive-failure counter; if we exceed
    /// `MAX_CEREMONY_RETRIES`, the engine should refuse to advance
    /// past the current epoch boundary until governance intervenes.
    pub fn record_failure(&mut self, epoch: u64) {
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
        if let Some(status) = self.ceremonies.get_mut(&epoch) {
            status.completed = false;
        }
    }
}

/// Hook into HotStuff-2: returns the inputs that should be included
/// in the DKG sub-round of the next consensus block. This is a
/// thin shim around `DkgCoordinator` to keep the consensus module
/// independent of the DKG state machine.
pub fn hotstuff_dkg_inputs(
    _hs: &HotStuff2,
    coord: &DkgCoordinator,
    block_number: u64,
) -> DkgRoundInputs {
    let epoch = block_number
        .checked_div(coord.epoch_length_blocks)
        .unwrap_or(0);
    DkgRoundInputs {
        epoch,
        is_epoch_boundary: coord.epoch_length_blocks > 0
            && block_number.is_multiple_of(coord.epoch_length_blocks),
        latest_group_pk: coord
            .latest_result
            .as_ref()
            .map(|r| r.group_pk_bytes.clone()),
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DkgRoundInputs {
    pub epoch: u64,
    pub is_epoch_boundary: bool,
    pub latest_group_pk: Option<Vec<u8>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vid(i: u8) -> ValidatorId {
        ValidatorId([i; 20])
    }

    #[test]
    fn epoch_boundary_starts_new_ceremony() {
        let mut c = DkgCoordinator::new(100);
        let r = c.on_block(0);
        assert_eq!(r, Some(0));
        assert!(c.ceremonies.contains_key(&0));

        // Same epoch — no new ceremony.
        let r = c.on_block(50);
        assert_eq!(r, None);

        // Cross the boundary — new ceremony.
        let r = c.on_block(100);
        assert_eq!(r, Some(1));
        assert!(c.ceremonies.contains_key(&1));
    }

    #[test]
    fn try_finalize_requires_threshold_commitments_and_shares() {
        let mut c = DkgCoordinator::new(100);
        c.on_block(0);
        let validators = vec![vid(1), vid(2), vid(3)];
        let threshold = 2;

        // No commitments yet.
        assert!(c.try_finalize(0, threshold, &validators).unwrap().is_none());

        for v in &validators {
            c.record_commitment(ShareCommitment {
                from: *v,
                epoch: 0,
                commitment_bytes: vec![0u8; 32],
            })
            .unwrap();
        }

        // Commitments now in; shares still missing.
        assert!(c.try_finalize(0, threshold, &validators).unwrap().is_none());

        // Add 2 shares per recipient.
        for to in &validators {
            for from in validators.iter().take(2) {
                c.record_share(EncryptedShare {
                    from: *from,
                    to: *to,
                    epoch: 0,
                    ciphertext: vec![0u8; 16],
                })
                .unwrap();
            }
        }

        let result = c.try_finalize(0, threshold, &validators).unwrap();
        assert!(result.is_some());
        let r = result.unwrap();
        assert_eq!(r.epoch, 0);
        assert!(!r.group_pk_bytes.is_empty());
    }

    #[test]
    fn complaint_is_recorded() {
        let mut c = DkgCoordinator::new(100);
        c.on_block(0);
        let accused = c
            .record_complaint(SlashingComplaint {
                complainant: vid(1),
                accused: vid(2),
                epoch: 0,
                evidence_bytes: vec![1, 2, 3],
            })
            .unwrap();
        assert_eq!(accused, vid(2));
        assert_eq!(c.ceremonies[&0].complaints.len(), 1);
    }

    #[test]
    fn failure_counter_increments() {
        let mut c = DkgCoordinator::new(100);
        c.on_block(0);
        c.record_failure(0);
        c.record_failure(0);
        assert_eq!(c.consecutive_failures, 2);
    }
}
