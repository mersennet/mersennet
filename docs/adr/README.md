# Architecture Decision Records

This directory holds Mersennet's ADRs. Each ADR captures a single
decision that materially shapes the codebase, the operations posture,
or the protocol. ADRs are immutable once accepted; if a decision is
later revisited, a new ADR supersedes it and links back.

| # | Title | Status | Decision in one line |
|---|---|---|---|
| 014 | [Shielded notes](ADR-014-shielded-notes.md) | Accepted | Move trader-specific state from address-keyed maps to commitment-keyed shielded notes (Aztec model). |
| 015 | [Threshold-encrypted mempool](ADR-015-threshold-encrypted-mempool.md) | Accepted | Encrypt incoming intents under a validator-held BLS threshold key; decrypt in batches per block. |
| 016 | [Liquidation auctions](ADR-016-liquidation-auctions.md) | Accepted | Replace open `liquidate(addr)` with sealed-bid auctions for bonded liquidators. |
| 017 | [SP1 state proofs](ADR-017-sp1-state-proofs.md) | Accepted | Attach a succinct SP1 proof of re-execution to every block for light clients and the Ethereum bridge. |
| 018 | [Privacy hard fork](ADR-018-privacy-hard-fork.md) | Proposed (testnet ready) | Activate the privacy stack atomically at a fixed height on chain 7919, with chain 7920 as the rehearsal. |
| 019 | [Selective disclosure and viewing keys](ADR-019-selective-disclosure-viewing-keys.md) | Proposed | Use scoped, revocable capability tokens for self-view, delegation, and regulator disclosure without reopening public trader state. |

## How to write an ADR

1. Copy an existing ADR; keep the same five sections (Context,
   Decision, Activation/Implementation, Consequences, References).
2. Number it monotonically (next is ADR-020).
3. Open a PR. The PR description must answer:
   - What is the alternative we are *not* choosing, and why?
   - What would invalidate this decision and force a successor ADR?
4. After merge, link the ADR from the table above and from any
   touched module's doc comment.

## Cross-references

The cryptography is formally specified in
[`../security/cryptography-spec.md`](../security/cryptography-spec.md).
The operational runbooks are in [`../runbooks/`](../runbooks/). The
live workstream tracker is [`../STATUS.md`](../STATUS.md).
