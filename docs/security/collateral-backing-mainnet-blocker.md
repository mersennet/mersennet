# CLOB collateral backing — RESOLVED (precompile path)

**Status:** resolved for the precompile path · **Filed:** 2026-06-12 · **Fixed:** 2026-06-12

## What was wrong

CLOB collateral was unbacked: `depositCollateral` credited an internal balance
without removing any native MRSN, because the orders precompile was registered as
`Precompile::Env` and only received an immutable `&Env` — it had no handle to
account balances.

## Fix

The orders precompile is now a stateful `ContextStatefulMut` precompile
(`crates/core/src/precompiles.rs`), so it receives `&mut InnerEvmContext` (the
journaled state + db) and moves native MRSN:

- **depositCollateral** transfers `amount` native MRSN from the caller into the
  precompile's own address (the escrow) via `journaled_state.transfer`, then
  credits orders-side collateral. The native debit happens first; collateral is
  only credited on success, so the two ledgers never desync. If the caller can't
  cover it, the transfer fails and the call reverts (no collateral conjured).
- **withdrawCollateral** validates + decrements orders-side collateral (margin
  check), then pays the native MRSN back out of the escrow, rolling the
  decrement back if the payout somehow fails.
- The precompile is **non-payable**: it rejects nonzero `msg.value` (the deposit
  amount comes from calldata and is debited explicitly), so the existing
  `value: 0x0` web/API deposit tx is unchanged.

Native MRSN is only *relocated* (caller ↔ escrow), so the conservation-of-value
invariant is unchanged — no mint/burn.

### Verification

`crates/core/tests/collateral_backing.rs` drives a real tx through the EVM
precompile inside `execute_block` and asserts: deposit escrows the MRSN at
`0x…0100`; withdraw drains it; the caller is made whole minus gas; an unbackable
deposit reverts and escrows nothing; no conservation violation. `cargo test`
passes, and the existing orders/consensus/conservation suites still pass.

## Migration note for redeploy

- The **unsigned RPC seeding path** (`mersennet_orders_depositCollateral`, gated
  by `allow_unsigned_orders_rpc`, used by the testnet market-maker bots) still
  credits collateral *without* native backing — it is a direct engine call, not
  the precompile, and is testnet-only. Bots don't withdraw, so this is harmless,
  but any collateral created that way (or by the precompile before this change)
  has **no escrow backing** and cannot be withdrawn through the new path.
- **On redeploy, start from a fresh orders state** (or fund `0x…0100` with native
  MRSN equal to any outstanding collateral) so there is no pre-existing unbacked
  collateral whose withdrawal would hit an underfunded escrow.

## Residual / future

- `env.tx.value` is rejected, and deposits use the calldata amount debited from
  the caller — frame-agnostic and correct for direct txs and contract-initiated
  calls alike.
- The orders precompile still uses a process-global context and is not registered
  in the parallel executor; keep CLOB execution single-threaded until that's
  addressed.
