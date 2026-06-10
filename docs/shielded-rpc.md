# Shielded JSON-RPC + WebSocket Reference

**Audience:** SDK authors, wallet developers, indexers.
**Chain:** 7920 (privacy testnet), 7919 (transparent testnet), and 13370 (mainnet, post-hard-fork).
**Pre-activation:** All mutation methods return `-32605` ("disabled
in current chain mode"). Read methods return zero/empty values until
the activation height is reached.

For the formal cryptographic definitions of every blob below, see
[`security/cryptography-spec.md`](security/cryptography-spec.md).

---

## 1. Encoding convention

All opaque ZK payloads — proofs, encrypted blobs, intent envelopes —
are encoded as `bincode` over the typed Rust struct, then 0x-hex.
A wallet that imports the [`mersennet SDK`](../sdk/) gets these
encoders for free.

Field names use camelCase in JSON to match Ethereum conventions.

---

## 2. Shielded RPC methods

### `prime_getChainConfig()` *(planned — not yet implemented; use `mersennetId` / `eth_chainId` today)*

Returns the chain ID, privacy activation height, and feature flags.

```json
{
  "chainId": "0x1ef0",            // 7920
  "privacyModeActivated": false,
  "privacyActivationHeight": 100,
  "dkg": { "epochLengthBlocks": 1800, "k": 5, "n": 7 }
}
```

### `prime_getShieldedRoot()`

Returns the current note commitment tree root.

```json
{ "root": "0x…32 bytes…", "blockNumber": "0x..." }
```

### `prime_getShieldedBalance({ viewKey, accountTag })`

Wallet-side helper: scans the tree for notes owned by the
account tag derived from the viewing key. Returns the cumulative
balance per asset. The chain does *not* learn the address — the
match happens client-side; this RPC is convenience-only.

### `prime_getShieldedMarketAggregates(marketId)`

Public market-level stats: last clearing price, last matched size,
intent count for the most recent FBA tick.

### `prime_submitShieldedTransfer({ shieldedTransferBincodeHex })`

Submit a shielded P2P transfer. The payload is a `ShieldedTransferTx`
carrying input nullifiers, output commitments, and a Noir proof.

### `prime_submitShield({ shieldBincodeHex })`

Cross the transparent → shielded bridge. The payload carries the
transparent EOA, amount, and the new shielded note commitment.

### `prime_submitUnshield({ unshieldBincodeHex })`

Cross the shielded → transparent bridge. The payload carries the
nullifier of the spent shielded note, the recipient EOA, and the
amount.

### `prime_submitShieldedOrder({ anchorRootHex, nullifierHex, newCommitmentHex, marketId, side, price, size, ownerPkHex, saltHex, tif?, gasLimit?, maxFeePerGas?, proofBytesHex? })`

Submit a shielded order intent. The RPC builds the canonical
`ThresholdOrderIntent { tx, intent }`, wraps the `OrderPlace` proof
bytes into a `CircuitProof` envelope when provided (or falls back to a
mock proof in the current SDK path), encrypts the canonical payload via
the threshold mempool, and returns the resulting `intentId`. The
decrypted payload is routed through `run_shielded_tick()` at the next
block boundary and admitted into the shielded CLOB.

### `prime_submitLiquidationClaim({ claimBincodeHex })`

Bonded-liquidator-only. Claims that a position is liquidatable at
the current oracle price. The bid is threshold-encrypted.

### `prime_submitLiquidationExecute({ executeBincodeHex })`

The auction winner submits the execution payload. Settles the
victim's nullifier, mints the bounty + insurance commitments.

### `prime_registerLiquidator({ bondCommitment, bondAmount })`

One-time registration with a Pedersen bond commitment. Requires
`bondAmount >= MIN_LIQUIDATOR_BOND` (10,000 MRSN at 18 decimals).

### `prime_getStateProof(blockNumberOrTag?)`

Returns the SP1 state-transition proof for a specific block (or
`"latest"`).

Parameter forms:

- `prime_getStateProof([])` — latest
- `prime_getStateProof(["latest"])` — latest
- `prime_getStateProof(["0x1f4"])` — block 500
- `prime_getStateProof([500])` — block 500

Response:

```json
{
  "blockHeight": 500,
  "prevStateRoot": "0x…",
  "newStateRoot":  "0x…",
  "prevNullifierRoot": "0x…",
  "newNullifierRoot":  "0x…",
  "blockHash":     "0x…",
  "newMarketStateHash": "0x…",
  "txCount": 12,
  "proofBincodeHex": "0x…",
  "proofType": "SP1"
}
```

If the block does not carry a proof (pre-activation or missing),
the response is `{ "blockHeight": …, "proof": null, "reason": … }`.

### `prime_getLatestStateProof()`

Convenience alias for `prime_getStateProof(["latest"])`.

### `prime_verifyStateProof({ proofBincodeHex })`

Stateless verifier; returns `{ "ok": true|false }`.

---

## 3. WebSocket subscriptions

Subscribe via either standard `eth_subscribe` (Ethereum-style) or
`prime_subscribe` (Prime-specific). Both return a hex subscription
ID; events are pushed as JSON-RPC notifications under method
`eth_subscription` / `prime_subscription`.

### Ethereum-style (`eth_subscribe`)

- `newHeads`
- `newPendingTransactions`
- `logs { address?, topics? }`

### Prime-specific (`prime_subscribe`)

| Subscription | Params | Payload shape |
|---|---|---|
| `PrimeOrdersTrades` | `(marketId?)` | `{ marketId, price, size, side, ts }` |
| `PrimeOrdersBook` | `(marketId)` | `{ bids, asks, ts }` |
| `BatchAuctionResults` | `(marketId?)` | `{ marketId, clearingPrice, matchedSize, intentCount }` |

#### Privacy-mode subscriptions (added in C3)

All payloads are **address-free by construction**; the CI K2 grep
enforces that no address fields leak into shielded events.

| Subscription | Params | Payload shape |
|---|---|---|
| `newShieldedRoot` | `()` | `{ blockNumber, newRoot, notesAdded, nullifiersAdded }` |
| `newClearingPrice` | `(marketId?)` | `{ marketId, clearingPrice, matchedSize, intentCount }` |
| `newAuctionSettled` | `(marketId?)` | `{ marketId, winnerBondCommitment, winningBid }` |
| `newStateProof` | `()` | `{ blockNumber, prevStateRoot, newStateRoot, blockHash, txCount, proofType }` |

Example subscription:

```bash
wscat -c ws://localhost:8546
> {"jsonrpc":"2.0","id":1,"method":"prime_subscribe","params":["newShieldedRoot"]}
< {"jsonrpc":"2.0","id":1,"result":"0x1"}
< {"jsonrpc":"2.0","method":"prime_subscription","params":{"subscription":"0x1","result":{"blockNumber":"0x65","newRoot":"0x…","notesAdded":"0x1","nullifiersAdded":"0x0"}}}
```

---

## 4. Error codes

| Code | Meaning |
|---|---|
| `-32700` | Parse error (invalid JSON) |
| `-32601` | Method not found |
| `-32602` | Invalid params (malformed envelopes, missing fields, bad bincode) |
| `-32604` | Forbidden (viewing-key grant missing, expired, or revoked) |
| `-32605` | **Method disabled in current chain mode** (privacy mode inactive) |
| `-32000` | Internal / engine rejection — proof rejected, stale anchor root, double-spend (nullifier already in the set), unregistered or under-bonded liquidator; the specific reason is in the error message |
| `-32005` | Transaction rejected (with `reason` in data) |

---

## 5. See also

- [`security/cryptography-spec.md`](security/cryptography-spec.md) — formal protocol.
- [`security/privacy-invariants.md`](security/privacy-invariants.md) — what cannot leak.
- [`adr/ADR-014-shielded-notes.md`](adr/ADR-014-shielded-notes.md) — design.
- [`adr/ADR-018-privacy-hard-fork.md`](adr/ADR-018-privacy-hard-fork.md) — activation.
- TypeScript SDK reference — `sdk/` (encoders + decoders for every payload).
