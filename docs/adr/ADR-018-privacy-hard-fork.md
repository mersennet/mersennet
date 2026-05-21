# ADR-018: Privacy Hard Fork

**Status:** Proposed (scheduled — block height TBD)
**Date:** 2026-05-21

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

## Pre-fork runbook (lives in `docs/runbooks/zk-fork-activation.md` — to be written)

Sketch:

1. T-8w: testnet activation.
2. T-7w through T-1w: bug-bash sprints, every subsystem.
3. T-1w: signed announcement, exact `H` published.
4. T-24h: validators upgrade.
5. T-1h: final dry-run on testnet.
6. T: activation.
7. T+1h: post-activation smoke (every RPC method, every precompile).
8. T+24h: post-mortem if needed.
