# ZK Adapter Scripts

These scripts are the reference implementations for the runtime hooks
added in `prime-zkp` and `prime-chain`.

## Files

- `compile_noir_artifacts.py`: materializes one temporary Nargo package
  per circuit, runs `nargo compile`, copies the resulting `target/`
  directory into `crates/zkp/params/noir/<circuit>/`, and writes the
  same deterministic `vk.hash` format that the Rust verifier expects.
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
- `PRIME_BB_ACIR_PATH`: override ACIR artifact discovery.
- `PRIME_BB_VK_PATH`: point the adapter at an already-generated VK.
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
$env:PRIME_BB_VERIFY_ADAPTER = (Resolve-Path .\scripts\zk\barretenberg_verify_adapter.cmd)
```

If your Barretenberg CLI needs nonstandard flags, set
`PRIME_BB_WRITE_VK_TEMPLATE` and `PRIME_BB_VERIFY_TEMPLATE` to an exact
command line. Example:

```powershell
$env:PRIME_BB_VERIFY_TEMPLATE = 'bb verify --vk {vk} --proof {proof} --public-inputs {public_inputs}'
```

## SP1

The SP1 adapters are thin request/response shims around a real prover
and verifier command. The repository still ships the state-transition
program as a scaffold in `programs/state-transition/`, so you must
first materialize/build that program or point the adapters at an
external SP1 host runner.

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
- `{new_state_root_hex}`
- `{block_height}`
- `{block_hash_hex}`
- `{tx_count}`
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
- On Windows, native `--features real-sp1` still returns a clear runtime
  error because the upstream `sp1-sdk` dependency pulls Unix-only
  `sp1-jit` pieces.
- For Windows/Linux parity, set `PRIME_SP1_HOST_EXECUTOR=wsl` so the
  adapter runs the same host-runner command inside WSL and automatically
  rewrites the request/response/program ELF paths to `/mnt/...` form.

Example Windows parity setup via WSL:

```powershell
$env:PRIME_SP1_HOST_EXECUTOR = 'wsl'
$env:PRIME_SP1_PROVE_TEMPLATE = 'cargo run --manifest-path programs/state-transition-host/Cargo.toml --features real-sp1 -- --prove-request {request} --prove-response {response}'
$env:PRIME_SP1_VERIFY_TEMPLATE = 'cargo run --manifest-path programs/state-transition-host/Cargo.toml --features real-sp1 -- --verify-request {request} --verify-response {response}'
```

The adapters validate the JSON shape that the Rust runtime expects:

- prove response: `vkey_hash_hex`, `public_values_hex`, `proof_bytes_hex`, `proof_system`
- verify response: `verified`