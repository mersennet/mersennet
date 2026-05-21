# prime-zkp

Zero-knowledge primitives shared between the Prime Chain node and the
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
├── params/             Verifying keys + KZG SRS (Phase 1.4)
└── src/                Rust API
    ├── lib.rs
    ├── field.rs        BN254 scalar field arithmetic (minimal)
    ├── poseidon.rs     in-Rust Poseidon (matches circuits)
    ├── pedersen.rs     commitment helper
    ├── merkle.rs       sparse Poseidon Merkle tree
    ├── nullifier.rs    nullifier set
    ├── note.rs         note schema + commit + nullifier
    ├── threshold.rs    threshold-decryption trait + dummy impl
    ├── noir.rs         proof envelope + verifier trait + MockVerifier
    └── sp1.rs          SP1 program input/output envelope
```

## Build

```bash
cargo build -p prime-zkp
cargo test -p prime-zkp
```

Real Barretenberg-backed proving and verification are gated behind the
`prover` feature; CI runs the workspace without it. Real BLS12-381
threshold cryptography is gated behind the same feature. Tests use the
`MockVerifier` and `DummyThreshold` so the rest of the workspace can
integrate against the proof surface without pulling in pairing-friendly
curve libraries.

## Roadmap (matches `docs/internal/zk-privacy-plan.md`)

- Phase 1.1 — module skeleton (DONE)
- Phase 1.2 — Poseidon + Merkle tree + nullifier set (DONE)
- Phase 1.3 — real BLS12-381 threshold ElGamal (in progress)
- Phase 1.4 — pinned Aztec parameter set + audited Pedersen
- Phase 1.5 — Barretenberg verifier behind `prover` feature
- Phase 5   — `sp1_sdk::ProverClient` swap for block proofs
