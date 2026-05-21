# SP1 state-transition program

Proves that a Prime Chain block's state transition is valid, end to
end. The proof is consumed by:

- **Light clients** verifying chain state without re-executing.
- **Bridges** receiving Prime Chain state updates on other chains
  (verifier precompile `0x0300`).
- **The chain itself**, optionally, as a "self-checkpointing" mechanism
  for restart-from-snapshot scenarios.

## Inputs (private witness)

Bincode-encoded [`prime_zkp::sp1::BlockProgramInput`]:

| Field | Source |
|---|---|
| `prev_state_root` | previous block's `ShieldedState::current_root()` |
| `prev_nullifier_root` | previous block's nullifier set root |
| `block_number` | header |
| `timestamp` | header |
| `txs` | each shielded tx envelope (`ShieldedTransferTx`, `ShieldedOrderTx`, `ShieldTx`, `UnshieldTx`, `LiquidationClaim`, `LiquidationExecute`) bincode-encoded |
| `prev_market_state` | previous block's market aggregates bincode-encoded |

## Public output

Bincode-encoded [`prime_zkp::sp1::BlockProgramOutput`]:

| Field | Meaning |
|---|---|
| `prev_state_root` | echo of input — binds the proof to a specific anchor |
| `new_state_root` | post-block `ShieldedState::current_root()` |
| `prev_nullifier_root` | echo of input |
| `new_nullifier_root` | post-block nullifier set root |
| `block_number` | echo |
| `block_hash` | post-block header hash |
| `new_market_state_hash` | Keccak256 of the bincode-encoded post-block market aggregates |
| `tx_count` | number of `txs` processed |

## What the program proves

For every tx in `txs`, in order:

1. Decode the tx envelope.
2. Dispatch on type:
   - `ShieldedTransferTx`, `UnshieldTx`: re-verify the `spend` Noir
     proof against the current root + nullifier set; insert
     nullifier; insert any output commitment(s).
   - `ShieldTx`: re-verify the `output` Noir proof; debit the
     transparent balance (from a Merkle-Patricia trie of EOA balances
     also passed as input); insert the output commitment.
   - `ShieldedOrderTx`: re-verify the `order_place` Noir proof;
     re-derive `imm_required` and `side_hash`; bucket-band check;
     insert nullifier; insert new collateral commitment.
   - `LiquidationClaim`: re-verify `liquidate_claim`; record claim_tag.
   - `LiquidationExecute`: re-verify `liquidate_execute`; insert
     victim nullifier; insert bounty + insurance commitments.
3. After all txs processed, re-run FBA matching for each market and
   commit the resulting aggregates.
4. Compute and emit `new_state_root`, `new_nullifier_root`,
   `new_market_state_hash`.

## Today

[`crate::state_proof`] runs the chain side of this pipeline and uses
the [`crate::zk_sp1::SP1Prover`] mock. Tests cover the input/output
boundary and the prove-then-verify round trip.

## Phase 5 cut-over

When the `sp1` feature is enabled on `prime-zkp`:

1. Add `sp1_sdk = "3"` to `crates/zkp/Cargo.toml` under the `sp1` feature.
2. Implement the program in `programs/state-transition/src/main.rs`
   (RISC-V binary that consumes `BlockProgramInput`).
3. Swap `SP1Prover::new(ProverMode::Mock)` in
   `crates/core/src/state_proof.rs` for
   `sp1_sdk::ProverClient::network()` (or `local()` for self-hosting).
4. Compile the program ELF; pin its `vkey_hash` in
   `crates/zkp/params/state-transition.vkey`.
5. The on-chain verifier precompile at `0x0300` consumes the
   resulting Groth16-wrapped proof and the program's public values,
   exactly as the `verifyStateProof(bytes)` selector specifies in
   `crates/core/src/precompile_abi.rs`.

## Why this lives outside the crates/ tree

The SP1 build chain compiles to a riscv32im target, separate from the
node's `x86_64` / `aarch64` build. Keeping the program in a sibling
directory keeps `cargo build --workspace` runnable without a
RISC-V toolchain installed.
