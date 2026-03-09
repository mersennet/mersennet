---
slug: /architecture/prime-orders
sidebar_position: 3
title: "PrimeOrders (On-chain CLOB)"
---

# PrimeOrders (On-chain CLOB)

PrimeOrders is Prime Chain's **native on-chain central limit order book (CLOB)** -- a key differentiator that enables atomic DeFi strategies impossible on traditional chains. It is accessible to smart contracts via an EVM precompile at address `0x0000000000000000000000000000000000000100`.

## Overview

| Aspect | Detail |
|--------|--------|
| **Type** | Native order matching engine |
| **Precompile Address** | `0x0000000000000000000000000000000000000100` |
| **Access** | EVM contracts (Solidity) via `CALL` / `STATICCALL` |
| **Matching** | Price-time priority |

Unlike CLOBs implemented purely in Solidity (gas-intensive, slow) or on separate chains (no atomic composability), PrimeOrders is:

- **Native** -- Built into the chain execution layer
- **Atomic** -- Same block, same state, same transaction as EVM calls
- **Composable** -- Smart contracts can place orders, cancel, deposit collateral, and read positions in a single tx

## Architecture

```
+-----------------------------------------------------------------+
|                        Single Transaction                        |
+------------------------------------------------------------------+
|                                                                  |
|   EVM Contract                    PrimeOrders Precompile         |
|   +---------------+              +---------------------------+   |
|   | VaultStrategy  |--CALL(0x100)->| placeOrder / cancelOrder |   |
|   | AtomicArbitrage|              | getPosition / deposit     |   |
|   +---------------+              +---------------------------+   |
|            |                                |                    |
|            |         Shared State           |                    |
|            +--------------------------------+                    |
|                                                                  |
+------------------------------------------------------------------+
```

## IPrimeOrders Interface

The canonical Solidity interface is defined in `contracts/src/interfaces/IPrimeOrders.sol`. Smart contracts interact with the precompile by casting the precompile address:

```solidity
import "../interfaces/IPrimeOrders.sol";

IPrimeOrders orders = IPrimeOrders(0x0000000000000000000000000000000000000100);
```

### Function Reference

#### placeOrder

Place a limit order on the order book.

```solidity
function placeOrder(
    uint64 marketId,
    bool isBuy,
    uint256 price,
    uint256 size,
    uint8 tif
) external returns (uint256 orderId, uint256 filled, uint256 remaining);
```

| Parameter | Type | Description |
|-----------|------|-------------|
| `marketId` | `uint64` | Numeric market identifier (e.g. 1 = PRIM/USDC) |
| `isBuy` | `bool` | `true` = buy, `false` = sell |
| `price` | `uint256` | Price in quote-asset units (18 decimals) |
| `size` | `uint256` | Order size in base-asset units (18 decimals) |
| `tif` | `uint8` | Time-in-force: 0 = GTC, 1 = IOC, 2 = FOK |

**Returns:** `orderId` (unique ID), `filled` (amount matched immediately), `remaining` (amount left on book).

**Gas:** 50,000

#### cancelOrder

Cancel an open order.

```solidity
function cancelOrder(uint256 orderId) external returns (bool success);
```

**Gas:** 20,000

#### depositCollateral

Deposit native PRIM as trading collateral. The `amount` must match `msg.value`.

```solidity
function depositCollateral(uint256 amount) external returns (bool success);
```

**Gas:** 25,000

#### withdrawCollateral

Withdraw collateral back to the caller (subject to margin requirements).

```solidity
function withdrawCollateral(uint256 amount) external returns (bool success);
```

**Gas:** 25,000

#### getPosition

Query the caller's position for a given market.

```solidity
function getPosition(uint64 marketId) external view returns (int128 size, uint256 entryPrice);
```

`size` is signed: positive = long, negative = short.

**Gas:** 5,000

#### getCollateral

Query the caller's total collateral balance.

```solidity
function getCollateral() external view returns (uint256 collateral);
```

**Gas:** 3,000

#### isLiquidatable

Check if an account can be liquidated.

```solidity
function isLiquidatable(address account) external view returns (bool);
```

**Gas:** 10,000

#### getBestBidAsk

Get the current best bid and ask prices for a market.

```solidity
function getBestBidAsk(uint64 marketId) external view returns (uint256 bestBid, uint256 bestAsk);
```

**Gas:** 5,000

## Function Selectors

| Selector | Function |
|----------|----------|
| `0x4c570d73` | `placeOrder(uint64,bool,uint256,uint256,uint8)` |
| `0x514fcac7` | `cancelOrder(uint256)` |
| `0xbad4a01f` | `depositCollateral(uint256)` |
| `0x6112fe2e` | `withdrawCollateral(uint256)` |
| `0x0f85fc5a` | `getPosition(uint64)` |
| `0x5c1548fb` | `getCollateral()` |
| `0x042e02cf` | `isLiquidatable(address)` |
| `0x8ee0a7fa` | `getBestBidAsk(uint64)` |

## Example: Vault Strategy

A vault strategy combines yield farming with order matching atomically:

```solidity
import "../interfaces/IPrimeOrders.sol";

contract VaultStrategy {
    IPrimeOrders constant PRIME_ORDERS = IPrimeOrders(PRIME_ORDERS_ADDRESS);

    function updateQuotes(uint256 midPrice) external {
        // Cancel old orders
        PRIME_ORDERS.cancelOrder(lastBidOrderId);
        PRIME_ORDERS.cancelOrder(lastAskOrderId);

        // Place new quotes (atomic in single tx)
        (uint256 bidId,,) = PRIME_ORDERS.placeOrder(marketId, true, midPrice - spread, 1 ether, 0);
        (uint256 askId,,) = PRIME_ORDERS.placeOrder(marketId, false, midPrice + spread, 1 ether, 0);
    }

    function deposit() external payable {
        PRIME_ORDERS.depositCollateral(msg.value);
    }
}
```

All of this happens in **one transaction** -- no cross-chain bridges, no multi-step user flows, no race conditions.

## Example: Atomic Arbitrage

An arbitrage contract can exploit price differences between PrimeOrders and an AMM:

```solidity
function clobToAmm(uint64 marketId, uint256 buyPrice, uint256 size, address ammPool) external {
    // Buy on CLOB (fills atomically)
    (, uint256 filled,) = PRIME_ORDERS.placeOrder(marketId, true, buyPrice, size, 1); // IOC
    require(filled > 0, "not filled");

    // Sell on AMM in same tx
    ammPool.call(abi.encodeWithSignature("swap(uint256,uint256)", filled, 0));
}
```

Because the EVM and PrimeOrders share the same state, the arbitrage either succeeds entirely or reverts.

## Matching Engine

Price-time priority:

- **Price**: Best bid/ask filled first
- **Time**: Earlier orders at same price have priority
- **Atomic**: IOC/FOK orders fill within the same transaction they're placed

## Collateral and Risk

- PrimeOrders supports **margin trading** with configurable initial and maintenance margin
- **Liquidations** can be triggered when margin falls below maintenance via `isLiquidatable()`
- Smart contracts can call liquidation logic atomically with other operations
- Collateral is global (not per-market) and denominated in native PRIM

## Summary

| Feature | Benefit |
|---------|---------|
| **Precompile at `0x...0100`** | Direct EVM access, no separate RPC |
| **Atomic composability** | Vault, order, fill in one transaction |
| **Native matching** | O(log n) order book ops, no gas-heavy Solidity loops |
| **Shared state** | EVM and CLOB see the same balances and positions |
| **8 functions** | Complete trading lifecycle via standard Solidity calls |

No other L1 offers atomic EVM + CLOB interaction in a single transaction. Prime Chain enables institutional-grade DeFi strategies -- vaults, arbitrage, market making -- that are infeasible elsewhere.
