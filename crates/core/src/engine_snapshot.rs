//! Engine-level snapshot envelope.
//!
//! Wraps the existing EVM-only `StateBackend::export_snapshot_bytes`
//! output with the shielded-subsystem state (notes, nullifiers, recent
//! roots, liquidation registry, transparent mirror balances) plus a
//! version byte so future hard-fork extensions can extend the schema
//! without breaking older nodes.
//!
//! ## Wire format
//!
//! ```text
//!   bytes 0..4   : magic 'P','Z','S','1' (legacy magic, 'Prime Zk Snapshot v1')
//!   bytes 4..8   : envelope version (LE u32; current = 2)
//!   bytes 8..end : bincode-encoded `EngineSnapshotEnvelope`
//! ```
//!
//! ## Backward compatibility
//!
//! Snapshots written before this module existed used the raw
//! `StateBackend::export_snapshot_bytes` output without the magic
//! prefix. [`EngineSnapshotEnvelope::decode`] detects this and
//! routes a legacy snapshot through the existing import code, with
//! zeroed shielded fields — equivalent to the pre-privacy-fork
//! state.

use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::code_publication::CodePublicationRegistrySnapshot;
use crate::liquidation_auction::LiquidationAuctionSnapshot;
use crate::shielded_evm::{ViewingGrantRevocation, ViewingGrantToken};
use crate::shielded_state::ShieldedSnapshot;
use revm::primitives::{Address, U256};

const MAGIC: [u8; 4] = *b"PZS1";
/// Envelope version 1: EVM snapshot only (no shielded fields). Used
/// only for the auto-upgrade legacy path.
pub const VERSION_LEGACY_NO_SHIELDED: u32 = 1;
/// Envelope version 2: EVM snapshot + full shielded state +
/// transparent-balance mirror + privacy mode flags.
pub const VERSION_WITH_SHIELDED: u32 = 2;

/// Serializable view of all shielded subsystems at a block boundary.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ShieldedSnapshotData {
    pub state: ShieldedSnapshot,
    pub auction: LiquidationAuctionSnapshot,
    /// Mirror of transparent EVM balances for accounts that have a
    /// shielded counterpart. Persisted as `(address_bytes, u256_be_bytes)`
    /// for a deterministic on-disk byte order.
    pub transparent_balances: Vec<([u8; 20], [u8; 32])>,
    /// `true` iff the operator has run the genesis migration step
    /// that mirrors transparent → shielded notes for every legacy
    /// account.
    pub migration_plan_applied: bool,
    /// Opt-in public contract code-hash attestations.
    #[serde(default)]
    pub code_publication_registry: CodePublicationRegistrySnapshot,
    /// Encrypted note payload sidecar keyed by note commitment.
    #[serde(default)]
    pub encrypted_note_payloads: Vec<([u8; 32], Vec<u8>)>,
    /// Registered selective-disclosure grant tokens.
    #[serde(default)]
    pub viewing_grants: Vec<ViewingGrantToken>,
    /// Grant revocations keyed by `grant_id`.
    #[serde(default)]
    pub viewing_grant_revocations: Vec<ViewingGrantRevocation>,
}

/// Wrapped snapshot containing the legacy `StateBackend` output plus
/// the shielded subsystems.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EngineSnapshotEnvelope {
    /// Block height the snapshot was taken at.
    pub height: u64,
    /// `chain_id` the snapshot belongs to. Cross-chain imports are
    /// refused if this doesn't match the local chain ID.
    pub chain_id: u64,
    /// Privacy hard-fork toggle.
    pub privacy_mode_activated: bool,
    /// Activation height for the privacy hard fork, if scheduled.
    pub privacy_activation_height: Option<u64>,
    /// EVM-side opaque snapshot bytes (output of
    /// `StateBackend::export_snapshot_bytes`).
    pub evm_snapshot: Vec<u8>,
    /// Shielded subsystems. `None` on legacy snapshots.
    pub shielded: Option<ShieldedSnapshotData>,
}

impl EngineSnapshotEnvelope {
    /// Encode this envelope to the on-disk byte layout (magic +
    /// version + bincode body).
    pub fn encode(&self) -> Result<Vec<u8>> {
        let body = bincode::serialize(self).map_err(|e| anyhow!("bincode encode: {e}"))?;
        let mut out = Vec::with_capacity(8 + body.len());
        out.extend_from_slice(&MAGIC);
        out.extend_from_slice(&VERSION_WITH_SHIELDED.to_le_bytes());
        out.extend_from_slice(&body);
        Ok(out)
    }

    /// Decode a snapshot envelope. If the input begins with the
    /// `PZS1` magic, parse as a versioned envelope. Otherwise,
    /// treat as a legacy `StateBackend::export_snapshot_bytes`
    /// payload and wrap it with zeroed shielded fields.
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        if bytes.len() >= 8 && bytes[..4] == MAGIC {
            let version = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
            match version {
                v if v == VERSION_WITH_SHIELDED => {
                    let body = &bytes[8..];
                    let env: EngineSnapshotEnvelope =
                        bincode::deserialize(body).map_err(|e| anyhow!("bincode decode: {e}"))?;
                    Ok(env)
                }
                v if v == VERSION_LEGACY_NO_SHIELDED => {
                    // V1 envelope: header byte only; body is the raw
                    // EVM snapshot. Used for migration during the
                    // hard fork — operators emit V1 from a pre-fork
                    // node and the V2 importer adds the shielded
                    // fields automatically.
                    let body = bytes[8..].to_vec();
                    Ok(EngineSnapshotEnvelope::wrap_legacy(body))
                }
                v => Err(anyhow!("unsupported snapshot envelope version: {v}")),
            }
        } else {
            // No magic prefix → pre-envelope legacy snapshot.
            Ok(EngineSnapshotEnvelope::wrap_legacy(bytes.to_vec()))
        }
    }

    /// Build a V2 envelope from a pre-envelope (legacy)
    /// `StateBackend::export_snapshot_bytes` payload. Used by the
    /// migration tool to upgrade a pre-fork testnet snapshot to the
    /// privacy-fork envelope.
    pub fn wrap_legacy(evm_snapshot: Vec<u8>) -> Self {
        Self {
            height: 0,
            chain_id: 0,
            privacy_mode_activated: false,
            privacy_activation_height: None,
            evm_snapshot,
            shielded: None,
        }
    }

    /// Helper for the `migrate-genesis` binary: insert a fresh
    /// shielded subsystem block into a legacy envelope, ready for
    /// the privacy testnet to boot from.
    pub fn with_shielded(mut self, data: ShieldedSnapshotData) -> Self {
        self.shielded = Some(data);
        self
    }

    /// Helper for the migration tool: stamp the activation height.
    pub fn with_privacy_activation(mut self, height: u64) -> Self {
        self.privacy_activation_height = Some(height);
        self
    }
}

/// Convert a transparent-balance map into the canonical persisted
/// representation used by [`ShieldedSnapshotData::transparent_balances`].
pub fn encode_transparent_balances(
    balances: &HashMap<Address, U256>, // privacy-allow: transparent-side mirror encode
) -> Vec<([u8; 20], [u8; 32])> {
    let mut out: Vec<([u8; 20], [u8; 32])> = balances
        .iter()
        .map(|(addr, amount)| {
            let mut addr_bytes = [0u8; 20];
            addr_bytes.copy_from_slice(addr.as_slice());
            (addr_bytes, amount.to_be_bytes::<32>())
        })
        .collect();
    out.sort_by_key(|a| a.0);
    out
}

/// Reverse of [`encode_transparent_balances`].
pub fn decode_transparent_balances(entries: &[([u8; 20], [u8; 32])]) -> HashMap<Address, U256> {
    // privacy-allow: transparent-side mirror decode
    let mut out = HashMap::with_capacity(entries.len());
    for (addr_bytes, amount_bytes) in entries {
        let addr = Address::from_slice(addr_bytes);
        let amount = U256::from_be_slice(amount_bytes);
        out.insert(addr, amount);
    }
    out
}

pub fn encode_viewing_grants(
    grants: &HashMap<[u8; 32], ViewingGrantToken>,
) -> Vec<ViewingGrantToken> {
    let mut out: Vec<ViewingGrantToken> = grants.values().cloned().collect();
    out.sort_by_key(|grant| grant.grant_id);
    out
}

pub fn encode_encrypted_note_payloads(
    payloads: &HashMap<[u8; 32], Vec<u8>>,
) -> Vec<([u8; 32], Vec<u8>)> {
    let mut out: Vec<([u8; 32], Vec<u8>)> = payloads
        .iter()
        .map(|(commitment, payload)| (*commitment, payload.clone()))
        .collect();
    out.sort_by_key(|entry| entry.0);
    out
}

pub fn decode_encrypted_note_payloads(
    entries: &[([u8; 32], Vec<u8>)],
) -> HashMap<[u8; 32], Vec<u8>> {
    let mut out = HashMap::with_capacity(entries.len());
    for (commitment, payload) in entries {
        out.insert(*commitment, payload.clone());
    }
    out
}

pub fn decode_viewing_grants(
    entries: &[ViewingGrantToken],
) -> HashMap<[u8; 32], ViewingGrantToken> {
    let mut out = HashMap::with_capacity(entries.len());
    for entry in entries {
        out.insert(entry.grant_id, entry.clone());
    }
    out
}

pub fn encode_viewing_grant_revocations(
    revocations: &HashMap<[u8; 32], ViewingGrantRevocation>,
) -> Vec<ViewingGrantRevocation> {
    let mut out: Vec<ViewingGrantRevocation> = revocations.values().cloned().collect();
    out.sort_by_key(|revocation| revocation.grant_id);
    out
}

pub fn decode_viewing_grant_revocations(
    entries: &[ViewingGrantRevocation],
) -> HashMap<[u8; 32], ViewingGrantRevocation> {
    let mut out = HashMap::with_capacity(entries.len());
    for entry in entries {
        out.insert(entry.grant_id, entry.clone());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_envelope() -> EngineSnapshotEnvelope {
        EngineSnapshotEnvelope {
            height: 42,
            chain_id: 7920,
            privacy_mode_activated: true,
            privacy_activation_height: Some(100),
            evm_snapshot: vec![1, 2, 3, 4, 5, 6, 7, 8],
            shielded: Some(ShieldedSnapshotData::default()),
        }
    }

    #[test]
    fn round_trip() {
        let env = dummy_envelope();
        let bytes = env.encode().unwrap();
        let decoded = EngineSnapshotEnvelope::decode(&bytes).unwrap();
        assert_eq!(decoded.height, env.height);
        assert_eq!(decoded.chain_id, env.chain_id);
        assert_eq!(decoded.privacy_mode_activated, env.privacy_mode_activated);
        assert_eq!(decoded.evm_snapshot, env.evm_snapshot);
        assert!(decoded.shielded.is_some());
    }

    #[test]
    fn legacy_input_wraps_with_no_shielded() {
        // A pre-envelope snapshot (no magic) decodes as a V2 with
        // shielded=None.
        let legacy = vec![0x01, 0x02, 0x03, 0x04, 0x05];
        let env = EngineSnapshotEnvelope::decode(&legacy).unwrap();
        assert!(env.shielded.is_none());
        assert_eq!(env.evm_snapshot, legacy);
    }

    #[test]
    fn v1_header_yields_empty_shielded() {
        // PZS1 + version=1 + body.
        let body = vec![0xAA, 0xBB, 0xCC];
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&MAGIC);
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&body);
        let env = EngineSnapshotEnvelope::decode(&bytes).unwrap();
        assert!(env.shielded.is_none());
        assert_eq!(env.evm_snapshot, body);
    }

    #[test]
    fn unknown_version_is_rejected() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&MAGIC);
        bytes.extend_from_slice(&999u32.to_le_bytes());
        bytes.extend_from_slice(&[0; 8]);
        let err = EngineSnapshotEnvelope::decode(&bytes).unwrap_err();
        assert!(err.to_string().contains("unsupported"));
    }

    #[test]
    fn transparent_balances_round_trip() {
        let mut m: HashMap<Address, U256> = HashMap::new();
        m.insert(Address::from([1u8; 20]), U256::from(100u64));
        m.insert(Address::from([2u8; 20]), U256::from(200u64));
        let encoded = encode_transparent_balances(&m);
        // Ordered output, descending.
        assert_eq!(encoded.len(), 2);
        assert_eq!(encoded[0].0, [1u8; 20]);
        assert_eq!(encoded[1].0, [2u8; 20]);
        let decoded = decode_transparent_balances(&encoded);
        assert_eq!(decoded, m);
    }
}
