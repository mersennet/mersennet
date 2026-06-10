---
slug: /developers/rpc/overview
sidebar_position: 1
title: "JSON-RPC Overview"
---

# JSON-RPC Overview

Mersennet exposes a JSON-RPC API compatible with the Ethereum JSON-RPC specification, plus Mersennet–specific extensions. Use it to query chain state, send transactions, and interact with smart contracts.

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

Mersennet supports the core Ethereum JSON-RPC methods:

- **Block/Chain:** `eth_blockNumber`, `eth_chainId`, `eth_getBlockByNumber`, `eth_getBlockByHash`
- **Account:** `eth_getBalance`, `eth_getTransactionCount`, `eth_getCode`, `eth_getStorageAt`
- **Transaction:** `eth_getTransactionByHash`, `eth_getTransactionReceipt`, `eth_sendTransaction`, `eth_sendRawTransaction`
- **Execution:** `eth_call`, `eth_estimateGas`, `eth_gasPrice`, `eth_feeHistory`, `eth_maxPriorityFeePerGas`
- **Logs:** `eth_getLogs`
- **Filters:** `eth_newFilter`, `eth_newBlockFilter`, `eth_newPendingTransactionFilter`, `eth_getFilterChanges`, `eth_getFilterLogs`, `eth_uninstallFilter`

See [RPC Methods Reference](/developers/rpc/methods) for full details.

### Mersennet Extensions

| Method | Description |
|--------|-------------|
| `prime_sendTransaction` | Send a transaction (alternative to `eth_sendTransaction`) |
| `prime_validators` | Get list of validators |
| `prime_getDomainEvents` | Get domain events for a block range |
| `prime_getCodeAttestation` / `prime_getCodeHash` | On-chain contract code-publication registry lookups |
| `primeorders_*` | PrimeOrders trading methods (addMarket, submitOrder, cancelOrder, getOrderBook, etc.) |
| `primebridge_*` | PrimeBridge bridge methods (enqueueOrdersToEvm, enqueueEvmToOrders, dequeueOrdersToEvm, dequeueEvmToOrders) |
| **Shielded / ZK** | Shielded transfers & orders, SP1 state proofs, and selective-disclosure reads — see the [Shielded JSON-RPC reference](/developers/privacy/shielded-rpc) |
| **WebSocket** | `eth_subscribe` and `prime_subscribe` push notifications (new heads, trades, shielded roots, state proofs) |

### Notes on specific methods

| Method | Notes |
|--------|-------|
| `eth_subscribe` / `eth_unsubscribe` | Available over **WebSocket connections only** (not HTTP). May be disabled on some public nodes — fall back to filters/polling. |
| `eth_maxPriorityFeePerGas` | Returns `0x0` — Mersennet uses an EIP-1559 base fee with no separate priority tip. |
| `debug_*` / `trace_*` / `personal_*` | Not implemented. |

## Transaction Format

:::important
`eth_sendRawTransaction` accepts **both** standard Ethereum RLP-encoded transactions (legacy, EIP-2930, EIP-1559) and Mersennet's custom binary format. MetaMask-, ethers.js-, and Foundry-signed transactions work natively.
:::

For deployment and sending transactions, you can use:

- **Hardhat / Foundry / ethers.js** signing locally and submitting via `eth_sendRawTransaction`
- **Remix** with MetaMask (injected provider)
- **eth_sendTransaction / prime_sendTransaction** for server-side flows with unlocked accounts

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
