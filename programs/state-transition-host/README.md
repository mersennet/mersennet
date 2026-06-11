# State-Transition Host Runner

This crate is the checked-in host-side runner for the SP1 adapter
contract used by [scripts/zk/README.md](../../scripts/zk/README.md).

It is deliberately separate from the main workspace so ordinary
`cargo build --workspace` does not pull in any extra program-host
dependencies.

## What it does today

It serves as the default target for the SP1 adapters and speaks the
same request/response JSON shape expected by
[crates/core/src/zk_sp1.rs](../../crates/core/src/zk_sp1.rs).

Current behavior is intentionally aligned with the repo's present
runtime semantics:

- `--prove-request <json> --prove-response <json>` emits a deterministic
  SP1-shaped proof using the same mock proof bytes and public-values
  layout that the chain currently uses.
- `--verify-request <json> --verify-response <json>` validates the same
  proof shape and writes `{ "verified": true|false }`.

With the optional `real-sp1` feature enabled on supported targets, the
same CLI switches to a real `sp1_sdk` host flow:

- the provided `program_elf_path` is loaded and passed into the blocking
  `sp1_sdk::ProverClient`
- `proof_bytes_hex` carries a serialized `SP1ProofWithPublicValues`
  envelope so the verify path can reconstruct and SDK-verify the proof
  without changing the adapter JSON contract
- `public_values_hex` continues to expose the raw public values bytes so
  the Rust runtime contract remains unchanged

On Windows, the crate still builds with `--features real-sp1`, but the
real proving and verification paths currently fail at runtime with a
clear error because the upstream `sp1-sdk` toolchain pulls Unix-only
`sp1-jit` components.

If you need Windows/Linux parity today, keep this crate's command line
unchanged and invoke it from the SP1 adapters with
`MERSENNET_SP1_HOST_EXECUTOR=wsl`. That runs the same `cargo run --release ... --features real-sp1`
command inside WSL while preserving the request/response JSON contract.

That means the adapters now have a concrete default command path, even
before the full zkVM program is materialized.

## Commands

```powershell
cargo run --manifest-path programs/state-transition-host/Cargo.toml -- --prove-request request.json --prove-response response.json
cargo run --manifest-path programs/state-transition-host/Cargo.toml -- --verify-request request.json --verify-response response.json
```

Enable the real SDK-backed path on supported targets:

```powershell
cargo run --release --manifest-path programs/state-transition-host/Cargo.toml --features real-sp1 -- --prove-request request.json --prove-response response.json
cargo run --release --manifest-path programs/state-transition-host/Cargo.toml --features real-sp1 -- --verify-request request.json --verify-response response.json
```

For `real-sp1`, prefer optimized binaries. The prover client bootstrap
eagerly builds recursion proving state, so debug binaries can spend
minutes inside `ProverClient::builder().cpu().build()` before the first
prove or verify request even starts.

## Next cut-over

The CLI contract is now stable for both paths. The remaining follow-up is
to point it at the final materialized state-transition zkVM program once
`programs/state-transition/` stops being scaffold-only.

## Bridge-wrap helper

The E5 bridge-wrap flow now has a checked-in request renderer example:

```powershell
cargo run --manifest-path programs/state-transition-host/Cargo.toml --example render_bridge_wrap_request -- \
  scripts/zk/sp1-prove-response.json \
  scripts/zk/sp1-bridge-wrap-request.json \
  <program-elf-path>
```

It reads the host `prove-response.json`, decodes the canonical
`BlockProgramOutput`, and emits the repo-local bridge-wrap request contract
consumed by `programs/state-transition-wrap/`.
