//! Shielded EVM extension.
//!
//! This module is the integration point for Phase 4 of the privacy
//! redesign: transparent EVM contract execution stays exactly as it
//! is in revm today, but EOA *balance transfers* are routed through
//! the shielded pool.
//!
//! Three new things:
//!
//! 1. **`0x0200` precompile** — `shieldedTransfer(bytes)`. Takes a
//!    bincode-encoded [`ShieldedTransferTx`], verifies its ZK proof,
//!    inserts the nullifier and the new commitment into the
//!    [`crate::shielded_state::ShieldedState`].
//!
//! 2. **`0x0201` precompile** — `shield(uint256,bytes)` and
//!    `unshield(bytes)`. Bridges transparent ↔ shielded.
//!    - `shield(amount, encrypted_note_payload)`: debits the caller's
//!      transparent balance by `amount`, mints a shielded note of
//!      equal value.
//!    - `unshield(bytes)`: consumes a shielded note, credits a
//!      transparent address with the value (the only event that
//!      intentionally links a shielded note to a transparent
//!      address).
//!
//! 3. **Tx envelope type `0x7E`** — EIP-2718 typed transactions for
//!    privacy. The chain accepts `0x7E` envelopes alongside legacy
//!    `0x00` and EIP-1559 `0x02`.
//!
//! 4. **Hard-fork migration tool** — at activation height `H`, every
//!    EOA's transparent balance is snapshotted and minted as a
//!    single shielded note for that owner. This is the only event
//!    that links a transparent account to its shielded successor.

#![allow(dead_code)]

use crate::shielded_state::ShieldedState;
use prime_zkp::{
    Fr, NoteCommitment, Nullifier,
    noir::{Circuit, CircuitProof, MockVerifier, Verifier, VerifyError},
};
use revm::primitives::{Address, U256};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

/// EIP-2718 type byte for shielded transactions.
pub const SHIELDED_TX_TYPE: u8 = 0x7E;

/// Discriminated union of every shielded-transaction body type the
/// chain accepts inside a `0x7E` envelope. Carried as
/// [`crate::engine::Transaction::shielded_payload`].
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ShieldedEnvelope {
    Transfer(ShieldedTransferTx),
    Order(Box<crate::shielded_orders::ShieldedOrderTx>),
    Shield(ShieldTx),
    Unshield(UnshieldTx),
    LiquidationClaim(Box<crate::liquidation_auction::LiquidationClaim>),
    LiquidationExecute(Box<crate::liquidation_auction::LiquidationExecute>),
}

impl ShieldedEnvelope {
    /// Discriminant byte used inside the envelope's serialized
    /// preamble (independent from the EIP-2718 type byte). Stable
    /// across `bincode` versions because we don't rely on enum
    /// ordering — we re-derive on each serialize.
    pub fn discriminant(&self) -> u8 {
        match self {
            ShieldedEnvelope::Transfer(_) => 0x01,
            ShieldedEnvelope::Order(_) => 0x02,
            ShieldedEnvelope::Shield(_) => 0x03,
            ShieldedEnvelope::Unshield(_) => 0x04,
            ShieldedEnvelope::LiquidationClaim(_) => 0x05,
            ShieldedEnvelope::LiquidationExecute(_) => 0x06,
        }
    }
}

#[derive(Debug, Error)]
pub enum ShieldedEvmError {
    #[error("zk proof verification failed: {0}")]
    InvalidProof(#[from] VerifyError),
    #[error("anchor root is not in the recent-roots window")]
    StaleAnchor,
    #[error("nullifier already spent")]
    DoubleSpend,
    #[error("transparent balance is too low to shield")]
    InsufficientTransparentBalance,
    #[error("unshield amount mismatches the spent note value")]
    UnshieldAmountMismatch,
    #[error("malformed precompile input")]
    MalformedInput,
}

/// Shielded transfer transaction. Encoded inside a `0x7E` tx envelope.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ShieldedTransferTx {
    pub anchor_root: Fr,
    pub nullifier: Fr,
    pub output_commitment: Fr,
    /// Encrypted payload (the recipient's note, sealed under their
    /// viewing key via ECDH). The chain doesn't decrypt; recipients
    /// scan the chain.
    pub encrypted_output: Vec<u8>,
    pub proof: CircuitProof,
}

/// Shield (transparent → shielded) transaction.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ShieldTx {
    /// The transparent EOA being debited. `tx.origin` must equal this.
    pub from: Address, // privacy-allow: shield bridge requires the transparent source EOA
    /// Amount being debited from the transparent balance and minted
    /// as a shielded note.
    pub amount: U256,
    /// The freshly-minted note's commitment.
    pub output_commitment: Fr,
    /// Encrypted payload addressed to the recipient (may be the same
    /// as `from`, may be a different shielded pubkey).
    pub encrypted_output: Vec<u8>,
    /// Proof of the output circuit: `commit(note) == output_commitment`
    /// and `note.value == amount`.
    pub proof: CircuitProof,
}

/// Unshield (shielded → transparent) transaction.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UnshieldTx {
    pub anchor_root: Fr,
    pub nullifier: Fr,
    /// Amount credited to `to`.
    pub amount: U256,
    /// Transparent destination. **This is the only place a shielded
    /// flow links to a transparent address.** Required for the
    /// out-of-pool semantics; cannot be hidden.
    pub to: Address, // privacy-allow: unshield bridge requires the transparent destination EOA
    /// Change-note commitment (may be `Fr::ZERO` for a full unshield).
    pub change_commitment: Fr,
    pub encrypted_change: Vec<u8>,
    /// Spend circuit proof; `public_amount` field equals `amount`.
    pub proof: CircuitProof,
}

/// Adapter that owns the [`ShieldedState`] and the transparent
/// balances for shielded EOAs. The chain engine holds one of these
/// and routes precompile dispatches through it.
#[derive(Debug)]
pub struct ShieldedEvm {
    pub state: ShieldedState,
    /// Mirror of revm's account-balance map for EOAs that have been
    /// "shielded". For now we keep our own copy because routing into
    /// revm's DB requires the EVM context to be live; in the
    /// final wiring this becomes a thin adapter over revm's account
    /// API.
    pub transparent_balances: HashMap<Address, U256>, // privacy-allow: explicit transparent-side mirror at the privacy boundary
    pub verifier: Box<dyn Verifier>,
}

impl Default for ShieldedEvm {
    fn default() -> Self {
        Self {
            state: ShieldedState::new(),
            transparent_balances: HashMap::new(),
            verifier: Box::new(MockVerifier::new()),
        }
    }
}

impl ShieldedEvm {
    pub fn new() -> Self {
        Self::default()
    }

    /// Credit a transparent EOA. The chain calls this from genesis
    /// state-loading and from the bridge module.
    pub fn credit_transparent(&mut self, who: Address, amount: U256) {
        let bal = self.transparent_balances.entry(who).or_insert(U256::ZERO);
        *bal = bal.saturating_add(amount);
    }

    /// Debit a transparent EOA. Returns `Err` on insufficient funds.
    pub fn debit_transparent(
        &mut self,
        who: Address,
        amount: U256,
    ) -> Result<(), ShieldedEvmError> {
        let bal = self
            .transparent_balances
            .get_mut(&who)
            .ok_or(ShieldedEvmError::InsufficientTransparentBalance)?;
        if *bal < amount {
            return Err(ShieldedEvmError::InsufficientTransparentBalance);
        }
        *bal -= amount;
        Ok(())
    }

    pub fn transparent_balance(&self, who: &Address) -> U256 {
        self.transparent_balances
            .get(who)
            .copied()
            .unwrap_or(U256::ZERO)
    }

    /// Apply a shielded transfer. Caller is responsible for
    /// charging gas before this.
    pub fn apply_shielded_transfer(
        &mut self,
        tx: &ShieldedTransferTx,
    ) -> Result<(), ShieldedEvmError> {
        if !self.state.is_recent_root(&tx.anchor_root) {
            return Err(ShieldedEvmError::StaleAnchor);
        }
        let public_inputs = vec![
            tx.anchor_root,
            tx.nullifier,
            tx.output_commitment,
            Fr::ZERO, // public_amount = 0 for pure shielded transfer
        ];
        self.verifier
            .verify(&tx.proof, Circuit::Spend, &public_inputs)?;
        if self.state.is_spent(&Nullifier(tx.nullifier)) {
            return Err(ShieldedEvmError::DoubleSpend);
        }
        self.state.spend(Nullifier(tx.nullifier))?;
        let _ = self
            .state
            .insert_note(NoteCommitment(tx.output_commitment))?;
        Ok(())
    }

    /// Apply a shield (transparent → shielded) operation.
    pub fn apply_shield(&mut self, tx: &ShieldTx) -> Result<(), ShieldedEvmError> {
        // 1. Debit the transparent EOA.
        self.debit_transparent(tx.from, tx.amount)?;
        // 2. Verify the output circuit proof.
        let public_inputs = vec![
            tx.output_commitment,
            Fr::ZERO, // asset_id = 0 (native PRIM); a real impl reads from `tx`
            Fr::from_u64(u256_low(&tx.amount)),
        ];
        self.verifier
            .verify(&tx.proof, Circuit::Output, &public_inputs)?;
        // 3. Insert the new note commitment.
        let _ = self
            .state
            .insert_note(NoteCommitment(tx.output_commitment))?;
        Ok(())
    }

    /// Apply an unshield (shielded → transparent) operation.
    pub fn apply_unshield(&mut self, tx: &UnshieldTx) -> Result<(), ShieldedEvmError> {
        if !self.state.is_recent_root(&tx.anchor_root) {
            return Err(ShieldedEvmError::StaleAnchor);
        }
        let public_inputs = vec![
            tx.anchor_root,
            tx.nullifier,
            tx.change_commitment,
            Fr::from_u64(u256_low(&tx.amount)),
        ];
        self.verifier
            .verify(&tx.proof, Circuit::Spend, &public_inputs)?;
        if self.state.is_spent(&Nullifier(tx.nullifier)) {
            return Err(ShieldedEvmError::DoubleSpend);
        }
        self.state.spend(Nullifier(tx.nullifier))?;
        // The change-commitment may be `Fr::ZERO` to indicate "no
        // change note". Insert only if non-zero.
        if tx.change_commitment != Fr::ZERO {
            let _ = self
                .state
                .insert_note(NoteCommitment(tx.change_commitment))?;
        }
        // Credit the transparent destination.
        self.credit_transparent(tx.to, tx.amount);
        Ok(())
    }
}

impl From<crate::shielded_state::ShieldedStateError> for ShieldedEvmError {
    fn from(e: crate::shielded_state::ShieldedStateError) -> Self {
        use crate::shielded_state::ShieldedStateError;
        match e {
            ShieldedStateError::DoubleSpend => ShieldedEvmError::DoubleSpend,
            ShieldedStateError::StaleAnchor => ShieldedEvmError::StaleAnchor,
            ShieldedStateError::InvalidMembershipProof => {
                ShieldedEvmError::InvalidProof(VerifyError::InvalidProof)
            }
            ShieldedStateError::DuplicateCommitment => {
                ShieldedEvmError::InvalidProof(VerifyError::InvalidProof)
            }
        }
    }
}

fn u256_low(x: &U256) -> u64 {
    x.as_limbs()[0]
}

// ---------------------------------------------------------------------------
// Hard-fork migration
// ---------------------------------------------------------------------------

/// Migration: at activation height `H`, take a snapshot of every
/// transparent EOA balance and mint a single shielded note for each.
/// This is the only block in chain history where transparent balances
/// link directly to shielded successors; from `H+1` onward, all new
/// flows are shielded.
///
/// The chain emits a single `MigrationCompleted` event with the new
/// note tree root and the count of migrated accounts. Individual
/// account → note mappings are kept locally by each wallet (the
/// owner derives `rho`, `psi` from a known seed).
#[derive(Clone, Debug)]
pub struct MigrationPlan {
    pub activation_height: u64,
    pub accounts: Vec<(Address, U256, Fr)>, // (owner, amount, owner_pk)
}

impl MigrationPlan {
    /// Apply the migration to a fresh ShieldedEvm. Returns the new
    /// tree root + the number of migrated accounts.
    pub fn apply(self, evm: &mut ShieldedEvm) -> Result<(Fr, u64), ShieldedEvmError> {
        let mut migrated = 0u64;
        for (owner, amount, owner_pk) in self.accounts {
            // Each migrated account gets a deterministic-yet-blinded
            // note. `rho`/`psi` are derived from the account address +
            // activation height so wallets can recover them
            // locally without any on-chain link.
            let rho = derive_migration_rho(owner, self.activation_height);
            let psi = derive_migration_psi(owner, self.activation_height);
            let note = prime_zkp::note::Note {
                value: u256_low(&amount) as u128,
                asset_id: 0,
                owner_pk,
                rho,
                psi,
            };
            let cm = note.commit(&prime_zkp::poseidon::Poseidon::default());
            let _ = evm.state.insert_note(cm)?;
            // Clear the transparent balance; from now on the funds
            // live as a shielded note.
            evm.transparent_balances.insert(owner, U256::ZERO);
            migrated += 1;
        }
        Ok((evm.state.current_root(), migrated))
    }
}

fn derive_migration_rho(owner: Address, height: u64) -> Fr {
    // privacy-allow: one-time migration derives shielded params from transparent EOA
    use sha3::{Digest, Keccak256};
    let mut h = Keccak256::new();
    h.update(b"PrimeChain-MigrationRho");
    h.update(owner.as_slice());
    h.update(height.to_le_bytes());
    let bytes: [u8; 32] = h.finalize().into();
    Fr::from_bytes_reduce(&bytes)
}

fn derive_migration_psi(owner: Address, height: u64) -> Fr {
    // privacy-allow: one-time migration derives shielded params from transparent EOA
    use sha3::{Digest, Keccak256};
    let mut h = Keccak256::new();
    h.update(b"PrimeChain-MigrationPsi");
    h.update(owner.as_slice());
    h.update(height.to_le_bytes());
    let bytes: [u8; 32] = h.finalize().into();
    Fr::from_bytes_reduce(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use prime_zkp::note::Note;
    use prime_zkp::poseidon::Poseidon;

    #[test]
    fn shield_round_trip() {
        let mut evm = ShieldedEvm::new();
        let alice = Address::repeat_byte(0xaa);
        evm.credit_transparent(alice, U256::from(1_000u64));
        let p = Poseidon::default();
        let note = Note {
            value: 1_000,
            asset_id: 0,
            owner_pk: Fr::from_u64(0xa11ce),
            rho: Fr::from_u64(1),
            psi: Fr::from_u64(2),
        };
        let cm = note.commit(&p).0;
        let public_inputs = vec![cm, Fr::ZERO, Fr::from_u64(1_000)];
        let proof = MockVerifier::new().prove(Circuit::Output, public_inputs);
        let tx = ShieldTx {
            from: alice,
            amount: U256::from(1_000u64),
            output_commitment: cm,
            encrypted_output: Vec::new(),
            proof,
        };
        evm.apply_shield(&tx).unwrap();
        assert_eq!(evm.transparent_balance(&alice), U256::ZERO);
        assert_eq!(evm.state.note_count(), 1);
    }

    #[test]
    fn shield_rejects_insufficient_balance() {
        let mut evm = ShieldedEvm::new();
        let alice = Address::repeat_byte(0xaa);
        // No credit.
        let p = Poseidon::default();
        let note = Note {
            value: 1_000,
            asset_id: 0,
            owner_pk: Fr::from_u64(1),
            rho: Fr::from_u64(1),
            psi: Fr::from_u64(2),
        };
        let cm = note.commit(&p).0;
        let public_inputs = vec![cm, Fr::ZERO, Fr::from_u64(1_000)];
        let proof = MockVerifier::new().prove(Circuit::Output, public_inputs);
        let tx = ShieldTx {
            from: alice,
            amount: U256::from(1_000u64),
            output_commitment: cm,
            encrypted_output: Vec::new(),
            proof,
        };
        let err = evm.apply_shield(&tx).unwrap_err();
        assert!(matches!(
            err,
            ShieldedEvmError::InsufficientTransparentBalance
        ));
    }

    #[test]
    fn unshield_credits_transparent() {
        let mut evm = ShieldedEvm::new();
        let p = Poseidon::default();
        let note = Note {
            value: 500,
            asset_id: 0,
            owner_pk: Fr::from_u64(1),
            rho: Fr::from_u64(10),
            psi: Fr::from_u64(20),
        };
        evm.state.insert_note(note.commit(&p)).unwrap();
        let sk = Fr::from_u64(0xcafe);
        let nullifier = note.nullifier(&p, &sk).0;
        let bob = Address::repeat_byte(0xbb);
        let anchor = evm.state.current_root();
        let public_inputs = vec![anchor, nullifier, Fr::ZERO, Fr::from_u64(500)];
        let proof = MockVerifier::new().prove(Circuit::Spend, public_inputs);
        let tx = UnshieldTx {
            anchor_root: anchor,
            nullifier,
            amount: U256::from(500u64),
            to: bob,
            change_commitment: Fr::ZERO,
            encrypted_change: Vec::new(),
            proof,
        };
        evm.apply_unshield(&tx).unwrap();
        assert_eq!(evm.transparent_balance(&bob), U256::from(500u64));
        assert!(evm.state.is_spent(&Nullifier(nullifier)));
    }

    #[test]
    fn shielded_transfer_inserts_new_commitment() {
        let mut evm = ShieldedEvm::new();
        let p = Poseidon::default();
        let in_note = Note {
            value: 800,
            asset_id: 0,
            owner_pk: Fr::from_u64(1),
            rho: Fr::from_u64(7),
            psi: Fr::from_u64(8),
        };
        evm.state.insert_note(in_note.commit(&p)).unwrap();
        let sk = Fr::from_u64(0xc0fe);
        let nullifier = in_note.nullifier(&p, &sk).0;
        let out_note = Note {
            value: 800,
            asset_id: 0,
            owner_pk: Fr::from_u64(2),
            rho: Fr::from_u64(11),
            psi: Fr::from_u64(12),
        };
        let out_cm = out_note.commit(&p).0;
        let anchor = evm.state.current_root();
        let public_inputs = vec![anchor, nullifier, out_cm, Fr::ZERO];
        let proof = MockVerifier::new().prove(Circuit::Spend, public_inputs);
        let tx = ShieldedTransferTx {
            anchor_root: anchor,
            nullifier,
            output_commitment: out_cm,
            encrypted_output: Vec::new(),
            proof,
        };
        evm.apply_shielded_transfer(&tx).unwrap();
        assert_eq!(evm.state.note_count(), 2);
    }

    #[test]
    fn migration_mints_one_note_per_account() {
        let mut evm = ShieldedEvm::new();
        let a = Address::repeat_byte(0x11);
        let b = Address::repeat_byte(0x22);
        evm.credit_transparent(a, U256::from(1_234u64));
        evm.credit_transparent(b, U256::from(5_678u64));
        let plan = MigrationPlan {
            activation_height: 1000,
            accounts: vec![
                (a, U256::from(1_234u64), Fr::from_u64(0xaa)),
                (b, U256::from(5_678u64), Fr::from_u64(0xbb)),
            ],
        };
        let (root, count) = plan.apply(&mut evm).unwrap();
        assert_eq!(count, 2);
        assert_eq!(evm.state.note_count(), 2);
        assert_ne!(root, Fr::ZERO);
        assert_eq!(evm.transparent_balance(&a), U256::ZERO);
        assert_eq!(evm.transparent_balance(&b), U256::ZERO);
    }
}
