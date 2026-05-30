# Privacy-Fork Audit Packet

Date: 2026-05-28
Branch: `feat/zk-privacy`
Status: pre-audit packet assembled, not yet release-complete

## Scope

This packet is the repo-side handoff for the privacy-fork audit path
described in [SECURITY_AUDIT.md](../../SECURITY_AUDIT.md). It captures
what is already evidenced in-repo, what was attempted in this session,
and the exact blockers that still prevent claiming E completion or
starting the real H6 bake clock.

## Included Evidence

- Public-output contract alignment landed across the chain proof
  envelope, host runner, zkVM program, and RPC response surface.
- Canonical SP1 ELF build succeeded in WSL via `cargo-prove prove build`.
- Real SP1 verifying-key hash was captured from that ELF and pinned as
      `0047c7a71a6cb605ffddafdf3c32d73dc7b0bb3d707da87293cbfdd02e5ce651`.
- Real-SP1 host runner now passes `cargo check --manifest-path
      programs/state-transition-host/Cargo.toml --features real-sp1` in WSL.
- Host-side real-SP1 path now has explicit mode handling:
  `PRIME_SP1_MODE=local` uses blocking CPU proving and
  `PRIME_SP1_MODE=network` fails with an explicit blocker message
  instead of silently falling back.
- Dedicated SP1 validation lanes exist in CI for:
  - `cargo check -p prime-chain-node --features prover,sp1`
  - `cargo check --manifest-path programs/state-transition/Cargo.toml`
  - `cargo test --manifest-path programs/state-transition-host/Cargo.toml`

## Commands Successfully Run

Windows host:

```powershell
cargo check -p prime-chain-node --features prover,sp1
cargo check --manifest-path programs/state-transition/Cargo.toml
cargo test --manifest-path programs/state-transition-host/Cargo.toml
py -3 scripts/zk/sp1_prove_adapter.py --help
py -3 scripts/zk/sp1_verify_adapter.py --help
```

WSL bootstrap:

```bash
wsl.exe --status
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
curl -L https://sp1up.succinct.xyz | bash
```

WSL SP1 artifact capture:

```bash
cd programs/state-transition
/home/rodaemonic/.sp1/bin/cargo-prove prove build
/home/rodaemonic/.sp1/bin/cargo-prove prove vkey --elf target/elf-compilation/riscv64im-succinct-zkvm-elf/release/prime-chain-state-transition

cd ..
PROTOC=/home/rodaemonic/.local/bin/protoc \
PROTOC_INCLUDE=/home/rodaemonic/.local/share/protoc/extracted/include \
cargo check --manifest-path programs/state-transition-host/Cargo.toml --features real-sp1
```

## Current Blockers

### 1. Release-grade prove/verify transcript is not captured yet

The canonical ELF is built, the real verifying-key hash is captured, and
the real-SP1 host runner compiles in WSL. What is still missing is one
completed local prove/verify transcript against the pinned artifact set.
That transcript should be captured with `PRIME_SP1_MODE=local`; it does
not depend on the blocked E4 network-prover path.

Current status:

- ELF path:
      `/mnt/c/Users/rod_o/Documents/projects/personal/prime-chain/programs/state-transition/target/elf-compilation/riscv64im-succinct-zkvm-elf/release/prime-chain-state-transition`
- Pinned hash:
      `0047c7a71a6cb605ffddafdf3c32d73dc7b0bb3d707da87293cbfdd02e5ce651`
- Checked-in artifact:
      `crates/zkp/params/sp1/state-transition.vk.hash`
- Prepared prove request:
      `scripts/zk/sp1-prove-request.request.json`
- Prepared prove request is now self-contained:
      `programs/state-transition-host/examples/render_prove_request.rs`
      materializes the exact serialized `BlockProgramInput` into
      `blockProgramInputHex`, so the transcript request no longer relies
      on host-side fallback reconstruction.
- Last local prove failure:
      stale repo pin `00cee367b911744ff17d8fad9e2954e272df4a1e571edbe49634321796161477` did not match the current ELF verifying key `0047c7a71a6cb605ffddafdf3c32d73dc7b0bb3d707da87293cbfdd02e5ce651`; the pin and request were updated to the current ELF.
- Latest local real-SP1 runtime blocker:
      WSL OOM-killed `prime-chain-state-transition-host` during proving even after the pin mismatch was fixed. The most recent kernel evidence was:
      `Out of memory: Killed process 832 (prime-chain-sta) total-vm:21708000kB, anon-rss:15662908kB, ...`
- Local WSL mitigation now applied on this host:
      created `C:\Users\rod_o\.wslconfig` with:
      `[wsl2]`
      `memory=28GB`
      `swap=16GB`
      `processors=16`
      After `wsl.exe --shutdown`, WSL reported `Mem: 27Gi` and `Swap: 16Gi`.
- Local real-SP1 prove command attempted in WSL:
      `PROTOC=/home/rodaemonic/.local/bin/protoc PROTOC_INCLUDE=/home/rodaemonic/.local/share/protoc/extracted/include PRIME_SP1_MODE=local cargo run --release --manifest-path programs/state-transition-host/Cargo.toml --features real-sp1 -- --prove-request scripts/zk/sp1-prove-request.request.json --prove-response scripts/zk/sp1-prove-response.json`
- Lower-concurrency retries attempted:
      `RAYON_NUM_THREADS=4 PRIME_SP1_MODE=local programs/state-transition-host/target/debug/prime-chain-state-transition-host --prove-request scripts/zk/sp1-prove-request.request.json --prove-response scripts/zk/sp1-prove-response.json`
      and later
      `RAYON_NUM_THREADS=2 PRIME_SP1_MODE=local programs/state-transition-host/target/debug/prime-chain-state-transition-host --prove-request scripts/zk/sp1-prove-request.request.json --prove-response scripts/zk/sp1-prove-response.json`
- Debug-profile reproducer finding:
      the checked-in host path isolated the slow startup to local SP1 SDK client initialization before prover setup. Stage tracing in `programs/state-transition-host/src/main.rs` reached:
      `[prime-sp1-stage] elf:read:start`
      `[prime-sp1-stage] elf:read:done`
      `[prime-sp1-stage] stdin:build:start`
      `[prime-sp1-stage] stdin:write:start`
      `[prime-sp1-stage] stdin:write:done`
      `[prime-sp1-stage] client:build:start`
      and the debug run did not reach `client:build:done` within the earlier probe window.
- Standalone reproducer:
      `programs/state-transition-host/examples/probe_prover_client.rs` reproduces the same startup cost without any Prime Chain input handling. Running it in WSL with `cargo run --release --manifest-path programs/state-transition-host/Cargo.toml --example probe_prover_client --features real-sp1` now prints:
      `[prime-sp1-probe] client:build:start`
      followed by
      `[prime-sp1-probe] client:build:done`.
- Current local conclusion:
      the earlier "hang" was caused by running the real SP1 prover path through debug binaries. The checked-in adapters now normalize `cargo run ... --features real-sp1` commands to `--release` unless an explicit profile is already provided.
- Transcript artifacts:
      `scripts/zk/sp1-prove-response.json` and `scripts/zk/sp1-verify-request.request.json` were not produced.
- Transcript: repo-side setup is ready, and local SP1 client initialization now succeeds on this WSL host when invoked via optimized `--release` binaries. The next validation step is to rerun the full prove/verify transcript in local mode with the release host path.

### 2. The canonical zkVM executor now covers the current proof boundary

The checked-in zkVM program no longer echoes host-supplied outputs. It
consumes canonical `prime_zkp::sp1::BlockProgramInput` and derives
`BlockProgramOutput` via the shared deterministic executor used by the
chain and host runner.

That executor now covers the current proof-authoritative shielded path
for `BlockProgramInput`: shield / transfer / unshield /
liquidation-execute tx replay, shared order admission, shared FBA market
transition, shared liquidation claim validation and settlement, and
canonical `shielded_event_root` derivation.

The focused regression
`crates/zkp/src/sp1.rs::tests::execute_block_program_replays_liquidation_events()`
now covers the last stale liquidation replay seam at the SP1 event-root
boundary.

### 3. Network proving is blocked by the current dependency graph

The requested `ProverClient::network()` cut-over cannot be enabled in
this host crate today because `sp1-sdk/network` pulls a `c-kzg` version
that conflicts with the `revm` dependency graph already present here.

Observed resolver failure:

```text
package `c-kzg` links to the native library `ckzg`, but it conflicts with a previous package which links to `ckzg` as well
```

The host runner now surfaces this as an explicit runtime blocker when
`PRIME_SP1_MODE=network` is requested.

This means the honest near-term path is:

- finish E3 with a local/WSL real-SP1 transcript against the pinned ELF
- keep `PRIME_SP1_MODE=network` as a loud failure mode until the
      dependency conflict is resolved

## H6 Bake Status

The actual 8-week H6 bake window has not started.

Reason:

- no completed prove/verify transcript against the pinned artifact set
- no network prover cut-over

It would be inaccurate to mark the bake window as started before those
preconditions are cleared.

## Release-Pin Checklist

- [x] Build `programs/state-transition` ELF on Linux or WSL.
- [x] Capture the SP1 verifying-key hash from the built ELF and record
      it as the release pin.
- [ ] Produce one successful real prove/verify transcript.
- [x] Replace the host-echo zkVM program with canonical
      `BlockProgramInput` re-execution.
- [x] Replace the simplified shared executor with the current
      `BlockProgramInput`-authoritative zkVM block execution path.
- [ ] Resolve the `sp1-sdk/network` vs `revm` `c-kzg` conflict before
      enabling `PRIME_SP1_MODE=network` in production.
- [ ] Start the actual H6 bake window only after the items above are complete.

## Code References

- [crates/zkp/src/sp1.rs](../../crates/zkp/src/sp1.rs)
- [crates/core/src/state_proof.rs](../../crates/core/src/state_proof.rs)
- [crates/core/src/zk_sp1.rs](../../crates/core/src/zk_sp1.rs)
- [programs/state-transition/src/main.rs](../../programs/state-transition/src/main.rs)
- [programs/state-transition-host/src/main.rs](../../programs/state-transition-host/src/main.rs)
- [scripts/zk/README.md](../../scripts/zk/README.md)