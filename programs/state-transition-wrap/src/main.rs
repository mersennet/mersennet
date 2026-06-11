use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    ProveBridgeWrap {
        #[arg(long)]
        wrap_request: std::path::PathBuf,
        #[arg(long)]
        work_dir: Option<std::path::PathBuf>,
        #[arg(long)]
        proof_out: Option<std::path::PathBuf>,
        #[arg(long)]
        vk_bytes_out: Option<std::path::PathBuf>,
        #[arg(long)]
        vk_json_out: Option<std::path::PathBuf>,
    },
    RenderOnchainProofFromResponse {
        #[arg(long)]
        prove_response: std::path::PathBuf,
        #[arg(long)]
        bytes_out: std::path::PathBuf,
        #[arg(long)]
        json_out: Option<std::path::PathBuf>,
    },
    RenderSolidityVk {
        #[arg(long)]
        vk_bytes: std::path::PathBuf,
        #[arg(long)]
        json_out: std::path::PathBuf,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    run(cli)
}

#[cfg(windows)]
fn run(_cli: Cli) -> Result<()> {
    anyhow::bail!(
        "programs/state-transition-wrap currently requires Linux or WSL because it depends on the upstream SP1 Groth16 artifact types"
    )
}

#[cfg(not(windows))]
fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Command::ProveBridgeWrap {
            wrap_request,
            work_dir,
            proof_out,
            vk_bytes_out,
            vk_json_out,
        } => prove_bridge_wrap(
            &wrap_request,
            work_dir.as_ref(),
            proof_out.as_ref(),
            vk_bytes_out.as_ref(),
            vk_json_out.as_ref(),
        ),
        Command::RenderOnchainProofFromResponse {
            prove_response,
            bytes_out,
            json_out,
        } => render_onchain_proof_from_response(&prove_response, &bytes_out, json_out.as_ref()),
        Command::RenderSolidityVk { vk_bytes, json_out } => {
            render_solidity_vk(&vk_bytes, &json_out)
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use anyhow::{Context, Result, bail};
    use ark_ff::{BigInteger, PrimeField};
    use serde::{Deserialize, Serialize};
    use sp1_sdk::{SP1Proof, SP1ProofWithPublicValues};
    use sp1_verifier::load_ark_groth16_verifying_key_from_bytes;
    use std::{env, fs, path::{Path, PathBuf}, process::Command};

    #[derive(Debug, Deserialize)]
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

    #[derive(Debug, Deserialize, Serialize)]
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

    #[derive(Debug, Deserialize)]
    struct ProveResponse {
        proof_bytes_hex: String,
        proof_system: String,
        public_values_hex: String,
        vkey_hash_hex: String,
    }

    #[derive(Debug, Serialize)]
    #[serde(rename_all = "camelCase")]
    struct OnchainProofJson {
        proof_system: String,
        vkey_hash_hex: String,
        public_values_hex: String,
        proof_bytes_hex: String,
    }

    #[derive(Debug, Serialize)]
    #[serde(rename_all = "camelCase")]
    struct G1PointJson {
        x_hex: String,
        y_hex: String,
    }

    #[derive(Debug, Serialize)]
    #[serde(rename_all = "camelCase")]
    struct G2PointJson {
        x_hex: [String; 2],
        y_hex: [String; 2],
    }

    #[derive(Debug, Serialize)]
    #[serde(rename_all = "camelCase")]
    struct SolidityVkJson {
        alpha1: G1PointJson,
        beta2: G2PointJson,
        gamma2: G2PointJson,
        delta2: G2PointJson,
        ic: Vec<G1PointJson>,
    }

    #[derive(Debug, Serialize)]
    #[serde(rename_all = "camelCase")]
    struct BridgeWrapBundle {
        sp1_vkey_hash_hex: String,
        sp1_proof_system: String,
        sp1_public_values_hex: String,
        sp1_onchain_proof_hex: String,
        sp1_groth16_public_inputs: [String; 5],
        block_program_output_hex: String,
        block_program_output: BlockProgramOutputJson,
        current_bridge_public_inputs: [String; 9],
        program_elf_path: String,
        blocker: &'static str,
    }

    const BRIDGE_WRAP_BLOCKER: &str = "current repo-local recursive proving surfaces are Honk-oriented, while PrimeChainBridge is hard-wired to a Groth16 verifier with 9 raw BlockProgramOutput public inputs; provide PRIME_BRIDGE_WRAP_PROVE_ADAPTER to run an external Groth16-capable wrapper backend or change the bridge ABI";

    pub(super) fn prove_bridge_wrap(
        wrap_request: &Path,
        work_dir: Option<&PathBuf>,
        proof_out: Option<&PathBuf>,
        vk_bytes_out: Option<&PathBuf>,
        vk_json_out: Option<&PathBuf>,
    ) -> Result<()> {
        let request: BridgeWrapRequest = read_json(wrap_request)?;
        let envelope_bytes = hex::decode(request.sp1_proof_envelope_hex.trim())
            .context("invalid sp1ProofEnvelopeHex")?;
        let proof: SP1ProofWithPublicValues =
            bincode::deserialize(&envelope_bytes).context("deserialize SP1 proof envelope")?;

        let SP1Proof::Groth16(groth16_proof) = &proof.proof else {
            bail!(
                "bridge wrap requires an SP1 Groth16 proof, got {}",
                request.sp1_proof_system
            );
        };

        let bundle_dir = work_dir
            .cloned()
            .unwrap_or_else(|| unique_temp_dir("bridge-wrap-bundle"));
        fs::create_dir_all(&bundle_dir)
            .with_context(|| format!("create {}", bundle_dir.display()))?;

        let onchain_proof_bytes = proof.bytes();
        let bundle = BridgeWrapBundle {
            sp1_vkey_hash_hex: request.sp1_vkey_hash_hex.clone(),
            sp1_proof_system: request.sp1_proof_system.clone(),
            sp1_public_values_hex: request.sp1_public_values_hex.clone(),
            sp1_onchain_proof_hex: format!("0x{}", hex::encode(&onchain_proof_bytes)),
            sp1_groth16_public_inputs: groth16_proof.public_inputs.clone(),
            block_program_output_hex: request.block_program_output_hex.clone(),
            block_program_output: request.block_program_output,
            current_bridge_public_inputs: current_bridge_public_inputs(&request),
            program_elf_path: request.program_elf_path.clone(),
            blocker: BRIDGE_WRAP_BLOCKER,
        };

        let bundle_path = bundle_dir.join("bridge-wrap-bundle.json");
        write_json(&bundle_path, &bundle)?;
        let envelope_path = bundle_dir.join("sp1-proof-envelope.bin");
        write_bytes(&envelope_path, &envelope_bytes)?;
        let onchain_proof_path = bundle_dir.join("sp1-onchain-proof.bytes");
        write_bytes(&onchain_proof_path, &onchain_proof_bytes)?;

        if let Some(adapter) = env::var_os("PRIME_BRIDGE_WRAP_PROVE_ADAPTER") {
            let output = Command::new(adapter)
                .arg("--bundle")
                .arg(&bundle_path)
                .arg("--proof-out")
                .arg(proof_out.cloned().unwrap_or_else(|| bundle_dir.join("bridge-wrap-proof.bytes")))
                .arg("--vk-bytes-out")
                .arg(vk_bytes_out.cloned().unwrap_or_else(|| bundle_dir.join("bridge-wrap-vk.bytes")))
                .arg("--vk-json-out")
                .arg(vk_json_out.cloned().unwrap_or_else(|| bundle_dir.join("bridge-wrap-vk.json")))
                .output()
                .context("run PRIME_BRIDGE_WRAP_PROVE_ADAPTER")?;

            if !output.status.success() {
                bail!(
                    "bridge-wrap adapter failed: {}",
                    String::from_utf8_lossy(&output.stderr).trim()
                );
            }
            return Ok(());
        }

        bail!(
            "{}; staged canonical bundle at {}",
            BRIDGE_WRAP_BLOCKER,
            bundle_path.display()
        )
    }

    pub(super) fn render_onchain_proof_from_response(
        prove_response: &Path,
        bytes_out: &Path,
        json_out: Option<&std::path::PathBuf>,
    ) -> Result<()> {
        let response: ProveResponse = read_json(prove_response)?;
        let envelope_bytes = hex::decode(response.proof_bytes_hex.trim())
            .context("invalid proof_bytes_hex in prove response")?;
        let proof: SP1ProofWithPublicValues =
            bincode::deserialize(&envelope_bytes).context("deserialize SP1 proof envelope")?;
        let onchain_bytes = proof.bytes();
        write_bytes(bytes_out, &onchain_bytes)?;

        if let Some(path) = json_out {
            let payload = OnchainProofJson {
                proof_system: response.proof_system,
                vkey_hash_hex: response.vkey_hash_hex,
                public_values_hex: response.public_values_hex,
                proof_bytes_hex: format!("0x{}", hex::encode(&onchain_bytes)),
            };
            write_json(path, &payload)?;
        }

        Ok(())
    }

    pub(super) fn render_solidity_vk(vk_bytes: &Path, json_out: &Path) -> Result<()> {
        let vk_bytes = fs::read(vk_bytes)
            .with_context(|| format!("read Groth16 VK bytes at {}", vk_bytes.display()))?;
        let vk = load_ark_groth16_verifying_key_from_bytes(&vk_bytes)
            .context("decode Groth16 VK bytes")?;

        if vk.gamma_abc_g1.len() != 10 {
            bail!(
                "expected 10 IC points for PrimeChainBridge, got {}",
                vk.gamma_abc_g1.len()
            );
        }

        let payload = SolidityVkJson {
            alpha1: encode_g1(&vk.alpha_g1),
            beta2: encode_g2(&vk.beta_g2),
            gamma2: encode_g2(&vk.gamma_g2),
            delta2: encode_g2(&vk.delta_g2),
            ic: vk.gamma_abc_g1.iter().map(encode_g1).collect(),
        };
        write_json(json_out, &payload)
    }

    fn encode_g1(point: &ark_bn254::G1Affine) -> G1PointJson {
        G1PointJson {
            x_hex: fq_hex(&point.x),
            y_hex: fq_hex(&point.y),
        }
    }

    fn encode_g2(point: &ark_bn254::G2Affine) -> G2PointJson {
        G2PointJson {
            x_hex: [fq_hex(&point.x.c1), fq_hex(&point.x.c0)],
            y_hex: [fq_hex(&point.y.c1), fq_hex(&point.y.c0)],
        }
    }

    fn fq_hex<F: PrimeField>(value: &F) -> String {
        let bigint = value.into_bigint();
        let bytes = bigint.to_bytes_be();
        let mut word = [0u8; 32];
        let start = word.len().saturating_sub(bytes.len());
        word[start..].copy_from_slice(&bytes);
        format!("0x{}", hex::encode(word))
    }

    fn current_bridge_public_inputs(request: &BridgeWrapRequest) -> [String; 9] {
        [
            format!("0x{}", request.block_program_output.prev_state_root_hex),
            format!("0x{}", request.block_program_output.new_state_root_hex),
            format!("0x{}", request.block_program_output.prev_nullifier_root_hex),
            format!("0x{}", request.block_program_output.new_nullifier_root_hex),
            format!("0x{:064x}", request.block_program_output.block_number),
            format!("0x{}", request.block_program_output.block_hash_hex),
            format!("0x{}", request.block_program_output.new_market_state_hash_hex),
            format!("0x{}", request.block_program_output.shielded_event_root_hex),
            format!("0x{:064x}", request.block_program_output.tx_count),
        ]
    }

    fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T> {
        let bytes = fs::read(path).with_context(|| format!("read {}", path.display()))?;
        serde_json::from_slice(&bytes).with_context(|| format!("parse {}", path.display()))
    }

    fn write_json<T: Serialize>(path: &Path, payload: &T) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("create {}", parent.display()))?;
        }
        let bytes = serde_json::to_vec_pretty(payload).context("serialize JSON")?;
        fs::write(path, bytes).with_context(|| format!("write {}", path.display()))
    }

    fn write_bytes(path: &Path, bytes: &[u8]) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("create {}", parent.display()))?;
        }
        fs::write(path, bytes).with_context(|| format!("write {}", path.display()))
    }

    fn unique_temp_dir(label: &str) -> PathBuf {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        env::temp_dir().join(format!("prime-chain-{label}-{}-{now}", std::process::id()))
    }
}

#[cfg(not(windows))]
use imp::{prove_bridge_wrap, render_onchain_proof_from_response, render_solidity_vk};