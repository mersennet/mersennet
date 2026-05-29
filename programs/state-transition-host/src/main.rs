use anyhow::{Context, Result, bail};
use clap::Parser;
use prime_chain::zk_proofs::{ProofType, StateTransitionProof};
use prime_chain::zk_sp1::{SP1Proof, SP1ProofVerifier};
use prime_zkp::sp1::{BlockHeaderWitness, BlockProgramInput, BlockProgramOutput, derive_block_hash, execute_block_program};
use revm::primitives::{B256, keccak256};
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf, time::{SystemTime, UNIX_EPOCH}};

#[cfg(all(feature = "real-sp1", not(windows)))]
use std::env;

#[cfg(all(feature = "real-sp1", not(windows)))]
use sp1_sdk::{
    HashableKey,
    ProvingKey,
    blocking::{Elf, ProveRequest as Sp1ProveRequest, Prover, ProverClient, SP1Stdin},
    proof::SP1ProofWithPublicValues,
};

#[derive(Parser, Debug)]
struct Cli {
    #[arg(long, conflicts_with = "verify_request")]
    prove_request: Option<PathBuf>,
    #[arg(long, requires = "prove_request", conflicts_with = "verify_response")]
    prove_response: Option<PathBuf>,
    #[arg(long, conflicts_with = "prove_request")]
    verify_request: Option<PathBuf>,
    #[arg(long, requires = "verify_request", conflicts_with = "prove_response")]
    verify_response: Option<PathBuf>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProveRequest {
    #[serde(default)]
    block_program_input_hex: String,
    prev_state_root_hex: String,
    prev_nullifier_root_hex: String,
    #[serde(alias = "blockHeight")]
    block_number: u64,
    timestamp: u64,
    #[serde(default)]
    txs_hex: Vec<String>,
    #[serde(default)]
    prev_market_state_hex: String,
    vkey_hash_hex: String,
    program_elf_path: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VerifyRequest {
    prev_state_root_hex: String,
    new_state_root_hex: String,
    prev_nullifier_root_hex: String,
    new_nullifier_root_hex: String,
    block_height: u64,
    block_hash_hex: String,
    new_market_state_hash_hex: String,
    shielded_event_root_hex: String,
    tx_count: u64,
    vkey_hash_hex: String,
    public_values_hex: String,
    proof_bytes_hex: String,
    proof_system: String,
    program_elf_path: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ProveResponse {
    vkey_hash_hex: String,
    public_values_hex: String,
    proof_bytes_hex: String,
    proof_system: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct VerifyResponse {
    verified: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match (
        cli.prove_request.as_ref(),
        cli.prove_response.as_ref(),
        cli.verify_request.as_ref(),
        cli.verify_response.as_ref(),
    ) {
        (Some(request), Some(response), None, None) => run_prove(request, response),
        (None, None, Some(request), Some(response)) => run_verify(request, response),
        _ => bail!("provide either --prove-request/--prove-response or --verify-request/--verify-response"),
    }
}

fn run_prove(request_path: &PathBuf, response_path: &PathBuf) -> Result<()> {
    let request: ProveRequest = read_json(request_path)?;
    let proof = build_proof(&request)?;
    let response = ProveResponse {
        vkey_hash_hex: hex::encode(proof.vkey_hash.as_slice()),
        public_values_hex: hex::encode(&proof.public_values),
        proof_bytes_hex: hex::encode(&proof.proof_bytes),
        proof_system: proof.proof_system,
    };
    write_json(response_path, &response)
}

fn run_verify(request_path: &PathBuf, response_path: &PathBuf) -> Result<()> {
    let request: VerifyRequest = read_json(request_path)?;
    let prev_root = decode_b256(&request.prev_state_root_hex)?;
    let new_root = decode_b256(&request.new_state_root_hex)?;
    let prev_nullifier_root = decode_b256(&request.prev_nullifier_root_hex)?;
    let new_nullifier_root = decode_b256(&request.new_nullifier_root_hex)?;
    let block_hash = decode_b256(&request.block_hash_hex)?;
    let new_market_state_hash = decode_b256(&request.new_market_state_hash_hex)?;
    let shielded_event_root = decode_b256(&request.shielded_event_root_hex)?;
    let vkey_hash = resolve_vkey_hash(&request.vkey_hash_hex, request.program_elf_path.as_deref())?;
    let public_values = hex::decode(request.public_values_hex.trim())
        .context("invalid public_values_hex")?;
    let proof_bytes = hex::decode(request.proof_bytes_hex.trim()).context("invalid proof_bytes_hex")?;

    let sp1_proof = SP1Proof {
        vkey_hash,
        public_values: public_values.clone(),
        proof_bytes,
        proof_system: request.proof_system.clone(),
    };
    let state_proof = StateTransitionProof {
        prev_state_root: prev_root,
        new_state_root: new_root,
        prev_nullifier_root,
        new_nullifier_root,
        block_height: request.block_height,
        block_hash,
        new_market_state_hash,
        shielded_event_root,
        tx_count: request.tx_count,
        proof_data: bincode::serialize(&sp1_proof).context("serialize SP1 proof")?,
        proof_type: ProofType::SP1,
        timestamp: unix_timestamp_secs(),
    };

    let expected_output = BlockProgramOutput {
        prev_state_root: prev_root.0,
        new_state_root: new_root.0,
        prev_nullifier_root: prev_nullifier_root.0,
        new_nullifier_root: new_nullifier_root.0,
        block_number: request.block_height,
        block_hash: block_hash.0,
        new_market_state_hash: new_market_state_hash.0,
        shielded_event_root: shielded_event_root.0,
        tx_count: request.tx_count,
    };
    let expected_public_values = bincode::serialize(&expected_output).context("serialize expected public values")?;

    #[cfg(feature = "real-sp1")]
    let verified = if request.program_elf_path.as_deref().filter(|value| !value.is_empty()).is_some() {
        verify_real_sp1(&request, &expected_output)?
    } else {
        sp1_proof.vkey_hash == vkey_hash
            && sp1_proof.public_values == expected_public_values
            && SP1ProofVerifier::verify(&sp1_proof, &state_proof)
    };

    #[cfg(not(feature = "real-sp1"))]
    let verified = sp1_proof.vkey_hash == vkey_hash
        && sp1_proof.public_values == expected_public_values
        && SP1ProofVerifier::verify(&sp1_proof, &state_proof);

    write_json(response_path, &VerifyResponse { verified })
}

fn build_proof(
    request: &ProveRequest,
) -> Result<SP1Proof> {
    let program_input = if request.block_program_input_hex.trim().is_empty() {
        let prev_root = decode_b256(&request.prev_state_root_hex)?;
        let prev_nullifier_root = decode_b256(&request.prev_nullifier_root_hex)?;
        let prev_market_state = decode_bytes(&request.prev_market_state_hex)?;
        let txs = request
            .txs_hex
            .iter()
            .map(|tx| decode_bytes(tx))
            .collect::<Result<Vec<_>>>()?;
        let header = BlockHeaderWitness {
            tx_count: txs.len() as u64,
            ..Default::default()
        };
        BlockProgramInput {
            prev_state_root: prev_root.0,
            prev_nullifier_root: prev_nullifier_root.0,
            block_number: request.block_number,
            timestamp: request.timestamp,
            header: header.clone(),
            txs,
            prev_market_state,
            prev_shielded_state: Default::default(),
            transparent_balances: Vec::new(),
            pre_tick_witness: Default::default(),
            expected_block_hash: derive_block_hash(request.block_number, &header),
            expected_market_state_hash: [0u8; 32],
        }
    } else {
        let bytes = hex::decode(request.block_program_input_hex.trim())
            .context("invalid block_program_input_hex")?;
        bincode::deserialize(&bytes).context("deserialize block program input")?
    };
    let output = execute_block_program(&program_input)?;

    #[cfg(all(feature = "real-sp1", not(windows)))]
    if let Some(program_elf_path) = request.program_elf_path.as_deref().filter(|value| !value.is_empty()) {
        return build_real_sp1_proof(&program_input, &output, &request.vkey_hash_hex, program_elf_path);
    }

    #[cfg(all(feature = "real-sp1", windows))]
    if request.program_elf_path.as_deref().filter(|value| !value.is_empty()).is_some() {
        return build_real_sp1_proof(
            &program_input,
            &output,
            &request.vkey_hash_hex,
            request.program_elf_path.as_deref().unwrap(),
        );
    }

    let vkey_hash = resolve_vkey_hash(&request.vkey_hash_hex, request.program_elf_path.as_deref())?;
    let proof_bytes = compute_mock_proof_bytes(&output);
    let public_values = bincode::serialize(&output)
    .context("serialize SP1 public values")?;
    Ok(SP1Proof {
        vkey_hash,
        public_values,
        proof_bytes,
        proof_system: "compressed".to_string(),
    })
}

fn compute_mock_proof_bytes(output: &BlockProgramOutput) -> Vec<u8> {
    let mut buf = Vec::with_capacity(32 * 6 + 8 * 2);
    buf.extend_from_slice(&output.prev_state_root);
    buf.extend_from_slice(&output.new_state_root);
    buf.extend_from_slice(&output.prev_nullifier_root);
    buf.extend_from_slice(&output.new_nullifier_root);
    buf.extend_from_slice(&output.block_hash);
    buf.extend_from_slice(&output.block_number.to_be_bytes());
    buf.extend_from_slice(&output.tx_count.to_be_bytes());
    buf.extend_from_slice(&output.new_market_state_hash);
    buf.extend_from_slice(&output.shielded_event_root);
    keccak256(&buf).0.to_vec()
}

fn resolve_vkey_hash(vkey_hash_hex: &str, program_elf_path: Option<&str>) -> Result<B256> {
    if let Some(path) = program_elf_path.filter(|value| !value.is_empty()) {
        let elf = fs::read(path).with_context(|| format!("read program ELF at {path}"))?;
        let elf_hash = keccak256(&elf);
        let requested = decode_b256(vkey_hash_hex)?;
        if requested != elf_hash {
            bail!(
                "request vkey hash {} does not match program ELF hash {}",
                hex::encode(requested.as_slice()),
                hex::encode(elf_hash.as_slice())
            );
        }
        Ok(elf_hash)
    } else {
        decode_b256(vkey_hash_hex)
    }
}

#[cfg(all(feature = "real-sp1", not(windows)))]
fn configured_sp1_mode() -> String {
    env::var("PRIME_SP1_MODE")
        .unwrap_or_else(|_| "env".to_string())
        .trim()
        .to_ascii_lowercase()
}

#[cfg(all(feature = "real-sp1", not(windows)))]
fn build_real_sp1_proof_with<P>(
    prover: &P,
    elf_bytes: Vec<u8>,
    stdin: SP1Stdin,
    expected_output: &BlockProgramOutput,
    vkey_hash_hex: &str,
) -> Result<SP1Proof>
where
    P: Prover,
    P::Error: std::fmt::Display,
{
    let proving_key = prover
        .setup(Elf::from(elf_bytes))
        .map_err(|err| anyhow::anyhow!("setup SP1 prover: {err}"))?;
    let verifying_key = proving_key.verifying_key();
    let vkey_hash = B256::from(verifying_key.bytes32_raw());
    let requested_vkey_hash = decode_b256(vkey_hash_hex)?;
    if requested_vkey_hash != vkey_hash {
        bail!(
            "request vkey hash {} does not match SP1 verifying key hash {}",
            hex::encode(requested_vkey_hash.as_slice()),
            hex::encode(vkey_hash.as_slice())
        );
    }

    let proof = prover
        .prove(&proving_key, stdin)
        .compressed()
        .run()
        .map_err(|err| anyhow::anyhow!("generate SP1 proof: {err}"))?;

    let mut decoded_output = proof.public_values.clone();
    let actual_output: BlockProgramOutput = decoded_output.read();
    if actual_output.prev_state_root != expected_output.prev_state_root
        || actual_output.new_state_root != expected_output.new_state_root
        || actual_output.prev_nullifier_root != expected_output.prev_nullifier_root
        || actual_output.new_nullifier_root != expected_output.new_nullifier_root
        || actual_output.block_number != expected_output.block_number
        || actual_output.block_hash != expected_output.block_hash
        || actual_output.new_market_state_hash != expected_output.new_market_state_hash
        || actual_output.tx_count != expected_output.tx_count
    {
        bail!("SP1 public values do not match canonical BlockProgramInput execution");
    }

    prover
        .verify(&proof, verifying_key, None)
        .context("verify generated SP1 proof")?;

    let proof_bytes = bincode::serialize(&proof).context("serialize SP1 proof envelope")?;
    Ok(SP1Proof {
        vkey_hash,
        public_values: proof.public_values.to_vec(),
        proof_bytes,
        proof_system: "compressed".to_string(),
    })
}

#[cfg(all(feature = "real-sp1", not(windows)))]
fn verify_real_sp1_with<P>(
    prover: &P,
    elf_bytes: Vec<u8>,
    request: &VerifyRequest,
    expected_output: &BlockProgramOutput,
) -> Result<bool>
where
    P: Prover,
    P::Error: std::fmt::Display,
{
    let proving_key = prover
        .setup(Elf::from(elf_bytes))
        .map_err(|err| anyhow::anyhow!("setup SP1 prover: {err}"))?;
    let verifying_key = proving_key.verifying_key();
    let requested_vkey_hash = decode_b256(&request.vkey_hash_hex)?;
    let actual_vkey_hash = B256::from(verifying_key.bytes32_raw());
    if requested_vkey_hash != actual_vkey_hash {
        return Ok(false);
    }

    let proof_bytes = hex::decode(request.proof_bytes_hex.trim()).context("invalid proof_bytes_hex")?;
    let proof: SP1ProofWithPublicValues =
        bincode::deserialize(&proof_bytes).context("deserialize SP1 proof envelope")?;
    let public_values = hex::decode(request.public_values_hex.trim())
        .context("invalid public_values_hex")?;
    if proof.public_values.as_slice() != public_values.as_slice() {
        return Ok(false);
    }

    let mut decoded_output = proof.public_values.clone();
    let actual_output: BlockProgramOutput = decoded_output.read();
    if actual_output.prev_state_root != expected_output.prev_state_root
        || actual_output.new_state_root != expected_output.new_state_root
        || actual_output.prev_nullifier_root != expected_output.prev_nullifier_root
        || actual_output.new_nullifier_root != expected_output.new_nullifier_root
        || actual_output.block_number != expected_output.block_number
        || actual_output.block_hash != expected_output.block_hash
        || actual_output.new_market_state_hash != expected_output.new_market_state_hash
        || actual_output.tx_count != expected_output.tx_count
    {
        return Ok(false);
    }

    Ok(prover.verify(&proof, verifying_key, None).is_ok())
}

#[cfg(all(feature = "real-sp1", not(windows)))]
fn build_real_sp1_proof(
    input: &BlockProgramInput,
    expected_output: &BlockProgramOutput,
    vkey_hash_hex: &str,
    program_elf_path: &str,
) -> Result<SP1Proof> {
    let elf_bytes = fs::read(program_elf_path)
        .with_context(|| format!("read program ELF at {program_elf_path}"))?;
    let mut stdin = SP1Stdin::new();
    stdin.write(input);

    match configured_sp1_mode().as_str() {
        "network" => bail!(
            "PRIME_SP1_MODE=network is currently blocked in this host crate because sp1-sdk/network conflicts with the revm c-kzg dependency graph"
        ),
        "local" => {
            let prover = ProverClient::builder().cpu().build();
            build_real_sp1_proof_with(&prover, elf_bytes, stdin, expected_output, vkey_hash_hex)
        }
        _ => {
            let prover = ProverClient::from_env();
            build_real_sp1_proof_with(&prover, elf_bytes, stdin, expected_output, vkey_hash_hex)
        }
    }
}

#[cfg(all(feature = "real-sp1", windows))]
fn build_real_sp1_proof(
    _input: &BlockProgramInput,
    _expected_output: &BlockProgramOutput,
    _vkey_hash_hex: &str,
    _program_elf_path: &str,
) -> Result<SP1Proof> {
    bail!("real-sp1 host proving is not supported on Windows because the current sp1-sdk toolchain pulls Unix-only sp1-jit components")
}

#[cfg(all(feature = "real-sp1", not(windows)))]
fn verify_real_sp1(request: &VerifyRequest, expected_output: &BlockProgramOutput) -> Result<bool> {
    let Some(program_elf_path) = request.program_elf_path.as_deref().filter(|value| !value.is_empty()) else {
        return Ok(false);
    };

    let elf_bytes = fs::read(program_elf_path)
        .with_context(|| format!("read program ELF at {program_elf_path}"))?;
    match configured_sp1_mode().as_str() {
        "network" => bail!(
            "PRIME_SP1_MODE=network is currently blocked in this host crate because sp1-sdk/network conflicts with the revm c-kzg dependency graph"
        ),
        "local" => {
            let prover = ProverClient::builder().cpu().build();
            verify_real_sp1_with(&prover, elf_bytes, request, expected_output)
        }
        _ => {
            let prover = ProverClient::from_env();
            verify_real_sp1_with(&prover, elf_bytes, request, expected_output)
        }
    }
}

#[cfg(all(feature = "real-sp1", windows))]
fn verify_real_sp1(_request: &VerifyRequest, _expected_output: &BlockProgramOutput) -> Result<bool> {
    bail!("real-sp1 host verification is not supported on Windows because the current sp1-sdk toolchain pulls Unix-only sp1-jit components")
}

fn decode_b256(raw: &str) -> Result<B256> {
    let bytes = hex::decode(raw.trim()).with_context(|| format!("invalid 32-byte hex: {raw}"))?;
    if bytes.len() != 32 {
        bail!("expected 32 bytes, got {}", bytes.len());
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    Ok(B256::from(out))
}

fn decode_bytes(raw: &str) -> Result<Vec<u8>> {
    if raw.trim().is_empty() {
        return Ok(Vec::new());
    }
    hex::decode(raw.trim()).with_context(|| format!("invalid hex bytes: {raw}"))
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &PathBuf) -> Result<T> {
    let bytes = fs::read(path).with_context(|| format!("read {}", path.display()))?;
    serde_json::from_slice(&bytes).with_context(|| format!("parse {}", path.display()))
}

fn write_json<T: Serialize>(path: &PathBuf, payload: &T) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    let bytes = serde_json::to_vec_pretty(payload).context("serialize JSON")?;
    fs::write(path, bytes).with_context(|| format!("write {}", path.display()))
}

fn unix_timestamp_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prove_and_verify_round_trip() {
        let proof = build_proof(
            &hex::encode([1u8; 32]),
            &hex::encode([2u8; 32]),
            &hex::encode([3u8; 32]),
            &hex::encode([4u8; 32]),
            12,
            &hex::encode([5u8; 32]),
            &hex::encode([6u8; 32]),
            4,
            &hex::encode([9u8; 32]),
            None,
        )
        .unwrap();

        let request = VerifyRequest {
            prev_state_root_hex: hex::encode([1u8; 32]),
            new_state_root_hex: hex::encode([2u8; 32]),
            prev_nullifier_root_hex: hex::encode([3u8; 32]),
            new_nullifier_root_hex: hex::encode([4u8; 32]),
            block_height: 12,
            block_hash_hex: hex::encode([5u8; 32]),
            new_market_state_hash_hex: hex::encode([6u8; 32]),
            tx_count: 4,
            vkey_hash_hex: hex::encode([9u8; 32]),
            public_values_hex: hex::encode(&proof.public_values),
            proof_bytes_hex: hex::encode(&proof.proof_bytes),
            proof_system: proof.proof_system.clone(),
            program_elf_path: None,
        };

        let tmp = std::env::temp_dir().join("prime-chain-state-transition-host-test.json");
        let out = std::env::temp_dir().join("prime-chain-state-transition-host-test-out.json");
        write_json(&tmp, &request).unwrap();
        run_verify(&tmp, &out).unwrap();
        let response: VerifyResponse = read_json(&out).unwrap();
        assert!(response.verified);
        let _ = fs::remove_file(tmp);
        let _ = fs::remove_file(out);
    }
}