---
sidebar_position: 2
title: "Shielded JSON-RPC reference"
description: "Shielded JSON-RPC and WebSocket methods for wallets, SDKs, and indexers on Mersennet."
---

# Shielded JSON-RPC reference

Reference for the shielded JSON-RPC and WebSocket surface used by wallets, SDK authors, and indexers. For the conceptual model see [Privacy on Mersennet](../../privacy/overview.md); for typed helpers see the [Shielded SDK](./shielded-sdk.md).

:::note Activation
Shielded methods are gated by the privacy hard fork. Before activation, **mutation** methods return `-32605` ("method disabled in current chain mode") and **read** methods return zero/empty values.
:::

## Encoding convention

All opaque ZK payloads — proofs, encrypted blobs, intent envelopes — are encoded as `bincode` over the typed Rust struct, then `0x`-hex. The SDK provides these encoders/decoders for free. JSON field names use camelCase to match Ethereum conventions.

## Chain & tree (read-only)

These reads are always available; before privacy activation they return zero/empty values.

### `prime_getShieldedRoot()`
Returns the current note commitment tree state: `{ shieldedStateRoot, blockNumber, noteCount, nullifierCount }`.

### `prime_getShieldedBalance()`
Returns **aggregate** shielded-pool counters — `{ totalNoteCount, totalNullifierCount, transparentEoaCount }`. The node never decrypts balances; per-account balances are reconstructed **client-side** from notes obtained via a viewing grant (see [`prime_viewBalances`](#selective-disclosure-viewing-grants) and the [Shielded SDK](./shielded-sdk.md)).

### `prime_getShieldedNotes()`
Returns `{ noteCount, currentRoot }`. Per-account note ciphertexts are fetched through grant-gated reads (`prime_viewNotes`), not this method — viewing keys are never handled server-side.

### `prime_getShieldedMarketAggregates()`
Public per-market stats for the most recent batch-auction tick: `{ markets: [{ marketId, markPrice, longOpenInterest, shortOpenInterest, lastClearingPrice, lastVolume, liquidatableCount }] }`.

## Shielded mutations

| Method | Purpose |
|---|---|
| `prime_submitShieldedTransfer({ envelopeBincodeHex })` | Private P2P transfer: input nullifiers, output commitments, Noir proof. |
| `prime_submitShield({ envelopeBincodeHex })` | Transparent → shielded: EOA, amount, new note commitment. |
| `prime_submitUnshield({ envelopeBincodeHex })` | Shielded → transparent: spent nullifier, recipient EOA, amount. |
| `prime_submitShieldedOrder({ anchorRootHex, nullifierHex, newCommitmentHex, marketId, side, price, size, ownerPkHex, saltHex, tif?, gasLimit?, maxFeePerGas?, proofBytesHex? })` | Submit a threshold-encrypted shielded order intent; returns an `intentId`. |
| `prime_submitLiquidationClaim({ claimBincodeHex })` | Bonded-liquidator-only encrypted liquidation claim. |
| `prime_submitLiquidationExecute({ executeBincodeHex })` | Auction winner settles the victim's nullifier; mints bounty + insurance. |
| `prime_registerLiquidator({ bondCommitmentHex, bondAmount })` | One-time registration with a Pedersen bond (`bondAmount >= 10,000 PRIM`). |

See [Risk checks in zero knowledge](../../privacy/zk-risk-checks.md) for the liquidation model.

## State proofs

| Method | Purpose |
|---|---|
| `prime_getStateProof(blockNumberOrTag?)` | SP1 state-transition proof for a block (or `"latest"`). Accepts `[]`, `["latest"]`, `["0x1f4"]`, or `[500]`. |
| `prime_getLatestStateProof()` | Alias for `prime_getStateProof(["latest"])`. |
| `prime_verifyStateProof({ proofBincodeHex })` | Stateless verifier; returns `{ "valid": true|false }`. |

Response shape (when a proof exists):

```json
{
  "blockHeight": 500,
  "prevStateRoot": "0x…",
  "newStateRoot":  "0x…",
  "prevNullifierRoot": "0x…",
  "newNullifierRoot":  "0x…",
  "blockHash":     "0x…",
  "newMarketStateHash": "0x…",
  "shieldedEventRoot": "0x…",
  "txCount": 12,
  "proofBincodeHex": "0x…",
  "proofType": "SP1"
}
```

If the block carries no proof, the response is `{ "blockHeight": …, "proof": null, "reason": … }`. See [Verifiable state](../../privacy/state-proofs.md).

## Selective-disclosure (viewing grants)

These methods implement the [selective-disclosure grant lifecycle](../../privacy/selective-disclosure.md). Reads are **authorization gates, not decryption oracles** — they return encrypted notes or public clearing context for the SDK to reconstruct client-side.

| Method | Scope | Purpose |
|---|---|---|
| `prime_viewGrantToken` | — | Mint a scoped, expiring viewing grant for a grantee. |
| `prime_viewRevokeToken` | — | Revoke a grant immediately. |
| `prime_viewGrantStatus` | — | Report whether a grant is active, expired, or revoked. |
| `prime_viewPortfolioDigest` | `exports:portfolio_digest` | Authorized digest of the granted portfolio. |
| `prime_viewNotes` | `notes:read` | Paginated encrypted notes a grant authorizes (for client decryption). |
| `prime_viewBalances` | `balances:read` | Encrypted notes for `reconstructPortfolio`; node never decrypts. |
| `prime_viewPositions` | `positions:read` | Public clearing context for `reconstructPositions`. |
| `prime_viewOrders` | `orders:read` | Public clearing context for `reconstructOpenOrders`. |

Pagination: balance-style reads take `(grantIdHex, [limit], [cursorHex])`.

## WebSocket subscriptions

Subscribe via `eth_subscribe` (Ethereum-style) or `prime_subscribe` (Mersennet-specific). Both return a hex subscription ID; events arrive as JSON-RPC notifications.

**Ethereum-style:** `newHeads`, `newPendingTransactions`, `logs { address?, topics? }`.

**Mersennet-specific:**

| Subscription | Params | Payload |
|---|---|---|
| `PrimeOrdersTrades` | `(marketId?)` | `{ marketId, price, size, side, ts }` |
| `PrimeOrdersBook` | `(marketId)` | `{ bids, asks, ts }` |
| `BatchAuctionResults` | `(marketId?)` | `{ marketId, clearingPrice, matchedSize, intentCount }` |
| `newShieldedRoot` | `()` | `{ blockNumber, newRoot, notesAdded, nullifiersAdded }` |
| `newClearingPrice` | `(marketId?)` | `{ marketId, clearingPrice, matchedSize, intentCount }` |
| `newAuctionSettled` | `(marketId?)` | `{ marketId, winnerBondCommitment, winningBid }` |
| `newStateProof` | `()` | `{ blockNumber, prevStateRoot, newStateRoot, blockHash, txCount, proofType }` |

Privacy-mode payloads are **address-free by construction** — CI enforces that no address fields leak into shielded events.

```bash
wscat -c ws://46.225.30.187:8546
> {"jsonrpc":"2.0","id":1,"method":"prime_subscribe","params":["newShieldedRoot"]}
< {"jsonrpc":"2.0","id":1,"result":"0x1"}
```

## Error codes

| Code | Meaning |
|---|---|
| `-32600` | Invalid request |
| `-32601` | Method not found |
| `-32602` | Invalid params |
| `-32603` | Internal error |
| `-32605` | **Method disabled in current chain mode** (privacy inactive) |
| `-32606` | Proof rejected by verifier |
| `-32607` | Stale anchor root (note tree advanced past the wallet's snapshot) |
| `-32608` | Double-spend (nullifier already in the set) |
| `-32609` | Liquidator not registered / bond below minimum |

## See also

- [Shielded SDK](./shielded-sdk.md) — typed client for these methods.
- [JSON-RPC overview](../rpc/overview.md) and [methods](../rpc/methods.md) — the transparent surface.
