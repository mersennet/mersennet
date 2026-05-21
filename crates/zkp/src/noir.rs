//! Noir circuit proof envelope and verifier trait.
//!
//! The chain's hot path is verification (every block verifies many
//! proofs); proving happens client-side. This module gives both sides
//! a typed envelope and a stable error surface.
//!
//! When the `prover` feature is enabled at the workspace level, the
//! verifier dispatches to a real Barretenberg verifier via FFI. With
//! the feature off (the default workspace build), the verifier uses a
//! deterministic mock that accepts proofs whose `proof_bytes` is the
//! Poseidon hash of `public_inputs`. The mock matches the prover in
//! lockstep so unit tests cover the integration surfaces of the
//! shielded engine without depending on Barretenberg in CI.

use crate::field::Fr;
use crate::poseidon::Poseidon;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Identifier for which Noir circuit produced a given proof.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Circuit {
    Spend,
    Output,
    JoinSplit,
    OrderPlace,
    LiquidateClaim,
    LiquidateExecute,
    /// Reserved for tests.
    #[doc(hidden)]
    Test,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CircuitProof {
    pub circuit: Circuit,
    /// Canonical public-input field elements that the verifier must
    /// re-derive and compare.
    pub public_inputs: Vec<Fr>,
    /// Opaque proof bytes (Barretenberg UltraPlonk proof in production;
    /// Poseidon hash in the mock).
    pub proof_bytes: Vec<u8>,
    /// Verifying-key hash. The verifier rejects proofs whose vk_hash
    /// doesn't match the expected vk for `circuit`.
    pub vk_hash: [u8; 32],
}

#[derive(Debug, Error)]
pub enum VerifyError {
    #[error("circuit mismatch: expected {expected:?}, got {actual:?}")]
    CircuitMismatch { expected: Circuit, actual: Circuit },
    #[error("verifying-key mismatch")]
    VerifyingKeyMismatch,
    #[error("public-input length mismatch: expected {expected}, got {actual}")]
    PublicInputLength { expected: usize, actual: usize },
    #[error("public input #{index} mismatch")]
    PublicInputMismatch { index: usize },
    #[error("proof bytes failed cryptographic verification")]
    InvalidProof,
}

pub trait Verifier: Send + Sync + std::fmt::Debug {
    /// Verify `proof` against `expected_public_inputs`. The caller is
    /// responsible for assembling `expected_public_inputs` from the
    /// chain's state (e.g. the current Merkle root, the order's
    /// market id, the oracle price).
    fn verify(
        &self,
        proof: &CircuitProof,
        expected_circuit: Circuit,
        expected_public_inputs: &[Fr],
    ) -> Result<(), VerifyError>;
}

/// Mock verifier used when the `prover` feature is off. Accepts proofs
/// whose `proof_bytes` is `Poseidon.hash_many(public_inputs).to_bytes()`
/// and whose `vk_hash` matches a per-circuit constant.
#[derive(Clone, Debug, Default)]
pub struct MockVerifier {
    poseidon: Poseidon,
}

impl MockVerifier {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn vk_hash_for(circuit: Circuit) -> [u8; 32] {
        use sha3::{Digest, Keccak256};
        let label = match circuit {
            Circuit::Spend => "PrimeChain-MockVK-Spend",
            Circuit::Output => "PrimeChain-MockVK-Output",
            Circuit::JoinSplit => "PrimeChain-MockVK-JoinSplit",
            Circuit::OrderPlace => "PrimeChain-MockVK-OrderPlace",
            Circuit::LiquidateClaim => "PrimeChain-MockVK-LiquidateClaim",
            Circuit::LiquidateExecute => "PrimeChain-MockVK-LiquidateExecute",
            Circuit::Test => "PrimeChain-MockVK-Test",
        };
        let mut h = Keccak256::new();
        h.update(label.as_bytes());
        let out = h.finalize();
        let mut buf = [0u8; 32];
        buf.copy_from_slice(&out);
        buf
    }

    /// Build a mock proof. Test/SDK callers use this until the real
    /// prover is available.
    pub fn prove(&self, circuit: Circuit, public_inputs: Vec<Fr>) -> CircuitProof {
        let h = self.poseidon.hash_many(&public_inputs);
        CircuitProof {
            circuit,
            public_inputs,
            proof_bytes: h.to_bytes().to_vec(),
            vk_hash: Self::vk_hash_for(circuit),
        }
    }
}

impl Verifier for MockVerifier {
    fn verify(
        &self,
        proof: &CircuitProof,
        expected_circuit: Circuit,
        expected_public_inputs: &[Fr],
    ) -> Result<(), VerifyError> {
        if proof.circuit != expected_circuit {
            return Err(VerifyError::CircuitMismatch {
                expected: expected_circuit,
                actual: proof.circuit,
            });
        }
        if proof.vk_hash != Self::vk_hash_for(expected_circuit) {
            return Err(VerifyError::VerifyingKeyMismatch);
        }
        if proof.public_inputs.len() != expected_public_inputs.len() {
            return Err(VerifyError::PublicInputLength {
                expected: expected_public_inputs.len(),
                actual: proof.public_inputs.len(),
            });
        }
        for (i, (a, b)) in proof
            .public_inputs
            .iter()
            .zip(expected_public_inputs.iter())
            .enumerate()
        {
            if a != b {
                return Err(VerifyError::PublicInputMismatch { index: i });
            }
        }
        let expected = self.poseidon.hash_many(expected_public_inputs).to_bytes();
        if proof.proof_bytes != expected {
            return Err(VerifyError::InvalidProof);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_prove_then_verify_round_trip() {
        let v = MockVerifier::new();
        let inputs = vec![Fr::from_u64(1), Fr::from_u64(2), Fr::from_u64(3)];
        let proof = v.prove(Circuit::Test, inputs.clone());
        v.verify(&proof, Circuit::Test, &inputs).unwrap();
    }

    #[test]
    fn mock_rejects_wrong_circuit() {
        let v = MockVerifier::new();
        let inputs = vec![Fr::from_u64(1)];
        let proof = v.prove(Circuit::Spend, inputs.clone());
        let err = v
            .verify(&proof, Circuit::Output, &inputs)
            .expect_err("should fail");
        assert!(matches!(err, VerifyError::CircuitMismatch { .. }));
    }

    #[test]
    fn mock_rejects_tampered_public_input() {
        let v = MockVerifier::new();
        let inputs = vec![Fr::from_u64(1), Fr::from_u64(2)];
        let mut proof = v.prove(Circuit::Spend, inputs.clone());
        proof.public_inputs[1] = Fr::from_u64(3);
        let err = v
            .verify(&proof, Circuit::Spend, &inputs)
            .expect_err("should fail");
        assert!(matches!(err, VerifyError::PublicInputMismatch { .. }));
    }

    #[test]
    fn mock_rejects_tampered_proof_bytes() {
        let v = MockVerifier::new();
        let inputs = vec![Fr::from_u64(42)];
        let mut proof = v.prove(Circuit::Spend, inputs.clone());
        proof.proof_bytes[0] ^= 0xff;
        let err = v
            .verify(&proof, Circuit::Spend, &inputs)
            .expect_err("should fail");
        assert!(matches!(err, VerifyError::InvalidProof));
    }
}
