# Prime Chain — ZK Privacy Architecture Plan (Internal)

**Status:** Active. Phase 0 + Phase 1 in progress on `feat/zk-privacy`.
**Owner:** PrimeNumbersLabs / ZK group.
**Last updated:** see `git log -1 -- docs/internal/zk-privacy-plan.md`.

This is the internal source-of-truth for the privacy redesign. The
public-facing version will be folded into `docs/whitepaper.md` v8.0
in Phase 7.

---

## Why

The problem is observable account-level state. After October 10, 2025,
large traders are reluctant to use any chain where their positions,
collateral, and proximity to liquidation are publicly readable, because
those signals are routinely used to engineer cascading liquidations.

Prime Chain's existing CLOB
([crates/core/src/prime_orders.rs](../../crates/core/src/prime_orders.rs))
leaks all of this: `Order.owner: Address`, `AccountState`,
`HashMap<Address, AccountState>`, and `is_liquidatable(addr)`.

---

## What changes

We move from address-keyed transparent state to commitment-keyed
shielded state for everything trader-specific, while keeping market-level
aggregates public. ZK is used in two distinct ways:

1. **Privacy proofs (client-side, per transaction)** — the trader's wallet
   proves "I own a note in the tree and my new state is solvent" without
   revealing identity, balance, or exact position. Stack: Noir →
   UltraPlonk → BN254.
2. **State-transition proofs (chain-side, per block)** — SP1 produces a
   succinct proof attesting that the whole block was re-executed
   correctly, for light-client and bridge consumption. Stack:
   sp1_sdk → SP1 STARK → optional Groth16 wrap.

The two never conflate. Privacy proofs do not validate the chain;
state proofs do not provide privacy.

---

## Public vs private vs selectively-disclosable

| Datum | Public | Private | Selective |
|---|:---:|:---:|:---:|
| Market id, mark price, funding rate | x | | |
| Aggregate open interest per market | x | | |
| Aggregate volume per market | x | | |
| Batch clearing price | x | | |
| Order book depth (bucketed bands) | x | | |
| Trader identity | | x | viewing-key |
| Per-account collateral | | x | viewing-key |
| Per-account positions, PnL | | x | viewing-key |
| Exact order size | | x (bands leak) | viewing-key |
| Liquidation victim identity | | x | viewing-key |
| Block headers, state roots | x | | |

---

## Modules

```
crates/zkp/                      Phase 1 — new workspace member
├── circuits/                    Noir source
│   ├── spend.nr                 prove ownership of a note
│   ├── output.nr                produce a new note commitment
│   ├── join_split.nr            combine + split notes
│   ├── order_place.nr           prove solvency for an order intent
│   ├── liquidate_claim.nr       prove some position is liquidatable
│   └── liquidate_execute.nr     consume a position note
├── prover/                      Rust bindings to Barretenberg + SP1
├── verifier/                    Rust verifier used by node + SP1 program
└── params/                      KZG SRS + verifying keys (pinned)

crates/core/src/shielded_state.rs    Phase 1 — Poseidon Merkle tree + nullifier set + recent-roots ring
crates/core/src/encrypted_mempool.rs Phase 1 — replace XOR mock with real BLS12-381 threshold ElGamal
crates/core/src/prime_orders.rs      Phase 2 — refactor from address-keyed to commitment-keyed
crates/core/src/liquidation_auction.rs Phase 3 — new
crates/core/src/precompiles.rs       Phase 4 — new precompiles 0x0200, 0x0201 for shielded EVM
crates/core/src/zk_sp1.rs            Phase 5 — swap mock for real sp1_sdk::ProverClient
```

---

## Phase status

See the active todo list in the agent's plan
(`/.cursor/plans/prime-chain-zk-privacy_*.plan.md`).

| # | Title | Status |
|---|---|---|
| 0 | Repo consolidation | in progress |
| 1 | ZK primitives, shielded state, threshold mempool, first circuits | scheduled |
| 2 | Shielded CLOB | gated |
| 3 | Liquidation auctions | gated |
| 4 | Shielded EVM accounts | gated |
| 5 | Real SP1 state proofs | gated |
| 6 | SDK / RPC / wallet | gated |
| 7 | Hard fork + testnet bake | gated |

---

## Known risks

- **Client-side proving time** of 200ms–3s per order is acceptable for
  manual trading and painful for HFT. Mitigation: precompute auxiliary
  witnesses during the FBA pre-tick window.
- **Threshold-mempool DKG faults** can halt the chain. Mitigation:
  BLS DKG with at-most-`f` faults, epoch rotation, and a transparent
  fallback mode behind `FEATURE_ENCRYPTED_MEMPOOL`.
- **Liquidator collusion** is reduced but not eliminated by sealed bids
  and bonds. Mitigation: reveal losing bids one block after settlement
  so collusion is observable and slashable.
- **Compliance.** Some jurisdictions require viewing-key disclosure.
  Selective disclosure tooling is therefore part of Phase 6, not optional.
- **"Full chain" scope** is 9–12 months. Phasing is designed so we can
  stop after Phase 3 (shielded trading + auctions) with a complete
  product, and defer Phase 4 indefinitely without breaking anything.
