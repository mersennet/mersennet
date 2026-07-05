# Real SP1 proving (production)

By default the node emits a **deterministic development proof** for every
block: it commits to the full block content (prev/new state roots, block
hash, market-state hash, shielded-event root) and is verifiable via
`mersennet_verifyStateProof`, but it is **not** a zero-knowledge SP1
receipt. The explorer's Verifiable Chain page labels it "Development
proof" so this is never misrepresented.

Real SP1 zkVM proving is gated behind the `sp1` Cargo feature and a
prover adapter, and is designed to run **off the validators' hot path**
because full proving is far too heavy for the 2-4 vCPU validator hosts
(a single block proof can take minutes and gigabytes of RAM).

## Architecture

```
 validators ──(block witnesses via RPC)──▶ prover host (sp1 feature)
                                             │  generates SP1 receipt
                                             ▼
                                     publishes proof (out-of-band)
```

- Validators keep running the fast development prover so liveness never
  blocks on proving.
- A dedicated prover host (16+ vCPU, 32GB+ RAM, ideally a GPU for
  `groth16` wrapping) consumes each block's `BlockProgramInput` and
  produces the real receipt asynchronously.
- The published SP1 receipt replaces the development proof for that
  height in the explorer / light-client view.

## Building the prover

```bash
# On the prover host (needs the SP1 toolchain: https://docs.succinct.xyz)
cargo build --release --features sp1 --bin mersennet

# Point the node at the SP1 CLI adapters (see crates/core/src/zk_sp1.rs
# Sp1CliBackend::from_env):
export MERSENNET_SP1_PROVE_ADAPTER=/opt/mersennet/bin/sp1-prove
export MERSENNET_SP1_VERIFY_ADAPTER=/opt/mersennet/bin/sp1-verify
export MERSENNET_SP1_PROGRAM_ELF=/opt/mersennet/zk/state-transition.elf
export MERSENNET_SP1_VKEY_HASH=0x<vkey-hash>
```

When these are set and the binary is built with `--features sp1`, the
RPC reports `"proverMode": "sp1"` and the explorer shows the real SP1
badge instead of "Development proof".

## Status

- [x] Development prover live on every block (proof-only mode).
- [x] `proverMode` exposed on `mersennet_getStateProof` /
      `getLatestStateProof`; explorer labels dev vs sp1 honestly.
- [ ] Provision the dedicated prover host + SP1 toolchain.
- [ ] Wire the async proof-publication path.
