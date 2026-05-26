//! SP1 ZK integration for Prime Chain.
//!
//! Production-ready interface for SP1-compatible proofs. Uses a deterministic
//! simulation in mock mode; can be swapped to real SP1 SDK when deployed.

use crate::zk_proofs::{ProofType, ProofVerificationResult, StateProver, StateTransitionProof};
use anyhow::Result;
use revm::primitives::{B256, keccak256};
use serde::{Deserialize, Serialize};
use std::time::Instant;

#[cfg(feature = "sp1")]
use std::{
    env, fs,
    path::PathBuf,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

// ---------------------------------------------------------------------------
// SP1 Proof Types
// ---------------------------------------------------------------------------

/// SP1-compatible proof envelope.
#[allow(dead_code)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SP1Proof {
    pub vkey_hash: B256,
    pub public_values: Vec<u8>,
    pub proof_bytes: Vec<u8>,
    pub proof_system: String, // "groth16", "plonk", "compressed"
}

/// SP1 program input (what gets proved).
#[allow(dead_code)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SP1ProgramInput {
    pub prev_state_root: B256,
    pub transactions: Vec<Vec<u8>>,
    pub block_number: u64,
    pub timestamp: u64,
}

/// SP1 program output (public values).
#[allow(dead_code)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SP1ProgramOutput {
    pub prev_state_root: B256,
    pub new_state_root: B256,
    pub block_hash: B256,
    pub tx_count: u64,
}

// ---------------------------------------------------------------------------
// Prover Mode
// ---------------------------------------------------------------------------

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub enum ProverMode {
    Mock,    // Fast, for testing (hash-based proofs)
    Local,   // Full local proving (CPU-intensive)
    Network, // Delegated to SP1 prover network
}

// ---------------------------------------------------------------------------
// SP1 Prover
// ---------------------------------------------------------------------------

/// SP1 Prover that wraps the SP1 SDK.
/// In production, this would use sp1_sdk::ProverClient.
/// Currently uses a deterministic simulation that produces realistic proof envelopes.
#[allow(dead_code)]
pub struct SP1Prover {
    pub program_elf: Vec<u8>,
    pub vkey_hash: B256,
    mode: ProverMode,
    #[cfg(feature = "sp1")]
    backend: Option<Sp1CliBackend>,
}

impl SP1Prover {
    pub fn new(mode: ProverMode) -> Self {
        Self {
            program_elf: Vec::new(),
            vkey_hash: keccak256(b"sp1_prime_chain_mock_vkey"),
            mode,
            #[cfg(feature = "sp1")]
            backend: None,
        }
    }

    pub fn runtime_default() -> Self {
        #[cfg(feature = "sp1")]
        {
            if let Ok(backend) = Sp1CliBackend::from_env() {
                return Self::with_backend(backend);
            }
        }

        Self::new(ProverMode::Mock)
    }

    pub fn with_elf(elf: Vec<u8>, mode: ProverMode) -> Self {
        let vkey_hash = if elf.is_empty() {
            keccak256(b"sp1_prime_chain_mock_vkey")
        } else {
            keccak256(&elf)
        };
        Self {
            program_elf: elf,
            vkey_hash,
            mode,
            #[cfg(feature = "sp1")]
            backend: None,
        }
    }

    #[cfg(feature = "sp1")]
    fn with_backend(backend: Sp1CliBackend) -> Self {
        Self {
            program_elf: backend.program_elf.clone(),
            vkey_hash: backend.vkey_hash,
            mode: backend.mode.clone(),
            backend: Some(backend),
        }
    }

    fn compute_mock_proof_bytes(
        prev_state_root: B256,
        new_state_root: B256,
        block_height: u64,
        block_hash: B256,
        tx_count: u64,
    ) -> Vec<u8> {
        let mut buf = Vec::with_capacity(32 * 3 + 8 + 8);
        buf.extend_from_slice(prev_state_root.as_slice());
        buf.extend_from_slice(new_state_root.as_slice());
        buf.extend_from_slice(block_hash.as_slice());
        buf.extend_from_slice(&block_height.to_be_bytes());
        buf.extend_from_slice(&tx_count.to_be_bytes());
        keccak256(&buf).0.to_vec()
    }

    fn prove_mock(
        &self,
        prev_root: B256,
        new_root: B256,
        block_height: u64,
        block_hash: B256,
        tx_count: u64,
    ) -> SP1Proof {
        let proof_bytes =
            Self::compute_mock_proof_bytes(prev_root, new_root, block_height, block_hash, tx_count);

        let output = SP1ProgramOutput {
            prev_state_root: prev_root,
            new_state_root: new_root,
            block_hash,
            tx_count,
        };
        let public_values = bincode::serialize(&output).unwrap_or_default();

        SP1Proof {
            vkey_hash: self.vkey_hash,
            public_values,
            proof_bytes,
            proof_system: "compressed".to_string(),
        }
    }

    /// Prove multiple state transitions (batch).
    pub fn batch_prove(
        &self,
        transitions: &[(B256, B256, u64, B256, u64)],
    ) -> Result<Vec<SP1Proof>> {
        let mut proofs = Vec::with_capacity(transitions.len());
        for (prev, new, height, hash, tx_count) in transitions {
            #[cfg(feature = "sp1")]
            if let Some(backend) = &self.backend {
                proofs.push(backend.prove(*prev, *new, *height, *hash, *tx_count)?);
                continue;
            }

            proofs.push(self.prove_mock(*prev, *new, *height, *hash, *tx_count));
        }
        Ok(proofs)
    }
}

impl StateProver for SP1Prover {
    fn prove_state_transition(
        &self,
        prev_root: B256,
        new_root: B256,
        block_height: u64,
        block_hash: B256,
        tx_count: u64,
    ) -> Result<StateTransitionProof> {
        #[cfg(feature = "sp1")]
        let sp1_proof = if let Some(backend) = &self.backend {
            backend.prove(prev_root, new_root, block_height, block_hash, tx_count)?
        } else {
            self.prove_mock(prev_root, new_root, block_height, block_hash, tx_count)
        };

        #[cfg(not(feature = "sp1"))]
        let sp1_proof = self.prove_mock(prev_root, new_root, block_height, block_hash, tx_count);

        let proof_data = bincode::serialize(&sp1_proof).unwrap_or(sp1_proof.proof_bytes);

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Ok(StateTransitionProof {
            prev_state_root: prev_root,
            new_state_root: new_root,
            block_height,
            block_hash,
            tx_count,
            proof_data,
            proof_type: ProofType::SP1,
            timestamp,
        })
    }

    fn verify_proof(&self, proof: &StateTransitionProof) -> Result<ProofVerificationResult> {
        let start = Instant::now();

        #[cfg(feature = "sp1")]
        let valid = if proof.proof_type != ProofType::SP1 {
            false
        } else if let Some(backend) = &self.backend {
            backend.verify(proof)?
        } else if let Ok(sp1) = bincode::deserialize::<SP1Proof>(&proof.proof_data) {
            SP1ProofVerifier::verify(&sp1, proof)
        } else {
            let expected = SP1Prover::compute_mock_proof_bytes(
                proof.prev_state_root,
                proof.new_state_root,
                proof.block_height,
                proof.block_hash,
                proof.tx_count,
            );
            proof.proof_data == expected
        };

        #[cfg(not(feature = "sp1"))]
        let valid = if proof.proof_type != ProofType::SP1 {
            false
        } else if let Ok(sp1) = bincode::deserialize::<SP1Proof>(&proof.proof_data) {
            SP1ProofVerifier::verify(&sp1, proof)
        } else {
            // Fallback: treat proof_data as raw mock proof bytes
            let expected = SP1Prover::compute_mock_proof_bytes(
                proof.prev_state_root,
                proof.new_state_root,
                proof.block_height,
                proof.block_hash,
                proof.tx_count,
            );
            proof.proof_data == expected
        };

        let verification_time_ms = start.elapsed().as_secs_f64() * 1000.0;

        Ok(ProofVerificationResult {
            valid,
            prev_root: proof.prev_state_root,
            new_root: proof.new_state_root,
            proof_type: proof.proof_type.clone(),
            verification_time_ms,
        })
    }

    fn proof_type(&self) -> ProofType {
        ProofType::SP1
    }
}

// ---------------------------------------------------------------------------
// SP1 Proof Verifier
// ---------------------------------------------------------------------------

/// On-chain verification simulation for SP1 proofs.
#[allow(dead_code)]
pub struct SP1ProofVerifier;

impl SP1ProofVerifier {
    pub fn verify(sp1_proof: &SP1Proof, expected: &StateTransitionProof) -> bool {
        if sp1_proof.proof_bytes.len() != 32 {
            return false;
        }

        let computed = SP1Prover::compute_mock_proof_bytes(
            expected.prev_state_root,
            expected.new_state_root,
            expected.block_height,
            expected.block_hash,
            expected.tx_count,
        );

        sp1_proof.proof_bytes == computed && !sp1_proof.public_values.is_empty()
    }
}

#[cfg(feature = "sp1")]
#[derive(Clone, Debug)]
struct Sp1CliBackend {
    mode: ProverMode,
    program_elf: Vec<u8>,
    vkey_hash: B256,
    prove_adapter: String,
    verify_adapter: String,
}

#[cfg(feature = "sp1")]
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Sp1CliProofResponse {
    vkey_hash_hex: String,
    public_values_hex: String,
    proof_bytes_hex: String,
    proof_system: String,
}

#[cfg(feature = "sp1")]
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Sp1CliVerifyResponse {
    verified: bool,
}

#[cfg(feature = "sp1")]
impl Sp1CliBackend {
    fn from_env() -> Result<Self> {
        let prove_adapter = env::var("PRIME_SP1_PROVE_ADAPTER")?;
        let verify_adapter = env::var("PRIME_SP1_VERIFY_ADAPTER")?;
        let program_elf = match env::var_os("PRIME_SP1_PROGRAM_ELF") {
            Some(path) => fs::read(path)?,
            None => Vec::new(),
        };
        let vkey_hash = match env::var("PRIME_SP1_VKEY_HASH") {
            Ok(value) => parse_b256_hex(&value)?,
            Err(_) if !program_elf.is_empty() => keccak256(&program_elf),
            Err(_) => keccak256(b"sp1_prime_chain_mock_vkey"),
        };
        let mode = match env::var("PRIME_SP1_MODE")
            .unwrap_or_else(|_| "local".to_string())
            .to_ascii_lowercase()
            .as_str()
        {
            "network" => ProverMode::Network,
            _ => ProverMode::Local,
        };
        Ok(Self {
            mode,
            program_elf,
            vkey_hash,
            prove_adapter,
            verify_adapter,
        })
    }

    fn prove(
        &self,
        prev_root: B256,
        new_root: B256,
        block_height: u64,
        block_hash: B256,
        tx_count: u64,
    ) -> Result<SP1Proof> {
        let request_dir = unique_temp_dir("prove");
        fs::create_dir_all(&request_dir)?;
        let request_path = request_dir.join("request.json");
        let response_path = request_dir.join("response.json");
        let payload = serde_json::json!({
            "prevStateRootHex": hex::encode(prev_root.as_slice()),
            "newStateRootHex": hex::encode(new_root.as_slice()),
            "blockHeight": block_height,
            "blockHashHex": hex::encode(block_hash.as_slice()),
            "txCount": tx_count,
            "vkeyHashHex": hex::encode(self.vkey_hash.as_slice()),
            "programElfPath": env::var("PRIME_SP1_PROGRAM_ELF").ok(),
        });
        fs::write(&request_path, serde_json::to_vec_pretty(&payload)?)?;

        let output = Command::new(&self.prove_adapter)
            .arg("--request")
            .arg(&request_path)
            .arg("--response")
            .arg(&response_path)
            .output()?;
        if !output.status.success() {
            anyhow::bail!(String::from_utf8_lossy(&output.stderr).trim().to_string());
        }

        let response: Sp1CliProofResponse = serde_json::from_slice(&fs::read(&response_path)?)?;
        Ok(SP1Proof {
            vkey_hash: parse_b256_hex(&response.vkey_hash_hex)?,
            public_values: hex::decode(response.public_values_hex)?,
            proof_bytes: hex::decode(response.proof_bytes_hex)?,
            proof_system: response.proof_system,
        })
    }

    fn verify(&self, proof: &StateTransitionProof) -> Result<bool> {
        let request_dir = unique_temp_dir("verify");
        fs::create_dir_all(&request_dir)?;
        let request_path = request_dir.join("request.json");
        let response_path = request_dir.join("response.json");

        let sp1_proof: SP1Proof = bincode::deserialize(&proof.proof_data)?;
        let payload = serde_json::json!({
            "prevStateRootHex": hex::encode(proof.prev_state_root.as_slice()),
            "newStateRootHex": hex::encode(proof.new_state_root.as_slice()),
            "blockHeight": proof.block_height,
            "blockHashHex": hex::encode(proof.block_hash.as_slice()),
            "txCount": proof.tx_count,
            "vkeyHashHex": hex::encode(sp1_proof.vkey_hash.as_slice()),
            "publicValuesHex": hex::encode(&sp1_proof.public_values),
            "proofBytesHex": hex::encode(&sp1_proof.proof_bytes),
            "proofSystem": sp1_proof.proof_system,
            "programElfPath": env::var("PRIME_SP1_PROGRAM_ELF").ok(),
        });
        fs::write(&request_path, serde_json::to_vec_pretty(&payload)?)?;

        let output = Command::new(&self.verify_adapter)
            .arg("--request")
            .arg(&request_path)
            .arg("--response")
            .arg(&response_path)
            .output()?;
        if !output.status.success() {
            anyhow::bail!(String::from_utf8_lossy(&output.stderr).trim().to_string());
        }

        let response: Sp1CliVerifyResponse = serde_json::from_slice(&fs::read(&response_path)?)?;
        Ok(response.verified)
    }
}

#[cfg(feature = "sp1")]
fn unique_temp_dir(label: &str) -> PathBuf {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    env::temp_dir().join(format!("prime-chain-sp1-{label}-{}-{now}", std::process::id()))
}

#[cfg(feature = "sp1")]
fn parse_b256_hex(raw: &str) -> Result<B256> {
    let bytes = hex::decode(raw.trim())?;
    if bytes.len() != 32 {
        anyhow::bail!("expected 32-byte hex value, got {} bytes", bytes.len());
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    Ok(B256::from(out))
}

// ---------------------------------------------------------------------------
// SP1 Batch Aggregator
// ---------------------------------------------------------------------------

/// Aggregates multiple SP1 proofs into one.
#[allow(dead_code)]
pub struct SP1BatchAggregator {
    proofs: Vec<SP1Proof>,
    batch_size: usize,
}

impl SP1BatchAggregator {
    pub fn new(batch_size: usize) -> Self {
        Self {
            proofs: Vec::new(),
            batch_size,
        }
    }

    pub fn add(&mut self, proof: SP1Proof) {
        self.proofs.push(proof);
    }

    pub fn should_aggregate(&self) -> bool {
        self.proofs.len() >= self.batch_size
    }

    pub fn aggregate(&mut self) -> Option<SP1Proof> {
        if self.proofs.len() < self.batch_size {
            return None;
        }

        let batch: Vec<SP1Proof> = self.proofs.drain(..self.batch_size).collect();
        let first = batch.first()?;
        let last = batch.last()?;

        let first_output = bincode::deserialize::<SP1ProgramOutput>(&first.public_values).ok();
        let last_output = bincode::deserialize::<SP1ProgramOutput>(&last.public_values).ok();

        let (prev_state_root, new_state_root, block_hash, tx_count) =
            match (&first_output, &last_output) {
                (Some(f), Some(l)) => (
                    f.prev_state_root,
                    l.new_state_root,
                    l.block_hash,
                    batch
                        .iter()
                        .filter_map(|p| {
                            bincode::deserialize::<SP1ProgramOutput>(&p.public_values).ok()
                        })
                        .map(|o| o.tx_count)
                        .sum(),
                ),
                _ => (
                    first.vkey_hash,
                    last.vkey_hash,
                    last.vkey_hash,
                    batch.len() as u64,
                ),
            };

        let mut combined = Vec::new();
        for p in &batch {
            combined.extend_from_slice(&p.proof_bytes);
        }
        let aggregated_proof_bytes = keccak256(&combined).0.to_vec();

        let output = SP1ProgramOutput {
            prev_state_root,
            new_state_root,
            block_hash,
            tx_count,
        };
        let public_values = bincode::serialize(&output).unwrap_or_default();

        Some(SP1Proof {
            vkey_hash: first.vkey_hash,
            public_values,
            proof_bytes: aggregated_proof_bytes,
            proof_system: "compressed".to_string(),
        })
    }

    pub fn pending_count(&self) -> usize {
        self.proofs.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_round_trip_still_verifies() {
        let prover = SP1Prover::runtime_default();
        let proof = prover
            .prove_state_transition(B256::ZERO, B256::from([7u8; 32]), 3, B256::from([9u8; 32]), 2)
            .unwrap();
        let verified = prover.verify_proof(&proof).unwrap();
        assert!(verified.valid);
        assert_eq!(proof.proof_type, ProofType::SP1);
    }
}
