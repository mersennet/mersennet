---
title: "PrimeSwap V3 DEX"
---

**PrimeSwap V3** is a concentrated liquidity DEX on Mersennet, built on the Uniswap V3 protocol. It enables capital-efficient liquidity positions with custom price ranges and multiple fee tiers—giving liquidity providers fine-grained control over where their capital is deployed.

## Overview

| Feature | PrimeSwap V3 |
|---------|--------------|
| **Architecture** | Uniswap V3 fork |
| **Liquidity** | Concentrated (custom price ranges) |
| **Fee Tiers** | 0.05%, 0.3%, 1% |
| **Position Type** | NFT-based (ERC-721) |
| **Chain** | Mersennet Testnet (Chain ID 7919) |
| **Frontend** | [http://46.225.30.187:4002](http://46.225.30.187:4002) |

## V3 vs V2

| Feature | PrimeSwap V2 | PrimeSwap V3 |
|---------|-------------|--------------|
| **Liquidity Model** | Full range (x × y = k) | Concentrated (custom ranges) |
| **Capital Efficiency** | 1× | Up to 4000× for narrow ranges |
| **Fee Tiers** | 0.3% only | 0.05%, 0.3%, 1% |
| **Position Representation** | Fungible LP tokens (ERC-20) | Non-fungible positions (ERC-721 NFT) |
| **Best For** | Simple swaps, passive LPs | Active LPs, stablecoin pairs, advanced strategies |

## Key Concepts

### Concentrated Liquidity

Unlike V2 where liquidity is spread across the entire price curve (0 to ∞), V3 lets LPs choose a specific price range. Liquidity is only active when the current price is within that range, resulting in higher fee earnings per unit of capital.

### Fee Tiers

Each pool is created with one of three fee tiers, allowing the market to express different risk profiles:

| Fee Tier | Best For | Example Pairs |
|----------|----------|---------------|
| **0.05%** | Stable pairs with minimal price movement | USDC/USDT, USDC/DAI |
| **0.3%** | Standard volatile pairs | WMRSN/USDC, WMRSN/USDT |
| **1%** | Exotic or high-volatility pairs | New tokens, low-liquidity assets |

### NFT Position Management

Each liquidity position is represented as an ERC-721 NFT, managed through the **NonfungiblePositionManager** contract. The NFT encodes the pool, price range (tick bounds), and liquidity amount.

## Core Contracts

| Contract | Purpose |
|----------|---------|
| **UniswapV3Factory** | Creates and tracks V3 pools with fee tier configuration |
| **SwapRouter** | User-facing swap execution across V3 pools |
| **NonfungiblePositionManager** | Mint, modify, and burn concentrated liquidity positions (NFTs) |
| **Quoter** | Off-chain quote simulation for swap amounts |

:::note
V3 contract addresses are listed on the [Ecosystem Directory](/ecosystem/directory) and [Deployed Contracts](/resources/contracts) pages.
:::

## Adding Concentrated Liquidity

To provide liquidity in V3, you select a token pair, fee tier, and price range:

```solidity
INonfungiblePositionManager.MintParams memory params = INonfungiblePositionManager.MintParams({
    token0: USDC,
    token1: WMRSN,
    fee: 3000,           // 0.3% fee tier
    tickLower: -887220,  // lower bound of price range
    tickUpper: 887220,   // upper bound of price range
    amount0Desired: 1000e6,
    amount1Desired: 1000e18,
    amount0Min: 0,
    amount1Min: 0,
    recipient: msg.sender,
    deadline: block.timestamp + 1200
});

(uint256 tokenId, uint128 liquidity, uint256 amount0, uint256 amount1) =
    nonfungiblePositionManager.mint(params);
```

## Swapping on V3

Use the SwapRouter for exact-input or exact-output swaps:

```solidity
ISwapRouter.ExactInputSingleParams memory params = ISwapRouter.ExactInputSingleParams({
    tokenIn: USDC,
    tokenOut: WMRSN,
    fee: 3000,
    recipient: msg.sender,
    deadline: block.timestamp + 1200,
    amountIn: 100e6,
    amountOutMinimum: 0,
    sqrtPriceLimitX96: 0
});

uint256 amountOut = swapRouter.exactInputSingle(params);
```

## Related Resources

- [PrimeSwap V2](/ecosystem/primeswap) — AMM DEX with constant product pools
- [Ecosystem Directory](/ecosystem/directory) — All live services and contract addresses
- [Deployed Contracts](/resources/contracts) — Full contract reference
- [ERC-20 Guide](/developers/contracts/erc20-guide) — Deploy tokens to list on PrimeSwap
