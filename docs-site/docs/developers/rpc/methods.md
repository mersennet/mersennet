---
slug: /developers/rpc/methods
sidebar_position: 2
title: "RPC Methods Reference"
---

# RPC Methods Reference

Complete reference for JSON-RPC methods supported by Prime Chain. All examples use `http://46.225.30.187:8545` as the RPC URL.

---

## eth_blockNumber

Returns the current block number.

**Parameters:** None

**Example:**

```bash
curl -X POST http://46.225.30.187:8545 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}'
```

**Response:**

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": "0x1234"
}
```

---

## eth_getBlockByNumber

Returns block by block number.

**Parameters:**

1. `blockNumber` — Block number (hex) or `"latest"`, `"pending"`
2. `fullTransactions` — `true` for full tx objects, `false` for hashes only

**Example:**

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

## eth_getBlockByHash

Returns block by block hash.

**Parameters:**

1. `blockHash` — 32-byte block hash (hex)
2. `fullTransactions` — `true` or `false`

**Example:**

```bash
curl -X POST http://46.225.30.187:8545 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"eth_getBlockByHash","params":["0xblockHash",false],"id":1}'
```

---

## eth_getTransactionByHash

Returns transaction by hash.

**Parameters:**

1. `transactionHash` — 32-byte tx hash (hex)

**Example:**

```bash
curl -X POST http://46.225.30.187:8545 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"eth_getTransactionByHash","params":["0xtxHash"],"id":1}'
```

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

## eth_getTransactionReceipt

Returns transaction receipt by hash.

**Parameters:**

1. `transactionHash` — 32-byte tx hash (hex)

**Example:**

```bash
curl -X POST http://46.225.30.187:8545 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"eth_getTransactionReceipt","params":["0xtxHash"],"id":1}'
```

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

## eth_getBalance

Returns account balance in wei.

**Parameters:**

1. `address` — 20-byte address (hex)
2. `blockNumber` (optional) — Block number or `"latest"`

**Example:**

```bash
curl -X POST http://46.225.30.187:8545 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"eth_getBalance","params":["0x7f5ce38fb2553e95dd8ef9182a80bc219c9a0d45","latest"],"id":1}'
```

**Response:**

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": "0xde0b6b3a7640000"
}
```

---

## eth_getTransactionCount

Returns nonce (transaction count) for an address.

**Parameters:**

1. `address` — 20-byte address (hex)
2. `blockNumber` (optional) — `"latest"` or block number

**Example:**

```bash
curl -X POST http://46.225.30.187:8545 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"eth_getTransactionCount","params":["0x7f5ce38fb2553e95dd8ef9182a80bc219c9a0d45","latest"],"id":1}'
```

---

## eth_call

Executes a call without creating a transaction (read-only).

**Parameters:**

1. `callObject` — `{ from?, to, gas?, gasPrice?, value?, data? }`
2. `blockNumber` (optional) — `"latest"` or block number

**Example:**

```bash
curl -X POST http://46.225.30.187:8545 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"eth_call","params":[{"to":"0x079bf1207b51acda83e2e8178344f62a883f8479","data":"0x70a082310000000000000000000000007f5ce38fb2553e95dd8ef9182a80bc219c9a0d45"},"latest"],"id":1}'
```

**Response:**

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": "0x..."
}
```

---

## eth_estimateGas

Estimates gas for a transaction.

**Parameters:**

1. `transactionObject` — Same shape as `eth_call`

**Example:**

```bash
curl -X POST http://46.225.30.187:8545 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"eth_estimateGas","params":[{"from":"0x...","to":"0x...","data":"0x..."}],"id":1}'
```

---

## eth_sendTransaction

Sends a transaction. The account must be unlocked on the node (or use a wallet that signs and sends via another method).

**Parameters:**

1. `transactionObject` — `{ from, to?, value?, gas?, gasPrice?, data? }`

**Example:**

```bash
curl -X POST http://46.225.30.187:8545 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"eth_sendTransaction","params":[{"from":"0x...","to":"0x...","value":"0xde0b6b3a7640000","gas":"0x5208"}],"id":1}'
```

---

## eth_sendRawTransaction

Sends a signed raw transaction.

:::caution
Prime Chain uses a **custom raw transaction format** (not standard RLP). Standard Ethereum-signed raw transactions may not be accepted. Use `eth_sendTransaction` or `prime_sendTransaction` when possible.
:::

**Parameters:**

1. `signedTransactionData` — Hex-encoded signed transaction (Prime Chain format)

---

## eth_chainId

Returns the chain ID.

**Parameters:** None

**Example:**

```bash
curl -X POST http://46.225.30.187:8545 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"eth_chainId","params":[],"id":1}'
```

**Response:**

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": "0x1eef"
}
```

(0x1eef = 7919)

---

## eth_gasPrice

Returns current gas price in wei.

**Parameters:** None

**Example:**

```bash
curl -X POST http://46.225.30.187:8545 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"eth_gasPrice","params":[],"id":1}'
```

---

## eth_getLogs

Returns logs matching a filter.

**Parameters:**

1. `filterObject` — `{ fromBlock?, toBlock?, address?, topics? }`

**Example:**

```bash
curl -X POST http://46.225.30.187:8545 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"eth_getLogs","params":[{"fromBlock":"0x0","toBlock":"latest","address":"0x63f7a64db6d2b965189b8b48b7435668021f6b17"}],"id":1}'
```

---

## prime_sendTransaction

Prime Chain–specific method to send a transaction. Use when `eth_sendTransaction` is preferred for deployment or transaction submission.

**Parameters:**

1. `transactionObject` — Same as `eth_sendTransaction`

---

## eth_getCode

Returns the bytecode at an address (hex).

**Parameters:**

1. `address` — 20-byte address (hex)
2. `blockNumber` (optional) — Block number or `"latest"`

**Example:**

```bash
curl -X POST http://46.225.30.187:8545 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"eth_getCode","params":["0x7f5ce38fb2553e95dd8ef9182a80bc219c9a0d45","latest"],"id":1}'
```

**Response:**

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": "0x608060405234801561001057600080fd5b50..."
}
```

---

## eth_getStorageAt

Returns the value at a storage slot (hex).

**Parameters:**

1. `address` — 20-byte address (hex)
2. `slot` — Storage slot (hex)
3. `blockNumber` (optional) — Block number or `"latest"`

**Example:**

```bash
curl -X POST http://46.225.30.187:8545 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"eth_getStorageAt","params":["0x7f5ce38fb2553e95dd8ef9182a80bc219c9a0d45","0x0","latest"],"id":1}'
```

**Response:**

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": "0x0000000000000000000000000000000000000000000000000000000000000000"
}
```

---

## net_version

Returns the chain ID as a string.

**Parameters:** None

**Example:**

```bash
curl -X POST http://46.225.30.187:8545 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"net_version","params":[],"id":1}'
```

**Response:**

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": "7919"
}
```

---

## web3_clientVersion

Returns the client version string.

**Parameters:** None

**Example:**

```bash
curl -X POST http://46.225.30.187:8545 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"web3_clientVersion","params":[],"id":1}'
```

**Response:**

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": "PrimeChain/0.1.0"
}
```

---

## prime_validators

Returns the list of validators.

**Parameters:** None (or chain-specific)

**Example:**

```bash
curl -X POST http://46.225.30.187:8545 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"prime_validators","params":[],"id":1}'
```

---

## prime_getDomainEvents

Returns domain events for a block range.

**Parameters:**

1. `filterObject` — `{ fromBlock, toBlock, eventTypes? }` — Block range and optional event type filter

**Example:**

```bash
curl -X POST http://46.225.30.187:8545 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"prime_getDomainEvents","params":[{"fromBlock":"0x0","toBlock":"latest","eventTypes":[]}],"id":1}'
```

**Response:**

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": []
}
```

---

## PrimeOrders Methods

| Method | Description |
|--------|-------------|
| `primeorders_addMarket` | Add a new trading market |
| `primeorders_submitOrder` | Submit a trading order |
| `primeorders_cancelOrder` | Cancel an order |
| `primeorders_getOrderBook` | Get current order book |
| `primeorders_getOpenOrders` | Get open orders for an address |
| `primeorders_setMarginParams` | Set margin parameters |
| `primeorders_depositCollateral` | Deposit collateral |
| `primeorders_isLiquidatable` | Check if position is liquidatable |
| `primeorders_liquidate` | Liquidate a position |

---

## Unsupported: eth_feeHistory

`eth_feeHistory` is **not supported** on Prime Chain. Use `eth_gasPrice` for legacy transaction pricing. For tooling that requires fee history, use the `--legacy` flag where available.
