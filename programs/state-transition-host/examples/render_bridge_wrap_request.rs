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
struct BridgeWrapRequest {
    sp1_vkey_hash_hex: String,
    sp1_public_values_hex: String,
    sp1_proof_envelope_hex: String,
    sp1_proof_system: String,
    block_program_output_hex: String,
    block_program_output: BlockProgramOutputJson,
    program_elf_path: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BlockProgramOutputJson {
    prev_state_root_hex: String,
    new_state_root_hex: String,
    prev_nullifier_root_hex: String,
    new_nullifier_root_hex: String,
    block_number: u64,
    block_hash_hex: String,
    new_market_state_hash_hex: String,
    shielded_event_root_hex: String,
    tx_count: u64,
}

fn main() -> Result<()> {
    let mut args = env::args_os();
    let _program = args.next();
    let prove_response_path = PathBuf::from(args.next().context("missing prove response path")?);
    let wrap_request_path = PathBuf::from(args.next().context("missing wrap request path")?);
    let program_elf_path = args
        .next()
        .context("missing program ELF path")?
        .into_string()
        .map_err(|_| anyhow::anyhow!("program ELF path must be valid UTF-8"))?;

    if args.next().is_some() {
        anyhow::bail!(
            "usage: render_bridge_wrap_request <prove-response.json> <wrap-request.json> <program-elf-path>"
        );
    }

    let response: ProveResponse = serde_json::from_slice(&fs::read(&prove_response_path)?)
        .with_context(|| format!("read prove response at {}", prove_response_path.display()))?;
    let public_values =
        hex::decode(response.public_values_hex.trim()).context("invalid public_values_hex")?;
    let output: BlockProgramOutput = bincode::deserialize(&public_values)
        .context("decode BlockProgramOutput from public_values_hex")?;

    let wrap_request = BridgeWrapRequest {
        sp1_vkey_hash_hex: response.vkey_hash_hex,
        sp1_public_values_hex: response.public_values_hex.clone(),
        sp1_proof_envelope_hex: response.proof_bytes_hex,
        sp1_proof_system: response.proof_system,
        block_program_output_hex: hex::encode(&public_values),
        block_program_output: BlockProgramOutputJson {
            prev_state_root_hex: hex::encode(output.prev_state_root),
            new_state_root_hex: hex::encode(output.new_state_root),
            prev_nullifier_root_hex: hex::encode(output.prev_nullifier_root),
            new_nullifier_root_hex: hex::encode(output.new_nullifier_root),
            block_number: output.block_number,
            block_hash_hex: hex::encode(output.block_hash),
            new_market_state_hash_hex: hex::encode(output.new_market_state_hash),
            shielded_event_root_hex: hex::encode(output.shielded_event_root),
            tx_count: output.tx_count,
        },
        program_elf_path,
    };

    fs::write(
        &wrap_request_path,
        serde_json::to_vec_pretty(&wrap_request).context("serialize wrap request")?,
    )
    .with_context(|| format!("write wrap request to {}", wrap_request_path.display()))?;
    Ok(())
}
