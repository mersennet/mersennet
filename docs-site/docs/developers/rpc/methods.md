---
slug: /developers/rpc/methods
sidebar_position: 2
title: "RPC Methods Reference"
---

# RPC Methods Reference

Complete reference for JSON-RPC methods supported by Prime Chain. All examples use `http://46.225.30.187:8545` as the RPC URL.

Prime Chain implements **63 RPC methods** across 7 namespaces: `eth_`, `net_`, `web3_`, `txpool_`, `prime_`, `primeorders_`, and `primebridge_`.

---

## Standard Ethereum Methods

### eth_blockNumber

Returns the current block number.

**Parameters:** None

```bash
curl -X POST http://46.225.30.187:8545 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}'
```

**Response:**

```json
{ "jsonrpc": "2.0", "id": 1, "result": "0x1234" }
```

---

### eth_chainId

Returns the chain ID (`0x1eef` = 7919).

**Parameters:** None

```bash
curl -X POST http://46.225.30.187:8545 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"eth_chainId","params":[],"id":1}'
```

---

### eth_gasPrice

Returns the current gas price in wei.

**Parameters:** None

---

### eth_maxPriorityFeePerGas

Returns the current priority fee per gas in wei.

**Parameters:** None

---

### eth_feeHistory

Returns base fee and reward percentiles for a range of recent blocks.

**Parameters:**

1. `blockCount` — Number of blocks in the range (hex)
2. `newestBlock` — Highest block number or `"latest"`
3. `rewardPercentiles` — Array of percentile values (e.g. `[25, 50, 75]`)

```bash
curl -X POST http://46.225.30.187:8545 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"eth_feeHistory","params":["0x4","latest",[25,50,75]],"id":1}'
```

---

### eth_accounts

Returns a list of addresses owned by the node (typically empty on public nodes).

**Parameters:** None

---

### eth_syncing

Returns `false` when not syncing, or a sync status object.

**Parameters:** None

---

### eth_mining

Returns whether the node is actively mining/producing blocks.

**Parameters:** None

---

### eth_protocolVersion

Returns the current protocol version.

**Parameters:** None

---

### eth_getBalance

Returns account balance in wei.

**Parameters:**

1. `address` — 20-byte address (hex)
2. `blockNumber` (optional) — Block number or `"latest"`

```bash
curl -X POST http://46.225.30.187:8545 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"eth_getBalance","params":["0x7f5ce38fb2553e95dd8ef9182a80bc219c9a0d45","latest"],"id":1}'
```

---

### eth_getTransactionCount

Returns nonce (transaction count) for an address.

**Parameters:**

1. `address` — 20-byte address (hex)
2. `blockNumber` (optional) — `"latest"` or block number

---

### eth_getCode

Returns the bytecode at an address.

**Parameters:**

1. `address` — 20-byte address (hex)
2. `blockNumber` (optional) — Block number or `"latest"`

---

### eth_getStorageAt

Returns the value at a storage slot.

**Parameters:**

1. `address` — 20-byte address (hex)
2. `slot` — Storage slot (hex)
3. `blockNumber` (optional) — Block number or `"latest"`

---

### eth_call

Executes a call without creating a transaction (read-only).

**Parameters:**

1. `callObject` — `{ from?, to, gas?, gasPrice?, value?, data? }`
2. `blockNumber` (optional) — `"latest"` or block number

```bash
curl -X POST http://46.225.30.187:8545 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"eth_call","params":[{"to":"0x079bf1207b51acda83e2e8178344f62a883f8479","data":"0x70a082310000000000000000000000007f5ce38fb2553e95dd8ef9182a80bc219c9a0d45"},"latest"],"id":1}'
```

---

### eth_estimateGas

Estimates gas for a transaction.

**Parameters:**

1. `transactionObject` — Same shape as `eth_call`

---

### eth_getBlockByNumber

Returns block by block number.

After privacy activation, this method remains public only for
header-only access. Setting `fullTransactions=true` is rejected because
full transaction metadata is not part of the post-fork public surface.

**Parameters:**

1. `blockNumber` — Block number (hex) or `"latest"`, `"pending"`
2. `fullTransactions` — `true` for full tx objects, `false` for hashes only

```bash
curl -X POST http://46.225.30.187:8545 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"eth_getBlockByNumber","params":["latest",false],"id":1}'
```

**Response:**

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "number": "0x1234",
    "hash": "0x...",
    "proposer": "0x...",
    "gas_limit": "0x...",
    "gas_used": "0x...",
    "base_fee": "0x...",
    "state_root": "0x...",
    "domain_events": [],
    "transactions": ["0x...", "0x..."]
  }
}
```

---

### eth_getBlockByHash

Returns block by block hash.

After privacy activation, this method remains public only for
header-only access. Setting `fullTransactions=true` is rejected because
full transaction metadata is not part of the post-fork public surface.

**Parameters:**

1. `blockHash` — 32-byte block hash (hex)
2. `fullTransactions` — `true` or `false`

---

### eth_getTransactionByHash

Returns transaction by hash.

After privacy activation, this method is disabled on the public RPC
surface because transaction metadata remains private.

**Parameters:**

1. `transactionHash` — 32-byte tx hash (hex)

**Response:**

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "hash": "0x...",
    "from": "0x...",
    "to": "0x...",
    "value": "0x...",
    "gas": "0x...",
    "gasPrice": "0x...",
    "input": "0x...",
    "blockNumber": "0x...",
    "blockHash": "0x..."
  }
}
```

---

### eth_getTransactionReceipt

Returns transaction receipt by hash.

After privacy activation, this method is disabled on the public RPC
surface because receipt metadata and logs remain private.

**Parameters:**

1. `transactionHash` — 32-byte tx hash (hex)

**Response:**

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "transaction_hash": "0x...",
    "block_hash": "0x...",
    "block_number": "0x...",
    "transaction_index": "0x0",
    "status": "0x1",
    "gas_used": "0x...",
    "output": "0x...",
    "logs": [],
    "contract_address": "0x..."
  }
}
```

---

### eth_getLogs

Returns logs matching a filter.

**Parameters:**

1. `filterObject` — `{ fromBlock?, toBlock?, address?, topics? }`

```bash
curl -X POST http://46.225.30.187:8545 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"eth_getLogs","params":[{"fromBlock":"0x0","toBlock":"latest","address":"0x63f7a64db6d2b965189b8b48b7435668021f6b17"}],"id":1}'
```

---

### eth_sendTransaction

Sends a transaction signed by the node.

**Parameters:**

1. `transactionObject` — `{ from, to?, value?, gas?, gasPrice?, data? }`

---

### eth_sendRawTransaction

Sends a pre-signed raw transaction.

:::caution
Prime Chain accepts **both** standard Ethereum RLP-encoded transactions (EIP-155) and its own custom binary format. MetaMask-signed transactions work natively.
:::

**Parameters:**

1. `signedTransactionData` — Hex-encoded signed transaction

---

### eth_getUncleCountByBlockNumber

Returns `"0x0"` (Prime Chain uses BFT consensus with no uncles).

**Parameters:**

1. `blockNumber` — Block number (hex) or `"latest"`

---

### eth_getUncleCountByBlockHash

Returns `"0x0"` (no uncles in BFT consensus).

**Parameters:**

1. `blockHash` — Block hash (hex)

---

## Filter Methods

Prime Chain supports stateful log/block/tx filters for polling-based event subscription.

### eth_newFilter

Creates a new log filter.

**Parameters:**

1. `filterObject` — `{ fromBlock?, toBlock?, address?, topics? }`

**Returns:** Filter ID (hex string)

---

### eth_newBlockFilter

Creates a filter that notifies when new blocks arrive.

**Parameters:** None

**Returns:** Filter ID (hex string)

---

### eth_newPendingTransactionFilter

Creates a filter that notifies when new pending transactions arrive.

**Parameters:** None

**Returns:** Filter ID (hex string)

---

### eth_getFilterChanges

Polling method for a filter. Returns changes since last poll.

**Parameters:**

1. `filterId` — Filter ID from `eth_newFilter`, `eth_newBlockFilter`, or `eth_newPendingTransactionFilter`

---

### eth_getFilterLogs

Returns all logs for a log filter (equivalent to `eth_getLogs` with the filter's params).

**Parameters:**

1. `filterId` — Filter ID from `eth_newFilter`

---

### eth_uninstallFilter

Destroys a filter. Returns `true` if the filter existed.

**Parameters:**

1. `filterId` — Filter ID (hex string)

---

## Network Methods

### net_version

Returns the network ID as a string (`"7919"`).

**Parameters:** None

---

### net_peerCount

Returns the number of connected peers.

**Parameters:** None

```bash
curl -X POST http://46.225.30.187:8545 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"net_peerCount","params":[],"id":1}'
```

---

### net_listening

Returns `true` if the node is actively listening for network connections.

**Parameters:** None

---

### web3_clientVersion

Returns the client version string.

**Parameters:** None

**Response:**

```json
{ "jsonrpc": "2.0", "id": 1, "result": "PrimeChain/0.1.0" }
```

---

## Transaction Pool

### txpool_status

Returns the number of pending and queued transactions in the mempool.

**Parameters:** None

```bash
curl -X POST http://46.225.30.187:8545 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"txpool_status","params":[],"id":1}'
```

**Response:**

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": { "pending": "0x3", "queued": "0x0" }
}
```

---

## Prime Chain–Specific Methods

These methods mirror their `eth_` counterparts but use Prime Chain's native format.

### prime_chainId

Same as `eth_chainId`.

---

### prime_blockNumber

Same as `eth_blockNumber`.

---

### prime_getBalance

Same as `eth_getBalance`.

---

### prime_gasPrice

Same as `eth_gasPrice`.

---

### prime_getCode

Same as `eth_getCode`.

---

### prime_getStorageAt

Same as `eth_getStorageAt`.

---

### prime_getTransactionCount

Same as `eth_getTransactionCount`.

---

### prime_call

Same as `eth_call`.

---

### prime_getBlockByNumber

Same as `eth_getBlockByNumber`, using Prime Chain block format.

---

### prime_getTransactionReceipt

Same as `eth_getTransactionReceipt`.

---

### prime_getTransactionByHash

Same as `eth_getTransactionByHash`.

---

### prime_sendTransaction

Same as `eth_sendTransaction`.

---

### prime_getLogs

Same as `eth_getLogs`.

---

### prime_validators

Returns the list of active validators and their stake.

**Parameters:** None

```bash
curl -X POST http://46.225.30.187:8545 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"prime_validators","params":[],"id":1}'
```

---

### prime_getDomainEvents

Returns domain events (CLOB fills, bridge operations, validator changes) for a block range.

**Parameters:**

1. `filterObject` — `{ fromBlock, toBlock, domain?, kind? }`

```bash
curl -X POST http://46.225.30.187:8545 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"prime_getDomainEvents","params":[{"fromBlock":"0x0","toBlock":"latest"}],"id":1}'
```

---

## PrimeOrders CLOB Methods

The native order book engine accessible via RPC. The CLOB precompile is at address `0x0000000000000000000000000000000000000100`.

### primeorders_addMarket

Register a new trading market on the CLOB.

**Parameters:**

1. `symbol` — Market symbol string (e.g. `"PRIM/USDC"`)
2. `tickSize` — Minimum price increment (hex)
3. `lotSize` — Minimum order size (hex)

```bash
curl -X POST http://46.225.30.187:8545 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"primeorders_addMarket","params":["PRIM/USDC","0x01","0xde0b6b3a7640000"],"id":1}'
```

**Response:**

```json
{ "jsonrpc": "2.0", "id": 1, "result": "0x1" }
```

The result is the assigned market ID.

---

### primeorders_submitOrder

Submit a limit order to the CLOB.

**Parameters:**

1. `orderObject`:
   - `owner` — Trader address
   - `market_id` — Market ID (integer)
   - `side` — `"buy"` or `"sell"`
   - `price` — Limit price (hex)
   - `size` — Order size (hex)
   - `tif` — Time-in-force: `"gtc"` (Good-Til-Cancel), `"ioc"` (Immediate-or-Cancel), `"fok"` (Fill-or-Kill)

```bash
curl -X POST http://46.225.30.187:8545 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"primeorders_submitOrder","params":[{"owner":"0x7f5ce38fb2553e95dd8ef9182a80bc219c9a0d45","market_id":2,"side":"buy","price":"0xfde8","size":"0x64","tif":"gtc"}],"id":1}'
```

**Response:**

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "order_id": "0x42",
    "filled": "0x0",
    "remaining": "0x64",
    "trades": []
  }
}
```

If the order crosses existing resting orders, `trades` will contain fill details and `filled` will be non-zero.

---

### primeorders_cancelOrder

Cancel an open order.

**Parameters:**

1. `orderId` — Order ID (hex string)

**Returns:** `true` on success.

---

### primeorders_getOrderBook

Get the current order book for a market.

**Parameters:**

1. `marketId` — Market ID (hex string)

```bash
curl -X POST http://46.225.30.187:8545 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"primeorders_getOrderBook","params":["0x2"],"id":1}'
```

**Response:**

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "bids": [
      { "price": "0xfde8", "size": "0x64" }
    ],
    "asks": [
      { "price": "0xfe0c", "size": "0xc8" }
    ]
  }
}
```

Bids are sorted ascending (best bid last), asks are sorted ascending (best ask first).

---

### primeorders_getOpenOrders

Get all open orders for a specific address.

**Parameters:**

1. `address` — Trader address (hex)

---

### primeorders_depositCollateral

Deposit collateral for margin trading.

**Parameters:**

1. `address` — Account address
2. `amount` — Amount in wei (hex)

---

### primeorders_setMarginParams

Set margin parameters for a market.

**Parameters:**

1. `paramsObject` — `{ initialMarginBps, maintenanceMarginBps }`

---

### primeorders_isLiquidatable

Check if an account's position can be liquidated.

**Parameters:**

1. `address` — Account address

---

### primeorders_liquidate

Liquidate an under-collateralized position.

**Parameters:**

1. `address` — Account to liquidate

---

## PrimeBridge Methods

Cross-domain bridge between the EVM execution environment and the PrimeOrders CLOB.

### primebridge_enqueueOrdersToEvm

Queue an asset transfer from the CLOB domain to the EVM domain.

**Parameters:**

1. `bridgeObject` — `{ from, to, amount }`

---

### primebridge_enqueueEvmToOrders

Queue an asset transfer from the EVM domain to the CLOB domain.

**Parameters:**

1. `bridgeObject` — `{ from, to, amount }`

---

### primebridge_dequeueOrdersToEvm

Process pending bridge transfers from CLOB to EVM.

**Parameters:** None

---

### primebridge_dequeueEvmToOrders

Process pending bridge transfers from EVM to CLOB.

**Parameters:** None

---

## Active Markets

The following CLOB markets are registered on the testnet:

| Market ID | Symbol | Description |
|-----------|--------|-------------|
| 2 | PRIM/USDC | PRIM priced in USDC |
| 3 | PRIM/USDT | PRIM priced in USDT |
| 4 | PRIM/DAI | PRIM priced in DAI |
| 5 | BTC/USDC | BTC priced in USDC |
| 6 | ETH/USDC | ETH priced in USDC |

---

## Method Summary

| Namespace | Count | Description |
|-----------|-------|-------------|
| `eth_` | 30 | Standard Ethereum JSON-RPC |
| `net_` | 3 | Network status |
| `web3_` | 1 | Client info |
| `txpool_` | 1 | Mempool status |
| `prime_` | 15 | Prime Chain native equivalents |
| `primeorders_` | 9 | CLOB order book engine |
| `primebridge_` | 4 | Cross-domain bridge |
| **Total** | **63** | |
