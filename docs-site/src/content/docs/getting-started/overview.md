---
title: "What is Mersennet?"
---

**Mersennet** is a high-performance, EVM-compatible Layer 1 blockchain built for speed, composability, and institutional-grade DeFi. It combines the familiarity of Ethereum's smart contract ecosystem with a **native order matching engine (PrimeOrders)**—enabling atomic cross-domain workflows that are impossible on traditional chains.

## Why Mersennet?

| Feature | Mersennet |
|---------|-------------|
| **EVM Compatibility** | Deploy existing Solidity contracts without modification |
| **Block Time** | ~1 second for fast confirmation |
| **Native CLOB** | PrimeOrders—on-chain order matching with EVM composability |
| **Account-level privacy** | Shielded accounts, ZK risk checks, and shielded orders (privacy hard fork) |
| **Verifiable state** | State transitions proven with SP1 and verifiable via a Groth16 bridge |
| **Consensus** | BFT proof-of-stake (prevote/precommit, stake-weighted proposer) |
| **Token** | PRIM (18 decimals, 1B max supply) |
| **Implementation** | Rust-based node for reliability and performance |

## Key Capabilities

- **EVM Compatibility** — Use Hardhat, Foundry, Remix, and all standard Ethereum tooling. Your contracts work as-is.
- **Fast Finality** — ~1 second block times with BFT consensus for quick confirmations.
- **PrimeOrders** — A native central limit order book (CLOB) accessible via EVM precompile, enabling DeFi strategies that combine smart contracts with order matching in a single transaction.
- **Account-level privacy** — Shielded accounts conceal balances, positions, and order flow, and leverage is secured by zero-knowledge risk checks instead of public liquidation auctions. See [Privacy on Mersennet](/privacy).
- **Verifiable state** — Every block's state transition is proven with SP1 and wrapped into a Groth16 proof an Ethereum contract can verify, so the chain is checkable from a succinct proof.
- **BFT Proof-of-Stake** — Stake-weighted proposer rotation with two-round prevote/precommit finality and escalating slashing.
- **1 Billion PRIM** — Fixed max supply with halving block rewards and structured tokenomics.

## Built for Developers

Mersennet is designed for builders. Whether you're deploying a simple ERC-20, building a DEX, or integrating lending protocols—the same tools and patterns you know from Ethereum apply. The network is live on **testnet** (Chain ID 7919) with a block explorer, faucet, and deployed infrastructure ready for development.

## What's Next?

| Step | Link |
|------|------|
| 1. Add the network to your wallet | [Wallet Setup](/getting-started/wallet-setup) |
| 2. Get testnet PRIM from the faucet | [Faucet](/getting-started/faucet) |
| 3. Send your first transaction | [First Transaction](/getting-started/first-transaction) |
| 4. Deploy a smart contract | [Hardhat Quick Start](/developers/quick-start/hardhat) |
| 5. Explore the architecture | [Consensus](/architecture/consensus) · [Tokenomics](/architecture/tokenomics) |
| 6. Learn about privacy | [Privacy on Mersennet](/privacy) |
| 7. Read the whitepaper | [Whitepaper](/whitepaper) |
