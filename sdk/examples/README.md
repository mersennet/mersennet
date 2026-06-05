# Shielded SDK Examples

## `view-notes-end-to-end.js`

Runs a local mock JSON-RPC server that serves `prime_viewNotes`, then
uses `PrimeProvider` + `scanGrantedNotes` with a concrete decryptor to
recover a sample note.

Run from `sdk/` after `pnpm build`:

```bash
node ./examples/view-notes-end-to-end.js
```

The decryptor is intentionally example-scoped: it uses a deterministic
shared-secret + XOR scheme so the end-to-end scanner flow is runnable
today without waiting for a cross-language production viewing-key
decrypt primitive. The example now reuses the shared
`createMockNoteDecryptor(...)` helper exported by the SDK.

## `noir-prover-wiring.js` (Workstream F1)

Shows how to wire a `NoirProvingBackend` (the `@noir-lang/noir_js` +
`@aztec/bb.js` adapter) into `NoirWasmProver`, and how the typed prover
calls map to the named Noir circuit inputs. Uses a fake backend so the
wiring is runnable today; the file documents the production backend.

```bash
node ./examples/noir-prover-wiring.js
```

## `migration-drive.js` (Workstream F4)

Drives the privacy-fork migration UX: `planMigration` previews the notes
the fork will mint (pre-fork), then `confirmMigration` checks each note
landed against the scanned notes (post-fork).

```bash
node ./examples/migration-drive.js
```

## Grant-gated reconstruction reads (Workstream F5)

For balance reconstruction over a `balances:read` viewing grant, use
`provider.viewBalances(grantId)` + `scanAndReconstructBalances(...)`. The
node returns encrypted notes plus the spent-nullifier set; the spendable
balance is reconstructed client-side and never leaves the wallet. Position
and open-order reads (`provider.viewPositions` / `provider.viewOrders`,
scopes `positions:read` / `orders:read`) return the public market context +
grant binding and pair with `reconstructPositions` / `reconstructOpenOrders`
over the wallet's local order/fill records. See
`tests/scan-reconstruct.test.js` for a runnable end-to-end balance flow.