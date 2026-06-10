---
title: "PrimeTrade"
---

**PrimeTrade** is the order book trading terminal for Mersennet, providing a professional-grade interface for trading against the native on-chain Central Limit Order Book (CLOB) powered by [PrimeOrders](/architecture/prime-orders).

:::tip[Live on Testnet]
PrimeTrade is live at **[http://46.225.30.187:4004](http://46.225.30.187:4004)**
:::

## Overview

| Feature | Details |
|---------|---------|
| **Type** | Order book trading terminal |
| **Order Engine** | PrimeOrders native precompile (`0x0100`) |
| **Order Types** | Limit, Market |
| **Chain** | Mersennet Testnet (Chain ID 131071) |
| **Wallet** | MetaMask or any EVM-compatible wallet |

## How It Works

PrimeTrade connects directly to the PrimeOrders precompile—a native on-chain order matching engine embedded at the EVM level. Unlike AMM-based DEXes, PrimeTrade uses a Central Limit Order Book (CLOB) model where:

- **Limit orders** rest on the book at a specified price until filled or cancelled
- **Market orders** execute immediately against the best available resting orders
- **Matching** is atomic and happens within a single transaction via the precompile

```
User Wallet ──► PrimeTrade UI ──► Smart Contract ──► PrimeOrders Precompile (0x0100)
                                                          │
                                                  ┌───────┴───────┐
                                                  │  Order Book   │
                                                  │  (on-chain)   │
                                                  └───────────────┘
```

## Trading Pairs

PrimeTrade supports any pair listed on the PrimeOrders book. Current testnet pairs include:

| Pair | Base Token | Quote Token |
|------|-----------|-------------|
| WMRSN/USDC | WMRSN | MockUSDC |
| WMRSN/USDT | WMRSN | MockUSDT |

## Getting Started

1. Visit [http://46.225.30.187:4004](http://46.225.30.187:4004)
2. Connect your MetaMask wallet to Mersennet (Chain ID 131071)
3. Get testnet MRSN from the [Faucet](/getting-started/faucet)
4. Get test stablecoins by calling `faucet()` on the [mock token contracts](/resources/contracts)
5. Approve the token you want to trade
6. Place limit or market orders

## Comparison: PrimeTrade vs PrimeSwap

| | PrimeTrade (CLOB) | PrimeSwap (AMM) |
|-|-------------------|-----------------|
| **Model** | Order book | Constant product / Concentrated liquidity |
| **Price discovery** | Explicit bid/ask | Algorithmic (x*y=k) |
| **Order types** | Limit + Market | Swap only |
| **Slippage** | None on limit orders | Variable |
| **Best for** | Precise entries, professional trading | Quick swaps, LP yield |

## Related Resources

- [PrimeOrders Architecture](/architecture/prime-orders) — How the native CLOB precompile works
- [PrimeSwap V2](/ecosystem/primeswap) — AMM DEX for simple swaps
- [PrimeSwap V3](/ecosystem/primeswap-v3) — Concentrated liquidity AMM
- [Deployed Contracts](/resources/contracts) — Token addresses and ABIs
