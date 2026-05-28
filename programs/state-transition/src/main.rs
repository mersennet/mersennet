#![no_main]
sp1_zkvm::entrypoint!(main);

use prime_zkp::sp1::{BlockProgramInput, execute_block_program};

pub fn main() {
    let input = sp1_zkvm::io::read::<BlockProgramInput>();
    let output = execute_block_program(&input);

    sp1_zkvm::io::commit(&output);
}