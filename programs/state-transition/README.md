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

[`crate::state_proof`] now proves from canonical
`prime_zkp::sp1::BlockProgramInput`, and the checked-in SP1 program in
[src/main.rs](./src/main.rs) re-derives `BlockProgramOutput` from that
input inside the zkVM. The host-echo contract is gone.

The current executor is still a simplified deterministic transition,
shared between the chain, host runner, and zkVM:

1. Hash the transaction list and previous market-state bytes.
2. Derive `new_market_state_hash` from the previous market state,
   block metadata, and transaction commitment.
3. Derive `new_nullifier_root` from the previous nullifier root and
   the transaction commitment.
4. Derive `new_state_root` and `block_hash` from the full witness.

That means the program now consumes the real private-witness shape and
computes its own public values, but it does not yet replay the full
Prime Chain engine (`revm`, Noir-proof verification, FBA matching,
liquidation flows) inside the zkVM.

Reference request/response adapters for the real prover live under
[scripts/zk/README.md](../../scripts/zk/README.md).

## Minimal build target

Once a Linux SP1 toolchain is available, build the ELF from this directory, for
example with the usual SP1 build flow for your environment. The resulting ELF can
then be passed through `PRIME_SP1_PROGRAM_ELF` to the checked-in host runner and
adapter scripts.

## Phase 5 cut-over

The remaining cut-over from this minimal ELF to the full block prover is:

1. Replace the simplified deterministic executor with a zkVM-friendly
  extraction of the real state-transition core documented above.
2. Re-run the full block transition inside the zkVM: tx decoding,
  Noir-proof verification, nullifier/commitment updates, and market
  matching.
3. Swap `SP1Prover::new(ProverMode::Mock)` in
   `crates/core/src/state_proof.rs` for
   `sp1_sdk::ProverClient::network()` (or `local()` for self-hosting).
4. Produce a release-grade prove/verify transcript against the pinned
  ELF and vkey hash.
5. The on-chain verifier precompile at `0x0300` consumes the
   resulting Groth16-wrapped proof and the program's public values,
   exactly as the `verifyStateProof(bytes)` selector specifies in
   `crates/core/src/precompile_abi.rs`.

## Why this lives outside the crates/ tree

The SP1 build chain compiles to a riscv32im target, separate from the
node's `x86_64` / `aarch64` build. Keeping the program in a sibling
directory keeps `cargo build --workspace` runnable without a
RISC-V toolchain installed.
