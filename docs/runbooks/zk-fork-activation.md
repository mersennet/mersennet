# Runbook: Privacy Hard-Fork Activation

**Audience:** Mersennet validators, infrastructure operators, and the
ecosystem leadership.
**Linked ADRs:** [ADR-014](../adr/ADR-014-shielded-notes.md),
[ADR-015](../adr/ADR-015-threshold-encrypted-mempool.md),
[ADR-016](../adr/ADR-016-liquidation-auctions.md),
[ADR-017](../adr/ADR-017-sp1-state-proofs.md),
[ADR-018](../adr/ADR-018-privacy-hard-fork.md).

This runbook is the operational checklist for activating the
privacy hard fork on the production chain (mainnet chain ID 13370;
the transparent testnet runs chain ID 7919).

---

## T-8 weeks: testnet activation

- [ ] Record the SP1 proving artifact set for the candidate release:
      - program ELF build provenance
      - pinned `PRIME_SP1_VKEY_HASH`
      - one successful `PRIME_SP1_MODE=local` prove + verify transcript against the checked-in host runner
- [ ] Tag `v1.0.0-zk-rc.0` on `feat/zk-privacy`.
- [ ] Pre-mainnet testnet (chain ID 7920) restarts from snapshot
      with the new genesis. Migration runs. Every faucet account
      should hold one shielded note.
- [ ] Validators sign the testnet activation block. Confirm
      `prime_blockNumber` advances on every validator.
- [ ] Confirm `prime_getShieldedBalance` returns the migrated
      balance for at least 50 sample addresses.
- [ ] Confirm `prime_submitShieldedOrder` round-trips for at least
      1 000 orders without a single rejection that wasn't
      explicitly expected (stale anchor, oracle mismatch, etc.).
- [ ] Confirm the bonded liquidator auction settles at least 10
      live liquidations.

## Weeks T-8 to T-1: bug-bash

Every two weeks:

- Replace one mock dependency (`MockVerifier`, `DummyThreshold`,
  `SP1Prover::Mock`) with its production backend.
- Reconfirm the SP1 CI lanes stay green:
      `cargo check -p prime-chain-node --features prover,sp1`,
      `cargo check --manifest-path programs/state-transition/Cargo.toml`,
      `cargo test --manifest-path programs/state-transition-host/Cargo.toml`.
- Run an internal red-team exercise: a privileged team tries to
  identify a trader by network observation. Result must be:
  unidentifiable.

## T-1 week: announcement

- [ ] Publish `v1.0.0-zk-rc.N` with `N` the iteration count.
- [ ] Publish the exact `H` block on Twitter / Discord / blog /
      docs site.
- [ ] Distribute updated `prime-chain` binaries.
- [ ] Distribute updated `@prime-chain/sdk` (npm), `prime-chain-go`,
      `prime-chain-python`.
- [ ] PrimeTrade UI ships the shielded mode behind a feature flag.
- [ ] Validators that have not upgraded by T-48h get a personal
      Slack ping.

## T-24h: validator upgrades

- [ ] Every validator restarts on `v1.0.0-zk` (no `-rc` suffix).
- [ ] Confirm `web3_clientVersion` returns `prime-chain/1.0.0-zk`.
- [ ] Confirm validator count and voting power match expectations
      via `prime_validators`.

## T-1h: final dry-run

- [ ] On testnet, run `migration-dry-run.sh` and confirm:
      - the migration plan size matches `prime_getStateTrieSize`
      - the resulting tree root matches the deterministic recomputation
      - elapsed time is `<10 minutes`
- [ ] Re-run one SP1 prove + verify round-trip against the exact release
      artifact set (`program ELF`, `PRIME_SP1_VKEY_HASH`, host runner build),
      using `scripts/zk/sp1-prove-request.template.json` and
      `scripts/zk/sp1-verify-request.template.json` as the request
      skeletons. On Windows, run the checked-in host runner through
      `PRIME_SP1_HOST_EXECUTOR=wsl`; keep `PRIME_SP1_MODE=local` until
      the delegated E4 network-prover path is qualified for release use.
- [ ] Record one delegated `PRIME_SP1_MODE=network` prove + verify round-trip
      for the candidate artifact set and archive:
      - `scripts/zk/sp1-network-prove-response.json`
      - `scripts/zk/sp1-network-verify-request.request.json`
      - `scripts/zk/sp1-network-verify-response.json`
      - ELF provenance + pinned `PRIME_SP1_VKEY_HASH`
      - delegated prover account / environment notes
- [ ] Ethereum bridge (E5 / G1–G4): re-run `cd contracts && forge build
      --sizes && forge test --match-path 'test/zk/*' -vvv` (21 tests) and
      `cargo test -p prime-chain --lib bridge_export` (4 tests). Before
      mainnet bridging, install the SP1→Groth16 wrapping verifying key via
      `Groth16Verifier.setVerifyingKey` + `lockVerifyingKey`, then archive
      one real `submitStateProof` calldata set produced by
      `bridge_export::build_bridge_submission`.
- [ ] Confirm the SP1 state-proof prover backend is healthy
      (latest checkpoint <2 minutes old).
- [ ] Confirm the threshold-decryption ceremony succeeded for the
      last 100 testnet blocks.

## T: activation

- [ ] Block `H-1` finalized.
- [ ] Block `H` produced with new header schema, migration
      transaction, `shielded_state_root` non-zero.
- [ ] `prime_blockNumber` advances within 10 seconds.
- [ ] No validator emits "fork mismatch" event.

## T+1h: post-activation smoke

- [ ] Every shielded RPC method (`prime_getShieldedBalance`,
      `prime_submitShieldedOrder`, `prime_getShieldedNotes`,
      `prime_submitShieldedTransfer`, `prime_submitUnshield`,
      `prime_submitLiquidationClaim`, `prime_submitLiquidationExecute`,
      `prime_registerLiquidator`, `prime_getStateProof`,
      `prime_viewGrantToken`) returns 200 OK for a known good input.
- [ ] PrimeTrade UI loads the shielded view and renders a balance.
- [ ] At least one community wallet (Argent, Rabby, etc.) lists
      the shielded balance correctly.

## T+24h: post-mortem (only if needed)

A "needed" post-mortem is one of:
- any validator reported a `fork mismatch`
- any wallet failed to read its shielded balance for >5 minutes
- the SP1 prover backend was unreachable for >1 hour
- any user lost funds

If none of the above triggered, no post-mortem; we ship a
"Privacy Hard Fork: Day One" recap to the community instead.

## Rollback policy

A rollback is only permitted if **either**:

- The chain halts (no block production for >10 minutes).
- A class of users is unable to access their funds (shielded
  balance returns wrong value or rejected on every spend attempt).

In both cases:

1. Validators coordinate via the validator Discord.
2. Governance issues an emergency signal to roll back to
   `H-1`-snapshot state.
3. `feat/zk-privacy` is force-rebased on top of `main`'s pre-`H`
   state.
4. A new activation is scheduled at `H' = H + 1 week`.

A rollback **MUST NOT** be used to censor specific transactions or
specific users. The only acceptable trigger is the technical
criteria above.
