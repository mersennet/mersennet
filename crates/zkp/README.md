# prime-zkp

Zero-knowledge primitives shared between the Mersennet node and the
client SDK.

```
crates/zkp/
├── Cargo.toml
├── README.md
├── circuits/           Noir source for shielded circuits
│   ├── Nargo.toml
│   └── src/
│       ├── lib.nr               common helpers
│       ├── poseidon.nr          hash wrapper
│       ├── merkle.nr            path verification
│       ├── note.nr              note schema
│       ├── main.nr              entry point (single-spend)
│       ├── spend.nr             1-in 1-out + optional unshield
│       ├── output.nr            shield (transparent → shielded)
│       ├── join_split.nr        2-in 2-out workhorse
│       ├── order_place.nr       perp order with ZK solvency proof
│       ├── liquidate_claim.nr   sealed-bid liquidation claim
│       └── liquidate_execute.nr auction-winner liquidation execution
├── params/             Pinned parameters (poseidon-bn254.bin, sp1/ vkey pin)
└── src/                Rust API
    ├── lib.rs
    ├── field.rs        BN254 scalar field arithmetic (minimal)
    ├── poseidon.rs     in-Rust Poseidon (matches circuits)
    ├── pedersen.rs     commitment helper
    ├── merkle.rs       sparse Poseidon Merkle tree
    ├── nullifier.rs    nullifier set
    ├── note.rs         note schema + commit + nullifier
    ├── threshold.rs    threshold-decryption trait + dummy impl
    ├── bls_threshold.rs BLS12-381 threshold ElGamal (behind `prover`)
    ├── noir.rs         proof envelope + verifier trait + MockVerifier
    └── sp1.rs          SP1 program input/output envelope
```

## Build

```bash
cargo build -p prime-zkp
cargo test -p prime-zkp
```

Feature-enabled runtime wiring now has two explicit lanes:

```bash
cargo check -p prime-zkp --features prover --lib
cargo check -p prime-chain-node --features prover,sp1
```

Real Barretenberg-backed proving and verification are gated behind the
`prover` feature; CI runs the workspace without it. Real BLS12-381
threshold cryptography is gated behind the same feature. Tests use the
`MockVerifier` and `DummyThreshold` so the rest of the workspace can
integrate against the proof surface without pulling in pairing-friendly
curve libraries.

## Toolchain adapters

The default build still falls back to the mock verifier/prover paths.
When the external toolchains are available, the runtime switches to the
real paths via environment variables instead of linking native SDKs
directly into every workspace build.

### Noir / Barretenberg

- `PRIME_NARGO_BIN`: optional override for the `nargo` executable.
- `PRIME_NOIR_CIRCUITS_DIR`: optional override for the shared circuit source tree.
- `PRIME_NOIR_ARTIFACTS_DIR`: directory where compiled per-circuit artifacts live.
- `PRIME_BB_VERIFY_ADAPTER`: executable or script that verifies a proof against one compiled circuit.

`NoirToolchain::compile_circuit` materializes a temporary per-circuit
package from `crates/zkp/circuits/src`, runs `nargo compile`, copies the
resulting `target/` directory into `PRIME_NOIR_ARTIFACTS_DIR/<circuit>/`,
then writes a deterministic `vk.hash` file over the compiled artifacts.

The verify adapter is invoked with:

```text
<adapter> --circuit <slug> --artifacts <dir> --proof <proof.bin> --public-inputs <public_inputs.txt>
```

where `public_inputs.txt` contains one little-endian field element per
line as hex.

### SP1

- `PRIME_SP1_PROVE_ADAPTER`: executable or script that produces an SP1 proof response.
- `PRIME_SP1_VERIFY_ADAPTER`: executable or script that verifies an SP1 proof response.
- `PRIME_SP1_PROGRAM_ELF`: optional path to the program ELF used by the adapter.
- `PRIME_SP1_VKEY_HASH`: optional pinned 32-byte hex vkey hash.
- `PRIME_SP1_MODE`: `local` or `network`.

The SP1 adapters receive `--request <json> --response <json>` and are
responsible for filling the response file with the proof or verification
result. When these variables are absent, `SP1Prover::runtime_default()`
falls back to the existing deterministic mock path.

Reference scripts for all of the above live in [scripts/zk/README.md](../../scripts/zk/README.md).

## Roadmap (matches `docs/internal/zk-privacy-plan.md`)

- Phase 1.1 — module skeleton (DONE)
- Phase 1.2 — Poseidon + Merkle tree + nullifier set (DONE)
- Phase 1.3 — real BLS12-381 threshold ElGamal (DONE, `bls_threshold.rs` behind `prover`)
- Phase 1.4 — pinned Aztec parameter set + audited Pedersen (DONE, `params/poseidon-bn254.bin`)
- Phase 1.5 — Barretenberg verifier behind `prover` feature (DONE, adapter-based — see `scripts/zk/`)
- Phase 5   — `sp1_sdk::ProverClient` swap for block proofs (in progress — workstream E; vkey pinned in `params/sp1/`)
