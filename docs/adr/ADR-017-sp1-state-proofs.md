# ADR-017: SP1 State-Transition Proofs

**Status:** Accepted (scaffold landed; real `sp1_sdk` pending Phase 5)
**Date:** 2026-05-21

## Context

Light clients, bridges, and external auditors need to verify the
chain's state transitions without re-executing every block. The
existing `crates/core/src/zk_sp1.rs` module already exposes the
SP1-shaped interface but is backed by a deterministic mock.

We are explicit about a distinction the prior ADRs are not: **ZK is
used in two distinct ways here, and we do not conflate them.**

| Use | Tool | What it proves |
|---|---|---|
| Privacy | Noir → UltraPlonk → BN254 | Per-tx solvency + nullifier well-formedness |
| Verification | SP1 zkVM → STARK + optional Groth16 wrap | Whole-block re-execution validity |

Privacy proofs do not verify the chain. Verification proofs do not
provide privacy.

## Decision

Use SP1 for the state-transition program because it is the only
proving system today that can prove the whole VM (revm + the CLOB
matching + the Noir-proof verification loop) inside a zkVM. Real
implementation behind the `sp1` feature on `mersennet-zkp`.

The SP1 program input/output envelope is `mersennet_zkp::sp1::BlockProgramInput`
and `BlockProgramOutput`. The chain-side pipeline is in
`crates/core/src/state_proof.rs`.

The on-chain verifier lives at precompile address `0x0300`
(`STATE_PROOF_VERIFIER_PRECOMPILE` in `precompile_abi.rs`). The
selector is `verifyStateProof(bytes)`. Bridges to Ethereum / other
EVM chains can reuse the same precompile.

## Consequences

**Positive.**
- Light clients verify in ~10 ms on a phone.
- Bridges become trustless; no validator subset signs off — the
  proof is self-contained.
- The chain can "self-checkpoint" via the same proofs for
  restart-from-snapshot scenarios.

**Negative.**
- Real SP1 proving takes minutes on a single machine, seconds on
  the SP1 network. Acceptable because proofs are produced post-block
  asynchronously; block production is not blocked.
- Adds a RISC-V toolchain to the developer setup. Mitigated by
  keeping `programs/state-transition/` out of the main workspace.

## Phase 5 cut-over

See `programs/state-transition/README.md`. Six concrete steps; the
biggest one is implementing the program itself (re-runs the entire
block in the zkVM).
