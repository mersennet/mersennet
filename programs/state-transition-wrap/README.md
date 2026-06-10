# State-Transition Wrap Utilities

This package is the repo-local home for the E5 bridge-wrap flow.

It is deliberately a standalone sibling package so ordinary
`cargo build --workspace` does not pull in the SP1 / wrapper toolchain.

What is implemented here today:

- `prove-bridge-wrap`: validates a bridge-wrap request, stages the canonical
  SP1/bridge artifact bundle, and dispatches to `PRIME_BRIDGE_WRAP_PROVE_ADAPTER`
  when a Groth16-capable external wrapper prover is available.
- `render-onchain-proof-from-response`: converts the checked-in SP1 host
  `prove-response.json` artifact into the exact on-chain Groth16 proof bytes
  exposed by `SP1ProofWithPublicValues::bytes()`.
- `render-solidity-vk`: converts gnark/SP1 Groth16 verifying-key bytes into a
  contract-friendly JSON shape for `Groth16Verifier.setVerifyingKey(...)`.

These utilities are the repo-local artifact-conversion layer for E5. They do
not by themselves prove the bridge-specific nine-public-input wrapper circuit.

That remaining proving backend still belongs here, but it has to bind the SP1
proof to the nine raw `BlockProgramOutput` fields expected by:

- `contracts/src/zk/Groth16Verifier.sol`
- `contracts/src/zk/PrimeChainBridge.sol`
- `crates/core/src/bridge_export.rs`

## Commands

Use WSL or Linux for the `real-sp1`-backed utilities.

Stage a canonical bridge-wrap bundle and invoke an external wrapper prover if
available:

```powershell
$env:PRIME_BRIDGE_WRAP_PROVE_ADAPTER = '<external-wrapper-prover>'
cargo run --manifest-path programs/state-transition-wrap/Cargo.toml --features real-sp1 -- \
  prove-bridge-wrap \
  --wrap-request scripts/zk/sp1-bridge-wrap-request.json \
  --proof-out scripts/zk/sp1-bridge-wrap-proof.bytes \
  --vk-bytes-out scripts/zk/sp1-bridge-wrap-vk.bytes \
  --vk-json-out scripts/zk/sp1-bridge-wrap-vk.json
```

Without `PRIME_BRIDGE_WRAP_PROVE_ADAPTER`, the command still writes the
canonical bundle into a temp/work directory and fails closed with the current
toolchain blocker: the repo-local recursive surfaces are Honk-oriented, while
the Prime Chain bridge path is fixed to Groth16.

Render on-chain Groth16 proof bytes from the SP1 host response:

```powershell
cargo run --manifest-path programs/state-transition-wrap/Cargo.toml --features real-sp1 -- \
  render-onchain-proof-from-response \
  --prove-response scripts/zk/sp1-prove-response.json \
  --bytes-out scripts/zk/sp1-onchain-proof.bytes \
  --json-out scripts/zk/sp1-onchain-proof.json
```

Render Solidity-friendly VK JSON from gnark/SP1 Groth16 VK bytes:

```powershell
cargo run --manifest-path programs/state-transition-wrap/Cargo.toml --features real-sp1 -- \
  render-solidity-vk \
  --vk-bytes scripts/zk/sp1-bridge-wrap-vk.bytes \
  --json-out scripts/zk/sp1-bridge-wrap-vk.json
```

## Relationship to E5

This package makes the bridge-wrap artifact flow repo-local, but E5 is only
fully closed once the bridge-specific wrapper backend lands and produces:

- one verifying key for the current nine-field bridge ABI,
- one real wrapped proof,
- one successful end-to-end `submitStateProof` acceptance.
