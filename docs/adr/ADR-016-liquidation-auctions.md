# ADR-016: Sealed-Bid Liquidation Auctions

**Status:** Accepted (scaffold landed in `crates/core/src/liquidation_auction.rs`)
**Date:** 2026-05-21
**Supersedes:** `PrimeOrdersState::is_liquidatable(addr)` /
`PrimeOrdersState::liquidate(addr)` in `crates/core/src/prime_orders.rs`.

## Context

The legacy liquidation flow is open-scan-and-front-run:

1. Any observer queries `is_liquidatable(addr)` for every address.
2. Whoever finds a liquidatable position first races to call
   `liquidate(addr)`.
3. The address is publicly named in the resulting event, doxing the
   victim and exposing them to follow-on attacks.

Under shielded state (ADR-014), there are no addresses to scan
against, so the legacy entry points are not just leaky — they are
**impossible**. We need a new mechanism.

## Decision

Replace the open-scan model with a **sealed-bid auction by bonded
liquidators**, using ZK claims and executes:

1. **Bonded liquidator set.** Anyone can deposit `MIN_LIQUIDATOR_BOND`
   PRIM via a precompile to become a liquidator. The bond is
   slashable for false claims.
2. **Liquidate-claim proof** (`Circuit::LiquidateClaim`). A
   liquidator proves "there exists some position note in the tree
   whose maintenance equity is negative at the oracle price P",
   without revealing which note. The proof emits a `claim_tag =
   Poseidon(victim_commitment, P)` for deduplication.
3. **Sealed bids.** The claim carries a threshold-encrypted bid
   value. Bids decrypt at the end of the block.
4. **Highest bid wins.** The auction settles at the next block
   boundary. The winning liquidator is expected to submit a
   `LiquidationExecute` proof within `BID_REVELATION_DELAY + N`
   blocks; otherwise their bid is slashed and the second-highest
   bid wins.
5. **Liquidate-execute proof** (`Circuit::LiquidateExecute`). The
   winner consumes the victim's position note, mints a bounty note
   for themselves, and a top-up note for the insurance fund. The
   victim's identity is never published on-chain.
6. **Bid revelation deadline.** All losing bids are revealed one
   block after settlement. Repeated patterns of suspiciously-low
   bids are observable, and the chain can slash colluders via
   governance vote.

## Consequences

**Positive.**
- Victim addresses never appear on-chain.
- Liquidator competition is on bid value, not on speed of
  scanning the mempool.
- The insurance fund grows automatically as a fraction of every
  liquidation, paid out via a separate note that is not linkable to
  the bounty.

**Negative.**
- Liquidator UX is heavier: build a ZK claim proof, encrypt the
  bid, wait for settlement, build an execute proof. ~5 seconds
  total. Acceptable because liquidators are professional actors,
  not retail.
- Bond capital is dead-weight for the liquidator until withdrawn.
  Mitigated by allowing the bond note itself to be used as
  collateral inside the shielded pool.

## Open questions

- Who decides the maintenance-margin formula off-chain so the
  claim circuit's `equity` derivation matches the chain's? Phase 7
  governance.
- What is the second-highest-bid fallback policy if the winner
  fails to execute? Phase 7 governance.
