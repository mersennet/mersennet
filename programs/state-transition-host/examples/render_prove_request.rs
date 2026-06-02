use anyhow::{Context, Result};
use alloy_primitives::B256;
use prime_zkp::sp1::{
    BlockHeaderWitness, BlockProgramInput, derive_block_hash, hash_market_aggregates,
};
use serde::{Deserialize, Serialize};
use std::{env, fs, path::PathBuf};

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

fn main() -> Result<()> {
    let mut args = env::args_os();
    let _program = args.next();
    let request_path = PathBuf::from(args.next().context("missing prove request path")?);
    let output_path = PathBuf::from(args.next().context("missing output path")?);

    if args.next().is_some() {
        anyhow::bail!("usage: render_prove_request <prove-request.json> <output.json>");
    }

    let mut request: ProveRequest = serde_json::from_slice(&fs::read(&request_path)?)
        .with_context(|| format!("read prove request at {}", request_path.display()))?;

    let mut program_input = if request.block_program_input_hex.trim().is_empty() {
        build_fallback_input(&request)?
    } else {
        let bytes = hex::decode(request.block_program_input_hex.trim())
            .context("invalid block_program_input_hex")?;
        bincode::deserialize(&bytes).context("deserialize block program input")?
    };

    normalize_program_input(&mut program_input);

    request.block_program_input_hex = hex::encode(
        bincode::serialize(&program_input).context("serialize BlockProgramInput")?,
    );

    fs::write(
        &output_path,
        serde_json::to_vec_pretty(&request).context("serialize prove request")?,
    )
    .with_context(|| format!("write prove request to {}", output_path.display()))?;

    Ok(())
}

fn build_fallback_input(request: &ProveRequest) -> Result<BlockProgramInput> {
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

    Ok(BlockProgramInput {
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
        expected_market_state_hash: hash_market_aggregates(&[]),
    })
}

fn normalize_program_input(program_input: &mut BlockProgramInput) {
    program_input.expected_block_hash =
        derive_block_hash(program_input.block_number, &program_input.header);
    program_input.expected_market_state_hash =
        hash_market_aggregates(&program_input.pre_tick_witness.orders.aggregates);
}

fn decode_b256(raw: &str) -> Result<B256> {
    let bytes = hex::decode(raw.trim()).with_context(|| format!("invalid 32-byte hex: {raw}"))?;
    if bytes.len() != 32 {
        anyhow::bail!("expected 32 bytes, got {}", bytes.len());
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