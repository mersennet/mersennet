# ADR-018: Privacy Hard Fork

**Status:** Proposed — **testnet ready** on chain 7920.
**Date:** 2026-05-21 (last revised after H1–H7 land on `feat/zk-privacy`).
**Related:** [ADR-014](ADR-014-shielded-notes.md),
[ADR-015](ADR-015-threshold-encrypted-mempool.md),
[ADR-016](ADR-016-liquidation-auctions.md),
[ADR-017](ADR-017-sp1-state-proofs.md).
**Spec:** [`../security/cryptography-spec.md`](../security/cryptography-spec.md).
**Runbook:** [`../runbooks/privacy-testnet-bootstrap.md`](../runbooks/privacy-testnet-bootstrap.md).

## Context

The shielded-notes model (ADR-014), threshold-encrypted mempool
(ADR-015), and shielded EVM EOA balances (ADR-018 dependency on
Phase 4) cannot be activated piecemeal. They require a coordinated
hard fork because:

1. Block-header serialization changes (new `shielded_state_root` and
   `nullifier_root` fields).
2. Tx envelope `0x7E` becomes a valid type.
3. The mempool changes from optional-encrypted to required-encrypted
   for shielded txs.
4. Every EOA's transparent balance is snapshotted into a single
   shielded note via the migration tool.

## Decision

Activate at a fixed height `H` on chain ID 7919 (Prime Chain
mainnet). Pre-activation testnet runs for at least 8 weeks with the
identical fork rules.

### Activation behavior

At block `H`:

1. The block-header schema gains `shielded_state_root` and
   `nullifier_root` fields. Pre-`H` blocks treat these as zero.
2. The genesis migration runs:
   `crates/core/src/shielded_evm.rs::MigrationPlan::apply()`. Every
   EOA in the state-trie snapshot at `H-1` produces a single
   shielded note `(value=balance, asset_id=0,
   owner_pk=Poseidon(eoa_address))`. The transparent balance for
   each EOA is set to zero in the same block.
3. The `0x7E` tx type becomes accepted. The legacy `0x00` and `0x02`
   tx types continue to work, but only for transparent contract
   calls — they cannot move PRIM between EOAs.
4. The shielded RPC methods (`prime_getShieldedBalance`,
   `prime_submitShieldedOrder`, etc.) become live. Pre-`H` they
   return `-32605` ("disabled in current chain mode").
5. The legacy `is_liquidatable(addr)` and `liquidate(addr)`
   precompile selectors are deactivated. The bonded liquidator
   auction (ADR-016) is the only liquidation path.

### Activation criteria

The fork activates only when all of the following are true at
height `H-1`:

- `>=8 weeks` of continuous testnet operation under fork rules
  with `>=99.5%` block-producer uptime.
- All audited subsystems green: Noir circuits, BLS DKG,
  SP1 program.
- `>=2/3` of voting power has signed the activation block.
- Validator-software version `>=1.0.0-zk` is running on `>=80%` of
  voting power.

If any criterion fails, the chain continues in legacy transparent
mode and the next activation attempt is scheduled `>=1 week` later.

## Consequences

**Positive.**
- One atomic switch; no operating period where the chain runs in a
  half-shielded half-transparent state.
- Migration is deterministic; every wallet can recover its
  shielded note from the address and the activation height alone.

**Negative.**
- Wallets must upgrade before `H` to read their post-`H` balance.
  Mitigated by a multi-week pre-activation announcement and a
  fallback `prime_getShieldedBalanceLegacy(addr)` RPC method
  retained for one quarter post-fork.
- Block headers grow by 64 bytes. Acceptable.

## Pre-fork runbook

Two runbooks now exist:

1. **[`../runbooks/privacy-testnet-bootstrap.md`](../runbooks/privacy-testnet-bootstrap.md)** —
   bring up chain 7920 (the privacy testnet) from a fresh host. This
   is the one operators use today.
2. **[`../runbooks/zk-fork-activation.md`](../runbooks/zk-fork-activation.md)** —
   T-8w through T+24h checklist for the mainnet activation. Drafted
   now, gated on D5/D6/E completion + audit.

### Implementation status

| Workstream | Status | Notes |
|---|---|---|
| State + snapshot extension (A8) | ✅ | `PZS1` envelope wraps the legacy snapshot with the shielded subsystems |
| Migration tool (H1) | ✅ | `cargo run --bin migrate-genesis` |
| 5-of-7 DKG configs (H2) | ✅ | `testnet/configs/privacy/validator-{1..7}.json` |
| Docker-compose bring-up (H3) | ✅ | `testnet/docker-compose.privacy.yml` |
| Synthetic load (H4) + chaos drill (H5) | ✅ | `testnet/scripts/` |
| Privacy metrics + Grafana dashboard (H7) | ✅ | `deploy/monitoring/grafana-privacy-dashboard.json` |
| CI privacy invariants (K2) | ✅ | `scripts/ci/check-privacy-invariants.sh` |
| Barretenberg verifier (D5), Noir compilation (D6), SP1 program (E) | 🔒 | Gated on `nargo` / `barretenberg-sys` / `sp1up` install |
| Third-party audit (I) | 🔒 | Gated on D5/D6/E completion |

See [`../STATUS.md`](../STATUS.md) for the full live tracker.
