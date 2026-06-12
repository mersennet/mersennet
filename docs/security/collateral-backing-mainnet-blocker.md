# MAINNET BLOCKER: CLOB collateral is not backed by native MRSN

**Status:** open · **Severity:** critical (mainnet) · **Filed:** 2026-06-12

## Summary

`mersennet_orders` collateral is currently **unbacked**: depositing collateral
credits an internal balance without removing any native MRSN from the caller,
and (until the fix in this changeset) any RPC client could credit any address.
This is acceptable for the testnet faucet model but **must be fixed before
mainnet**, where collateral has to be 1:1 backed by escrowed native MRSN.

## Evidence

- `crates/core/src/mersennet_orders.rs` — `deposit_collateral(owner, amount)`
  simply does `account.collateral += amount`. No native balance is touched.
- `crates/core/src/precompiles.rs` — `handle_deposit_collateral(input, gas, caller)`
  reads the **calldata** amount and calls `state.deposit_collateral(caller, amount)`.
  It does **not** consult `env.tx.value`, and the precompile signature has **no
  mutable access to the EVM journaled state**, so it cannot debit the caller's
  native balance from where it currently sits.
- `withdraw_collateral` symmetrically credits the internal balance back without
  paying out native MRSN.

So today: deposit calldata mints collateral from nothing; the web tx correctly
sends `value: 0x0` because msg.value is ignored.

## Mitigation already shipped (this changeset)

`mersennet_orders.allow_unsigned_orders_rpc` (default **true** on testnet, set
**false** in `mainnet/genesis.json` + `mainnet/config/config.json.example`)
gates the unsigned, owner-spoofable state-mutating RPC methods
(`addMarket`, `submitOrder`, `cancelOrder`, `depositCollateral`,
`setMarginParams`, `liquidate`). With it off, those can no longer be called
over JSON-RPC; orders must arrive as signed txs to the CLOB precompile. Read
queries (`getOrderBook`, `getOpenOrders`, `isLiquidatable`) remain open.

This closes the "anyone credits/trades as anyone over RPC" hole, but does **not**
make collateral backed — the precompile deposit path still mints from calldata.

## Required mainnet fix (needs design + live testing — NOT done here)

Make collateral 1:1 backed by native MRSN. Two viable designs:

1. **msg.value deposit.** Give the precompile call frame access so the deposit
   tx carries `value = amount`; the EVM transfers native MRSN into the precompile
   account and the handler credits `collateral = call value`. `withdraw` pays
   native MRSN back out via the journaled state. Requires plumbing the call value
   and a host/journal handle into `mersennet_orders_precompile` (today it only
   receives `&Bytes`, `gas`, `&Env`). The web/API deposit path must then send
   `value = amount` instead of `0x0` (`trade/api/src/routes/collateral.js`,
   `trade/web/src/lib/vault.ts`).
2. **Balance-debit in the handler.** Debit `amount` from the caller's native
   balance inside the handler (also needs journaled-state access) and hold it in
   a module-owned escrow account.

Either way this is a consensus-affecting change and must be exercised on a live
node (deposit → trade → withdraw round-trip, conservation-of-value invariant)
before shipping. It was deliberately **not** implemented blind in this changeset
because it cannot be verified end-to-end without a running node.

## Acceptance criteria

- Depositing N MRSN reduces the caller's native balance by N and increases their
  CLOB collateral by N; withdrawing reverses it exactly.
- Total escrowed native MRSN == sum of all accounts' collateral at every block
  (add to the conservation-of-value invariant check).
- `allow_unsigned_orders_rpc = false` on mainnet (already defaulted in configs).
