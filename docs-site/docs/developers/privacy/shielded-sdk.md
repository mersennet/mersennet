---
sidebar_position: 1
title: "Shielded SDK"
description: "Client-side privacy primitives in the Mersennet JavaScript SDK: proving, note scanning, reconstruction, selective disclosure, and migration."
---

# Shielded SDK

The Mersennet JavaScript SDK (`@prime-chain/sdk`) ships a **shielded surface** for wallets and apps that connect to a chain with the ZK privacy hard fork activated. It covers client-side proving, note scanning, balance/position/order reconstruction, selective-disclosure reads, and migration.

All shielded crypto runs **client-side**. The node is never asked to decrypt your data; it only verifies proofs and gates authorized reads. See [Privacy on Mersennet](../../privacy/overview.md) for the conceptual model.

## Install

```bash
npm install @prime-chain/sdk
# or, from a checkout:
npm install /path/to/prime-chain/sdk
```

## Modules at a glance

| Area | Exports |
|---|---|
| Shielded client | `ShieldedClient`, `ViewingKeyHelpers`, `createOwnerViewingMaterial`, `createMockNoteDecryptor` |
| Note scanning | `scanGrantedNotes`, `parseEncryptedNotePayload`, `parseShieldedNotePlaintext` |
| Reconstruction | `reconstructPortfolio`, `scanAndReconstructBalances`, `defaultNullifierDeriver` |
| Positions & orders | `reconstructPositions`, `reconstructOpenOrders` |
| Client-side proving | `NoirWasmProver` |
| Migration | `planMigration`, `confirmMigration`, `deriveMigrationNote`, `matchesMigrationNote`, `defaultNoteCommitment` |

## Client-side proving (`NoirWasmProver`)

Shielded transactions require a Noir proof generated in the wallet, so keys never leave the device. `NoirWasmProver` wraps a proving backend that the wallet injects (built on `@noir-lang/noir_js` + `@aztec/bb.js`).

```ts
import { NoirWasmProver } from '@prime-chain/sdk';

const prover = new NoirWasmProver({ backend });   // backend: NoirProvingBackend
// Typed circuit helpers map to named Noir circuit inputs:
const proof = await prover.proveOrderPlace(orderInputs);
// also available: prover.proveSpend(...), prover.proveOutput(...)
```

Relevant types: `NoirCircuitName`, `NoirInputValue`, `NoirProvingBackend`, `NoirWasmProverOptions`.

## Note scanning & balance reconstruction

Rebuild private balances locally from encrypted notes (see [Note scanning](../../privacy/note-scanning.md)):

```ts
import { scanAndReconstructBalances } from '@prime-chain/sdk';

const portfolio = await scanAndReconstructBalances(
  provider,          // PrimeProvider
  viewingMaterial,   // GrantedViewingMaterial (owner or grantee)
  { grantIdHex },    // options: drives a paged prime_viewBalances scan
);
// portfolio.balances → per-asset totals, spent notes excluded via nullifiers
```

Lower-level building blocks are also exported: `scanGrantedNotes` (decrypt authorized notes), `defaultNullifierDeriver` (derive nullifiers), and `reconstructPortfolio` (sum unspent notes).

Types: `Note`, `EncryptedNote`, `GrantedDecryptedNote`, `GrantedNoteScanOptions`, `GrantedNoteScanResult`, `GrantedViewingMaterial`, `ShieldedBalance`, `ViewingKey`, `PortfolioNote`, `ReconstructedPortfolio`, `NullifierDeriver`, `BalanceReconstructionResult`.

## Positions & open orders

```ts
import { reconstructPositions, reconstructOpenOrders } from '@prime-chain/sdk';

const positions = reconstructPositions({ orders, fills /* local records */ });
const openOrders = reconstructOpenOrders({ orders });
```

Types: `OrderSide`, `OrderRecord`, `FillRecord`, `OpenOrder`, `ReconstructedPosition`, `ReconstructTradingOptions`.

## Selective disclosure

A grantee uses the same reconstruction primitives, but over the data a [viewing grant](../../privacy/selective-disclosure.md) authorizes. Fetch authorized notes/records via the grant-gated RPC methods (`prime_viewNotes`, `prime_viewBalances`, `prime_viewPositions`, `prime_viewOrders`) and run `reconstructPortfolio` / `reconstructPositions` / `reconstructOpenOrders` client-side.

## Migration

Drive transparent → shielded migration with a plan-then-confirm flow (see [Migration](../../privacy/migration.md)):

```ts
import { planMigration, confirmMigration } from '@prime-chain/sdk';

const plan = planMigration(accounts);            // accounts: MigrationNoteParams[]
// plan → pre-fork per-asset totals to preview; submit each via prime_submitShield ...
const result = confirmMigration(accounts, scannedNotes); // post-fork landed-note confirmation
```

Types: `MigrationNote`, `MigrationNoteParams`, `MigrationPlan`, `MigrationConfirmation`, `NoteCommitmentHasher`.

## See also

- [Shielded JSON-RPC reference](./shielded-rpc.md) — the methods these helpers call.
- [JavaScript SDK](../sdks/javascript.md) — the transparent (eth_* / prime_* / primeorders_*) surface.
