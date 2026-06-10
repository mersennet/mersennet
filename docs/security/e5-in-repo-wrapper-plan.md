# E5 In-Repo Minimal Change Plan

This note captures the smallest defensible repo change set for bringing the
E5 wrapper flow into this repository.

## The actual incompatibility

The current bridge path is built around nine raw public inputs:

- `prev_state_root`
- `new_state_root`
- `prev_nullifier_root`
- `new_nullifier_root`
- `block_number`
- `block_hash`
- `new_market_state_hash`
- `shielded_event_root`
- `tx_count`

That contract is fixed in:

- `contracts/src/zk/Groth16Verifier.sol`
- `contracts/src/zk/PrimeChainBridge.sol`
- `crates/core/src/bridge_export.rs`

The checked-in SP1 host already supports `PRIME_SP1_PROOF_SYSTEM=groth16`, but
that is **not** a drop-in match for the current bridge ABI.

What the host / SDK produces today:

- `SP1ProofWithPublicValues::bytes()` emits the on-chain SP1 Groth16 proof blob
  (`4-byte vkey prefix || encoded proof`).
- The proof verifies against the SP1 verifier key shipped by `sp1-verifier`.
- The Groth16 circuit exposed by the SDK has **five** public inputs:
  `sp1_vkey_hash`, `committed_values_digest`, `exit_code`, `vk_root`,
  `proof_nonce`.

What the current Prime Chain bridge expects:

- a Groth16 verifier configured for **nine** public inputs, each one matching a
  raw `BlockProgramOutput` field.

Therefore, a thin "export the host's Groth16 proof bytes and VK" patch is not
enough. It would produce a valid SP1 Groth16 artifact for the wrong circuit.

## Recommended minimal path

If we keep the current bridge ABI and continuity checks, the minimum in-repo
change is to add a dedicated wrapper package that proves the current nine-field
contract.

### 1. Add a standalone wrapper package

Add a new sibling package, outside the main workspace, so ordinary
`cargo build --workspace` does not pull in the wrapper toolchain:

- `programs/state-transition-wrap/Cargo.toml`
- `programs/state-transition-wrap/src/main.rs`
- `programs/state-transition-wrap/README.md`

Why a new sibling package:

- it keeps the wrapper toolchain separate from the node workspace,
- it matches the existing `programs/state-transition/` and
  `programs/state-transition-host/` split,
- it avoids reintroducing host-only / prover-only dependencies into
  `crates/core`.

### 2. Make the wrapper package own the bridge-specific circuit

The wrapper package must define the bridge-specific proving flow whose public
interface is exactly the current `PrimeChainBridge.PI_*` ordering.

Inputs to the wrapper flow:

- the canonical `BlockProgramOutput`,
- the proof artifact produced by the SP1 host,
- the pinned SP1 program verifying-key hash.

Required constraints:

- bind the wrapper proof to the pinned SP1 verifier key,
- bind it to the exact `BlockProgramOutput` bytes committed by the SP1 proof,
- re-expose the nine raw `BlockProgramOutput` fields as Groth16 public inputs in
  the current bridge order.

This is the missing piece that the current repo does not contain.

### 3. Add a host-side request renderer for the wrapper package

Add a small example under the existing SP1 host crate:

- `programs/state-transition-host/examples/render_bridge_wrap_request.rs`

Purpose:

- read the checked-in host `prove-response.json`,
- decode the `SP1ProofWithPublicValues` envelope,
- decode `BlockProgramOutput` from `public_values_hex`,
- emit the wrapper package's request JSON.

This keeps the host/request contract stable and avoids baking bridge-specific
logic into `programs/state-transition-host/src/main.rs`.

### 4. Add wrapper artifact export commands

The wrapper package should emit three artifact shapes:

- raw wrapper proof bytes for archival,
- verifier-key bytes plus a contract-friendly JSON rendering,
- the exact proof bytes consumed by `crates/core/src/bridge_export.rs`.

Minimum output files:

- `scripts/zk/sp1-bridge-wrap-proof.json`
- `scripts/zk/sp1-bridge-wrap-vk.json`
- `scripts/zk/sp1-bridge-wrap-proof.bytes`
- `scripts/zk/sp1-bridge-wrap-vk.bytes`

The VK JSON should map directly to `Groth16Verifier.setVerifyingKey(...)`:

- `alpha1.x`, `alpha1.y`
- `beta2.x[0]`, `beta2.x[1]`, `beta2.y[0]`, `beta2.y[1]`
- `gamma2.*`
- `delta2.*`
- `ic[10]`

### 5. Add a core example to render bridge calldata

Add a tiny core example:

- `crates/core/examples/render_bridge_submission.rs`

Purpose:

- load the wrapper proof bytes,
- load or reconstruct the corresponding `BlockProgramOutput`,
- call `build_bridge_submission(...)`,
- print the `submitStateProof(uint256[8], uint256[])` payload.

This uses the already-tested logic in `crates/core/src/bridge_export.rs`
instead of duplicating bridge ABI packing in the wrapper package.

### 6. Update docs only after the wrapper package exists

Once the wrapper package is added, update these files:

- `programs/state-transition-host/README.md`
- `scripts/zk/README.md`
- `contracts/src/zk/README.md`
- `docs/security/privacy-fork-audit-packet.md`
- `docs/STATUS.md`

The updated docs should stop saying "out-of-repo" and instead say that the
wrapper flow is repo-local but still requires one real proof/VK capture.

## What does not need to change on this path

If the wrapper package re-exposes the current nine raw public inputs, these
files do not need semantic changes:

- `contracts/src/zk/Groth16Verifier.sol`
- `contracts/src/zk/PrimeChainBridge.sol`
- `crates/core/src/bridge_export.rs`
- `crates/core/src/precompiles.rs`

That is why this is the minimal path that preserves the existing bridge design.

## Smaller alternative, but with ABI changes

There is a smaller engineering path if we are willing to change the Ethereum
bridge contract:

- switch the verifier path to SP1's native Groth16 artifact shape,
- accept the SP1 proof's five hashed public inputs instead of nine raw block
  fields,
- redesign `PrimeChainBridge.submitStateProof` around that contract.

That path would require semantic changes to the Solidity verifier, the bridge,
the calldata helper, and the tests. It is smaller only if changing the bridge
ABI is acceptable.

For the current E5 definition, the wrapper-package path above is the minimal
repo-local change set.
