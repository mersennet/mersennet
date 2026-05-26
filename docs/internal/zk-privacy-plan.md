# Prime Chain — ZK Privacy Architecture Plan (Internal)

**Status:** Active. Privacy perimeter hardening is largely landed on
`feat/zk-privacy`; selective disclosure, real proving backends, wallet
UX, and fork rehearsal remain in progress.
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

Use [docs/STATUS.md](../STATUS.md) as the live workstream tracker. The
table below is the architectural roll-up for this plan.

| # | Title | Status |
|---|---|---|
| 0 | Repo consolidation + privacy perimeter definition | mostly done |
| 1 | ZK primitives, shielded state, threshold mempool, first circuits | in progress |
| 2 | Shielded CLOB | largely landed |
| 3 | Liquidation auctions | largely landed |
| 4 | Shielded EVM accounts | partially landed |
| 5 | Real SP1 state proofs | gated on SP1 toolchain + budget |
| 6 | SDK / RPC / wallet | in progress |
| 7 | Hard fork + testnet bake | in progress |

### What is already landed on `feat/zk-privacy`

- Shielded state, shielded orders, liquidation auctions, threshold
  mempool, and the shielded EVM bridge are wired into the core engine.
- Shielded RPC and WebSocket methods are live behind the privacy
  activation switch.
- Transparent PrimeOrders RPC, WebSocket subscriptions, and precompile
  access are cut off after privacy activation.
- Transparent account-state, contract-state, simulation, log/filter,
  pending-tx, transaction metadata, receipt metadata, and tx-expanded
  block retrieval are cut off after privacy activation.
- Public contract publication now uses an opt-in code-attestation model
  (`prime_getCodeHash`, `prime_getCodeAttestation`) instead of raw
  post-fork bytecode access.
- Post-fork public block retrieval is header-only; transaction-by-hash,
  receipt-by-hash, and tx-expanded block responses are disabled.
- CI now enforces the privacy invariants and the audit lane is green.

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

---

## Phase 0.5 — Privacy perimeter / cutoff plan

Before Phase 2, 3, or 4 can be called "zk-first", the protocol needs
one explicit privacy perimeter. The current branch already contains
both transparent and shielded execution paths, so the remaining work is
less about adding more crypto and more about deciding which surfaces
remain public, which are removed, and which become selectively
disclosable.

### Current state of the codebase

Today the chain is best described as a **hybrid transparent + shielded
system**:

1. Transparent PrimeOrders has been cut off post-fork at the
  RPC / WS / precompile / event layer.
2. Transparent account-state, contract-state, simulation, log/filter,
  pending-tx, transaction metadata, receipt metadata, and tx-expanded
  block retrieval methods are cut off after privacy activation.
3. Shielded order flow, liquidation auctions, and block-level state
  proofs exist and are wired behind the privacy activation switch.
4. The shielded EVM still maintains a transparent balance mirror at the
  privacy boundary in
  [crates/core/src/shielded_evm.rs](../../crates/core/src/shielded_evm.rs),
  and the migration machinery persists `transparent_balances` through
  snapshots in
  [crates/core/src/engine.rs](../../crates/core/src/engine.rs).
5. Viewing-key infrastructure is partially landed: grant issuance,
  revocation, signature verification, `prime_viewGrantStatus`, and
  `prime_viewPortfolioDigest` are live in
  [crates/rpc/src/rpc_shielded.rs](../../crates/rpc/src/rpc_shielded.rs),
  and shield / transfer / unshield-change note ciphertexts are now
  retained in core state for wallet reconstruction flows.
6. The remaining privacy-boundary work is now concentrated in selective
  disclosure, transparent-compatibility end-state decisions, and
  acceptance coverage for the surviving public surfaces.

### Decision gate: what "real zkEVM" means for Prime Chain

We must choose one of two targets before doing major additional work.

#### Option A — privacy-first hybrid chain

Transparent smart contracts remain supported.
Shielded balances and shielded order books become the privacy-critical
trading layer.

Implications:

- Public EVM storage and `eth_call` continue to exist for transparent
  contracts.
- Shielded order flow and trader balances are private.
- This is faster to ship and fits the current architecture.
- This is **not** a private-contract zkEVM; it is a hybrid chain with a
  zk-protected trading stack.

#### Option B — private-contract zkEVM

Trader-specific execution must migrate out of transparent revm-visible
state into a private execution model.

Implications:

- `eth_getBalance`, `eth_getStorageAt`, `eth_call`, and related methods
  cannot remain globally available against private state.
- Contract storage needs a private/state-commitment model instead of the
  current transparent revm storage model.
- Gas estimation, simulation, indexing, and wallet UX all change.
- This is materially more work, but it is the only route to a chain that
  can honestly market itself as a **real zkEVM with private order books**.

**Recommendation:** treat Option A as the short-term product and Option B
as the architectural end-state. The codebase is already much closer to A
than B.

### Surface classification

| Surface | Current state | Target state |
|---|---|---|
| Block headers, roots, proofs | Public | Keep public |
| Block bodies / tx-expanded block RPC | Still exposes tx metadata | Keep header-only block access public; disable tx-expanded responses after privacy activation |
| Transaction-by-hash / receipt RPC | Still exposes tx + receipt metadata | Disable after privacy activation |
| Market aggregates, clearing price, aggregate OI | Public | Keep public |
| Transparent PrimeOrders RPC / WS / precompile | Mostly gated post-fork | Remove fully after the fork |
| Transparent PrimeOrders events | Gated / redacted post-fork | Remove fully after the fork |
| Shielded order submission | Live behind privacy mode | Keep and harden |
| Liquidation auctions | Shielded / bonded | Keep and harden |
| `eth_getBalance` / `eth_getCode` / `eth_getStorageAt` | Still public | Cut off or scope to transparent-only domain |
| `eth_call` / `eth_estimateGas` | Still public | Redesign for private state or scope to transparent-only domain |
| Viewing keys | Grant lifecycle and first scoped read landed; richer wallet reads still pending | Implement selective disclosure |
| `transparent_balances` mirror | Still present | Transitional only; phase out once migration completes |

### Concrete cutoff tasks for Point 1

#### Method-by-method RPC boundary matrix

This matrix is the approval artifact for the transparent account/state
cutoff. "Decision" is the intended post-fork behavior. "Status" tracks
whether the code already enforces that boundary on `feat/zk-privacy`.

| Method | Leaks | Decision | Replacement / scope | Status |
|---|---|---|---|---|
| `prime_getBalance` | EOA balance | Disable after privacy activation | `prime_getShieldedBalance` or view-key flow | **landed** |
| `eth_getBalance` | EOA balance | Disable after privacy activation | `prime_getShieldedBalance` or view-key flow | **landed** |
| `prime_getTransactionCount` | EOA activity / nonce | Disable after privacy activation | shielded sequencer / relayer UX, not public RPC | **landed** |
| `eth_getTransactionCount` | EOA activity / nonce | Disable after privacy activation | shielded sequencer / relayer UX, not public RPC | **landed** |
| `prime_getCodeHash` | bytecode fingerprint only | Keep public only for opt-in published contracts | compare against locally compiled bytecode hash | **landed** |
| `prime_getCodeAttestation` | published attestation metadata | Keep public only for opt-in published contracts | expose deployer, code hash, metadata URI, publish block | **landed** |
| `prime_getCode` | raw contract bytecode | Disable after privacy activation | `prime_getCodeHash` + off-chain source publication | **landed** |
| `eth_getCode` | raw contract bytecode | Disable after privacy activation | `prime_getCodeHash` + off-chain source publication | **landed** |
| `prime_getStorageAt` | contract storage | Disable after privacy activation | future view-key or private proof path if needed | **landed** |
| `eth_getStorageAt` | contract storage | Disable after privacy activation | future view-key or private proof path if needed | **landed** |
| `prime_call` | read simulation over state | Disable after privacy activation | future private simulation path or view-key flow | **landed** |
| `eth_call` | read simulation over state | Disable after privacy activation | future private simulation path or view-key flow | **landed** |
| `eth_estimateGas` | simulation over state / tx intent | Disable after privacy activation | wallet-side shielded estimation or relayer quote path | **landed** |
| `prime_getBlockByNumber(..., true)` / `eth_getBlockByNumber(..., true)` | full tx objects in block response | Disable tx-expanded form after privacy activation; keep header-only form | header-only lookup remains public for hashes, roots, fees, and timestamps | **landed** |
| `eth_getBlockByHash(..., true)` | full tx objects in block response | Disable tx-expanded form after privacy activation; keep header-only form | header-only lookup remains public for hashes, roots, fees, and timestamps | **landed** |
| `prime_getTransactionByHash` / `eth_getTransactionByHash` | tx sender, recipient, calldata, value | Disable after privacy activation | block-level hash inclusion only; future view-key flow if needed | **landed** |
| `prime_getTransactionReceipt` / `eth_getTransactionReceipt` | receipt logs, created address, gas + execution metadata | Disable after privacy activation | block-level inclusion only; future view-key flow if needed | **landed** |

Rules implied by this matrix:

1. Trader-linked balance and nonce queries are no longer part of the
  public API once privacy mode activates.
2. Raw contract bytecode and storage are not part of the post-fork
  public surface.
3. Public code attestation is allowed only for contracts whose
  recorded deployer opts in through the publication registry
  precompile. The public surface is a one-way hash lookup plus
  published metadata, so users can verify locally compiled bytecode
  without fetching the deployed code from the chain.
4. Public call simulation is not part of the post-fork privacy surface;
  any replacement must be private, wallet-scoped, or relayer-mediated.
5. Post-fork block semantics are header-only for public RPC: hashes,
  roots, fees, timestamps, and transaction hashes remain public, while
  full transaction objects and receipts do not.
6. If the project chooses Option B above, `prime_getCodeHash` can stay
  as the public attestation endpoint even after all remaining
  transparent-contract compatibility surfaces are removed.

#### 1. Transparent account/state RPC cutoff

Inventory every method that leaks private account or contract state and
classify it as:

- **remove at activation**
- **keep for transparent-only contracts**
- **replace with viewing-key flow**

The initial list is:

- `eth_getBalance` / `prime_getBalance`
- `eth_getTransactionCount` / `prime_getTransactionCount`
- `eth_getCode` / `prime_getCode`
- `eth_getStorageAt` / `prime_getStorageAt`
- `eth_call` / `prime_call`
- `eth_estimateGas`
- `eth_getTransactionByHash` / `prime_getTransactionByHash`
- `eth_getTransactionReceipt` / `prime_getTransactionReceipt`
- `eth_getBlockByNumber(..., true)` / `eth_getBlockByHash(..., true)`

#### 2. Transparent execution domain boundary

Document whether post-fork transparent revm state remains:

- a permanent public subsystem, or
- a temporary compatibility layer that cannot touch trader-specific
  balances / positions.

This must be enforced both in RPC and in execution semantics.

#### 3. Selective disclosure design

The disclosure model is defined in
[ADR-019](../adr/ADR-019-selective-disclosure-viewing-keys.md). The
remaining work is implementation and acceptance coverage. At minimum:

- owner self-view
- delegated read access for wallets / custodians
- regulator / auditor disclosure flow
- revocation and rotation

#### 4. Indexer / explorer privacy model

Specify what an indexer is allowed to know post-fork:

- headers, roots, proofs, shielded aggregate events
- no owner-linked PrimeOrders data
- no per-account balance endpoint without a viewing key

#### 5. Migration end-state

The current migration path snapshots EOAs into shielded notes, but still
keeps transparent balance plumbing for interoperability. We need a clear
end-state:

- when the transparent mirror is authoritative
- when it is compatibility-only
- when it can be removed from snapshots and engine state entirely

### Output of Point 1

Point 1 is complete only when the repo has these artifacts:

1. A method-by-method privacy matrix covering RPC, WS, precompiles,
  events, snapshots, and indexer outputs.
2. A written decision between Option A and Option B above.
3. Acceptance tests that prove post-fork private trader state cannot be
  recovered from public APIs.

Only after that should Point 2 (protocol transition design) and Point 3
(implementation sequencing) be treated as stable.

### Point 1 completion snapshot

As of the current branch state:

- The RPC / WS / precompile / event privacy boundary is mostly landed.
- The written architectural decision remains: ship Option A first, keep
  Option B as the long-term end-state.
- The remaining Point 1 gap is no longer broad public RPC leakage; it is
  the selective-disclosure design, the explicit transparent-
  compatibility boundary in execution/snapshots, and acceptance tests
  proving that private trader state cannot be reconstructed from the
  public APIs that remain.

### Point 2 preview — protocol transition design

Once the privacy perimeter is fixed, the protocol design work should
cover:

- transparent-contract compatibility rules
- shielded fee payment and relaying
- private simulation / execution UX
- viewing-key cryptography and wallet flows
- post-fork block / receipt / log semantics

### Point 3 preview — implementation sequencing

Recommended order after Point 1:

1. Extend selective disclosure from grant lifecycle into wallet-facing
  note and balance reads.
2. Remove or demote `transparent_balances` to compatibility-only.
3. Decide whether transparent revm stays permanent or becomes a bounded
  compatibility subsystem.
4. Add acceptance coverage for the surviving public metadata surfaces
  (headers, hashes, proofs, aggregates, code attestations).
5. Replace threshold-encryption scaffolding and mock proving backends
  with production crypto / proving toolchains.

### Next actions

1. Extend ADR-019 beyond the now-landed `prime_viewNotes` scoped read:
  implement balance / position / order grant-gated `prime_view*`
  methods on top of the persisted encrypted EVM note payloads, and
  decide how shielded-order and liquidation-created notes should be
  indexed for the same flows.
2. Decide and document the `transparent_balances` end-state: whether it
  stays as a compatibility mirror for transparent contracts or is fully
  removed after migration.
3. Add privacy acceptance tests that prove post-fork public APIs cannot
  recover balances, positions, transaction metadata, or receipt logs.
4. Land real proving dependencies: Barretenberg install path, Noir
  compilation flow, and SP1 prover integration.
5. Extend SDK / wallet surfaces so shielded balances, shielded orders,
  code attestations, and the eventual viewing-key flow are first-class.
6. Start the pre-mainnet bake only after D5, D6, and E* move out of the
  gated state in [docs/STATUS.md](../STATUS.md).
