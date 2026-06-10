# ADR-014: Shielded Notes

**Status:** Accepted (Phase 1 implemented, Phase 4 wired)
**Date:** 2026-05-21
**Supersedes:** Implicitly supersedes the transparent-account model used in `crates/core/src/prime_orders.rs::AccountState`.

## Context

Account-level state on Mersennet — collateral, positions, PnL — is
publicly readable today. After October 10, 2025, large traders use
this transparency to engineer cascading liquidations: scan the chain
for accounts close to maintenance margin, push the mark away from
their entry, and harvest forced closures.

The whole point of running a perp venue on a public chain dies if
this is allowed to continue.

## Decision

Adopt a **shielded note model** for all trader-keyed state:

- The chain holds a single sparse Poseidon Merkle tree (depth 32) of
  note commitments. A note is `(value, asset_id, owner_pk, rho, psi)`;
  its commitment is `Poseidon(...)` of those fields.
- Spending a note publishes a nullifier `Poseidon(spend_sk, rho)`. The
  chain maintains a nullifier set; a re-published nullifier is a
  fatal validation error for the containing transaction.
- The chain maintains a recent-roots ring (last 64 roots) so wallets
  can prove against a slightly-stale root and still land within the
  proving-latency window.
- All trader-keyed state types — collateral, positions, order ownership
  — are represented as notes. No `HashMap<Address, ...>` for any
  trader-keyed quantity.

## Consequences

**Positive.**
- Trader identity is unobservable to a network adversary.
- Per-account collateral, position, and PnL are unobservable.
- Liquidation predation is impossible — there is no addressable
  victim to scan for.

**Negative.**
- Wallets must build ZK proofs for every transaction. 200ms–3s
  proving time on a laptop. Acceptable for manual trading; painful
  for HFT.
- Wallets must scan blocks for encrypted notes addressed to them.
  Trial-decrypt is `O(unseen-blocks * notes-per-block)`. Mitigated
  by emitting the recipient public key in plaintext so wallets can
  prefilter with one field-equality check per note.
- The mempool must threshold-decrypt every shielded intent; this
  adds a DKG dependency to consensus and a per-block decryption
  ceremony.

## Implementation status

- **Phase 1 (DONE):** `crates/zkp` primitives (Poseidon, Merkle,
  nullifier set, note schema), `crates/core/src/shielded_state.rs`.
- **Phase 2 (DONE):** `crates/core/src/shielded_orders.rs`.
- **Phase 4 (DONE):** `crates/core/src/shielded_evm.rs`.

## Open questions

- Real BLS12-381 threshold ElGamal (Phase 1.3.x). Tracked.
- Aztec parameter pinning for Poseidon (Phase 1.4). Tracked.
- Production WASM prover for browser-side proving (Phase 6.x). Tracked.
