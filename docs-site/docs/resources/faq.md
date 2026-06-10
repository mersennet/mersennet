---
slug: /resources/faq
sidebar_position: 3
title: "FAQ"
---

# Frequently Asked Questions

## General

### What is Mersennet?

**Mersennet** is a high-performance, EVM-compatible Layer 1 blockchain built for speed, composability, and institutional-grade DeFi. It combines Ethereum's smart contract ecosystem with a **native order matching engine (PrimeOrders)**—enabling atomic cross-domain workflows (EVM + CLOB) in a single transaction. Key features include ~1 second block times, DPoS consensus, and a 1 billion PRIM max supply with halving block rewards.

### Is Mersennet EVM compatible?

Yes. Mersennet is fully EVM compatible. You can deploy Solidity contracts without modification using Hardhat, Foundry, or Remix. Standard tooling (ethers.js, wagmi, viem) works out of the box.

### What is the Chain ID?

**7919** (hex: `0x1EEF`) for Mersennet testnet.

### What is PrimeOrders?

**PrimeOrders** is Mersennet's native central limit order book (CLOB). It's an on-chain matching engine accessible via an EVM precompile, allowing smart contracts to place, cancel, and fill orders atomically in the same transaction. This enables DeFi strategies that combine AMM liquidity with order book execution—something not possible on traditional EVM-only chains.

## Tokens & Faucet

### How do I get testnet tokens?

1. Add Mersennet to your wallet (see [Wallet Setup](/getting-started/wallet-setup)).
2. Use the **Faucet** at [http://46.225.30.187:4003](http://46.225.30.187:4003) to receive testnet PRIM.
3. For mock stablecoins (USDC, USDT, DAI), call the `faucet()` function on each contract—see [Deployed Contracts](/resources/contracts).

### What is the max supply of PRIM?

**1 billion (1,000,000,000) PRIM** with 18 decimals. 70% is allocated to block rewards (halving schedule), 10% ecosystem/grants, 10% foundation, 5% team, 5% sales. See [Tokenomics](/architecture/tokenomics) for details.

### What is WPRIM?

**WPRIM** is the ERC-20 wrapped version of native PRIM. It's required for DEX pairs (e.g., WPRIM/USDC on PrimeSwap) and DeFi protocols that expect ERC-20 tokens. Wrap with `deposit()` and unwrap with `withdraw()`. Address: `0x079bf1207b51acda83e2e8178344f62a883f8479`.

## Development

### How do I deploy a contract to Mersennet?

Use Hardhat or Foundry with Mersennet as a network. Add the RPC URL `http://46.225.30.187:8545` and Chain ID `7919`. See [Deploy with Hardhat](/developers/quick-start/hardhat) and [Deploy with Foundry](/developers/quick-start/foundry).

### What wallets are supported?

- **PrimeXDC Wallet** — Native Chrome extension for Mersennet (Chain ID 7919 preconfigured).
- **MetaMask** — Add Mersennet manually via [Wallet Setup](/getting-started/wallet-setup).
- **PrimeXDC Mobile** — React Native wallet (APK available).

Any EIP-1193–compatible wallet can connect once Mersennet is added as a custom network.

### Is there a bridge?

A cross-chain bridge is planned (Tier 3 in the roadmap). For now, testnet assets exist only on Mersennet. Bridge infrastructure is documented in the whitepaper for future deployment.

## Network

### Where is the block explorer?

[http://46.225.30.187](http://46.225.30.187) — View blocks, transactions, addresses, and contract interactions.

### Where is the RPC endpoint?

- **HTTP:** `http://46.225.30.187:8545`
- **WebSocket:** `ws://46.225.30.187:8546` (may not be enabled on all nodes)

### What is the block time?

Approximately **1 second** per block.

## Ecosystem

### What is PrimeSwap?

PrimeSwap is the native AMM/DEX on Mersennet—a Uniswap V2 fork with 0.3% swap fee. See [PrimeSwap DEX](/ecosystem/primeswap).

### What is PrimeFi?

PrimeFi is an Aave-style lending/borrowing protocol. Contracts are built and ready for deployment. See [PrimeFi Lending](/ecosystem/primefi).

### What is Primeport?

Primeport is a Seaport-based NFT marketplace supporting ERC-721 and ERC-1155. See [Primeport NFT Marketplace](/ecosystem/primeport).

---

Have more questions? Check the [Getting Started](/getting-started/overview) guides or open an issue on [GitHub](https://github.com/PrimeNumbersLabs/mersennet).
