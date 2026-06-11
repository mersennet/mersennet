# Privacy-Fork Audit Packet

Date: 2026-06-03
Branch: `feat/zk-e3-transcript`
Status: pre-audit packet assembled, E3 transcript captured, not yet release-complete

## Scope

This packet is the repo-side handoff for the privacy-fork audit path
described in [SECURITY_AUDIT.md](../../SECURITY_AUDIT.md). It captures
what is already evidenced in-repo, what was attempted in this session,
and the exact blockers that still prevent claiming full E completion or
starting the real H6 bake clock.

## Included Evidence

- Public-output contract alignment landed across the chain proof
  envelope, host runner, zkVM program, and RPC response surface.
- Canonical SP1 ELF built reproducibly via
      `cargo-prove prove build --docker --tag v6.2.2 --workspace-directory <repo>`.
- Real SP1 verifying-key hash was captured from that ELF and pinned as
      `0013c6c783c5266f4b361816fb1d25c186582811b90a11edcd15d69ee286200d`.
- Real local `PRIME_SP1_MODE=local` core prove/verify transcript captured
      (`scripts/zk/sp1-prove-response.json`, `scripts/zk/sp1-verify-response.json`).
- Real-SP1 host runner now passes `cargo check --manifest-path
      programs/state-transition-host/Cargo.toml --features real-sp1` in WSL.
- Host-side real-SP1 path now has explicit mode handling:
  `PRIME_SP1_MODE=local` uses blocking CPU proving and
  `PRIME_SP1_MODE=network` fails with an explicit blocker message
  instead of silently falling back.
- Dedicated SP1 validation lanes exist in CI for:
  - `cargo check -p mersennet-node --features prover,sp1`
  - `cargo check --manifest-path programs/state-transition/Cargo.toml`
  - `cargo test --manifest-path programs/state-transition-host/Cargo.toml`

## Commands Successfully Run

Windows host:

```powershell
cargo check -p mersennet-node --features prover,sp1
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
/home/rodaemonic/.sp1/bin/cargo-prove prove vkey --elf target/elf-compilation/riscv64im-succinct-zkvm-elf/release/mersennet-state-transition

cd ..
PROTOC=/home/rodaemonic/.local/bin/protoc \
PROTOC_INCLUDE=/home/rodaemonic/.local/share/protoc/extracted/include \
cargo check --manifest-path programs/state-transition-host/Cargo.toml --features real-sp1
```

## Current Blockers

### 1. Release-grade prove/verify transcript — CAPTURED (E3 closed)

The release-grade local SP1 prove/verify transcript has now been captured
end to end with `PRIME_SP1_MODE=local` against a reproducible ELF:

- Guest ELF built reproducibly via
  `cargo-prove prove build --docker --tag v6.2.2 --workspace-directory <repo>`
  (SP1 toolchain 6.2.2). Building with `--docker` makes the ELF
  location-independent (compiled at the canonical `/root/program` path
  inside the container), so the verifying key is reproducible by any
  auditor instead of being tied to one developer's absolute build path.
  - ELF sha256: `9debe1cc1267c4f51a1e15885a051a6cd70dcd05b48e0995b6324f43ae22bd89`
  - SP1 verifying-key hash (pinned):
    `0013c6c783c5266f4b361816fb1d25c186582811b90a11edcd15d69ee286200d`
- Real core proof produced: `scripts/zk/sp1-prove-response.json`
  (`proof_system = core`, ~8.8 MB), trace in `scripts/zk/sp1-prove.trace.log`.
- Real cryptographic verify: `scripts/zk/sp1-verify-response.json` →
  `{ "verified": true }` via the real-SP1 ELF verify path.
- The proof's `public_values` matched the pinned deterministic output
  exactly (state/nullifier/event roots and block hash below).

> Note on the prior pin: the earlier hash
> `0047c7a71a6cb605ffddafdf3c32d73dc7b0bb3d707da87293cbfdd02e5ce651`
> came from a non-`--docker` local build whose ELF embedded a
> developer-specific absolute path, so it could not be reproduced on
> another machine. It has been replaced everywhere by the reproducible
> docker vkey above.

> Verify-tool fix: `run_verify` in
> `programs/state-transition-host/src/main.rs` previously forced
> `vkeyHashHex == keccak256(ELF)` via `resolve_vkey_hash` even on the
> real-SP1 ELF path, which made the real verify path unreachable (the
> real path requires `vkeyHashHex == SP1 verifying-key hash`). The keccak
> enforcement is now scoped to non-real-sp1 (mock) builds only.

Capture details:

- ELF path (reproducible docker build, relative to repo root):
      `programs/state-transition/target/elf-compilation/docker/riscv64im-succinct-zkvm-elf/release/mersennet-state-transition`
- Pinned hash:
      `0013c6c783c5266f4b361816fb1d25c186582811b90a11edcd15d69ee286200d`
- Checked-in artifact:
      `crates/zkp/params/sp1/state-transition.vk.hash`
- Prepared prove request:
      `scripts/zk/sp1-prove-request.request.json`
- Prepared prove request is now self-contained:
      `programs/state-transition-host/examples/render_prove_request.rs`
      materializes the exact serialized `BlockProgramInput` into
      `blockProgramInputHex`, so the transcript request no longer relies
      on host-side fallback reconstruction.
- Last stale-request failure now resolved:
      the checked-in request path had preserved a stale `BlockProgramInput`
      commitment with `expected_market_state_hash = 0x00..00`, but the
      current executor derives `hash_market_aggregates(&[])` for the
      empty-tick boundary. `programs/state-transition-host/examples/render_prove_request.rs`
      and `programs/state-transition-host/src/main.rs` now normalize the
      derived `expected_block_hash` and `expected_market_state_hash` fields,
      and `scripts/zk/sp1-prove-request.request.json` was regenerated with
      `expected_market_state_hash =
      c5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470`.
- Latest local real-SP1 runtime blocker:
      WSL OOM-killed `mersennet-state-transition-host` during proving even after the pin mismatch was fixed. The most recent kernel evidence was:
      `Out of memory: Killed process 832 (mersennet-sta) total-vm:21708000kB, anon-rss:15662908kB, ...`
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
      `RAYON_NUM_THREADS=4 PRIME_SP1_MODE=local programs/state-transition-host/target/debug/mersennet-state-transition-host --prove-request scripts/zk/sp1-prove-request.request.json --prove-response scripts/zk/sp1-prove-response.json`
      and later
      `RAYON_NUM_THREADS=2 PRIME_SP1_MODE=local programs/state-transition-host/target/debug/mersennet-state-transition-host --prove-request scripts/zk/sp1-prove-request.request.json --prove-response scripts/zk/sp1-prove-response.json`
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
      `programs/state-transition-host/examples/probe_prover_client.rs` reproduces the same startup cost without any Mersennet input handling. Running it in WSL with `cargo run --release --manifest-path programs/state-transition-host/Cargo.toml --example probe_prover_client --features real-sp1` now prints:
      `[prime-sp1-probe] client:build:start`
      followed by
      `[prime-sp1-probe] client:build:done`.
- Current local conclusion:
      the earlier "hang" was caused by running the real SP1 prover path through debug binaries. The checked-in adapters now normalize `cargo run ... --features real-sp1` commands to `--release` unless an explicit profile is already provided.
- Transcript artifacts:
      `scripts/zk/sp1-prove-response.json` (real core proof) and
      `scripts/zk/sp1-verify-response.json` (`{"verified": true}`) are now
      captured, with the prove stage trace in
      `scripts/zk/sp1-prove.trace.log`. `scripts/zk/sp1-verify-request.request.json`
      carries the matching public values, vkey, and proof system; its
      `proofBytesHex` field points at the proof bytes in
      `scripts/zk/sp1-prove-response.json` (the ~8.8 MB proof is not
      duplicated in the request).
- Expected deterministic public output (block 1, empty, pinned input
      `scripts/zk/sp1-prove-request.request.json`), re-derived offline via
      the canonical `execute_block_program` executor (host mock path) and
      round-trip-verified:
      - `blockHeight` = `1`, `txCount` = `0`
      - `prevStateRootHex` = `0000…0000`
      - `newStateRootHex` =
        `73dc7781dd3c9efc74b22daa99e0477d3315f99d03c850bbce2527ad6c9d2d52`
      - `prevNullifierRootHex` = `0000…0000`
      - `newNullifierRootHex` =
        `011b4d03dd8c01f1049143cf9c4c817e4b167f1d1b83e5c6f0f10d89ba1e7bce`
      - `blockHashHex` =
        `b5e597b42a4b31f3a4fc119f055838edce6facf3a4d5138a8b772adac622c343`
      - `newMarketStateHashHex` =
        `c5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470`
      - `shieldedEventRootHex` =
        `adaed4dd1e55ac43f334db32a4a185b1762d9e3b5448336ed3a1e697511c2b97`
      - `publicValuesHex` (bincoded `BlockProgramOutput`) as pinned in the
        verify request.
      The real local prove output's `public_values` equalled this pinned
      `publicValuesHex` exactly, confirming the ELF/input did not drift.
- Transcript: CAPTURED. The real `PRIME_SP1_MODE=local` core prove
  completed on a 47 GB host (the `core` proof system is far lighter than
  the recursion/wrap paths that previously OOM-killed a 28-32 GB WSL box),
  and the rendered verify request verified cryptographically
  (`{"verified": true}`). E3 is closed.

### 2. The canonical zkVM executor now covers the current proof boundary

The checked-in zkVM program no longer echoes host-supplied outputs. It
consumes canonical `mersennet_zkp::sp1::BlockProgramInput` and derives
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

### 3. Network proving — dependency conflict resolved, delegated transcript still required (E4)

The `c-kzg` link conflict that previously blocked `sp1-sdk/network` has
been resolved by removing `revm` from the SP1 host crate entirely.

Previously the host depended on `mersennet` (`crates/core`), which pulls
the full `revm` execution stack and therefore `c-kzg` 1.x. Enabling
`sp1-sdk/network` additionally pulls the Alloy 1.0 stack (`c-kzg` 2.x),
and because `c-kzg` declares `links = "ckzg"` the two could not coexist:

```text
package `c-kzg` links to the native library `ckzg`, but it conflicts with a previous package which links to `ckzg` as well
```

Fix: the proof-envelope types the host actually needs
(`StateTransitionProof`, `ProofType`, `SP1Proof`, `SP1ProofVerifier`, …)
were extracted into a new `revm`-free crate, `mersennet-state-proof`, that
depends only on `mersennet-zkp` + `alloy-primitives`. `mersennet`
re-exports them so `mersennet::zk_proofs` / `mersennet::zk_sp1` paths
are unchanged. The host now depends on `mersennet-state-proof` + `mersennet-zkp`
+ `alloy-primitives` only — no `revm`, no `c-kzg` 1.x.

Verification (this environment):

- `cargo tree -i revm` / `-i c-kzg` in the host: **no matches** (default
      and `--features real-sp1`).
- `cargo check --features real-sp1`: compiles.
- `cargo check --manifest-path programs/state-transition-host/Cargo.toml --features network`:
      compiles locally on this host. The remaining E4 close-out is one
      delegated proof against the Succinct network with real credentials.

Delegated-proof evidence still required to close E4:

- one successful `PRIME_SP1_MODE=network` prove response captured from
      the network-enabled host
- one rendered network verify request derived from that prove response
- one successful `PRIME_SP1_MODE=network` verify response
- proof/vkey/public-values equality against the pinned E3 Docker ELF
      artifact set
- operator notes identifying the prover-network account / environment
      used for the delegated run

The network path is gated behind a new `network` cargo feature
(`network = ["real-sp1", "sp1-sdk/network"]`). With it enabled,
`PRIME_SP1_MODE=network` drives `ProverClient::builder().network().build()`
on **both** the prove and verify paths; without it, the host still fails
loudly telling the operator to rebuild with `--features network`. A staged
delegated prove request is checked in at
`scripts/zk/sp1-network-prove-request.request.json`, and the exact
credentialed run sequence is in `scripts/zk/README.md` (E4 checklist).

### 4. Ethereum bridge — Groth16 verifier + bridge complete and tested (E5 / G1–G4)

The Ethereum-side bridge is implemented and tested in-repo:

- `contracts/src/zk/Groth16Verifier.sol` — a real BN254 Groth16 verifier
  using the `ecAdd` (0x06), `ecMul` (0x07) and `ecPairing` (0x08)
  precompiles, with a settable + permanently lockable verifying key
  (`PUBLIC_INPUT_COUNT = 9`).
- `contracts/src/zk/PrimeChainBridge.sol` — consumes Groth16-wrapped
  state-transition proofs to advance the canonical shielded/nullifier
  roots (block monotonicity + prev→new root continuity), and runs a
  deposit/withdraw message bus with single-spend, sorted-pair keccak
  Merkle withdrawal authorization.
- `contracts/test/zk/*` — **21 Foundry tests pass** (`forge test
  --match-path 'test/zk/*' -vvv`).
- Chain side: `crates/core/src/precompiles.rs` exposes `verifyStateProof`
  (`0x0300`); `crates/core/src/bridge_export.rs` converts a
  `BlockProgramOutput` + Groth16 proof blob into the bridge's
  `submitStateProof(uint256[8], uint256[])` calldata (public-input order
  matches `PrimeChainBridge.PI_*` and `BlockProgramOutput::to_field_elements`),
  with 4 unit tests (`cargo test -p mersennet --lib bridge_export`).

Still required to fully close E5 (out-of-repo):

- the SP1→Groth16 wrapping circuit that re-exposes the 9
  `BlockProgramOutput` public inputs,
- its trusted-setup verifying key (installed via
  `Groth16Verifier.setVerifyingKey`, then `lockVerifyingKey`),
- one real wrapped proof verified end-to-end through `submitStateProof`.

Important boundary: the checked-in SP1 host already supports
`PRIME_SP1_PROOF_SYSTEM=groth16`, but that proof shape is not a drop-in match
for the current bridge ABI. The SDK Groth16 path exposes the SP1 verifier's
five hashed public inputs (`vkey hash`, committed-values digest, exit code,
VK root, nonce), while `PrimeChainBridge` and `bridge_export` are wired for
nine raw `BlockProgramOutput` fields. The minimal repo-local change set that
preserves the current bridge ABI is captured in
`docs/security/e5-in-repo-wrapper-plan.md`.

## H6 Bake Status

The actual 8-week H6 bake window has not started.

Reason (both remaining preconditions are external/out-of-repo):

- no delegated Succinct network proof transcript captured yet (E4 —
  credential-gated; code + turnkey runbook complete)
- the Groth16 bridge verifier/contracts/chain-export path is complete and
  tested, but the SP1→Groth16 wrapping circuit + verifying key + one real
  wrapped proof are still outstanding (E5 — wrapping-toolchain-gated)

It would be inaccurate to mark the bake window as started before those
preconditions are cleared.

## Release-Pin Checklist

- [x] Build `programs/state-transition` ELF on Linux or WSL.
- [x] Capture the SP1 verifying-key hash from the built ELF and record
      it as the release pin.
- [x] Produce one successful real prove/verify transcript.
- [x] Replace the host-echo zkVM program with canonical
      `BlockProgramInput` re-execution.
- [x] Replace the simplified shared executor with the current
      `BlockProgramInput`-authoritative zkVM block execution path.
- [x] Resolve the `sp1-sdk/network` vs `revm` `c-kzg` conflict before
      enabling `PRIME_SP1_MODE=network` in production.
- [ ] Start the actual H6 bake window only after the items above are complete.

## Code References

- [crates/zkp/src/sp1.rs](../../crates/zkp/src/sp1.rs)
- [crates/core/src/state_proof.rs](../../crates/core/src/state_proof.rs)
- [crates/core/src/zk_sp1.rs](../../crates/core/src/zk_sp1.rs)
- [programs/state-transition/src/main.rs](../../programs/state-transition/src/main.rs)
- [programs/state-transition-host/src/main.rs](../../programs/state-transition-host/src/main.rs)
- [scripts/zk/README.md](../../scripts/zk/README.md)
- [crates/core/src/bridge_export.rs](../../crates/core/src/bridge_export.rs)
- [contracts/src/zk/Groth16Verifier.sol](../../contracts/src/zk/Groth16Verifier.sol)
- [contracts/src/zk/PrimeChainBridge.sol](../../contracts/src/zk/PrimeChainBridge.sol)
