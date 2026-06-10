# Mersennet — Workstream Status

**Branch:** `feat/zk-privacy` (close-out work on `feat/zk-e4-e5-f-closeout`)
**Last sync:** 2026-06-05

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
| D5 | Barretenberg verifier + prover adapter bindings | ✅ | `crates/zkp/src/noir.rs`, `scripts/zk/barretenberg_*.py` |
| D6 | Noir circuit compilation pipeline | ✅ | `crates/zkp/src/noir.rs`, `scripts/zk/compile_noir_artifacts.py` |
| D7 | Cryptography spec for the auditor | ✅ | `docs/security/cryptography-spec.md` |

## E — SP1 state proofs

| ID | Description | Status | Reference |
|---|---|---|---|
| E1 | SP1 RISC-V toolchain | ✅ | WSL toolchain bootstrap + `cargo-prove prove build` |
| E2 | SP1 prove path now consumes full witness-bearing `BlockProgramInput`, re-derives `BlockProgramOutput`, replays the shielded transfer / shield / unshield / liquidation-execute sub-path, replays deterministic pre-tick FBA market clearing, replays order admission from canonical decrypted-intent witnesses bound to an explicit oracle snapshot, replays liquidation claim/settle from the pre-tick witness, and proves `shielded_event_root` | ✅ | `crates/core/src/engine.rs`, `crates/core/src/state_proof.rs`, `crates/core/src/zk_sp1.rs`, `crates/zkp/src/sp1.rs`, `programs/state-transition/`, `programs/state-transition-host/` |
| E3 | Vkey pin + release-artifact capture — DONE. Reproducible Docker ELF (vkey `0013c6c7…`) proved (`core`) and cryptographically verified (`{"verified": true}`) via `PRIME_SP1_MODE=local`. | ✅ | `scripts/zk/sp1-prove-response.json`, `scripts/zk/sp1-verify-response.json`, `scripts/zk/sp1-prove.trace.log`, `crates/zkp/params/sp1/state-transition.vk.hash`, `scripts/zk/README.md` |
| E4 | `ProverClient::network()` integration — c-kzg conflict resolved by making the SP1 host revm-free (proof types extracted to `mersennet-state-proof`). Both prove and verify dispatch `ProverClient::builder().network().build()` behind the `network` feature; `cargo check --features network` passes. Turnkey: `scripts/zk/sp1-network-prove-request.request.json` is staged and the one-command delegated flow is documented. **Only the credentialed `PRIME_SP1_MODE=network` execution remains** (gated on Succinct `NETWORK_PRIVATE_KEY` / `NETWORK_RPC_URL`). | 🟡 | `crates/state-proof/`, `programs/state-transition-host/`, `scripts/zk/sp1-network-prove-request.request.json` |
| E5 | Groth16 wrap for Ethereum bridge verifier — verifier + bridge contracts complete and tested (21 Foundry tests), chain-side `verifyStateProof` precompile wired, and a tested chain-side `bridge_export` helper emits the `submitStateProof(uint256[8], uint256[])` calldata. **Remaining: the SP1→Groth16 wrapping circuit + verifying key + one real wrapped proof** (out-of-repo: needs the wrapping-circuit toolchain / trusted setup). | 🟡 | `contracts/src/zk/`, `crates/core/src/bridge_export.rs`, `crates/core/src/precompiles.rs` |

Remaining E close-out items are both external/out-of-repo: one delegated
network proof against the Succinct network (E4, gated on credentials), and
the SP1→Groth16 wrapping circuit + verifying key (E5, gated on the wrapping
toolchain / trusted setup). All in-repo code, tests, calldata helpers, and
docs for both are complete; the network-prover c-kzg conflict is resolved
and the host `--features network` compile lane passes locally.

## F — Client / SDK / UX

| ID | Description | Status |
|---|---|---|
| F1 | WASM Noir prover — `NoirWasmProver` (`ZkProver`) maps typed calls to named Noir circuit inputs via an injected `NoirProvingBackend`; exported + typed + tested + wiring example (`sdk/examples/noir-prover-wiring.js`). Real `@noir-lang/noir_js` + `@aztec/bb.js` backend is wired by the wallet. | ✅ (`sdk/src/noir-prover.ts`) |
| F2 | Wallet note scanner — `scanGrantedNotes` (grant-gated ciphertext scan + decrypt) + `scanAndReconstructBalances` (paged `viewBalances`, nullifier-aware spendable balance reconstruction) + `reconstructPortfolio`; tested end-to-end. | ✅ (`sdk/src/reconstruction.ts`, `sdk/src/shielded.ts`) |
| F3 | PrimeTrade shielded order UI | ⬜ external (tracked in [prime-trade](https://github.com/PrimeNumbersLabs/prime-trade)); all repo-local SDK/API support complete — `ShieldedClient.placeOrder` (`prime_submitShieldedOrder`), `NoirWasmProver` injection, and grant-gated reads are exported and tested |
| F4 | Migration UX — `deriveMigrationNote` / `matchesMigrationNote` plus `planMigration` (pre-fork preview + per-asset totals) / `confirmMigration` (post-fork landed-note confirmation); tested + example (`sdk/examples/migration-drive.js`). | ✅ (`sdk/src/migration.ts`) |
| F5 | Selective-disclosure grant lifecycle (ADR-019) — grant signature verification, persistence + revocation, `prime_viewGrantStatus`, gated `prime_viewPortfolioDigest` / `prime_viewNotes`, and the reconstruction reads `prime_viewBalances` (`balances:read`, notes + spent nullifiers), `prime_viewPositions` (`positions:read`), `prime_viewOrders` (`orders:read`); SDK `provider.viewBalances/viewPositions/viewOrders` + `scanAndReconstructBalances`; tested. | ✅ (`crates/rpc/src/rpc_shielded.rs`, `sdk/src/provider.ts`) |
| F6 | Go / Python SDK shielded extensions | ✅ (`sdk-go/`, `sdk-python/`) |

## G — Ethereum bridge

| ID | Description | Status | Reference |
|---|---|---|---|
| G1 | `Groth16Verifier.sol` — BN254 Groth16 verifier on Ethereum (ecAdd/ecMul/ecPairing precompiles, settable+lockable VK) | ✅ | `contracts/src/zk/Groth16Verifier.sol` |
| G2 | `PrimeChainBridge.sol` — state-proof verifier + deposit/withdraw message bus (monotonic block + root continuity, sorted-pair Merkle withdrawals) | ✅ | `contracts/src/zk/PrimeChainBridge.sol` |
| G3 | Foundry test suite (21 tests passing) | ✅ | `contracts/test/zk/Groth16Verifier.t.sol`, `contracts/test/zk/PrimeChainBridge.t.sol` |
| G4 | Audit-prep pass | 🟡 (verifier/bridge frozen + tested; final pass pending the E5 wrapping VK) | `contracts/src/zk/README.md` |

## H — Testnet bring-up

| ID | Description | Status | Reference |
|---|---|---|---|
| H2 | Privacy testnet validator configs (5-of-7, chain 131071) | ✅ | `testnet/configs/privacy/` |
| H3 | docker-compose + bootstrap script + runbook | ✅ | `testnet/docker-compose.privacy.yml`, `testnet/scripts/bootstrap-privacy-genesis.sh`, `docs/runbooks/privacy-testnet-bootstrap.md` |
| H4 | Synthetic load script | ✅ | `testnet/scripts/privacy-load.sh` |
| H5 | Chaos exercise — kill-validator-at-random | ✅ | `testnet/scripts/chaos-kill-validator.sh` |
| H6 | 8-week bake | 🟡 | bake checklist drafted; clock starts once remaining E blockers are cleared |
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

All I* gated on the remaining E blockers being cleared and the privacy-fork audit packet being assembled.

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
| K1 | Expanded `.github/workflows/ci.yml` (prover feature, SP1 node/program/host lanes, tsc, forge, audit) | ✅ | `.github/workflows/ci.yml` |
| K2 | Privacy-invariant grep guard | ✅ | `scripts/ci/check-privacy-invariants.sh` + `docs/security/privacy-invariants.md` |
| K3 | `cargo llvm-cov` coverage in CI artifacts | ✅ |
| K4 | `Dockerfile.dev` validation in CI | ✅ |
| K5 | `cargo doc` publish artifact | ✅ |

---

## Execution Checklist — Remaining E / H / I

Use this section as the working close-out list for the zk privacy fork.
Owners are taken from `.github/CODEOWNERS`.

### E — SP1 state-proof close-out

E2 and E3 are now closed on this branch. The remaining SP1 close-out
work is E4 through E5.

| Item | Owner | Files | Commands | Exit criteria |
|---|---|---|---|---|
| E4. `ProverClient::network()` cut-over | `@PrimeNumbersLabs/zk` | `crates/state-proof/`, `programs/state-transition-host/src/main.rs`, `scripts/zk/README.md`, `docs/security/privacy-fork-audit-packet.md`, `docs/runbooks/zk-fork-activation.md` | `cargo check --manifest-path programs/state-transition-host/Cargo.toml --features real-sp1`<br>`cd programs/state-transition-host && cargo tree -i c-kzg` (default/real-sp1: no match)<br>`cargo check --manifest-path programs/state-transition-host/Cargo.toml --features network`<br>`PRIME_SP1_MODE=network cargo run --release --manifest-path programs/state-transition-host/Cargo.toml --features network -- --prove-request scripts/zk/sp1-prove-request.request.json --prove-response scripts/zk/sp1-network-prove-response.json`<br>`cargo run --manifest-path programs/state-transition-host/Cargo.toml --example render_verify_request -- scripts/zk/sp1-network-prove-response.json scripts/zk/sp1-network-verify-request.request.json <program-elf-path>`<br>`PRIME_SP1_MODE=network cargo run --release --manifest-path programs/state-transition-host/Cargo.toml --features network -- --verify-request scripts/zk/sp1-network-verify-request.request.json --verify-response scripts/zk/sp1-network-verify-response.json` | **Compile conflict resolved and verified locally**: the SP1 host is revm-free (proof types live in `mersennet-state-proof`), so `sp1-sdk/network` no longer collides on the `ckzg` native link. Remaining: capture one delegated network prove/verify transcript with real Succinct credentials and archive the response, verify response, and request/ELF provenance in the audit packet. |
| E5. Groth16 wrap for Ethereum verifier path | `@PrimeNumbersLabs/contracts`, `@PrimeNumbersLabs/core` | `contracts/src/zk/`, `crates/core/src/bridge_export.rs`, `crates/core/src/precompiles.rs` | `cd contracts && forge build --sizes`<br>`cd contracts && forge test --match-path 'test/zk/*' -vvv`<br>`cargo test -p mersennet --lib bridge_export` | **In-repo done**: Groth16 verifier + bridge contracts pass 21 Foundry tests, the chain-side `verifyStateProof` precompile is wired, and `bridge_export` emits the `submitStateProof(uint256[8], uint256[])` calldata (4 unit tests). **Remaining (out-of-repo)**: build the SP1→Groth16 wrapping circuit, run its trusted setup to produce the verifying key for `Groth16Verifier.setVerifyingKey`, and capture one real wrapped proof. |

#### E2 Completed Scope

- E2a. `crates/core/src/engine.rs::shielded_block_header()` now fails closed behind required-proof mode and sources post-fork shielded roots from the proof whenever a proof exists, rather than preserving a proof-validity bypass through host fallback values.
- E2a. `crates/core/src/state_proof.rs::collect_block_input()` and `crates/core/src/state_proof.rs::prove_block()` now treat `expected_block_hash` and `expected_market_state_hash` as commitments checked against the produced output, and they bind the live shielded/nullifier state instead of ignoring the host-side post-state.
- E2a. The generic `StateProver::prove_state_transition()` escape hatch has been removed; mock-only public-output proving now lives on `crates/core/src/zk_proofs.rs::MockProver::prove_public_output()`, while canonical backends prove only from `BlockProgramInput`.
- E2a. `crates/zkp/src/sp1.rs::execute_block_program()` now derives `block_hash` from `header` and rejects `expected_block_hash` mismatches as part of the public-output contract; the old host-forwarded block-hash handoff is no longer present.
- E2b. Shared tick-event sequencing now flows through `crates/zkp/src/sp1.rs::build_shielded_tick_events()`, and shared market clearing/apply now flows through `crates/zkp/src/sp1.rs::apply_market_tick_witness()` with runtime consumption in `crates/core/src/shielded_orders.rs::run_fba()`.
- E2b. Shared order-admission validation now lives in `crates/zkp/src/sp1.rs::validate_order_admission_witness()`, with runtime `crates/core/src/shielded_orders.rs::admit_intent()` and zk replay `crates/zkp/src/sp1.rs::replay_order_admission()` both calling the same core.
- E2b. Liquidation claim validation now lives in `crates/zkp/src/sp1.rs::validate_liquidation_claim_witness()`, with runtime `crates/core/src/liquidation_auction.rs::submit_claim()` and zk replay `crates/zkp/src/sp1.rs::replay_liquidation_events()` both using the same stale-anchor, registration, and proof checks.
- E2b. Liquidation claim shaping, winner ordering, settlement, canonical event derivation, and executor-side liquidation event replay are now shared across runtime and zk replay. The focused regression `crates/zkp/src/sp1.rs::tests::execute_block_program_replays_liquidation_events()` covers the canonical SP1 event-root path.
- E2b. `crates/core/src/state_proof.rs::shielded_tick_witness()` and `crates/core/src/state_proof.rs::snapshot_subsystem_digests()` should stay serialization/binding helpers over shared witness-level helpers, not drift back into runtime-only semantic hashing or transition logic.

### H — Testnet bake start criteria

| Item | Owner | Files | Commands | Exit criteria |
|---|---|---|---|---|
| H6a. Authorize bake start | `@PrimeNumbersLabs/core`, `@PrimeNumbersLabs/docs` | `docs/STATUS.md`, `docs/runbooks/zk-fork-activation.md`, `docs/security/privacy-fork-audit-packet.md` | No new command. Review E2-E5 and confirm all exit criteria are met. | `docs/STATUS.md` moves H6 from “checklist drafted” to active bake with a start date. |
| H6b. Start T-8 testnet activation lane | `@PrimeNumbersLabs/core` | `testnet/configs/privacy/`, `testnet/docker-compose.privacy.yml`, `testnet/scripts/bootstrap-privacy-genesis.sh`, `testnet/scripts/privacy-load.sh`, `docs/runbooks/privacy-testnet-bootstrap.md`, `docs/runbooks/zk-fork-activation.md` | `cargo check -p mersennet-node --features prover,sp1`<br>`cargo check --manifest-path programs/state-transition/Cargo.toml`<br>`cargo test --manifest-path programs/state-transition-host/Cargo.toml`<br>Then execute the T-8 checklist in `docs/runbooks/zk-fork-activation.md`. | Privacy testnet is restarted on the candidate release, SP1 lanes stay green, shielded RPC/order flow passes the runbook checks, and the 8-week bake clock is officially running. |
| H6c. Ongoing bake evidence | `@PrimeNumbersLabs/core`, `@PrimeNumbersLabs/docs` | `docs/STATUS.md`, `docs/runbooks/zk-fork-activation.md`, `deploy/monitoring/grafana-privacy-dashboard.json` | Every two weeks, rerun:<br>`cargo check -p mersennet-node --features prover,sp1`<br>`cargo check --manifest-path programs/state-transition/Cargo.toml`<br>`cargo test --manifest-path programs/state-transition-host/Cargo.toml` | Bake log captures recurring validation, backend swaps, and incident-free operation through the full 8-week window. |

### I — Audit path enablement and execution

| Item | Owner | Files | Commands | Exit criteria |
|---|---|---|---|---|
| I0. Assemble audit packet | `@PrimeNumbersLabs/zk`, `@PrimeNumbersLabs/docs`, `@PrimeNumbersLabs/core` | `SECURITY_AUDIT.md`, `docs/security/privacy-fork-audit-packet.md`, `docs/security/cryptography-spec.md`, `docs/security/privacy-invariants.md`, `docs/runbooks/zk-fork-activation.md` | `cargo check -p mersennet-node --features prover,sp1`<br>`cargo check --manifest-path programs/state-transition/Cargo.toml`<br>`cargo test --manifest-path programs/state-transition-host/Cargo.toml`<br>`cargo clippy --workspace --all-targets -- -D warnings`<br>`cargo audit` | Engagement packet contains the artifacts required in `SECURITY_AUDIT.md`, plus fresh CI/static-analysis evidence. |
| I1-I3. External crypto / protocol / Solidity audits | `@PrimeNumbersLabs/zk`, `@PrimeNumbersLabs/core`, `@PrimeNumbersLabs/contracts` | `crates/zkp/`, `crates/core/`, `contracts/` | `cargo check -p mersennet-node --features prover,sp1`<br>`Set-Location contracts; forge build --sizes`<br>`Set-Location contracts; forge test -vvv` | Audit scopes are funded and kicked off only after E is complete and I0 is assembled. |
| I4. ImmuneFi bounty launch | `@PrimeNumbersLabs/core`, `@PrimeNumbersLabs/docs` | `SECURITY_AUDIT.md`, `docs/SECURITY.md`, `docs/security/privacy-fork-audit-packet.md` | No repo-only command. Requires published policy + funded program. | Bounty program is live with the privacy-fork scope and known limitations disclosed. |
| I5-I6. Fix cycle and re-audit | `@PrimeNumbersLabs/zk`, `@PrimeNumbersLabs/core`, `@PrimeNumbersLabs/contracts`, `@PrimeNumbersLabs/docs` | Audit-touched files plus `docs/STATUS.md` and the audit packet | Re-run the relevant lane per finding:<br>`cargo test -p mersennet-zkp sp1`<br>`cargo check -p mersennet-node --features prover,sp1`<br>`Set-Location contracts; forge test -vvv`<br>`cargo audit` | All external findings are fixed, documented, and re-verified by the auditors. |

## At a glance

- **Testnet ready.** Chain 131071 can be brought up today with the
  bootstrap script. Privacy mode auto-activates at the configured
  height; the Noir / Barretenberg path uses the checked-in runtime
  adapters when configured and otherwise falls back to the deterministic
  mock.
- **Pre-audit work outstanding.** The Noir / Barretenberg runtime path
  (D5/D6) is wired through the checked-in adapters and compile
  pipeline. The SP1 prove path now carries canonical witness data and
  replays the shielded tx + tick path end to end, including order
  admission, liquidation settle, and `shielded_event_root`. The client
  SDK surfaces (F1–F5) are complete and tested. The two remaining
  blockers before a hard-fork rehearsal are both external: capturing one
  delegated Succinct network proof (E4, credential-gated) and producing
  the SP1→Groth16 wrapping circuit + verifying key (E5); the bridge
  verifier/contracts, chain-side verify precompile, and `bridge_export`
  calldata helper are already complete and tested.
- **Bake clock.** The 8-week pre-mainnet bake (H6) starts on the
  day E lands and the audit cycle (I) is funded.

When a row changes status, update this file in the same PR.
