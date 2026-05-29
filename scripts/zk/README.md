# ZK Adapter Scripts

These scripts are the reference implementations for the runtime hooks
added in `prime-zkp` and `prime-chain`.

## Files

- `compile_noir_artifacts.py`: materializes one temporary Nargo package
  per circuit, runs `nargo compile`, copies the resulting `target/`
  directory into `crates/zkp/params/noir/<circuit>/`, and writes the
  same deterministic `vk.hash` format that the Rust verifier expects.
- `barretenberg_prove_adapter.py`: adapter for `PRIME_BB_PROVE_ADAPTER`.
- `barretenberg_verify_adapter.py`: adapter for `PRIME_BB_VERIFY_ADAPTER`.
- `sp1_prove_adapter.py`: adapter for `PRIME_SP1_PROVE_ADAPTER`.
- `sp1_verify_adapter.py`: adapter for `PRIME_SP1_VERIFY_ADAPTER`.
- `*.cmd`: Windows wrappers so the Rust runtime can invoke the Python
  adapters directly via `Command::new`.

## Noir / Barretenberg

Compile artifacts:

```powershell
py -3 scripts/zk/compile_noir_artifacts.py
```

Useful environment variables:

- `PRIME_NARGO_BIN`: override the `nargo` executable.
- `PRIME_NOIR_ARTIFACTS_DIR`: override the compiled artifact directory.
- `PRIME_BB_BIN`: override the `bb` executable.
- `PRIME_BB_PROVE_ADAPTER`: executable or script that produces a proof file for one compiled circuit.
- `PRIME_BB_ACIR_PATH`: override ACIR artifact discovery.
- `PRIME_BB_VK_PATH`: point the adapter at an already-generated VK.
- `PRIME_BB_PROVE_TEMPLATE`: custom proof generation command.
- `PRIME_BB_WRITE_VK_TEMPLATE`: custom VK generation command.
- `PRIME_BB_VERIFY_TEMPLATE`: custom proof verification command.

Template placeholders:

- `{bb}`
- `{circuit}`
- `{artifacts}`
- `{acir}`
- `{vk}`
- `{proof}`
- `{public_inputs}`
- `{public_inputs_json}`

Default runtime setup on Windows:

```powershell
$env:PRIME_NOIR_ARTIFACTS_DIR = (Resolve-Path .\crates\zkp\params\noir)
$env:PRIME_BB_PROVE_ADAPTER = (Resolve-Path .\scripts\zk\barretenberg_prove_adapter.cmd)
$env:PRIME_BB_VERIFY_ADAPTER = (Resolve-Path .\scripts\zk\barretenberg_verify_adapter.cmd)
```

If your Barretenberg CLI needs nonstandard flags, set
`PRIME_BB_PROVE_TEMPLATE`, `PRIME_BB_WRITE_VK_TEMPLATE`, and
`PRIME_BB_VERIFY_TEMPLATE` to an exact command line. Example:

```powershell
$env:PRIME_BB_PROVE_TEMPLATE = 'nargo prove'
$env:PRIME_BB_VERIFY_TEMPLATE = 'bb verify --vk {vk} --proof {proof} --public-inputs {public_inputs}'
```

## SP1

The SP1 adapters are thin request/response shims around a prover and
verifier command. The repository now ships a checked-in minimal
state-transition program plus a host runner that can exercise either
the deterministic mock path or the SDK-backed real-SP1 path on
supported targets.

Useful environment variables:

- `PRIME_SP1_PROVE_TEMPLATE`
- `PRIME_SP1_VERIFY_TEMPLATE`
- `PRIME_SP1_HOST_EXECUTOR` (`native` or `wsl`)
- `PRIME_SP1_WSL_DISTRO` (optional WSL distro override when using `wsl`)
- `PRIME_SP1_PROGRAM_ELF`
- `PRIME_SP1_VKEY_HASH`
- `PRIME_SP1_MODE`

Template placeholders:

- `{request}`
- `{response}`
- `{program_elf}`
- `{prev_state_root_hex}`
- `{prev_nullifier_root_hex}`
- `{block_number}`
- `{timestamp}`
- `{txs_hex}`
- `{prev_market_state_hex}`
- `{vkey_hash_hex}`
- `{public_values_hex}`
- `{proof_bytes_hex}`
- `{proof_system}`
- `{mode}`

Default runtime setup on Windows:

```powershell
$env:PRIME_SP1_PROVE_ADAPTER = (Resolve-Path .\scripts\zk\sp1_prove_adapter.cmd)
$env:PRIME_SP1_VERIFY_ADAPTER = (Resolve-Path .\scripts\zk\sp1_verify_adapter.cmd)
```

By default the adapters now invoke the checked-in host runner at
`programs/state-transition-host/`:

```powershell
$env:PRIME_SP1_PROVE_ADAPTER = (Resolve-Path .\scripts\zk\sp1_prove_adapter.cmd)
$env:PRIME_SP1_VERIFY_ADAPTER = (Resolve-Path .\scripts\zk\sp1_verify_adapter.cmd)
```

You only need `PRIME_SP1_PROVE_TEMPLATE` / `PRIME_SP1_VERIFY_TEMPLATE`
if you want to override that default with a different host runner or a
real `sp1_sdk` command.

Example override for the checked-in host runner using the real SDK-backed
path on supported targets:

```powershell
$env:PRIME_SP1_PROVE_TEMPLATE = 'cargo run --manifest-path programs/state-transition-host/Cargo.toml --features real-sp1 -- --prove-request {request} --prove-response {response}'
$env:PRIME_SP1_VERIFY_TEMPLATE = 'cargo run --manifest-path programs/state-transition-host/Cargo.toml --features real-sp1 -- --verify-request {request} --verify-response {response}'
```

Notes:

- Without `--features real-sp1`, the checked-in host runner stays on the
  deterministic mock path.
- On Linux, `--features real-sp1` can use the SDK-backed host path directly.
- E3 transcript capture should use `PRIME_SP1_MODE=local` today; one
  successful local prove/verify transcript against the pinned ELF and
  `PRIME_SP1_VKEY_HASH` is enough to close E3.
- E4 is separate: `PRIME_SP1_MODE=network` is still blocked by the
  current `sp1-sdk/network` vs `revm` `c-kzg` conflict, and the host
  runner is expected to fail loudly in that mode.
- On Windows, native `--features real-sp1` still returns a clear runtime
  error because the upstream `sp1-sdk` dependency pulls Unix-only
  `sp1-jit` pieces.
- For Windows/Linux parity, set `PRIME_SP1_HOST_EXECUTOR=wsl` so the
  adapter runs the same host-runner command inside WSL and automatically
  rewrites the request/response/program ELF paths to `/mnt/...` form.

Example Windows parity setup via WSL:

```powershell
$env:PRIME_SP1_HOST_EXECUTOR = 'wsl'
$env:PRIME_SP1_MODE = 'local'
$env:PRIME_SP1_PROVE_TEMPLATE = 'cargo run --manifest-path programs/state-transition-host/Cargo.toml --features real-sp1 -- --prove-request {request} --prove-response {response}'
$env:PRIME_SP1_VERIFY_TEMPLATE = 'cargo run --manifest-path programs/state-transition-host/Cargo.toml --features real-sp1 -- --verify-request {request} --verify-response {response}'
```

The adapters validate the JSON shape that the Rust runtime expects:

- prove response: `vkey_hash_hex`, `public_values_hex`, `proof_bytes_hex`, `proof_system`
- verify response: `verified`

### Transcript capture request templates

For the E3 release-transcript step, start from these checked-in request
templates and replace every `REPLACE_*` placeholder before invoking the
host runner:

- `scripts/zk/sp1-prove-request.template.json`
- `scripts/zk/sp1-verify-request.template.json`

Notes:

- All hex fields are raw lowercase hex without a `0x` prefix.
- `blockProgramInputHex` must be the exact bincode serialization fed to
  the prover for the transcript you intend to record.
- The repository now ships a helper to materialize that exact value into
  a request file:

```powershell
cargo run --manifest-path programs/state-transition-host/Cargo.toml --example render_prove_request -- scripts/zk/sp1-prove-request.request.json scripts/zk/sp1-prove-request.request.json
```

  Run that before invoking the real host prover if the request was
  created from placeholder or fallback fields.
- `vkeyHashHex`, `publicValuesHex`, `proofBytesHex`, and `proofSystem`
  in the verify request should be copied from the corresponding prove
  response and wrapped `StateTransitionProof`.

The current SP1 prove request boundary includes canonical
`BlockProgramInput` fields:

- `prevStateRootHex`
- `prevNullifierRootHex`
- `blockNumber`
- `timestamp`
- `txsHex`
- `prevMarketStateHex`

The current SP1 verify/public-values boundary remains the full
`BlockProgramOutput` contract:

- `prevStateRootHex`
- `newStateRootHex`
- `prevNullifierRootHex`
- `newNullifierRootHex`
- `blockHeight`
- `blockHashHex`
- `newMarketStateHashHex`
- `txCount`

## Current vkey pinning boundary

The checked-in SP1 release pin is now captured from a real Linux/WSL ELF
build of `programs/state-transition`.

- Canonical ELF path:
  `programs/state-transition/target/elf-compilation/riscv64im-succinct-zkvm-elf/release/prime-chain-state-transition`
- Captured verifying-key hash:
  `0047c7a71a6cb605ffddafdf3c32d73dc7b0bb3d707da87293cbfdd02e5ce651`
- Checked-in pin artifact:
  `crates/zkp/params/sp1/state-transition.vk.hash`

Recommended setup:

```powershell
$env:PRIME_SP1_VKEY_HASH = (Get-Content .\crates\zkp\params\sp1\state-transition.vk.hash -Raw).Trim()
```

This completes the real vkey capture step for E3. The checked-in SP1 path
now also re-derives `BlockProgramOutput` from canonical
`BlockProgramInput` instead of echoing host-supplied outputs. The wider
SP1 cut-over still has separate milestones: release-grade prove/verify
transcript capture, full engine-parity block execution, and network
prover integration.