use anyhow::{Context, Result};
use mersennet_zkp::sp1::BlockProgramOutput;
use serde::{Deserialize, Serialize};
use std::{env, fs, path::PathBuf};

#[derive(Debug, Deserialize)]
struct ProveResponse {
    vkey_hash_hex: String,
    public_values_hex: String,
    proof_bytes_hex: String,
    proof_system: String,
}

#[derive(Debug, Serialize)]
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
    program_elf_path: String,
}

fn main() -> Result<()> {
    let mut args = env::args_os();
    let _program = args.next();
    let prove_response_path = PathBuf::from(args.next().context("missing prove response path")?);
    let verify_request_path = PathBuf::from(args.next().context("missing verify request path")?);
    let program_elf_path = args
        .next()
        .context("missing program ELF path")?
        .into_string()
        .map_err(|_| anyhow::anyhow!("program ELF path must be valid UTF-8"))?;

    if args.next().is_some() {
        anyhow::bail!("usage: render_verify_request <prove-response.json> <verify-request.json> <program-elf-path>");
    }

    let response: ProveResponse = serde_json::from_slice(&fs::read(&prove_response_path)?)
        .with_context(|| format!("read prove response at {}", prove_response_path.display()))?;
    let public_values = hex::decode(response.public_values_hex.trim())
        .context("invalid public_values_hex")?;
    let output: BlockProgramOutput =
        bincode::deserialize(&public_values).context("decode BlockProgramOutput from public_values_hex")?;

    let verify_request = VerifyRequest {
        prev_state_root_hex: hex::encode(output.prev_state_root),
        new_state_root_hex: hex::encode(output.new_state_root),
        prev_nullifier_root_hex: hex::encode(output.prev_nullifier_root),
        new_nullifier_root_hex: hex::encode(output.new_nullifier_root),
        block_height: output.block_number,
        block_hash_hex: hex::encode(output.block_hash),
        new_market_state_hash_hex: hex::encode(output.new_market_state_hash),
        shielded_event_root_hex: hex::encode(output.shielded_event_root),
        tx_count: output.tx_count,
        vkey_hash_hex: response.vkey_hash_hex,
        public_values_hex: response.public_values_hex,
        proof_bytes_hex: response.proof_bytes_hex,
        proof_system: response.proof_system,
        program_elf_path,
    };

    fs::write(
        &verify_request_path,
        serde_json::to_vec_pretty(&verify_request).context("serialize verify request")?,
    )
    .with_context(|| format!("write verify request to {}", verify_request_path.display()))?;
    Ok(())
}