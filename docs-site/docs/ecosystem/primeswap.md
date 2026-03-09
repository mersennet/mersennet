---
slug: /ecosystem/primeswap
sidebar_position: 1
title: "PrimeSwap V2 DEX"
---

# PrimeSwap V2 DEX

**PrimeSwap V2** is the native automated market maker (AMM) and decentralized exchange (DEX) on Prime Chain. Built as a Uniswap V2 fork, it provides permissionless token swaps, liquidity provision, and LP token mechanics—enabling the core DeFi primitives that power the Prime Chain ecosystem.

:::tip PrimeSwap V3 is live
**[PrimeSwap V3](/ecosystem/primeswap-v3)** adds concentrated liquidity with the Uniswap V3 protocol. Choose V2 for simple swaps and full-range liquidity, or V3 for capital-efficient concentrated positions.
:::

## Overview

| Feature | PrimeSwap |
|---------|-----------|
| **Architecture** | Uniswap V2 fork |
| **Swap Fee** | 0.3% |
| **Pair Model** | Constant product (x × y = k) |
| **LP Tokens** | ERC-20, proportional to pool share |
| **Chain** | Prime Chain Testnet (Chain ID 7919) |

## Factory + Router Architecture

PrimeSwap uses a two-contract design:

- **PrimeSwapFactory** — Creates and tracks liquidity pairs. Each pair is a separate contract.
- **PrimeSwapRouter** — User-facing contract for adding liquidity, removing liquidity, and executing swaps. Handles approvals, slippage, and deadline checks.

```
┌─────────────────┐     createPair()      ┌──────────────────┐
│ PrimeSwapFactory│ ───────────────────►  │ PrimeSwapPair    │
│                 │                       │ (WPRIM/USDC, etc)│
└────────┬────────┘                       └──────────────────┘
         │
         │ getPair(tokenA, tokenB)
         ▼
┌─────────────────┐     swap/addLiquidity  ┌──────────────────┐
│ PrimeSwapRouter │ ───────────────────►  │ PrimeSwapPair(s) │
└─────────────────┘                       └──────────────────┘
```

## Deployed Contract Addresses

| Contract | Address |
|----------|---------|
| **PrimeSwapFactory** | `0x63f7a64db6d2b965189b8b48b7435668021f6b17` |
| **PrimeSwapRouter** | `0x9f337f433e71ce969b991511f1dcd3d0622116bb` |

:::info
PrimeSwap depends on **WPRIM** (wrapped PRIM) for native token pairs. WPRIM address: `0x079bf1207b51acda83e2e8178344f62a883f8479`
:::

## Creating Pairs

Use the Factory to create a new trading pair:

```solidity
// Get the pair address (creates if it doesn't exist)
address pair = IPrimeSwapFactory(factory).createPair(tokenA, tokenB);
```

Token order does not matter—the Factory sorts addresses to ensure deterministic pair addresses.

## Adding Liquidity

### Token–Token Pair

```solidity
router.addLiquidity(
    tokenA,           // address
    tokenB,           // address
    amountADesired,   // uint256
    amountBDesired,   // uint256
    amountAMin,       // uint256 - slippage tolerance
    amountBMin,       // uint256 - slippage tolerance
    to,               // address - LP token recipient
    deadline          // uint256 - unix timestamp
);
```

### PRIM–Token Pair (addLiquidity)

For pairs involving native PRIM, use `addLiquidity` and send PRIM as `msg.value`:

```solidity
router.addLiquidity{value: amountPRIM}(
    token,              // address
    amountTokenDesired, // uint256
    amountTokenMin,     // uint256
    amountPRIMMin,      // uint256
    to,                 // address
    deadline            // uint256
);
```

## Swapping Tokens

### Exact Input (swapExactTokensForTokens)

```solidity
uint256[] memory amounts = router.swapExactTokensForTokens(
    amountIn,        // uint256
    amountOutMin,    // uint256 - minimum output (slippage)
    path,            // address[] - [tokenIn, ..., tokenOut]
    to,              // address
    deadline         // uint256
);
// amounts[amounts.length - 1] = actual output amount
```

### Exact Output (swapTokensForExactTokens)

```solidity
uint256[] memory amounts = router.swapTokensForExactTokens(
    amountOut,       // uint256 - exact output desired
    amountInMax,     // uint256 - maximum input (slippage)
    path,            // address[]
    to,              // address
    deadline         // uint256
);
```

### Swapping PRIM for Tokens (swapExactPRIMForTokens)

```solidity
router.swapExactPRIMForTokens{value: amountPRIM}(
    amountOutMin,
    path,    // path[0] must be WPRIM
    to,
    deadline
);
```

### Swapping Tokens for PRIM (swapExactTokensForPRIM)

```solidity
router.swapExactTokensForPRIM(
    amountIn,
    amountOutMin,
    path,    // path[path.length - 1] must be WPRIM
    to,
    deadline
);
```

## LP Tokens

When you add liquidity, you receive **LP (liquidity provider) tokens**—ERC-20 tokens representing your share of the pool. Your share is proportional to your deposited amount relative to total liquidity.

- **Mint** — LP tokens are minted when you add liquidity via `pair.mint(to)`.
- **Burn** — LP tokens are burned when you remove liquidity via `pair.burn(to)`.
- **Redemption** — Call `router.removeLiquidity` or `removeLiquidityPRIM` to burn LP tokens and receive both tokens (or PRIM) back.

## Example: Swap USDC for PRIM

```javascript
import { ethers } from 'ethers';

const ROUTER = '0x9f337f433e71ce969b991511f1dcd3d0622116bb';
const WPRIM = '0x079bf1207b51acda83e2e8178344f62a883f8479';
const USDC = '0xb22f77d89122e9e3784bfd3eee9616273f38238d';

const routerAbi = [
  'function swapExactTokensForPRIM(uint amountIn, uint amountOutMin, address[] path, address to, uint deadline) external returns (uint[] amounts)',
  'function getAmountsOut(uint amountIn, address[] path) view returns (uint[] amounts)'
];

const provider = new ethers.BrowserProvider(window.ethereum);
const signer = await provider.getSigner();
const router = new ethers.Contract(ROUTER, routerAbi, signer);

// Approve USDC
const usdc = new ethers.Contract(USDC, ['function approve(address,uint256) returns (bool)'], signer);
await usdc.approve(ROUTER, ethers.MaxUint256);

// Get expected output
const path = [USDC, WPRIM];
const amountIn = ethers.parseUnits('100', 6); // 100 USDC
const amounts = await router.getAmountsOut(amountIn, path);
const expectedOut = amounts[1];

// Swap with 1% slippage
const amountOutMin = expectedOut * 99n / 100n;
const deadline = Math.floor(Date.now() / 1000) + 60 * 20; // 20 minutes
await router.swapExactTokensForPRIM(amountIn, amountOutMin, path, await signer.getAddress(), deadline);
```

## Fee Structure

- **Swap fee:** 0.3% on each trade (0.003)
- **LP rewards:** Fees accrue to the pool and increase the value of LP tokens proportionally

## Related Resources

- [PrimeSwap V3](/ecosystem/primeswap-v3) — Concentrated liquidity DEX
- [Deployed Contracts](/resources/contracts) — Full contract reference
- [ERC-20 Guide](/developers/contracts/erc20-guide) — Deploy tokens to list on PrimeSwap
- [Network Information](/getting-started/network-info) — RPC, Chain ID, and configuration
