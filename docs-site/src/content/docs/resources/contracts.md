---
title: "Deployed Contracts"
---

Complete reference of smart contracts deployed on Mersennet testnet (Chain ID 7919). All addresses are verified for the current testnet deployment.

## Foundation

| Contract | Address | Description |
|----------|---------|-------------|
| **Multicall3** | `0x973ee1bf0907287d1eb8a144d88b34f515c83f29` | Batched RPC reads. Used by wagmi, viem, ethers.js for efficient multi-call queries. |
| **WMRSN** | `0x079bf1207b51acda83e2e8178344f62a883f8479` | ERC-20 wrapped MRSN. Required for DEX pairs and DeFi protocols that need ERC-20 native token representation. |

## DeFi — PrimeSwap V2

| Contract | Address | Description |
|----------|---------|-------------|
| **PrimeSwapFactory** | `0x63f7a64db6d2b965189b8b48b7435668021f6b17` | Creates and tracks liquidity pairs. Uniswap V2–style AMM factory. |
| **PrimeSwapRouter** | `0x9f337f433e71ce969b991511f1dcd3d0622116bb` | Router for adding/removing liquidity and executing swaps. User-facing entry point for PrimeSwap V2. |

## DeFi — PrimeSwap V3

| Contract | Address | Description |
|----------|---------|-------------|
| **UniswapV3Factory** | *Recently deployed* | Creates V3 pools with configurable fee tiers (0.05%, 0.3%, 1%). |
| **SwapRouter** | *Recently deployed* | Executes swaps across V3 concentrated liquidity pools. |
| **NonfungiblePositionManager** | *Recently deployed* | Mints, modifies, and burns concentrated liquidity positions as ERC-721 NFTs. |
| **Quoter** | *Recently deployed* | Off-chain quote simulation for swap amounts. |

:::note
PrimeSwap V3 contracts are live at [http://46.225.30.187:4002](http://46.225.30.187:4002). Verified addresses will be added here once confirmed on-chain. See [PrimeSwap V3](/ecosystem/primeswap-v3) for protocol documentation.
:::

## Mock Tokens

| Contract | Address | Description |
|----------|---------|-------------|
| **MockUSDC** | `0xb22f77d89122e9e3784bfd3eee9616273f38238d` | Test USDC (6 decimals). For DeFi development and testing. |
| **MockUSDT** | `0x877feca38919acd7aaf7cb81f100e0454aa95c17` | Test USDT (6 decimals). For DeFi development and testing. |
| **MockDAI** | `0xb88d63a65691effbf4b6808325b1588912c15cf4` | Test DAI (18 decimals). For DeFi development and testing. |

:::note[Testnet Token Faucet]
Mock tokens (MockUSDC, MockUSDT, MockDAI) include a public `faucet()` function. Anyone can call it to receive test tokens—no approval or whitelist required. Use this for development and testing.
:::

## DeFi — PrimeOrders CLOB Integration

| Contract | Address | Description |
|----------|---------|-------------|
| **VaultStrategy** | `0x3beef509cb30609ba62c56dae369c545d9b359a8` | Demonstrates atomic EVM + CLOB interaction via the precompile. Deposits collateral, places orders, and reacts to fills in a single transaction. |
| **SmartContractMM** | `0x9f6d643f12eae11b53509598016be83e9f5bc412` | On-chain market maker with grid strategy, inventory management, and automatic rebalancing. |
| **AtomicArbitrage** | `0x77c6de42d5629ac7e454910a46d06fab34be8f14` | Atomic arbitrage across CLOB and AMM pools. Buy on CLOB, sell on AMM (or vice versa) in a single transaction. |

:::tip
These contracts demonstrate Mersennet's unique capability: **atomic composability between EVM smart contracts and the native CLOB**. This is impossible on other order book chains like Hyperliquid or dYdX where the order book is off-chain.
:::

## Quick Reference (Copy-Paste)

```
Chain ID: 7919
PrimeOrders CLOB:    0x0000000000000000000000000000000000000100 (precompile)
Multicall3:          0x973ee1bf0907287d1eb8a144d88b34f515c83f29
WMRSN:               0x079bf1207b51acda83e2e8178344f62a883f8479
MockUSDC:            0xb22f77d89122e9e3784bfd3eee9616273f38238d
MockUSDT:            0x877feca38919acd7aaf7cb81f100e0454aa95c17
MockDAI:             0xb88d63a65691effbf4b6808325b1588912c15cf4
PrimeSwapV2Factory:  0x63f7a64db6d2b965189b8b48b7435668021f6b17
PrimeSwapV2Router:   0x9f337f433e71ce969b991511f1dcd3d0622116bb
VaultStrategy:       0x3beef509cb30609ba62c56dae369c545d9b359a8
SmartContractMM:     0x9f6d643f12eae11b53509598016be83e9f5bc412
AtomicArbitrage:     0x77c6de42d5629ac7e454910a46d06fab34be8f14
```

## ABI Links

Contract ABIs can be obtained from:

- **Block Explorer** — [http://46.225.30.187](http://46.225.30.187) — Search by address and view contract details.
- **Source Code** — Mersennet contracts repository (see [GitHub](https://github.com/mersennet/mersennet)).
- **Multicall3** — Standard [Multicall3](https://github.com/mds1/multicall) ABI; compatible with wagmi/viem defaults.

## Usage Examples

### Get Test Tokens (Faucet)

```solidity
// MockUSDC, MockUSDT, MockDAI all have:
function faucet() external;
```

```javascript
const usdc = new ethers.Contract(
  '0xb22f77d89122e9e3784bfd3eee9616273f38238d',
  ['function faucet() external'],
  signer
);
await usdc.faucet();
```

### Wrap MRSN

```solidity
// WMRSN
function deposit() external payable;
function withdraw(uint256 wad) external;
```

### PrimeSwap

See [PrimeSwap V2](/ecosystem/primeswap) and [PrimeSwap V3](/ecosystem/primeswap-v3) for swap and liquidity examples.

## Upcoming Deployments

| Contract | Status |
|----------|--------|
| PrimeFi (Lending) | Ready for deployment |
| Primeport (Seaport) | Ready for deployment |
| Liquid Staking | Contracts ready |
| PrimeOrders Batch Orders (`primeorders_submitBatchOrder`) | In development |
| MEV Commit-Reveal Protection | In development |

Contract addresses for new deployments will be added to this page and the [Network Information](/getting-started/network-info) doc.
