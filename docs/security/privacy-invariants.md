# Privacy Invariants

This document is the canonical rule set enforced by
`scripts/ci/check-privacy-invariants.sh` and the CI job
`privacy_invariants` (workflow `K2`).

Every rule below is **non-negotiable**. A violation means an
account-level identifier (`Address`, `from`, `to`, EOA-keyed amount)
has leaked into the shielded data path, which is the entire problem
the privacy hard fork exists to solve.

## Scope

Files in scope:

- `crates/core/src/shielded_evm.rs`
- `crates/core/src/shielded_orders.rs`
- `crates/core/src/shielded_state.rs`
- `crates/core/src/shielded_persistence.rs`
- `crates/core/src/liquidation_auction.rs`
- `crates/core/src/threshold_mempool.rs`
- All files under `crates/zkp/src/`

These files **must not** contain any of the following patterns
outside of an explicit `// privacy-allow: <reason>` allowlist comment.

## Rules

### R1 — No `HashMap<Address, ...>` in shielded modules

Shielded modules cannot store any map keyed by `Address`. Balances,
positions, orders, and notes are commitment-keyed, not EOA-keyed.

**Allowed in:** `crates/core/src/shielded_evm.rs`'s
`transparent_balances` field, which is the explicit bridge between
the transparent EVM side and the shielded side, kept in `shielded_evm`
so that the privacy boundary is visible in one place. That field is
in a struct named `transparent_*`, never `shielded_*`.

The grep is `HashMap<Address` — see
`crates/core/src/shielded_evm.rs` line that uses `transparent_balances:
HashMap<Address, U256>` for the canonical allowlist.

### R2 — No `owner: Address` field on shielded structs

Shielded notes, positions, orders, claims, and bids identify their
owner via `owner_pk: Fr` — a Poseidon-hashed BN254 public key — not
a 20-byte EOA. The proof of knowledge of the matching secret key
binds the spending right to the owner.

### R3 — No `from: Address` or `to: Address` in shielded code

Shielded transfers are commitment-to-commitment. A transfer consumes
some input commitments (proven via nullifiers) and produces some
output commitments (the new notes). It never carries plaintext
`from` or `to` addresses.

### R4 — `ShieldedEvent` variants cannot embed `address: Address`

The four public-facing shielded events
(`FbaCleared`, `MempoolBatchAdmitted`, `LiquidationSettled`,
`ShieldedRootAdvanced`) are emitted for observers (block explorers,
indexers, MEV monitors). They reveal market-level aggregates only —
clearing price, matched size, the winner's `winner_bond_commitment`
(a public commitment, not an address). An `address: Address` field
would tag the activity to a specific EOA, which is the exact privacy
violation the redesign exists to prevent.

### R5 — `prime-zkp` is address-free entirely

`crates/zkp/src/` is the cryptography crate. Its public API works
in `Fr` (BN254 scalar field), `G1`/`G2` (BN254 or BLS12-381 curve
points), and raw bytes. A bare mention of `Address` in any file
under `crates/zkp/src/` is almost always a layering bug — the
shielded EVM bridge belongs in `crates/core/src/shielded_evm.rs`,
not in the cryptography crate.

If a `prime-zkp` file genuinely needs to mention `Address` (e.g. a
docstring quoting the bridge behaviour), add a
`// privacy-allow: docstring` comment on that line.

### R6 — No `println!` / `tracing::*!("address=...")` style logs

The shielded code path must not emit a log line that tags a record
to an `address=`, `from=`, or `to=` field. Such logs would defeat
the privacy properties at runtime even if the data structures are
clean.

Allowed: aggregate `tracing` lines that carry no EOA-identifying
data. Tag them with `// privacy-allow: aggregate-only` if grep
catches one.

## How to suppress a known false positive

If the grep matches a line that is genuinely safe (e.g. a doc
comment that mentions the `Address` type by name in prose), add a
`// privacy-allow: <reason>` suffix to that line. The script skips
any line matching `privacy-allow:`. Use sparingly and justify in
the commit message.

## Running locally

```bash
bash scripts/ci/check-privacy-invariants.sh
```

Exit code is `0` on success, `1` if any rule fires.

## Audit traceability

Every rule above maps to a workstream-D6 or workstream-I review
item. The auditor's review report should reference the line numbers
of any added allowlist suppression and confirm they are safe.
