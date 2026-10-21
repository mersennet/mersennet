# ZK Adapter Scripts

These scripts are the reference implementations for the runtime hooks
added in `mersennet-zkp` and `mersennet`.

## Files

- `compile_noir_artifacts.py`: materializes one temporary Nargo package
  per circuit, runs `nargo compile`, copies the resulting `target/`
  directory into `crates/zkp/params/noir/<circuit>/`, and writes the
  same deterministic `vk.hash` format that the Rust verifier expects.
- `barretenberg_prove_adapter.py`: adapter for `MERSENNET_BB_PROVE_ADAPTER`.
- `barretenberg_verify_adapter.py`: adapter for `MERSENNET_BB_VERIFY_ADAPTER`.
- `sp1_prove_adapter.py`: adapter for `MERSENNET_SP1_PROVE_ADAPTER`.
- `sp1_verify_adapter.py`: adapter for `MERSENNET_SP1_VERIFY_ADAPTER`.
- `*.cmd`: Windows wrappers so the Rust runtime can invoke the Python
  adapters directly via `Command::new`.

## Noir / Barretenberg

Compile artifacts:

```powershell
py -3 scripts/zk/compile_noir_artifacts.py
```

Useful environment variables:

- `MERSENNET_NARGO_BIN`: override the `nargo` executable.
- `MERSENNET_NOIR_ARTIFACTS_DIR`: override the compiled artifact directory.
- `MERSENNET_BB_BIN`: override the `bb` executable.
- `MERSENNET_BB_PROVE_ADAPTER`: executable or script that produces a proof file for one compiled circuit.
- `MERSENNET_BB_ACIR_PATH`: override ACIR artifact discovery.
- `MERSENNET_BB_VK_PATH`: point the adapter at an already-generated VK.
- `MERSENNET_BB_PROVE_TEMPLATE`: custom proof generation command.
- `MERSENNET_BB_WRITE_VK_TEMPLATE`: custom VK generation command.
- `MERSENNET_BB_VERIFY_TEMPLATE`: custom proof verification command.

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
$env:MERSENNET_NOIR_ARTIFACTS_DIR = (Resolve-Path .\crates\zkp\params\noir)
$env:MERSENNET_BB_PROVE_ADAPTER = (Resolve-Path .\scripts\zk\barretenberg_prove_adapter.cmd)
$env:MERSENNET_BB_VERIFY_ADAPTER = (Resolve-Path .\scripts\zk\barretenberg_verify_adapter.cmd)
```

If your Barretenberg CLI needs nonstandard flags, set
`MERSENNET_BB_PROVE_TEMPLATE`, `MERSENNET_BB_WRITE_VK_TEMPLATE`, and
`MERSENNET_BB_VERIFY_TEMPLATE` to an exact command line. Example:

```powershell
$env:MERSENNET_BB_PROVE_TEMPLATE = 'nargo prove'
$env:MERSENNET_BB_VERIFY_TEMPLATE = 'bb verify --vk {vk} --proof {proof} --public-inputs {public_inputs}'
```

## SP1

The SP1 adapters are thin request/response shims around a prover and
verifier command. The repository now ships a checked-in minimal
state-transition program plus a host runner that can exercise either
the deterministic mock path or the SDK-backed real-SP1 path on
supported targets.

Useful environment variables:

- `MERSENNET_SP1_PROVE_TEMPLATE`
- `MERSENNET_SP1_VERIFY_TEMPLATE`
- `MERSENNET_SP1_HOST_EXECUTOR` (`native` or `wsl`)
- `MERSENNET_SP1_WSL_DISTRO` (optional WSL distro override when using `wsl`)
- `MERSENNET_SP1_PROGRAM_ELF`
- `MERSENNET_SP1_VKEY_HASH`
- `MERSENNET_SP1_MODE`

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
$env:MERSENNET_SP1_PROVE_ADAPTER = (Resolve-Path .\scripts\zk\sp1_prove_adapter.cmd)
$env:MERSENNET_SP1_VERIFY_ADAPTER = (Resolve-Path .\scripts\zk\sp1_verify_adapter.cmd)
```

By default the adapters now invoke the checked-in host runner at
`programs/state-transition-host/`:

```powershell
$env:MERSENNET_SP1_PROVE_ADAPTER = (Resolve-Path .\scripts\zk\sp1_prove_adapter.cmd)
$env:MERSENNET_SP1_VERIFY_ADAPTER = (Resolve-Path .\scripts\zk\sp1_verify_adapter.cmd)
```

You only need `MERSENNET_SP1_PROVE_TEMPLATE` / `MERSENNET_SP1_VERIFY_TEMPLATE`
if you want to override that default with a different host runner or a
real `sp1_sdk` command.

Example override for the checked-in host runner using the real SDK-backed
path on supported targets:

```powershell
$env:MERSENNET_SP1_PROVE_TEMPLATE = 'cargo run --release --manifest-path programs/state-transition-host/Cargo.toml --features real-sp1 -- --prove-request {request} --prove-response {response}'
$env:MERSENNET_SP1_VERIFY_TEMPLATE = 'cargo run --release --manifest-path programs/state-transition-host/Cargo.toml --features real-sp1 -- --verify-request {request} --verify-response {response}'
```

Notes:

- Without `--features real-sp1`, the checked-in host runner stays on the
  deterministic mock path.
- On Linux, `--features real-sp1` should be run in `--release`; the SP1
  prover client bootstrap is computationally heavy enough that debug
  binaries can look hung for minutes during `ProverClient::builder().cpu().build()`.
- For `MERSENNET_SP1_MODE=local` prove commands, the adapters now default to
  a lower-memory lane unless you explicitly override it: they inject
  `MERSENNET_SP1_PROOF_SYSTEM=core`, disable inline verify and deferred proof
  verification, and use a mixed worker profile tuned for WSL: bootstrap-
  critical workers stay at `2`, while the recursion/deferred/splicing
  prove-phase workers are capped at `1`. This is the current best-known
  tradeoff to avoid the compressed-path OOM on a 28 GB memory ceiling
  without pushing prover bootstrap back into multi-minute startup.
- E3 transcript capture should use `MERSENNET_SP1_MODE=local` today; one
  successful local prove/verify transcript against the pinned ELF and
  `MERSENNET_SP1_VKEY_HASH` is enough to close E3.
- E4: the `sp1-sdk/network` vs `revm` `c-kzg` conflict is resolved — the
  SP1 host no longer depends on `revm` (proof types moved to the
  `mersennet-state-proof` crate). Build the host with `--features network` to
  enable `MERSENNET_SP1_MODE=network`
  (`ProverClient::builder().network().build()`, credentials from
  `NETWORK_PRIVATE_KEY` / `NETWORK_RPC_URL`). Without the `network`
  feature the host still fails loudly in that mode.
- On Windows, native `--features real-sp1` still returns a clear runtime
  error because the upstream `sp1-sdk` dependency pulls Unix-only
  `sp1-jit` pieces.
- For Windows/Linux parity, set `MERSENNET_SP1_HOST_EXECUTOR=wsl` so the
  adapter runs the same host-runner command inside WSL and automatically
  rewrites the request/response/program ELF paths to `/mnt/...` form.
- The adapters now auto-insert `--release` for `cargo run ... --features real-sp1`
  templates unless you already supplied `--release` or an explicit
  `--profile`.

If you want to force a different tradeoff, set any of these yourself and
the adapters will preserve your explicit values:

- `MERSENNET_SP1_PROOF_SYSTEM`
- `MERSENNET_SP1_INLINE_VERIFY`
- `MERSENNET_SP1_DEFERRED_PROOF_VERIFICATION`
- `RAYON_NUM_THREADS`
- `SP1_WORKER_NUM_CORE_WORKERS`
- `SP1_WORKER_NUM_SETUP_WORKERS`
- `SP1_WORKER_NUM_PREPARE_REDUCE_WORKERS`
- `SP1_WORKER_NUM_RECURSION_EXECUTOR_WORKERS`
- `SP1_WORKER_NUM_RECURSION_PROVER_WORKERS`
- `SP1_WORKER_NUM_DEFERRED_WORKERS`
- `SP1_WORKER_NUM_SPLICING_WORKERS`

Example Windows parity setup via WSL:

```powershell
$env:MERSENNET_SP1_HOST_EXECUTOR = 'wsl'
$env:MERSENNET_SP1_MODE = 'local'
$env:MERSENNET_SP1_PROVE_TEMPLATE = 'cargo run --release --manifest-path programs/state-transition-host/Cargo.toml --features real-sp1 -- --prove-request {request} --prove-response {response}'
$env:MERSENNET_SP1_VERIFY_TEMPLATE = 'cargo run --release --manifest-path programs/state-transition-host/Cargo.toml --features real-sp1 -- --verify-request {request} --verify-response {response}'
```

The adapters validate the JSON shape that the Rust runtime expects:

- prove response: `vkey_hash_hex`, `public_values_hex`, `proof_bytes_hex`, `proof_system`
- verify response: `verified`

### E4 delegated network proof checklist

Use this when closing the remaining network-prover cut-over after the
local E3 transcript is already captured.

Prerequisites:

- `cargo check --manifest-path programs/state-transition-host/Cargo.toml --features network` passes.
- `NETWORK_PRIVATE_KEY` and `NETWORK_RPC_URL` are set for the
  delegated prover account.
- The reproducible Docker ELF and pinned vkey hash from the E3 release
  artifact set are available.
- A staged delegated prove request is checked in at
  `scripts/zk/sp1-network-prove-request.request.json` (a copy of the E3
  block-1 input). Refresh it from the exact `BlockProgramInput` you intend
  to prove if you want a different block.

Status: the `network` feature **compiles** (`cargo check … --features
network` is green on this branch) and both prove and verify dispatch
`ProverClient::builder().network().build()`. The single remaining step is
the credentialed execution below.

Recommended sequence:

```bash
cargo check --manifest-path programs/state-transition-host/Cargo.toml --features network

MERSENNET_SP1_MODE=network \
cargo run --release --manifest-path programs/state-transition-host/Cargo.toml --features network -- \
  --prove-request scripts/zk/sp1-network-prove-request.request.json \
  --prove-response scripts/zk/sp1-network-prove-response.json

cargo run --manifest-path programs/state-transition-host/Cargo.toml --example render_verify_request -- \
  scripts/zk/sp1-network-prove-response.json \
  scripts/zk/sp1-network-verify-request.request.json \
  programs/state-transition/target/elf-compilation/docker/riscv64im-succinct-zkvm-elf/release/mersennet-state-transition

MERSENNET_SP1_MODE=network \
cargo run --release --manifest-path programs/state-transition-host/Cargo.toml --features network -- \
  --verify-request scripts/zk/sp1-network-verify-request.request.json \
  --verify-response scripts/zk/sp1-network-verify-response.json
```

Artifacts to retain in the audit packet:

- `scripts/zk/sp1-network-prove-response.json`
- `scripts/zk/sp1-network-verify-request.request.json`
- `scripts/zk/sp1-network-verify-response.json`
- The exact prove request used for the delegated proof
- The ELF path / sha256 / pinned `MERSENNET_SP1_VKEY_HASH`
- The delegated prover account metadata needed to identify which
  network lane produced the proof

Exit criteria:

- The network-enabled host compiles on the candidate release.
- One delegated `MERSENNET_SP1_MODE=network` prove completes successfully.
- The rendered verify request verifies successfully.
- The proof's `public_values` and `vkey_hash_hex` match the pinned E3
  artifact set.

### E5 bridge-wrap scaffold

The repo-local E5 scaffold is split across the checked-in SP1 host and the new
`programs/state-transition-wrap/` utility package.

Render the bridge-wrap request from the SP1 host response:

```bash
cargo run --manifest-path programs/state-transition-host/Cargo.toml --example render_bridge_wrap_request -- \
  scripts/zk/sp1-prove-response.json \
  scripts/zk/sp1-bridge-wrap-request.json \
  <program-elf-path>
```

Then, on Linux or WSL, render the on-chain proof bytes or Solidity-friendly VK
JSON from wrapper/SP1 Groth16 artifacts:

```bash
cargo run --manifest-path programs/state-transition-wrap/Cargo.toml --features real-sp1 -- \
  render-onchain-proof-from-response \
  --prove-response scripts/zk/sp1-prove-response.json \
  --bytes-out scripts/zk/sp1-onchain-proof.bytes \
  --json-out scripts/zk/sp1-onchain-proof.json
```

```bash
cargo run --manifest-path programs/state-transition-wrap/Cargo.toml --features real-sp1 -- \
  render-solidity-vk \
  --vk-bytes scripts/zk/sp1-bridge-wrap-vk.bytes \
  --json-out scripts/zk/sp1-bridge-wrap-vk.json
```

Important: this scaffold brings the artifact-conversion flow into the repo, but
the bridge-specific nine-public-input wrapper backend is still distinct from
SP1's native five-public-input Groth16 mode.

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

The checked-in SP1 release pin is captured from a reproducible Docker ELF
build of `programs/state-transition`
(`cargo-prove prove build --docker --tag v6.2.2 --workspace-directory <repo>`),
so the verifying key is reproducible by any auditor rather than tied to a
single developer's absolute build path.

- Canonical ELF path:
  `programs/state-transition/target/elf-compilation/docker/riscv64im-succinct-zkvm-elf/release/mersennet-state-transition`
- ELF sha256:
  `03370b219d11ed928ef3ba3df471bd8d05029887a179ea8ccbeac09a50440eb7`
- Captured verifying-key hash:
  `008cd0050b67a4fe3800a848c1c22166b36b696b795ec0d4681fc7678dc1a9a4`
- Checked-in pin artifact:
  `crates/zkp/params/sp1/state-transition.vk.hash`
- Captured transcript (6 Oct 2026): `scripts/zk/e3-2026-10-06/` (real
  `compressed` proof, `{"verified": true}`); its README has the build, the
  machine and the memory it needs (~49 GB with single SP1 workers). The
  10 Jun files in this directory (`sp1-prove-response.json`, `core` proof,
  vkey `0013c6c7…`) are superseded: `programs/` changed after they were captured.
- The pin above is the program at `cf992f8`. The shield/unshield fixes of
  7 Oct (amounts bound as one field element, verify before debit) change
  the block program, so the ELF and its key move: re-pin with a new E3 run
  before the real prover is used. Mock proving is unaffected.

Recommended setup:

```powershell
$env:MERSENNET_SP1_VKEY_HASH = (Get-Content .\crates\zkp\params\sp1\state-transition.vk.hash -Raw).Trim()
```

This completes the real vkey capture step for E3. The checked-in SP1 path
now also re-derives `BlockProgramOutput` from canonical
`BlockProgramInput` instead of echoing host-supplied outputs. The wider
SP1 cut-over still has separate milestones: release-grade prove/verify
transcript capture, full engine-parity block execution, and network
prover integration.
