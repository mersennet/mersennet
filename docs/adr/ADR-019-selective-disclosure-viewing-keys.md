# ADR-019: Selective Disclosure and Viewing Keys

**Status:** Proposed
**Date:** 2026-05-24
**Related:** [ADR-014](ADR-014-shielded-notes.md),
[ADR-015](ADR-015-threshold-encrypted-mempool.md),
[ADR-016](ADR-016-liquidation-auctions.md),
[ADR-017](ADR-017-sp1-state-proofs.md),
[ADR-018](ADR-018-privacy-hard-fork.md).

## Context

The post-fork privacy perimeter now removes most transparent trader
metadata from the public RPC surface. That is necessary, but not
sufficient.

Mersennet still needs a way for the following actors to inspect
shielded state without reopening public account-level leakage:

1. The owner of a shielded account.
2. A delegated wallet, custodian, or back-office system.
3. A regulator, auditor, or court-appointed receiver operating under a
   narrowly scoped legal process.

The current branch has placeholder RPC methods
`prime_viewGrantToken` and `prime_viewRevokeToken`, but no repo-level
decision for what a viewing key is, how delegation works, how
revocation works, or what the node is allowed to reveal.

Without a concrete model, "selective disclosure" remains a slogan and
the SDK / wallet workstream has no stable target.

## Decision

Adopt a **capability-based viewing-key model** with three principles:

1. **No server-side plaintext portfolio API.** Nodes do not maintain or
   reveal decrypted per-account balances, positions, or note sets.
   Disclosure recipients reconstruct state client-side from encrypted
   note streams, nullifiers, fills, auction outcomes, and other
   already-authorized artifacts.
2. **Delegation is explicit, scoped, and revocable.** The owner issues a
   signed disclosure grant to a specific grantee public key with a
   bounded scope and expiry.
3. **There is no privileged backdoor.** Regulators and auditors use the
   same grant mechanism as any other delegate; the difference is scope,
   policy, and operational process, not secret chain-side powers.

### Key hierarchy

Each shielded identity derives distinct keys from one wallet seed:

- `spend_sk` — authorizes spends and order actions.
- `ivk` — incoming viewing key for scanning and decrypting notes owned
  by the identity.
- `nvk` — nullifier viewing key for linking owned notes to spent
  nullifiers.
- `dsk` — disclosure signing key used only to mint and revoke viewing
  grants.

The owner self-view path uses `ivk + nvk` locally. Those keys never need
to be sent to the RPC server.

### Grant token model

`prime_viewGrantToken` will mint a signed **grant token** bound to a
specific grantee public key.

Canonical fields:

- `version`
- `chain_id`
- `grant_id` (unique random nonce or hash)
- `grantor_commitment` (shielded identity commitment, not EOA address)
- `grantee_pubkey`
- `scope`
- `start_block`
- `end_block` or wall-clock expiry
- `capabilities_hash` (hash of the exact scope payload)
- `signature` by `dsk`

The grant token is portable: the grantor can hand it directly to the
grantee out of band, or the SDK can submit it to the chain so the node
network can verify revocation state consistently.

### Scope model

Scopes are additive and must be explicit. The initial supported scopes
should be:

- `notes:read` — read encrypted notes addressed to the grantor identity.
- `balances:read` — derive current shielded balances from owned notes and
  nullifiers.
- `positions:read` — reconstruct open positions, realized PnL, and
  maintenance state.
- `orders:read` — reconstruct owned shielded order lifecycle data.
- `liquidations:read` — inspect liquidation-claim and settlement data
  tied to the grantor's state.
- `exports:portfolio_digest` — allow an SDK-generated summarized export
  rather than raw note history.

The chain must reject wildcard scopes in the first version. Broad
institutional or regulator access is represented by a token carrying
multiple explicit scopes, not a magic "admin" bit.

### Revocation model

`prime_viewRevokeToken` records a revocation commitment on chain using
`grant_id` (or its hash). Nodes treat a revoked token as invalid from the
first block after inclusion.

Revocation rules:

- Revocation is grant-specific, not global to the identity.
- Expired grants are invalid even if never explicitly revoked.
- Wallet rotation is handled by issuing a new grant to a new
  `grantee_pubkey` and revoking the old one.
- Emergency compromise response is "revoke all outstanding grants for
  this identity" via batched revocations, not key disclosure.

### RPC shape

The view-key flow is split into two responsibilities:

1. **Capability lifecycle RPC**
   - `prime_viewGrantToken(...)` returns a signed grant token.
   - `prime_viewRevokeToken(grantId)` publishes revocation.
2. **Scope-limited data RPC**
   - future `prime_view*` methods accept a grant token and return only
     the encrypted/indexed material required for the declared scopes.

Those future read methods must satisfy two constraints:

- they must not return more data than the token scope allows
- they must not require the node to decrypt the portfolio on behalf of
  the caller

### Regulator / auditor flow

There is no protocol-level regulator bypass.

The supported institutional disclosure model is:

1. Grantor receives a valid legal/compliance request.
2. Grantor wallet (or custody operator) issues a time-bounded token to a
   regulator-controlled `grantee_pubkey` with explicit scopes.
3. Regulator reconstructs the authorized state locally.
4. Grantor revokes the token when the disclosure window ends.

If a future jurisdiction requires mandatory disclosure without the
grantor's cooperation, that would be a new protocol decision and must
supersede this ADR. It is not part of the current architecture.

## Activation / Implementation

Implementation is phased:

1. **Spec freeze**
   - finalize the grant token schema
   - finalize scope names and canonical serialization
   - define signature domain separation and chain-id binding
2. **Core / RPC**
   - implement `prime_viewGrantToken`
   - implement `prime_viewRevokeToken`
   - add on-chain revocation storage and verification
3. **SDK / wallet**
   - derive `ivk`, `nvk`, and `dsk`
   - locally reconstruct portfolio state from scoped data
   - expose UX for grant issuance, review, expiry, and revocation
4. **Acceptance coverage**
   - prove an expired or revoked token cannot read new data
   - prove scope escalation is rejected
   - prove nodes never expose decrypted account state without a valid
     scope-bound token

Initial non-goals:

- social recovery for disclosure keys
- threshold or multisig grant issuance
- server-side plaintext portfolio summaries
- forced regulator access without a grant token

## Consequences

**Positive.**
- Preserves the post-fork privacy boundary while still allowing
  legitimate self-view and delegated operational access.
- Gives SDK, wallet, and compliance work a stable contract to build
  against.
- Keeps the node simple: verify capability, serve scoped artifacts, let
  clients decrypt and reconstruct.

**Negative.**
- Wallets and SDKs become more complex because reconstruction is
  client-side, not delegated to the node.
- Operational support needs better key-management UX: expiry, rotation,
  and revocation become user-visible concepts.
- Some institutions may prefer a simpler server-side portfolio API, but
  that would weaken the privacy model and is explicitly rejected here.

## References

- [docs/internal/zk-privacy-plan.md](../internal/zk-privacy-plan.md)
- [docs/STATUS.md](../STATUS.md)
- [crates/rpc/src/rpc_shielded.rs](../../crates/rpc/src/rpc_shielded.rs)