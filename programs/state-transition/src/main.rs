#![no_main]
sp1_zkvm::entrypoint!(main);

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
struct HostProgramInput {
    prev_state_root: [u8; 32],
    new_state_root: [u8; 32],
    block_height: u64,
    block_hash: [u8; 32],
    tx_count: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct HostProgramOutput {
    prev_state_root: [u8; 32],
    new_state_root: [u8; 32],
    block_hash: [u8; 32],
    tx_count: u64,
}

pub fn main() {
    let input = sp1_zkvm::io::read::<HostProgramInput>();

    // This is the minimal real ELF surface for the current host-runner contract.
    // It proves that the zkVM consumed the expected witness shape and committed
    // the public values the host currently verifies.
    let output = HostProgramOutput {
        prev_state_root: input.prev_state_root,
        new_state_root: input.new_state_root,
        block_hash: input.block_hash,
        tx_count: input.tx_count,
    };

    let _ = input.block_height;

    sp1_zkvm::io::commit(&output);
}