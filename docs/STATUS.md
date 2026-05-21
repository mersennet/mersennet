# Prime Chain — Workstream Status

**Branch:** `feat/zk-privacy`
**Last sync:** 2026-05-21 (commit `387691d`)

This is the live tracker for the privacy-fork redesign. The
naming (A, B, C, …) matches the original architecture plan; each
row points at the canonical code or doc that satisfies the
workstream.

Legend: ✅ done · 🟡 in progress · ⬜ pending · 🔒 gated on
external dep (e.g. `nargo`, `sp1up`, audit).

---

## A — State + snapshot

| ID | Description | Status | Reference |
|---|---|---|---|
| A1 | `Block` schema with `shielded_state_root` + `nullifier_root` + `state_proof` | ✅ | `crates/core/src/engine.rs` |
| A2 | `Engine.shielded_*` subsystems wired | ✅ | `crates/core/src/engine.rs` |
| A3 | `apply_shielded_tx` dispatch | ✅ | `crates/core/src/engine.rs` |
| A4 | `run_shielded_tick` per-block hook | ✅ | `crates/core/src/engine.rs` |
| A5 | `0x7E` EIP-2718 tx envelope | ✅ | `crates/core/src/shielded_evm.rs` |
| A6 | `MigrationPlan::apply` | ✅ | `crates/core/src/shielded_evm.rs` |
| A7 | `ShieldedPersistence` (redb) | ✅ | `crates/core/src/shielded_persistence.rs` |
| A8 | `export/import_state_snapshot` with shielded tables + `PZS1` envelope | ✅ | `crates/core/src/engine_snapshot.rs` |

## B — Shielded subsystems

| ID | Description | Status | Reference |
|---|---|---|---|
| B1 | `ShieldedState` note tree + nullifier set + recent-roots ring | ✅ | `crates/core/src/shielded_state.rs` |
| B2 | `ShieldedOrdersEngine` + FBA per-market clearing | ✅ | `crates/core/src/shielded_orders.rs` |
| B3 | `LiquidationAuction` sealed-bid auction | ✅ | `crates/core/src/liquidation_auction.rs` |
| B4 | `ThresholdMempool` admit / decrypt / drain | ✅ | `crates/core/src/threshold_mempool.rs` |
| B5 | `ShieldedEvm` transparent ⇄ shielded bridge | ✅ | `crates/core/src/shielded_evm.rs` |

## C — RPC + WS

| ID | Description | Status | Reference |
|---|---|---|---|
| C1 | Shielded RPC method router (`prime_submit*`, `prime_get*`) | ✅ | `crates/rpc/src/rpc_shielded.rs` |
| C2 | Privacy-gated mutation methods (`-32605` pre-activation) | ✅ | `crates/rpc/src/rpc_shielded.rs` |
| C3 | WS: `newShieldedRoot`, `newClearingPrice`, `newAuctionSettled`, `newStateProof` | ✅ | `crates/rpc/src/ws.rs` + node dispatch |
| C4 | `prime_getStateProof(blockNumber)` reads from `block.state_proof` | ✅ | `crates/rpc/src/rpc_shielded.rs` |

## D — Cryptography

| ID | Description | Status | Reference |
|---|---|---|---|
| D1 | Poseidon-2 BN254 with pinned params | ✅ | `crates/zkp/src/poseidon.rs`, `crates/zkp/params/` |
| D2 | Pedersen commitment with two-mode fallback | ✅ | `crates/zkp/src/pedersen.rs` |
| D3 | BLS12-381 threshold ElGamal (real impl behind `prover`) | ✅ | `crates/zkp/src/bls_threshold.rs` |
| D4 | Pedersen-DKG coordinator | ✅ | `crates/core/src/dkg.rs` |
| D5 | Barretenberg verifier bindings | 🔒 | needs `barretenberg-sys` install |
| D6 | Noir circuit compilation pipeline | 🔒 | needs `nargo` install |
| D7 | Cryptography spec for the auditor | ✅ | `docs/security/cryptography-spec.md` |

## E — SP1 state proofs

| ID | Description | Status |
|---|---|---|
| E1 | SP1 RISC-V toolchain | 🔒 |
| E2 | SP1 program body (re-execute block, prove state transition) | 🔒 |
| E3 | Vkey pin | 🔒 |
| E4 | `ProverClient::network()` integration | 🔒 |
| E5 | Groth16 wrap for Ethereum bridge verifier | 🔒 |

All E* gated on `sp1up` install + Phase-5 budget allocation.

## F — Client / SDK / UX

| ID | Description | Status |
|---|---|---|
| F1 | WASM Noir prover | ⬜ |
| F2 | Wallet note scanner | ⬜ |
| F3 | PrimeTrade shielded order UI | ⬜ (tracked in [prime-trade](https://github.com/PrimeNumbersLabs/prime-trade)) |
| F4 | Migration UX | ⬜ |
| F5 | Selective-disclosure (viewing key) CLI | ⬜ |
| F6 | Go / Python SDK shielded extensions | ⬜ |

## G — Ethereum bridge

| ID | Description | Status |
|---|---|---|
| G1 | `PrimeChainVerifier.sol` — Groth16 verifier on Ethereum | ⬜ |
| G2 | `PrimeChainBridge.sol` — state proof verifier + message bus | ⬜ |
| G3 | Foundry test suite | ⬜ |
| G4 | Audit-prep pass | ⬜ |

## H — Testnet bring-up

| ID | Description | Status | Reference |
|---|---|---|---|
| H1 | Migration tool `migrate-genesis` | ✅ | `crates/node/src/bin/migrate_genesis.rs` |
| H2 | Privacy testnet validator configs (5-of-7, chain 7920) | ✅ | `testnet/configs/privacy/` |
| H3 | docker-compose + bootstrap script + runbook | ✅ | `testnet/docker-compose.privacy.yml`, `testnet/scripts/bootstrap-privacy-genesis.sh`, `docs/runbooks/privacy-testnet-bootstrap.md` |
| H4 | Synthetic load script | ✅ | `testnet/scripts/privacy-load.sh` |
| H5 | Chaos exercise — kill-validator-at-random | ✅ | `testnet/scripts/chaos-kill-validator.sh` |
| H6 | 8-week bake | 🟡 | bake clock starts when D5/D6/E land |
| H7 | Privacy metrics + Grafana dashboard | ✅ | `crates/core/src/prometheus.rs`, `deploy/monitoring/grafana-privacy-dashboard.json` |

## I — Third-party audit

| ID | Description | Status |
|---|---|---|
| I1 | Crypto audit (Poseidon, Pedersen, BLS, DKG) | 🔒 |
| I2 | Protocol audit | 🔒 |
| I3 | Solidity audit | 🔒 |
| I4 | ImmuneFi bounty program | 🔒 |
| I5 | Audit fix cycle | 🔒 |
| I6 | Re-audit | 🔒 |

All I* gated on D5/D6/E completion.

## J — Governance + activation

| ID | Description | Status |
|---|---|---|
| J1 | Activation vote | 🔒 |
| J2 | Validator upgrade schedule | 🔒 |
| J3 | Communications | 🔒 |
| J4 | Runbook execution | 🔒 (runbook drafted: `docs/runbooks/zk-fork-activation.md`) |
| J5 | Deprecation window | 🔒 |
| J6 | Post-mortem template | 🔒 |

## K — CI / tooling

| ID | Description | Status | Reference |
|---|---|---|---|
| K1 | Expanded `.github/workflows/ci.yml` (prover feature, tsc, forge, audit) | ✅ | `.github/workflows/ci.yml` |
| K2 | Privacy-invariant grep guard | ✅ | `scripts/ci/check-privacy-invariants.sh` + `docs/security/privacy-invariants.md` |
| K3 | `cargo llvm-cov` coverage in CI artifacts | ⬜ |
| K4 | `Dockerfile.dev` (Rust + Foundry + nargo + sp1up + Node 20) | ⬜ |
| K5 | `cargo doc` publish | ⬜ |

---

## At a glance

- **Testnet ready.** Chain 7920 can be brought up today with the
  bootstrap script. Privacy mode auto-activates at the configured
  height; the 5-of-7 DKG runs on the simulated path until D5/D6
  land.
- **Pre-audit work outstanding.** Workstreams D5, D6, and E need
  external toolchains (nargo, barretenberg, sp1up) installed and a
  Phase-5 commit before the testnet can graduate to a hard-fork
  rehearsal.
- **Bake clock.** The 8-week pre-mainnet bake (H6) starts on the
  day D5/D6/E land and the audit cycle (I) is funded.

When a row changes status, update this file in the same PR.
