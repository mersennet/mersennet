use anyhow::{Context, Result};
use mersennet::bridge_export::build_bridge_submission;
use mersennet_zkp::sp1::BlockProgramOutput;
use serde::{Deserialize, Serialize};
use std::{env, fs, path::PathBuf};

#[derive(Debug, Deserialize)]
struct ProveResponse {
    public_values_hex: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SubmissionJson {
    proof: [String; 8],
    public_inputs: [String; 9],
}

fn main() -> Result<()> {
    let mut args = env::args_os();
    let _program = args.next();
    let prove_response_path = PathBuf::from(args.next().context("missing prove response path")?);
    let proof_bytes_path = PathBuf::from(args.next().context("missing proof-bytes path")?);

    if args.next().is_some() {
        anyhow::bail!(
            "usage: render_bridge_submission <prove-response.json> <groth16-proof.bytes>"
        );
    }

    let response: ProveResponse = serde_json::from_slice(&fs::read(&prove_response_path)?)
        .with_context(|| format!("read prove response at {}", prove_response_path.display()))?;
    let public_values =
        hex::decode(response.public_values_hex.trim()).context("invalid public_values_hex")?;
    let output: BlockProgramOutput = bincode::deserialize(&public_values)
        .context("decode BlockProgramOutput from public_values_hex")?;
    let proof_bytes = fs::read(&proof_bytes_path)
        .with_context(|| format!("read {}", proof_bytes_path.display()))?;

    let submission = build_bridge_submission(&output, &proof_bytes)?;
    let hex = submission.to_hex();
    println!(
        "{}",
        serde_json::to_string_pretty(&SubmissionJson {
            proof: hex.proof,
            public_inputs: hex.public_inputs,
        })
        .context("serialize submission JSON")?
    );
    Ok(())
}
