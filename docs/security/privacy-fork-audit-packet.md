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
      `00cee367b911744ff17d8fad9e2954e272df4a1e571edbe49634321796161477`.
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
cargo-prove prove build
cargo-prove prove vkey --elf target/elf-compilation/riscv64im-succinct-zkvm-elf/release/prime-chain-state-transition

cd ..
cargo check --manifest-path programs/state-transition-host/Cargo.toml --features real-sp1
```

## Current Blockers

### 1. Release-grade prove/verify transcript is not captured yet

The canonical ELF is built, the real verifying-key hash is captured, and
the real-SP1 host runner compiles in WSL. What is still missing is one
completed local prove/verify transcript against the pinned artifact set.

Current status:

- ELF path:
      `programs/state-transition/target/elf-compilation/riscv64im-succinct-zkvm-elf/release/prime-chain-state-transition`
- Pinned hash:
      `00cee367b911744ff17d8fad9e2954e272df4a1e571edbe49634321796161477`
- Checked-in artifact:
      `crates/zkp/params/sp1/state-transition.vk.hash`
- Transcript: still pending capture

### 2. Full engine-parity zkVM block re-execution is not implemented yet

The checked-in zkVM program no longer echoes host-supplied outputs. It
now consumes canonical `prime_zkp::sp1::BlockProgramInput` and derives
`BlockProgramOutput` via the shared deterministic executor used by the
chain and host runner.

What is still missing is engine parity. The chain-side executor remains
in `crates/core/src/engine.rs`, which is tightly coupled to `std`,
`revm`, Noir-proof verification, and the rest of the full node runtime.

That means the remaining E2 work is now a narrower extraction problem:
replace the simplified shared executor with a zkVM-friendly state
transition core that reproduces the real engine semantics from
`BlockProgramInput`.

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

## H6 Bake Status

The actual 8-week H6 bake window has not started.

Reason:

- no completed prove/verify transcript against the pinned artifact set
- no full engine-parity zkVM block re-execution
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
- [ ] Replace the simplified shared executor with full engine-parity
      zkVM block execution.
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