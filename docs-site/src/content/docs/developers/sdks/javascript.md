---
title: "JavaScript SDK"
---

The Mersennet JavaScript SDK (`@mersennet/sdk`) provides a typed interface for the JSON-RPC API, PrimeOrders (on-chain order book), and WebSocket subscriptions.

:::tip[Privacy / shielded features]
This page covers the **transparent** surface (`eth_*` / `prime_*` / `primeorders_*`). For shielded transfers and orders, client-side Noir proving, note scanning, balance/position reconstruction, selective-disclosure reads, and migration, see the dedicated **[Shielded SDK](/developers/privacy/shielded-sdk)**.
:::

## Installation

From the Mersennet monorepo:

```bash
cd sdk
npm install
npm run build
```

For use in another project, link or publish the package:

```bash
# From your project
npm install /path/to/mersennet/sdk
# Or when published: npm install @mersennet/sdk
```

## Quick Start

```javascript
import { PrimeProvider, PrimeOrders } from "@mersennet/sdk";

const provider = new PrimeProvider("http://46.225.30.187:8545");
const orders = new PrimeOrders(provider);

// Query chain
const blockNumber = await provider.getBlockNumber();
const chainId = await provider.getChainId();
console.log("Block:", blockNumber, "Chain ID:", chainId);

// Query balance
const balance = await provider.getBalance("0x7f5ce38fb2553e95dd8ef9182a80bc219c9a0d45");
console.log("Balance (hex):", balance);

// Query order book
const book = await orders.getOrderBook(1);
console.log("Bids:", book.bids, "Asks:", book.asks);
```

## Creating a Provider

```javascript
import { PrimeProvider } from "@mersennet/sdk";

// HTTP only
const provider = new PrimeProvider("http://46.225.30.187:8545");

// With WebSocket URL for subscriptions
const providerWithWs = new PrimeProvider(
  "http://46.225.30.187:8545",
  "ws://46.225.30.187:8546"
);
```

## Querying Balances

```javascript
const balance = await provider.getBalance("0xYourAddress");
// Returns hex string, e.g. "0xde0b6b3a7640000"
const wei = BigInt(balance);
console.log("Balance in wei:", wei.toString());
```

## Sending Transactions

Use `eth_sendTransaction` (requires the node to have the account unlocked):

```javascript
const txHash = await provider.sendTransaction({
  from: "0x...",
  to: "0x...",
  value: "0xde0b6b3a7640000", // 1 PRIM in hex
  gas: "0x5208",
  gasPrice: "0x3b9aca00",
});
console.log("Tx hash:", txHash);
```

:::tip
For transactions from a wallet (e.g., browser), use ethers.js or viem with the Mersennet RPC. The SDK's `sendTransaction` is for server-side flows with unlocked accounts.
:::

## Interacting with PrimeOrders

PrimeOrders is Mersennet's on-chain order book (CLOB). Use it for limit orders, positions, and collateral.

### Get Order Book

```javascript
const book = await orders.getOrderBook(1);
// book: { bids: [{ price, size }], asks: [{ price, size }] }
console.log("Best bid:", book.bids[0]);
console.log("Best ask:", book.asks[0]);
```

### Get Open Orders

```javascript
const openOrders = await orders.getOpenOrders("0xOwnerAddress");
console.log("Open orders:", openOrders);
```

### Get Position

```javascript
const position = await orders.getPosition("0xOwnerAddress", 1);
// position: { size: string, entry_price: string }
console.log("Position size:", position.size, "Entry:", position.entry_price);
```

### Get Collateral

```javascript
const collateral = await orders.getCollateral("0xOwnerAddress");
console.log("Collateral:", collateral);
```

### Submit Order (RPC)

Requires the RPC node to have the owner account unlocked:

```javascript
const outcome = await orders.submitOrder(
  "0xOwnerAddress",
  1,           // marketId
  "buy",       // side: "buy" | "sell"
  "1000000",   // price
  "10",        // size
  "gtc"        // tif: "gtc" | "ioc" | "fok"
);
console.log("Order ID:", outcome.order_id);
console.log("Filled:", outcome.filled);
console.log("Trades:", outcome.trades);
```

### Deposit Collateral (RPC)

```javascript
await orders.depositCollateral("0xOwnerAddress", "1000000000000000000");
```

### Withdraw Collateral (Precompile)

Sends a transaction to the CLOB precompile:

```javascript
await orders.withdrawCollateral("0xOwnerAddress", "500000000000000000");
```

### Use Precompile Directly

For low-level control, use the precompile encoders:

```javascript
import {
  PrimePrecompile,
  encodePlaceOrder,
  encodeGetPosition,
  encodeGetCollateral,
} from "@mersennet/sdk";

// Encode call data for eth_call
const getPositionData = encodeGetPosition(1);
const result = await provider.call({
  from: "0x...",
  to: PrimePrecompile.ADDRESS, // 0x0000000000000000000000000000000000000100
  data: getPositionData,
});

// Encode place order for transaction
const placeOrderData = encodePlaceOrder(
  1,      // marketId
  true,   // isBuy
  BigInt(1000000),  // price
  BigInt(10),       // size
  0       // tif: 0=GTC, 1=IOC, 2=FOK
);
```

## WebSocket Subscriptions

```javascript
import { PrimeProvider, PrimeSubscription } from "@mersennet/sdk";

const provider = new PrimeProvider(
  "http://46.225.30.187:8545",
  "ws://46.225.30.187:8546"
);
const sub = new PrimeSubscription(provider);

await sub.connect();

// New blocks
const blockSubId = await sub.onNewBlock((block) => {
  console.log("New block:", block.number);
});

// PrimeOrders trades
const tradeSubId = await sub.onTrade((trade) => {
  console.log("Trade:", trade);
}, 1); // optional: market ID

// Order book updates
const bookSubId = await sub.onBookUpdate((update) => {
  console.log("Book update:", update);
}, 1);

// Unsubscribe
sub.unsubscribe(blockSubId);
```

:::note
WebSocket may not be enabled on all nodes. If subscriptions fail, use HTTP polling.
:::

## Full API Reference

| Class | Method | Description |
|-------|--------|-------------|
| `PrimeProvider` | `getBlockNumber()` | Latest block number |
| | `getBalance(address)` | Account balance (hex) |
| | `getTransactionCount(address)` | Nonce |
| | `getBlockByNumber(n, full)` | Block by number |
| | `getBlockByHash(hash, full)` | Block by hash |
| | `getTransactionReceipt(hash)` | Transaction receipt |
| | `call(tx)` | eth_call |
| | `estimateGas(tx)` | eth_estimateGas |
| | `sendTransaction(tx)` | eth_sendTransaction |
| | `getChainId()` | Chain ID |
| | `getGasPrice()` | Gas price (hex) |
| | `getCode(address)` / `getCodeHash(address)` | Contract bytecode / code hash |
| | `getCodeAttestation(address)` | Code-publication attestation |
| | `getStorageAt(address, slot)` | Storage slot value |
| | `viewNotes(grantIdHex, opts?)` | Grant-gated encrypted notes (selective disclosure) |
| | `viewBalances(grantIdHex, opts?)` | Grant-gated notes + spent nullifiers for reconstruction |
| | `viewPositions(grantIdHex)` / `viewOrders(grantIdHex)` | Grant-gated trading reconstruction context |
| `PrimeOrders` | `getOrderBook(marketId)` | Order book |
| | `getOpenOrders(owner)` | Open orders |
| | `getPosition(owner, marketId)` | Position |
| | `getCollateral(owner)` | Collateral |
| | `submitOrder(...)` | Submit order |
| | `cancelOrder(orderId)` | Cancel order |
| | `depositCollateral(owner, amount)` | Deposit |
| | `withdrawCollateral(owner, amount)` | Withdraw |
| | `isLiquidatable(owner)` | Liquidation check |
