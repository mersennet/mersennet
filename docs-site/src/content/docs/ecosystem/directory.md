---
title: "Ecosystem Directory"
---

Complete directory of live services, endpoints, and deployed contracts on Mersennet testnet (Chain ID 131071).

## Live Services

| Service | URL | Description |
|---------|-----|-------------|
| **Block Explorer** | [http://46.225.30.187](http://46.225.30.187) | PrimeScan block explorer |
| **PrimeSwap V2** | [http://46.225.30.187:4000](http://46.225.30.187:4000) | AMM DEX (Uniswap V2) |
| **PrimeSwap V3** | [http://46.225.30.187:4002](http://46.225.30.187:4002) | Concentrated Liquidity DEX (Uniswap V3) |
| **Testnet Faucet** | [http://46.225.30.187:4003](http://46.225.30.187:4003) | Get testnet MRSN tokens |
| **Validator Dashboard** | [http://46.225.30.187:4001](http://46.225.30.187:4001) | Monitor validators |
| **PrimeTrade** | [http://46.225.30.187:4004](http://46.225.30.187:4004) | Order book trading terminal |
| **Documentation** | [http://46.225.30.187:3001](http://46.225.30.187:3001) | This documentation site |
| **JSON-RPC** | `http://46.225.30.187:8545` | Ethereum-compatible RPC endpoint |
| **WebSocket** | `ws://46.225.30.187:8546` | WebSocket RPC endpoint |

## Deployed Contracts

### Foundation

| Contract | Address | Description |
|----------|---------|-------------|
| **Multicall3** | `0x973ee1bf0907287d1eb8a144d88b34f515c83f29` | Batched RPC reads (wagmi/viem compatible) |
| **WMRSN** | `0x079bf1207b51acda83e2e8178344f62a883f8479` | Wrapped MRSN (ERC-20) for DEX and DeFi |

### Mock Tokens

| Contract | Address | Decimals |
|----------|---------|----------|
| **MockUSDC** | `0xb22f77d89122e9e3784bfd3eee9616273f38238d` | 6 |
| **MockUSDT** | `0x877feca38919acd7aaf7cb81f100e0454aa95c17` | 6 |
| **MockDAI** | `0xb88d63a65691effbf4b6808325b1588912c15cf4` | 18 |

:::tip
All mock tokens have a public `faucet()` function — call it to receive test tokens.
:::

### PrimeSwap V2

| Contract | Address |
|----------|---------|
| **PrimeSwapFactory** | `0x63f7a64db6d2b965189b8b48b7435668021f6b17` |
| **PrimeSwapRouter** | `0x9f337f433e71ce969b991511f1dcd3d0622116bb` |

### PrimeSwap V3

| Contract | Address |
|----------|---------|
| **UniswapV3Factory** | *Deployed — see [Block Explorer](http://46.225.30.187)* |
| **SwapRouter** | *Deployed — see [Block Explorer](http://46.225.30.187)* |
| **NonfungiblePositionManager** | *Deployed — see [Block Explorer](http://46.225.30.187)* |
| **Quoter** | *Deployed — see [Block Explorer](http://46.225.30.187)* |

:::note
PrimeSwap V3 contracts are recently deployed. Verified addresses will be added to this page and the [Deployed Contracts](/resources/contracts) page as they are confirmed on-chain.
:::

## Quick Reference

```
Chain ID:             131071 (0x1FFFF)
RPC:                  http://46.225.30.187:8545
WebSocket:            ws://46.225.30.187:8546
Explorer:             http://46.225.30.187
Faucet:               http://46.225.30.187:4003
PrimeSwap V2:         http://46.225.30.187:4000
PrimeSwap V3:         http://46.225.30.187:4002
Validator Dashboard:  http://46.225.30.187:4001
PrimeTrade:           http://46.225.30.187:4004
Documentation:        http://46.225.30.187:3001
```

## Related Resources

- [PrimeSwap V2](/ecosystem/primeswap) — AMM DEX documentation
- [PrimeSwap V3](/ecosystem/primeswap-v3) — Concentrated liquidity DEX documentation
- [Deployed Contracts](/resources/contracts) — Detailed contract reference with ABIs
- [Network Information](/getting-started/network-info) — Full network configuration
