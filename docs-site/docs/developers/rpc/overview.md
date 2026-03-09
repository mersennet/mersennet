---
slug: /developers/rpc/overview
sidebar_position: 1
title: "JSON-RPC Overview"
---

# JSON-RPC Overview

Prime Chain exposes a JSON-RPC API compatible with the Ethereum JSON-RPC specification, plus Prime Chain–specific extensions. Use it to query chain state, send transactions, and interact with smart contracts.

## Endpoints

| Environment | HTTP RPC | WebSocket |
|-------------|----------|-----------|
| Testnet | `http://46.225.30.187:8545` | `ws://46.225.30.187:8546` |

:::tip
The WebSocket endpoint may not be enabled on all nodes. If subscriptions fail, use HTTP RPC for polling.
:::

## Authentication

**Testnet:** No authentication is required. The public RPC endpoint is open for development and testing.

**Mainnet (future):** API keys or authenticated endpoints may be introduced. Check the documentation for updates.

## Rate Limits

The testnet RPC does not enforce strict rate limits for normal development use. For high-volume applications, consider:

- Running your own node
- Implementing client-side throttling
- Caching read-only data (blocks, balances, contract state)

## Request Format

All requests use JSON-RPC 2.0 over HTTP POST:

```bash
curl -X POST http://46.225.30.187:8545 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"eth_chainId","params":[],"id":1}'
```

Response:

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": "0x1eef"
}
```

## Method Categories

### Standard Ethereum Methods

Prime Chain supports the core Ethereum JSON-RPC methods:

- **Block/Chain:** `eth_blockNumber`, `eth_chainId`, `eth_getBlockByNumber`, `eth_getBlockByHash`
- **Account:** `eth_getBalance`, `eth_getTransactionCount`, `eth_getCode`, `eth_getStorageAt`
- **Transaction:** `eth_getTransactionByHash`, `eth_getTransactionReceipt`, `eth_sendTransaction`, `eth_sendRawTransaction`
- **Execution:** `eth_call`, `eth_estimateGas`, `eth_gasPrice`
- **Logs:** `eth_getLogs`

See [RPC Methods Reference](/developers/rpc/methods) for full details.

### Prime Chain Extensions

| Method | Description |
|--------|-------------|
| `prime_sendTransaction` | Send a transaction (alternative to `eth_sendTransaction`) |
| `prime_validators` | Get list of validators |
| `prime_getDomainEvents` | Get domain events for a block range |
| `primeorders_*` | PrimeOrders trading methods (addMarket, submitOrder, cancelOrder, getOrderBook, etc.) |
| `primebridge_*` | PrimeBridge bridge methods (enqueueOrdersToEvm, enqueueEvmToOrders, dequeueOrdersToEvm, dequeueEvmToOrders) |

### Unsupported Methods

| Method | Notes |
|--------|-------|
| `eth_feeHistory` | Not supported. Use `eth_gasPrice` for legacy transactions. |
| `eth_subscribe` / `eth_unsubscribe` | WebSocket subscriptions may not be available on all nodes. |

## Transaction Format

:::important
Prime Chain uses a **custom raw transaction format** (not standard RLP). Standard tools that sign and send via `eth_sendRawTransaction` (e.g., `forge create`, some wallet libraries) may not work. Use `eth_sendTransaction` or `prime_sendTransaction` when the node has the account unlocked, or ensure your client uses the Prime Chain transaction format.
:::

For deployment and sending transactions, prefer:

- **Hardhat** with `eth_sendTransaction` (via ethers/wallet)
- **Remix** with MetaMask (injected provider)
- **prime_sendTransaction** for server-side flows with unlocked accounts

## Error Handling

Errors follow the JSON-RPC error format:

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "error": {
    "code": -32000,
    "message": "insufficient funds for transfer"
  }
}
```

Common error codes:

| Code | Meaning |
|------|---------|
| -32700 | Parse error |
| -32600 | Invalid request |
| -32601 | Method not found |
| -32602 | Invalid params |
| -32000 | Server error (e.g., insufficient funds, revert) |
